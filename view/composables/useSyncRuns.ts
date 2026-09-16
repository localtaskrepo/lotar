import { ref } from 'vue'
import { api } from '../api/client'
import type { SyncReportEntry, SyncReportMeta, SyncResponse } from '../api/types'
import { useSse } from './useSse'

export type SyncAction = 'pull' | 'push' | 'check'
export type SyncRunStatus = 'running' | 'success' | 'error' | 'unknown'
/** Config scope key: '' is the global scope, otherwise a project prefix. */
export type SyncScope = string

/**
 * Captured when a run starts from this UI. The origin scope (which config's
 * remotes map launched the run) is distinct from the execution task project
 * the backend resolves (explicit project, then default_project, then the
 * Jira remote project key); both are needed to attribute state and to look
 * up stored reports under the root the backend actually wrote to.
 */
export interface SyncRunOriginContext {
  originScope: SyncScope
  /** Scope whose reports root the backend resolves for this run. */
  reportQueryScope: SyncScope
  /** Task project the run is expected to execute into (display only). */
  expectedExecProject: string | null
}

export interface SyncRun {
  id: string
  remote: string
  action: SyncAction
  actionLabel: string
  status: SyncRunStatus
  startedAt: string
  finishedAt?: string
  context: SyncRunOriginContext
  summary?: SyncResponse['summary']
  report?: SyncReportMeta | null
  reportEntries?: SyncReportEntry[]
  dryRun?: boolean
  error?: string
  /** True when the HTTP request itself failed; distinct from a server sync_failed event. */
  transportError?: boolean
  warnings?: string[]
  info?: string[]
}

/**
 * A run observed only over SSE (CLI or another client started it). Its config
 * origin is unknowable, so it never influences per-scope remote state; it is
 * surfaced through report items carrying the server-reported task project.
 */
export interface ExternalSyncRun {
  id: string
  remote: string
  action: SyncAction
  actionLabel: string
  status: SyncRunStatus
  startedAt: string
  finishedAt?: string
  execProject: string | null
  report?: SyncReportMeta | null
  reportEntries?: SyncReportEntry[]
  error?: string
}

export interface StartRunInput {
  action: SyncAction
  remote: string
  context: SyncRunOriginContext
}

export interface SettleOutcome {
  finalized: 'now' | 'already' | 'missing'
}

let runCounter = 0
function makeRunId(remote: string): string {
  runCounter += 1
  const safe = remote.replace(/[^A-Za-z0-9_-]/g, '') || 'remote'
  return `sync-${safe}-${Date.now()}-${runCounter}-${Math.random().toString(36).slice(2, 8)}`
}

function actionLabel(action: SyncAction): string {
  return action === 'check' ? 'CHECK' : action.toUpperCase()
}

function projectOrNull(value: unknown): string | null {
  if (typeof value !== 'string') return null
  const trimmed = value.trim()
  return trimmed ? trimmed : null
}

const KNOWN_RUNS_CAP = 20
const EXTERNAL_RUNS_CAP = 10
const KNOWN_ENTRIES_CAP = 200
const EXTERNAL_ENTRIES_CAP = 50
const UNKNOWN_NOTE = 'Connection lost; final run status is unknown.'

export function useSyncRuns(options: {
  onRunFinalized?: (run: SyncRun, via: 'sse' | 'reconcile') => void
  onExternalFinalized?: (run: ExternalSyncRun) => void
  onRunProgress?: (run: SyncRun) => void
  onExternalProgress?: (run: ExternalSyncRun) => void
  /** Called after reconnect reconciliation finished; refresh server truth. */
  onReconnect?: () => void
} = {}) {
  const knownRuns = ref<SyncRun[]>([])
  const externalRuns = ref<ExternalSyncRun[]>([])

  let sse: ReturnType<typeof useSse> | null = null
  const unsubscribers: Array<() => void> = []

  function findKnown(runId: string): SyncRun | undefined {
    return knownRuns.value.find((run) => run.id === runId)
  }

  function patchKnown(runId: string, patch: Partial<SyncRun>): SyncRun | undefined {
    let updated: SyncRun | undefined
    knownRuns.value = knownRuns.value.map((run) => {
      if (run.id !== runId) return run
      updated = { ...run, ...patch }
      return updated
    })
    return updated
  }

  function upsertExternal(run: ExternalSyncRun) {
    externalRuns.value = [run, ...externalRuns.value.filter((entry) => entry.id !== run.id)].slice(0, EXTERNAL_RUNS_CAP)
  }

  function findExternal(runId: string): ExternalSyncRun | undefined {
    return externalRuns.value.find((run) => run.id === runId)
  }

  function startRun(input: StartRunInput): SyncRun {
    const run: SyncRun = {
      id: makeRunId(input.remote),
      remote: input.remote,
      action: input.action,
      actionLabel: actionLabel(input.action),
      status: 'running',
      startedAt: new Date().toISOString(),
      context: input.context,
      dryRun: input.action === 'check',
    }
    knownRuns.value = [run, ...knownRuns.value.filter((entry) => entry.id !== run.id)].slice(0, KNOWN_RUNS_CAP)
    return run
  }

  /** Late REST responses must never regress a run SSE already finalized. */
  function settleRunFromResponse(runId: string, result: SyncResponse): SettleOutcome {
    const run = findKnown(runId)
    if (!run) return { finalized: 'missing' }
    if (run.status === 'success') {
      if (!run.reportEntries?.length && result.report_entries?.length) {
        patchKnown(runId, { reportEntries: result.report_entries })
      }
      return { finalized: 'already' }
    }
    if (run.status === 'error') {
      // A run the server already reported failed must not be flipped by a
      // late or out-of-order response. (An 'unknown' run MAY be finalized:
      // its original request can still complete after a reconnect.)
      return { finalized: 'already' }
    }
    patchKnown(runId, {
      status: 'success',
      finishedAt: new Date().toISOString(),
      summary: result.summary,
      warnings: result.warnings,
      info: result.info,
      report: result.report ?? run.report ?? null,
      reportEntries: result.report_entries,
      dryRun: result.dry_run ?? run.dryRun,
      transportError: false,
    })
    return { finalized: 'now' }
  }

  function failRun(runId: string, message: string, transport = false): SettleOutcome {
    const run = findKnown(runId)
    if (!run) return { finalized: 'missing' }
    if (run.status === 'success') return { finalized: 'already' }
    patchKnown(runId, {
      status: 'error',
      finishedAt: new Date().toISOString(),
      error: message,
      transportError: transport,
    })
    return { finalized: 'now' }
  }

  function isRemoteBusy(scope: SyncScope, remote: string): boolean {
    return knownRuns.value.some(
      (run) => run.context.originScope === scope && run.remote === remote && run.status === 'running',
    )
  }

  function lastUnknownRunFor(scope: SyncScope, remote: string): SyncRun | undefined {
    return knownRuns.value.find(
      (run) => run.context.originScope === scope && run.remote === remote && run.status === 'unknown',
    )
  }

  function runsForScope(scope: SyncScope): SyncRun[] {
    return knownRuns.value.filter((run) => run.context.originScope === scope)
  }

  function externalRunsForScope(scope: SyncScope): ExternalSyncRun[] {
    return externalRuns.value.filter((run) => scope === '' || run.execProject === scope)
  }

  function handleStarted(payload: any) {
    const runId = String(payload?.run_id ?? '').trim()
    if (!runId) return
    const remote = String(payload?.remote ?? 'unknown')
    const action = String(payload?.direction ?? 'pull') as SyncAction
    const execProject = projectOrNull(payload?.project)
    const startedAt = String(payload?.started_at ?? new Date().toISOString())
    const dryRun = typeof payload?.dry_run === 'boolean' ? payload.dry_run : undefined
    const known = findKnown(runId)
    if (known) {
      patchKnown(runId, {
        remote,
        action,
        actionLabel: actionLabel(action),
        startedAt,
        dryRun: dryRun ?? known.dryRun,
        error: known.status === 'running' ? undefined : known.error,
      })
      return
    }
    upsertExternal({
      id: runId,
      remote,
      action,
      actionLabel: actionLabel(action),
      status: 'running',
      startedAt,
      execProject,
      reportEntries: [],
    })
  }

  function handleProgress(payload: any) {
    const runId = String(payload?.run_id ?? '').trim()
    const entry = payload?.entry as SyncReportEntry | undefined
    if (!runId || !entry) return
    const action = String(payload?.direction ?? 'pull') as SyncAction
    const known = findKnown(runId)
    if (known) {
      const updated = patchKnown(runId, {
        action,
        actionLabel: actionLabel(action),
        summary: payload?.summary || known.summary,
        dryRun: typeof payload?.dry_run === 'boolean' ? payload.dry_run : known.dryRun,
        reportEntries: [entry, ...(known.reportEntries ?? [])].slice(0, KNOWN_ENTRIES_CAP),
      })
      if (updated) options.onRunProgress?.(updated)
      return
    }
    const external = findExternal(runId)
    const base: ExternalSyncRun = external ?? {
      id: runId,
      remote: String(payload?.remote ?? 'unknown'),
      action,
      actionLabel: actionLabel(action),
      status: 'running',
      startedAt: new Date().toISOString(),
      execProject: projectOrNull(payload?.project),
      reportEntries: [],
    }
    const updated: ExternalSyncRun = {
      ...base,
      action,
      actionLabel: actionLabel(action),
      execProject: projectOrNull(payload?.project) ?? base.execProject,
      reportEntries: [entry, ...(base.reportEntries ?? [])].slice(0, EXTERNAL_ENTRIES_CAP),
    }
    upsertExternal(updated)
    options.onExternalProgress?.(updated)
  }

  function handleCompleted(payload: any) {
    const runId = String(payload?.run_id ?? '').trim()
    if (!runId) return
    const report = payload?.report as SyncReportMeta | undefined
    const finishedAt = String(payload?.finished_at ?? new Date().toISOString())
    const known = findKnown(runId)
    if (known) {
      if (known.status !== 'success') {
        const updated = patchKnown(runId, {
          status: 'success',
          finishedAt,
          summary: report?.summary ?? known.summary,
          warnings: report?.warnings ?? known.warnings,
          info: report?.info ?? known.info,
          report: report ?? known.report ?? null,
          dryRun: report?.dry_run ?? known.dryRun,
          transportError: false,
          error: undefined,
        })
        if (updated) options.onRunFinalized?.(updated, 'sse')
      }
      return
    }
    const action = String(report?.direction ?? 'pull') as SyncAction
    const external = findExternal(runId)
    const updated: ExternalSyncRun = {
      id: runId,
      remote: String(report?.remote ?? external?.remote ?? 'unknown'),
      action,
      actionLabel: actionLabel(action),
      status: 'success',
      startedAt: external?.startedAt ?? String(report?.created_at ?? new Date().toISOString()),
      finishedAt,
      execProject: projectOrNull(report?.project) ?? external?.execProject ?? null,
      report: report ?? null,
      // Deep-copied accumulated progress entries survive completion so the
      // live report stays inspectable; a fresh copy avoids aliasing the old run.
      reportEntries: external?.reportEntries ? external.reportEntries.map((entry) => ({ ...entry })) : [],
    }
    upsertExternal(updated)
    options.onExternalFinalized?.(updated)
  }

  function handleFailed(payload: any) {
    const runId = String(payload?.run_id ?? '').trim()
    if (!runId) return
    const message = String(payload?.error ?? 'Sync failed')
    const finishedAt = String(payload?.finished_at ?? new Date().toISOString())
    const known = findKnown(runId)
    if (known) {
      const outcome = failRun(runId, message)
      if (outcome.finalized !== 'missing') {
        const updated = findKnown(runId)
        if (updated) options.onRunFinalized?.(updated, 'sse')
      }
      return
    }
    const external = findExternal(runId)
    const updated: ExternalSyncRun = {
      id: runId,
      remote: external?.remote ?? 'unknown',
      action: external?.action ?? 'pull',
      actionLabel: external?.actionLabel ?? 'PULL',
      status: 'error',
      startedAt: external?.startedAt ?? new Date().toISOString(),
      finishedAt,
      execProject: external?.execProject ?? null,
      reportEntries: external?.reportEntries ?? [],
      error: message,
    }
    upsertExternal(updated)
    options.onExternalFinalized?.(updated)
  }

  /**
   * Finalize known running runs of a scope against a freshly loaded report
   * list for that scope. A list miss never marks a run unknown: the list root
   * or filters may legitimately exclude a run stored under another root.
   */
  function reconcileFromList(scope: SyncScope, metas: SyncReportMeta[]) {
    const candidates = knownRuns.value.filter(
      (run) => run.status === 'running' && run.context.originScope === scope,
    )
    for (const run of candidates) {
      const meta = metas.find((entry) => entry.id === run.id)
      if (!meta) continue
      const updated = patchKnown(run.id, {
        status: 'success',
        finishedAt: meta.created_at,
        summary: meta.summary ?? run.summary,
        warnings: meta.warnings,
        info: meta.info,
        report: meta,
        error: undefined,
      })
      if (updated) options.onRunFinalized?.(updated, 'reconcile')
    }
  }

  /**
   * After an SSE reconnect events may have been missed. Query each still
   * running known run's captured report root directly: a found report
   * finalizes the run; a miss only means the outcome is unknown (the run may
   * legitimately still be executing server-side — there is no run registry),
   * so the run is marked unknown rather than failed and a later completion
   * event can still finalize it.
   */
  async function reconcileAfterReconnect() {
    const running = knownRuns.value.filter((run) => run.status === 'running')
    for (const run of running) {
      let metas: SyncReportMeta[] = []
      let queryError: unknown = null
      try {
        const payload = await api.syncReportsList({ project: run.context.reportQueryScope || undefined, limit: 200 })
        metas = payload.reports
      } catch (err) {
        queryError = err
      }
      const fresh = findKnown(run.id)
      if (!fresh || fresh.status !== 'running') continue
      const meta = metas.find((entry) => entry.id === run.id)
      if (meta) {
        const updated = patchKnown(run.id, {
          status: 'success',
          finishedAt: meta.created_at,
          summary: meta.summary ?? fresh.summary,
          warnings: meta.warnings,
          info: meta.info,
          report: meta,
          error: undefined,
        })
        if (updated) options.onRunFinalized?.(updated, 'reconcile')
      } else if (queryError) {
        // The lookup itself failed: the outcome is uncertain, not known-absent.
        patchKnown(run.id, {
          status: 'unknown',
          error: 'Connection uncertain: the report lookup failed, so the final run status is unknown.',
        })
      } else {
        // The lookup succeeded and the report is absent. Without a server run
        // registry this still cannot prove failure: the run may be executing
        // or its report may live under another root. Report it honestly.
        patchKnown(run.id, {
          status: 'unknown',
          error: 'No stored report was found after reconnect; the run may still be executing or its outcome is unavailable.',
        })
      }
    }
    externalRuns.value = externalRuns.value.map((run) =>
      run.status === 'running' ? { ...run, status: 'unknown', error: UNKNOWN_NOTE } : run,
    )
  }

  /**
   * Persistent, deliberately UNFILTERED sync event stream: attribution is a
   * client-side concern keyed by run identity, so no project query filter is
   * applied and the connection survives project switches.
   */
  function connect() {
    if (sse) return
    sse = useSse(
      '/api/events',
      { kinds: 'sync_started,sync_progress,sync_completed,sync_failed' },
      {
        onReconnect: () => {
          void reconcileAfterReconnect().finally(() => options.onReconnect?.())
        },
      },
    )
    const bindings: Array<[string, (payload: any) => void]> = [
      ['sync_started', handleStarted],
      ['sync_progress', handleProgress],
      ['sync_completed', handleCompleted],
      ['sync_failed', handleFailed],
    ]
    for (const [kind, handler] of bindings) {
      const wrapped = (ev: MessageEvent) => {
        if (!ev.data) return
        try {
          handler(JSON.parse(ev.data))
        } catch (err) {
          console.warn('Failed to parse sync SSE payload', err)
        }
      }
      sse.on(kind, wrapped)
      unsubscribers.push(() => sse?.off(kind, wrapped))
    }
  }

  function disconnect() {
    unsubscribers.splice(0).forEach((fn) => fn())
    sse?.close()
    sse = null
  }

  return {
    knownRuns,
    externalRuns,
    connect,
    disconnect,
    startRun,
    settleRunFromResponse,
    failRun,
    isRemoteBusy,
    lastUnknownRunFor,
    runsForScope,
    externalRunsForScope,
    reconcileFromList,
    reconcileAfterReconnect,
  }
}

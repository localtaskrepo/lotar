import { computed, reactive, ref, watch, type ComputedRef, type Ref } from 'vue'
import type { AgentInstructionsValue, ConfigInspectResult, ConfigSource, ProjectDTO, TagProperties } from '../api/types'
import { resolveTagColor } from '../utils/tagColors'
import { parseThemeValue } from '../utils/themes'
import { provenanceClass as getProvenanceClass, provenanceLabel as getProvenanceLabel } from '../utils/provenance'

export type ToggleValue = 'inherit' | 'true' | 'false'

/**
 * How a value list (tags, custom fields) is restricted. `inherit` exists only
 * in project scope; `any` stores the wildcard `*`; `none` stores an explicit
 * empty list, which a blank value cannot express in project scope (blank
 * clears the override).
 */
export type ListMode = 'inherit' | 'any' | 'list' | 'none'

/** Where agent instructions come from; `inherit` is project scope only. */
export type AgentInstructionsMode = 'inherit' | 'builtin' | 'inline' | 'file'

export interface BranchAliasEntry {
    key: string
    value: string
}

/** One tag colour row on the Config page (DEV-20). */
export interface TagColorEntry {
    tag: string
    color: string
}

export interface ConfigFormState {
    serverPort: string
    defaultProject: string
    attachmentsDir: string
    attachmentsMaxUploadMb: string
    projectName: string
    defaultReporter: string
    defaultAssignee: string
    defaultTags: string[]
    defaultPriority: string
    defaultStatus: string
    issueStates: string[]
    issueTypes: string[]
    issuePriorities: string[]
    /**
     * Explicit done states for the edited scope. null = unset: automatic
     * legacy inference (global) / inherit the global setting (project). An
     * explicit empty array is invalid (one-or-more rule).
     */
    issueDoneStates: string[] | null
    tagsMode: ListMode
    tags: string[]
    customFieldsMode: ListMode
    customFields: string[]
    members: string[]
    strictMembers: ToggleValue
    autoPopulateMembers: ToggleValue
    /** Project-only named reviewers. */
    reviewReviewers: string[]
    autoSetReporter: ToggleValue
    autoAssignOnStatus: ToggleValue
    autoCodeownersAssign: ToggleValue
    autoTagsFromPath: ToggleValue
    autoBranchInferType: ToggleValue
    autoBranchInferStatus: ToggleValue
    autoBranchInferPriority: ToggleValue
    autoIdentity: ToggleValue
    autoIdentityGit: ToggleValue
    scanSignalWords: string[]
    scanTicketPatterns: string[]
    scanEnableTicketWords: ToggleValue
    scanEnableMentions: ToggleValue
    scanStripAttributes: ToggleValue
    branchTypeAliases: BranchAliasEntry[]
    branchStatusAliases: BranchAliasEntry[]
    branchPriorityAliases: BranchAliasEntry[]
    /** Tag colours of this scope (global, or the project's own overrides). */
    tagColors: TagColorEntry[]
    syncReportsDir: string
    syncWriteReports: ToggleValue
    agentContextEnabled: ToggleValue
    agentContextExtension: string
    agentLogsDir: string
    agentInstructionsMode: AgentInstructionsMode
    agentInstructions: string
    agentInstructionsFile: string
    agentWorktreeEnabled: ToggleValue
    agentWorktreeDir: string
    agentWorktreeBranchPrefix: string
    /** Global-only sprint defaults. */
    sprintLength: string
    sprintOverdueAfter: string
    sprintCapacityPoints: string
    sprintCapacityHours: string
    sprintNotificationsEnabled: ToggleValue
    /** Global-only external web bundle path. */
    webUiPath: string
    /** DEV-218: repository branding (global only). */
    uiTheme: string
    uiName: string
    uiLogo: string
    uiLogoDark: string
    uiIcon: string
    uiThemeFile: string
}

export type ToggleField =
    | 'autoSetReporter'
    | 'autoAssignOnStatus'
    | 'autoCodeownersAssign'
    | 'autoTagsFromPath'
    | 'autoBranchInferType'
    | 'autoBranchInferStatus'
    | 'autoBranchInferPriority'
    | 'autoIdentity'
    | 'autoIdentityGit'
    | 'scanEnableTicketWords'
    | 'scanEnableMentions'
    | 'scanStripAttributes'
    | 'strictMembers'
    | 'autoPopulateMembers'
    | 'syncWriteReports'
    | 'agentContextEnabled'
    | 'agentWorktreeEnabled'
    | 'sprintNotificationsEnabled'

type AliasField = 'branchTypeAliases' | 'branchStatusAliases' | 'branchPriorityAliases'

type AliasFieldKeyMap = Record<AliasField, 'branch_type_aliases' | 'branch_status_aliases' | 'branch_priority_aliases'>

type UseConfigFormOptions = {
    project: Ref<string>
    projects: Ref<ProjectDTO[]>
    inspectData: Ref<ConfigInspectResult | null>
    saving?: Ref<boolean>
}

type ToggleOption = {
    value: ToggleValue
    label: string
}

type UseConfigFormReturn = {
    form: ConfigFormState
    baseline: Ref<ConfigFormState | null>
    errors: Record<string, string | null>
    isGlobal: ComputedRef<boolean>
    currentProject: ComputedRef<ProjectDTO | null>
    tagWildcard: ComputedRef<boolean>
    customFieldWildcard: ComputedRef<boolean>
    projectExists: ComputedRef<boolean>
    tagSuggestions: ComputedRef<string[]>
    statusOptions: ComputedRef<string[]>
    priorityOptions: ComputedRef<string[]>
    typeOptions: ComputedRef<string[]>
    statusSuggestions: ComputedRef<string[]>
    prioritySuggestions: ComputedRef<string[]>
    typeSuggestions: ComputedRef<string[]>
    effectiveDoneLabels: ComputedRef<string[]>
    doneStatesMode: ComputedRef<'explicit' | 'inferred' | null>
    doneStatesAutomaticLabel: ComputedRef<string>
    doneStatesEffectiveSummary: ComputedRef<string>
    peopleDescription: ComputedRef<string>
    workflowDescription: ComputedRef<string>
    taxonomyDescription: ComputedRef<string>
    projectOverviewDescription: ComputedRef<string>
    automationDescription: ComputedRef<string>
    scanningDescription: ComputedRef<string>
    branchAliasDescription: ComputedRef<string>
    isDirty: ComputedRef<boolean>
    hasErrors: ComputedRef<boolean>
    saveDisabled: ComputedRef<boolean>
    toggleSelectOptions: (field: ToggleField) => ToggleOption[]
    globalToggleSummary: (field: ToggleField) => string
    provenanceLabel: (source: ConfigSource | undefined) => string
    provenanceClass: (source: ConfigSource | undefined) => string
    sourceFor: (field: string) => ConfigSource | undefined
    addAliasEntry: (field: AliasField) => void
    removeAliasEntry: (field: AliasField, index: number) => void
    addTagColor: (tag?: string) => void
    removeTagColor: (index: number) => void
    clearAliasField: (field: AliasField) => void
    validateField: (field: string) => void
    validateAll: () => boolean
    snapshotForm: () => ConfigFormState
    applySnapshot: (snapshot: ConfigFormState) => void
    clearErrors: () => void
    populateForm: (data: ConfigInspectResult) => void
    resetForm: () => void
    buildPayload: () => Record<string, string>
}

export function useConfigForm({ project, projects, inspectData, saving }: UseConfigFormOptions): UseConfigFormReturn {
    const savingRef = saving ?? ref(false)

    const form = reactive<ConfigFormState>({
        serverPort: '',
        defaultProject: '',
        attachmentsDir: '',
        attachmentsMaxUploadMb: '',
        projectName: '',
        defaultReporter: '',
        defaultAssignee: '',
        defaultTags: [],
        defaultPriority: '',
        defaultStatus: '',
        issueStates: [],
        issueTypes: [],
        issuePriorities: [],
        issueDoneStates: null,
        tagsMode: 'any',
        tags: [],
        customFieldsMode: 'any',
        customFields: [],
        members: [],
        strictMembers: 'false',
        autoPopulateMembers: 'true',
        reviewReviewers: [],
        autoSetReporter: 'inherit',
        autoAssignOnStatus: 'inherit',
        autoCodeownersAssign: 'true',
        autoTagsFromPath: 'true',
        autoBranchInferType: 'true',
        autoBranchInferStatus: 'true',
        autoBranchInferPriority: 'true',
        autoIdentity: 'true',
        autoIdentityGit: 'true',
        scanSignalWords: [],
        scanTicketPatterns: [],
        scanEnableTicketWords: 'inherit',
        scanEnableMentions: 'inherit',
        scanStripAttributes: 'inherit',
        branchTypeAliases: [],
        branchStatusAliases: [],
        branchPriorityAliases: [],
        tagColors: [],
        syncReportsDir: '',
        syncWriteReports: 'true',
        agentContextEnabled: 'true',
        agentContextExtension: '',
        agentLogsDir: '',
        agentInstructionsMode: 'builtin',
        agentInstructions: '',
        agentInstructionsFile: '',
        agentWorktreeEnabled: 'false',
        agentWorktreeDir: '',
        agentWorktreeBranchPrefix: '',
        sprintLength: '',
        sprintOverdueAfter: '',
        sprintCapacityPoints: '',
        sprintCapacityHours: '',
        sprintNotificationsEnabled: 'true',
        webUiPath: '',
        uiTheme: '',
        uiName: '',
        uiLogo: '',
        uiLogoDark: '',
        uiIcon: '',
        uiThemeFile: '',
    })

    const baseline = ref<ConfigFormState | null>(null)
    const errors = reactive<Record<string, string | null>>({})

    /** A branding image must be an image path inside the repository. */
    function brandingPathError(raw: string): string | null {
        const value = raw.trim()
        if (!value) return null
        if (value.startsWith('/') || value.startsWith('~') || /^[a-z]:[\\/]/i.test(value)) {
            return 'Use a path relative to the repository, e.g. assets/logo.svg.'
        }
        if (value.split(/[\\/]/).includes('..')) return 'The image must be inside the repository.'
        if (!/\.(svg|png|jpe?g|webp|gif|ico|avif)$/i.test(value)) {
            return 'Use an image file (SVG, PNG, JPEG, WebP, GIF, ICO or AVIF).'
        }
        return null
    }

    const isGlobal = computed(() => project.value === '')
    const currentProject = computed(() => projects.value.find((p) => p.prefix === project.value) || null)

    const tagWildcard = computed(() => inspectData.value?.effective.tags.includes('*') ?? false)
    const customFieldWildcard = computed(() => inspectData.value?.effective.custom_fields.includes('*') ?? false)
    const projectExists = computed(() => inspectData.value?.project_exists ?? false)

    const tagSuggestions = computed(() =>
        Array.from(
            new Set([
                ...(inspectData.value?.effective.tags ?? []),
                ...(inspectData.value?.effective.default_tags ?? []),
            ]),
        ).filter((value) => value && value !== '*'),
    )

    function dedupeCaseInsensitive(values: string[]): string[] {
        const seen = new Set<string>()
        const result: string[] = []
        for (const raw of values) {
            if (typeof raw !== 'string') continue
            const trimmed = raw.trim()
            if (!trimmed || trimmed === '*') continue
            const key = trimmed.toLowerCase()
            if (seen.has(key)) continue
            seen.add(key)
            result.push(trimmed)
        }
        return result
    }

    function findDuplicateValue(values: string[]): string | null {
        const seen = new Set<string>()
        for (const raw of values) {
            if (typeof raw !== 'string') continue
            const trimmed = raw.trim()
            if (!trimmed) continue
            const key = trimmed.toLowerCase()
            if (seen.has(key)) {
                return trimmed
            }
            seen.add(key)
        }
        return null
    }

    function toToggle(value: boolean): ToggleValue {
        return value ? 'true' : 'false'
    }

    function toTriToggle(value: boolean | undefined | null): ToggleValue {
        if (value === undefined || value === null) return 'inherit'
        return value ? 'true' : 'false'
    }

    function mapToAliasEntries(map: Record<string, string> | null | undefined): BranchAliasEntry[] {
        if (!map) return []
        return Object.entries(map)
            .map(([key, value]) => ({ key, value }))
            .sort((a, b) => a.key.localeCompare(b.key))
    }

    function normalizedAlias(entries: BranchAliasEntry[]): Record<string, string> {
        const normalized: Record<string, string> = {}
        for (const entry of entries) {
            const key = entry.key?.trim()
            const value = entry.value?.trim()
            if (!key || !value) continue
            normalized[key.toLowerCase()] = value
        }
        return normalized
    }

    /**
     * Tag properties as loaded for this scope. Saving rebuilds the map from
     * the colour rows and keeps every other property of a tag (DEV-20).
     */
    let loadedTagProperties: Record<string, TagProperties> = {}

    function tagColorEntries(map: Record<string, TagProperties> | null | undefined): TagColorEntry[] {
        if (!map) return []
        return Object.entries(map)
            .filter(([, props]) => typeof props?.color === 'string' && props.color.trim())
            .map(([tag, props]) => ({ tag, color: String(props.color).trim() }))
            .sort((a, b) => a.tag.localeCompare(b.tag))
    }

    /** The full `tag_properties` map for the colour rows, sorted by tag. */
    function tagPropertiesFromEntries(entries: TagColorEntry[]): Record<string, TagProperties> {
        const next: Record<string, TagProperties> = {}
        for (const [tag, props] of Object.entries(loadedTagProperties)) {
            const { color: _color, ...rest } = props ?? {}
            if (Object.keys(rest).length) next[tag] = { ...rest }
        }
        for (const entry of entries) {
            const tag = entry.tag.trim()
            const color = entry.color.trim()
            if (!tag || !color) continue
            next[tag] = { ...(next[tag] ?? {}), color }
        }
        return Object.fromEntries(Object.entries(next).sort(([a], [b]) => a.localeCompare(b)))
    }

    function validateTagColors(entries: TagColorEntry[]): string | null {
        const seen = new Set<string>()
        for (const entry of entries) {
            const tag = entry.tag.trim()
            const color = entry.color.trim()
            if (!tag && !color) continue
            if (!tag) return 'Name the tag for each colour.'
            if (tag === '*') return 'Pick a tag name, not *.'
            if (!resolveTagColor(color)) return `"${color}" is not a colour: use #rrggbb or a colour name.`
            const key = tag.toLowerCase()
            if (seen.has(key)) return `"${tag}" has more than one colour.`
            seen.add(key)
        }
        return null
    }

    function addTagColor(tag = '') {
        form.tagColors.push({ tag, color: '#0ea5e9' })
    }

    function removeTagColor(index: number) {
        form.tagColors.splice(index, 1)
        validateField('tag_properties')
    }

    function aliasEntriesEqual(a: BranchAliasEntry[], b: BranchAliasEntry[]): boolean {
        const left = normalizedAlias(a)
        const right = normalizedAlias(b)
        const leftKeys = Object.keys(left).sort()
        const rightKeys = Object.keys(right).sort()
        if (leftKeys.length !== rightKeys.length) return false
        for (let i = 0; i < leftKeys.length; i += 1) {
            const lk = leftKeys[i]
            const rk = rightKeys[i]
            if (lk === undefined || rk === undefined || lk !== rk) return false
            if ((left[lk] ?? '') !== (right[rk] ?? '')) {
                return false
            }
        }
        return true
    }

    function validateAliasEntries(entries: BranchAliasEntry[], label: string): string | null {
        const seen = new Set<string>()
        for (const entry of entries) {
            const key = entry.key?.trim() ?? ''
            const value = entry.value?.trim() ?? ''
            if (!key && !value) {
                continue
            }
            if (!key) {
                return `Add an alias token for the ${label}.`
            }
            if (!value) {
                return `Provide a target value for alias "${key}".`
            }
            if (key.length > 50) {
                return `Alias token "${key}" is too long (max 50 characters).`
            }
            if (value.length > 100) {
                return `Alias value "${value}" is too long (max 100 characters).`
            }
            const lowered = key.toLowerCase()
            if (seen.has(lowered)) {
                return `Alias token "${key}" is duplicated.`
            }
            seen.add(lowered)
        }
        return null
    }

    const globalWorkflow = computed(() => inspectData.value?.global_effective)

    const statusOptions = computed(() => {
        const own = form.issueStates
        if (isGlobal.value || own.length > 0) {
            return dedupeCaseInsensitive(own)
        }
        return dedupeCaseInsensitive(globalWorkflow.value?.issue_states ?? [])
    })

    const priorityOptions = computed(() => {
        const own = form.issuePriorities
        if (isGlobal.value || own.length > 0) {
            return dedupeCaseInsensitive(own)
        }
        return dedupeCaseInsensitive(globalWorkflow.value?.issue_priorities ?? [])
    })

    const typeOptions = computed(() => {
        const own = form.issueTypes
        if (isGlobal.value || own.length > 0) {
            return dedupeCaseInsensitive(own)
        }
        return dedupeCaseInsensitive(globalWorkflow.value?.issue_types ?? [])
    })

    const statusSuggestions = computed(() => statusOptions.value)
    const prioritySuggestions = computed(() => priorityOptions.value)
    const typeSuggestions = computed(() => typeOptions.value)

    // ---- Done states (DEV-21) ----------------------------------------------
    // Effective values come from the server-resolved config; the form edits
    // the raw override only.
    const effectiveDoneLabels = computed(() => {
        const labels = inspectData.value?.effective?.effective_done_states
        return Array.isArray(labels)
            ? labels.map((value) => String(value)).filter((value) => value.trim().length > 0)
            : []
    })

    const doneStatesMode = computed(() => {
        const mode = inspectData.value?.effective?.done_states_mode
        return mode === 'explicit' || mode === 'inferred' ? mode : null
    })

    const doneStatesAutomaticLabel = computed(() => {
        const effective = effectiveDoneLabels.value
        const summary = effective.length ? effective.join(', ') : 'inferred automatically'
        if (isGlobal.value) {
            return `Automatic (currently ${summary})`
        }
        return `Inherit global (Currently ${summary})`
    })

    const doneStatesEffectiveSummary = computed(() => {
        const effective = effectiveDoneLabels.value
        if (!effective.length) return ''
        const mode = doneStatesMode.value
        const modeLabel = mode === 'explicit' ? 'explicit' : mode === 'inferred' ? 'inferred' : ''
        return `Effective now: ${effective.join(', ')}${modeLabel ? ` (${modeLabel})` : ''}`
    })

    watch(
        [isGlobal, statusOptions],
        ([global, options]) => {
            if (!global) return
            if (!options.length) return
            // Blank is valid (creation uses the first status); only replace a
            // value that is no longer a configured status.
            if (form.defaultStatus && !options.includes(form.defaultStatus)) {
                form.defaultStatus = options[0] ?? ''
            }
        },
        { immediate: true },
    )

    const peopleDescription = computed(() =>
        isGlobal.value
            ? 'Used when tasks omit reporter/assignee information. @me resolves to the current identity.'
            : 'Overrides apply only to this project. Leave blank to inherit global fallbacks.',
    )

    const workflowDescription = computed(() =>
        isGlobal.value
            ? 'Statuses map to board columns, while types and priorities power creation menus and validation.'
            : 'Override only the pieces you need. Empty lists fall back to the global workflow.',
    )

    const taxonomyDescription = computed(() =>
        isGlobal.value
            ? 'Choose which tags and custom fields tasks may use: any, only a list, or none.'
            : 'Inherit the global tags and custom fields, or allow any, only a list, or none for this project.',
    )

    const projectOverviewDescription = computed(() =>
        projectExists.value
            ? 'The display name shown for this project.'
            : 'No project config file yet. Enter values to create one; otherwise the project inherits everything.',
    )

    const automationDescription = computed(() =>
        isGlobal.value
            ? 'Toggle automatic behaviors for all projects. Changes take effect immediately for new commands.'
            : 'Override automation only when this project needs different defaults. Choose inherit to reuse the global setting.',
    )

    const scanningDescription = computed(() =>
        isGlobal.value
            ? 'Tune repository scanning: control signal words, ticket detection, and attribute scrubbing.'
            : 'Project overrides apply when scanning this project’s files. Leave values empty or set to inherit to use global behavior.',
    )

    const branchAliasDescription = computed(() =>
        isGlobal.value
            ? 'Map branch name tokens to workflow metadata so new tasks pick sensible defaults.'
            : 'Project-specific aliases replace global mappings. Clear the list to fall back to global behavior.',
    )

    function sourceFor(field: string): ConfigSource | undefined {
        return inspectData.value?.sources?.[field]
    }

    function cloneState(state: ConfigFormState): ConfigFormState {
        const copy = { ...state } as Record<string, unknown>
        for (const [key, value] of Object.entries(copy)) {
            if (Array.isArray(value)) {
                copy[key] = value.map((entry) => (entry && typeof entry === 'object' ? { ...entry } : entry))
            }
        }
        return copy as unknown as ConfigFormState
    }

    function snapshotForm(): ConfigFormState {
        return cloneState(form)
    }

    function applySnapshot(snapshot: ConfigFormState) {
        Object.assign(form, cloneState(snapshot))
    }

    function deepEqual(a: ConfigFormState, b: ConfigFormState): boolean {
        return JSON.stringify(a) === JSON.stringify(b)
    }

    const isDirty = computed(() => {
        if (!baseline.value) return false
        return !deepEqual(snapshotForm(), baseline.value)
    })

    const hasErrors = computed(() => Object.values(errors).some(Boolean))
    const saveDisabled = computed(() => savingRef.value || !isDirty.value || hasErrors.value)

    function clearErrors() {
        for (const key of Object.keys(errors)) {
            errors[key] = null
        }
    }

    function normalizeCsv(values: string[]): string {
        return values
            .map((v) => v.trim())
            .filter((v) => v.length > 0)
            .join(',')
    }

    function arraysEqual(a: string[], b: string[]): boolean {
        if (a.length !== b.length) return false
        return normalizeCsv(a) === normalizeCsv(b)
    }

    const aliasFieldKey: AliasFieldKeyMap = {
        branchTypeAliases: 'branch_type_aliases',
        branchStatusAliases: 'branch_status_aliases',
        branchPriorityAliases: 'branch_priority_aliases',
    }

    function addAliasEntry(field: AliasField) {
        form[field].push({ key: '', value: '' })
    }

    function removeAliasEntry(field: AliasField, index: number) {
        form[field].splice(index, 1)
        validateField(aliasFieldKey[field])
    }

    function clearAliasField(field: AliasField) {
        form[field] = []
        validateField(aliasFieldKey[field])
    }

    function globalToggleValue(field: ToggleField): boolean | null {
        const global = inspectData.value?.global_effective
        if (!global) return null
        switch (field) {
            case 'autoSetReporter':
                return global.auto_set_reporter
            case 'autoAssignOnStatus':
                return global.auto_assign_on_status
            case 'autoCodeownersAssign':
                return global.auto_codeowners_assign
            case 'autoTagsFromPath':
                return global.auto_tags_from_path
            case 'autoBranchInferType':
                return global.auto_branch_infer_type
            case 'autoBranchInferStatus':
                return global.auto_branch_infer_status
            case 'autoBranchInferPriority':
                return global.auto_branch_infer_priority
            case 'autoIdentity':
                return global.auto_identity
            case 'autoIdentityGit':
                return global.auto_identity_git
            case 'scanEnableTicketWords':
                return global.scan_enable_ticket_words
            case 'scanEnableMentions':
                return global.scan_enable_mentions
            case 'scanStripAttributes':
                return global.scan_strip_attributes
            case 'strictMembers':
                return global.strict_members ?? null
            case 'autoPopulateMembers':
                return global.auto_populate_members ?? null
            case 'syncWriteReports':
                return global.sync_write_reports
            case 'agentContextEnabled':
                return global.agent_context_enabled ?? null
            case 'agentWorktreeEnabled':
                return global.agent_worktree?.enabled ?? null
            case 'sprintNotificationsEnabled':
                return global.sprint_notifications?.enabled ?? null
            default:
                return null
        }
    }

    function inheritToggleLabel(field: ToggleField, base: string): string {
        const value = globalToggleValue(field)
        if (value === null) return base
        return `${base} (Currently ${value ? 'On' : 'Off'})`
    }

    function toggleSelectOptions(field: ToggleField): ToggleOption[] {
        return [
            { value: 'inherit', label: inheritToggleLabel(field, 'Inherit global') },
            { value: 'true', label: 'Enabled' },
            { value: 'false', label: 'Disabled' },
        ]
    }

    function globalToggleSummary(field: ToggleField): string {
        const value = globalToggleValue(field)
        if (value === null) return ''
        return value ? 'On' : 'Off'
    }

    function validateRestrictedList(mode: ListMode, values: string[], label: string): string | null {
        if (mode !== 'list') return null
        const trimmed = values.map((value) => value.trim()).filter(Boolean)
        if (!trimmed.length) {
            return `Add at least one ${label}, or choose another option.`
        }
        const invalid = trimmed.find((value) => value.length > 50)
        return invalid ? `"${invalid}" is too long (max 50 characters).` : null
    }

    // Mirrors the server grammar (DEV-111): lengths take days/weeks and must be
    // positive; overdue grace also takes hours and may be zero. Max ten years.
    function validateSprintDuration(raw: string, isLength: boolean): string | null {
        const value = raw.trim().toLowerCase()
        if (!value) return null
        const units = isLength ? '(d|day|days|w|week|weeks)' : '(h|hour|hours|d|day|days|w|week|weeks)'
        const match = value.match(new RegExp(`^(\\d+)\\s*${units}$`))
        if (!match) {
            return isLength
                ? 'Use a whole number of days or weeks, e.g. 14d or 2w.'
                : 'Use a whole number of hours, days or weeks, e.g. 12h, 1d or 2w.'
        }
        const count = Number(match[1])
        const unit = match[2]![0]
        const hours = count * (unit === 'h' ? 1 : unit === 'd' ? 24 : 168)
        if (isLength && hours === 0) return 'The length must be longer than zero.'
        if (hours > 3650 * 24) return 'Keep it to ten years (3650 days) or less.'
        return null
    }

    function validateField(field: string) {
        switch (field) {
            case 'server_port': {
                if (!isGlobal.value) {
                    errors.server_port = null
                    return
                }
                const value = form.serverPort.trim()
                if (!value) {
                    errors.server_port = 'Server port is required.'
                    return
                }
                const port = Number(value)
                if (!Number.isInteger(port) || port < 1024 || port > 65535) {
                    errors.server_port = 'Enter a port between 1024 and 65535.'
                } else {
                    errors.server_port = null
                }
                break
            }
            // DEV-218: same rules as the server (src/services/branding_service.rs).
            case 'ui_theme': {
                const value = form.uiTheme.trim()
                errors.ui_theme = value && !parseThemeValue(value, true) ? 'Pick one of the listed themes.' : null
                break
            }
            case 'ui_theme_file': {
                const value = form.uiThemeFile.trim()
                const pathError = brandingPathError(value.replace(/\.css$/i, '.svg'))
                errors.ui_theme_file = value && !/\.css$/i.test(value) ? 'Use a .css file.' : pathError
                break
            }
            case 'ui_name': {
                errors.ui_name = form.uiName.trim().length > 40 ? 'Keep the name to 40 characters or fewer.' : null
                break
            }
            case 'ui_logo':
            case 'ui_logo_dark':
            case 'ui_icon': {
                const formKey = field === 'ui_logo' ? 'uiLogo' : field === 'ui_logo_dark' ? 'uiLogoDark' : 'uiIcon'
                errors[field] = brandingPathError(form[formKey])
                break
            }
            case 'attachments_dir': {
                const value = form.attachmentsDir.trim()
                if (!value) {
                    errors.attachments_dir = isGlobal.value ? 'Attachments folder is required.' : null
                    return
                }
                if (value.length > 512) {
                    errors.attachments_dir = 'Keep the attachments folder under 512 characters.'
                    return
                }
                errors.attachments_dir = null
                break
            }
            case 'attachments_max_upload_mb': {
                const raw = form.attachmentsMaxUploadMb.trim()
                if (!raw) {
                    errors.attachments_max_upload_mb = isGlobal.value ? 'Max upload size is required.' : null
                    return
                }
                const value = Number(raw)
                if (!Number.isInteger(value)) {
                    errors.attachments_max_upload_mb = 'Enter an integer (MiB).'
                    return
                }
                if (value < -1) {
                    errors.attachments_max_upload_mb = 'Use -1 (unlimited), 0 (disabled), or a positive integer.'
                    return
                }
                errors.attachments_max_upload_mb = null
                break
            }
            case 'default_project': {
                const value = form.defaultProject.trim()
                if (!value) {
                    errors.default_project = null
                    return
                }
                if (!/^[A-Za-z0-9_-]+$/.test(value)) {
                    errors.default_project = 'Only letters, numbers, hyphen, or underscore allowed.'
                } else if (value.length > 20) {
                    errors.default_project = 'Keep prefixes under 20 characters.'
                } else {
                    errors.default_project = null
                }
                break
            }
            case 'project_name': {
                if (isGlobal.value) {
                    errors.project_name = null
                    return
                }
                const value = form.projectName.trim()
                if (!value) {
                    errors.project_name = 'Project name cannot be empty.'
                } else if (value.length > 100) {
                    errors.project_name = 'Project name must be under 100 characters.'
                } else {
                    errors.project_name = null
                }
                break
            }
            case 'default_reporter':
            case 'default_assignee': {
                const value = field === 'default_reporter' ? form.defaultReporter.trim() : form.defaultAssignee.trim()
                errors[field] = value.length > 100 ? 'Keep names under 100 characters.' : null
                break
            }
            case 'default_tags': {
                const values = form.defaultTags
                const invalid = values.find((value) => value.trim().length > 50)
                errors.default_tags = invalid ? `"${invalid}" is too long (max 50 characters).` : null
                break
            }
            case 'tags': {
                errors.tags = validateRestrictedList(form.tagsMode, form.tags, 'tag')
                break
            }
            case 'custom_fields': {
                errors.custom_fields = validateRestrictedList(form.customFieldsMode, form.customFields, 'custom field')
                break
            }
            case 'members':
            case 'review_reviewers': {
                const values = field === 'members' ? form.members : form.reviewReviewers
                const invalid = values.find((value) => value.trim().length > 100)
                errors[field] = invalid ? `"${invalid}" is too long (max 100 characters).` : null
                break
            }
            case 'sync_reports_dir':
            case 'agent_context_extension':
            case 'agent_worktree_branch_prefix': {
                const labels: Record<string, [string, string]> = {
                    sync_reports_dir: ['syncReportsDir', 'Reports folder'],
                    agent_context_extension: ['agentContextExtension', 'Context file extension'],
                    agent_worktree_branch_prefix: ['agentWorktreeBranchPrefix', 'Branch prefix'],
                }
                const [key, label] = labels[field]!
                const value = String(form[key as keyof ConfigFormState] ?? '').trim()
                const loaded = String(baseline.value?.[key as keyof ConfigFormState] ?? '').trim()
                if (!value) {
                    // Global values cannot be blank; only flag a value the user cleared.
                    errors[field] = isGlobal.value && loaded ? `${label} is required.` : null
                } else {
                    errors[field] = value.length > 512 ? `Keep the ${label.toLowerCase()} under 512 characters.` : null
                }
                break
            }
            case 'agent_instructions': {
                const mode = form.agentInstructionsMode
                if (mode === 'inline' && !form.agentInstructions.trim()) {
                    errors.agent_instructions = 'Enter the instructions text, or choose another source.'
                } else if (mode === 'file' && !form.agentInstructionsFile.trim()) {
                    errors.agent_instructions = 'Enter the instructions file path, or choose another source.'
                } else {
                    errors.agent_instructions = null
                }
                break
            }
            case 'sprints_defaults_length':
            case 'sprints_defaults_overdue_after': {
                const isLength = field === 'sprints_defaults_length'
                errors[field] = validateSprintDuration(isLength ? form.sprintLength : form.sprintOverdueAfter, isLength)
                break
            }
            case 'sprints_defaults_capacity_points':
            case 'sprints_defaults_capacity_hours': {
                const raw = (
                    field === 'sprints_defaults_capacity_points' ? form.sprintCapacityPoints : form.sprintCapacityHours
                ).trim()
                errors[field] =
                    raw && !/^\d+$/.test(raw) ? 'Enter a whole number of 0 or more, or leave it empty.' : null
                break
            }
            case 'scan_signal_words': {
                const values = form.scanSignalWords.map((value) => value.trim()).filter(Boolean)
                const invalid = values.find((value) => value.length > 50)
                if (invalid) {
                    errors.scan_signal_words = `"${invalid}" is too long (max 50 characters).`
                    break
                }
                const duplicate = findDuplicateValue(values)
                errors.scan_signal_words = duplicate ? `"${duplicate}" appears more than once.` : null
                break
            }
            case 'scan_ticket_patterns': {
                const values = form.scanTicketPatterns.map((value) => value.trim()).filter(Boolean)
                const invalid = values.find((value) => value.length > 200)
                if (invalid) {
                    errors.scan_ticket_patterns = `"${invalid}" is too long (max 200 characters).`
                    break
                }
                const duplicate = findDuplicateValue(values)
                errors.scan_ticket_patterns = duplicate ? `"${duplicate}" appears more than once.` : null
                break
            }
            case 'default_priority': {
                const value = form.defaultPriority.trim()
                if (!value) {
                    errors.default_priority = null
                    return
                }
                const options = priorityOptions.value
                if (!options.length) {
                    errors.default_priority = 'Define priorities before choosing a default.'
                    return
                }
                const ok = options.some((opt) => opt.toLowerCase() === value.toLowerCase())
                errors.default_priority = ok ? null : 'Select a priority from the configured list.'
                break
            }
            case 'default_status': {
                const value = form.defaultStatus.trim()
                if (!value) {
                    errors.default_status = null
                    return
                }
                const options = statusOptions.value
                if (!options.length) {
                    errors.default_status = 'Define statuses before choosing a default.'
                    return
                }
                const ok = options.some((opt) => opt.toLowerCase() === value.toLowerCase())
                errors.default_status = ok ? null : 'Select a status from the configured workflow.'
                break
            }
            case 'issue_states': {
                const values = form.issueStates.map((val) => val.trim()).filter((val) => val.length > 0)
                if (isGlobal.value && values.length === 0) {
                    errors.issue_states = 'At least one status is required.'
                    return
                }
                if (!isGlobal.value && values.length === 0) {
                    errors.issue_states = null
                    validateField('default_status')
                    return
                }
                const duplicate = findDuplicateValue(values)
                if (duplicate) {
                    errors.issue_states = `"${duplicate}" appears more than once.`
                    return
                }
                const tooLong = values.find((val) => val.length > 50)
                if (tooLong) {
                    errors.issue_states = `"${tooLong}" is too long (max 50 characters).`
                    return
                }
                errors.issue_states = null
                validateField('default_status')
                break
            }
            case 'issue_types': {
                const values = form.issueTypes.map((val) => val.trim()).filter((val) => val.length > 0)
                if (!isGlobal.value && values.length === 0) {
                    errors.issue_types = null
                    return
                }
                if (values.length === 0) {
                    errors.issue_types = 'Provide at least one issue type.'
                    return
                }
                const duplicate = findDuplicateValue(values)
                if (duplicate) {
                    errors.issue_types = `"${duplicate}" appears more than once.`
                    return
                }
                const tooLong = values.find((val) => val.length > 50)
                if (tooLong) {
                    errors.issue_types = `"${tooLong}" is too long (max 50 characters).`
                    return
                }
                errors.issue_types = null
                break
            }
            case 'issue_priorities': {
                const values = form.issuePriorities.map((val) => val.trim()).filter((val) => val.length > 0)
                if (!isGlobal.value && values.length === 0) {
                    errors.issue_priorities = null
                    validateField('default_priority')
                    return
                }
                if (values.length === 0) {
                    errors.issue_priorities = 'Provide at least one priority.'
                    return
                }
                const duplicate = findDuplicateValue(values)
                if (duplicate) {
                    errors.issue_priorities = `"${duplicate}" appears more than once.`
                    return
                }
                const tooLong = values.find((val) => val.length > 50)
                if (tooLong) {
                    errors.issue_priorities = `"${tooLong}" is too long (max 50 characters).`
                    return
                }
                errors.issue_priorities = null
                validateField('default_priority')
                break
            }
            case 'issue_done_states': {
                const values = form.issueDoneStates
                if (values === null || values === undefined) {
                    errors.issue_done_states = null
                    return
                }
                const trimmed = values.map((val) => val.trim()).filter((val) => val.length > 0)
                if (!trimmed.length) {
                    errors.issue_done_states = 'Choose at least one done state, or switch back to automatic/inherit.'
                    return
                }
                const duplicate = findDuplicateValue(trimmed)
                if (duplicate) {
                    errors.issue_done_states = `"${duplicate}" appears more than once.`
                    return
                }
                const options = statusOptions.value
                if (!options.length) {
                    errors.issue_done_states = 'Define statuses before choosing done states.'
                    return
                }
                const invalid = trimmed.find((val) => !options.some((opt) => opt.toLowerCase() === val.toLowerCase()))
                if (invalid) {
                    errors.issue_done_states = `"${invalid}" is not a configured status.`
                    return
                }
                errors.issue_done_states = null
                break
            }
            case 'branch_type_aliases': {
                errors.branch_type_aliases = validateAliasEntries(form.branchTypeAliases, 'branch type')
                break
            }
            case 'tag_properties': {
                errors.tag_properties = validateTagColors(form.tagColors)
                break
            }
            case 'branch_status_aliases': {
                errors.branch_status_aliases = validateAliasEntries(form.branchStatusAliases, 'branch status')
                break
            }
            case 'branch_priority_aliases': {
                errors.branch_priority_aliases = validateAliasEntries(form.branchPriorityAliases, 'branch priority')
                break
            }
            default:
                break
        }
    }

    function validateAll(): boolean {
        const fields: string[] = [
            'ui_theme',
            'ui_name',
            'ui_logo',
            'ui_logo_dark',
            'ui_icon',
            'ui_theme_file',
            'attachments_dir',
            'attachments_max_upload_mb',
            'default_reporter',
            'default_assignee',
            'default_tags',
            'default_priority',
            'default_status',
            'issue_states',
            'issue_types',
            'issue_priorities',
            'issue_done_states',
            'tags',
            'custom_fields',
            'members',
            'review_reviewers',
            'sync_reports_dir',
            'agent_context_extension',
            'agent_worktree_branch_prefix',
            'agent_instructions',
            'scan_signal_words',
            'scan_ticket_patterns',
            'branch_type_aliases',
            'branch_status_aliases',
            'branch_priority_aliases',
            'tag_properties',
        ]
        if (isGlobal.value) {
            fields.push(
                'server_port',
                'default_project',
                'sprints_defaults_length',
                'sprints_defaults_overdue_after',
                'sprints_defaults_capacity_points',
                'sprints_defaults_capacity_hours',
            )
        } else {
            fields.push('project_name')
        }
        fields.forEach((field) => validateField(field))
        return !hasErrors.value
    }

    function normalizeDoneStatesRaw(value: unknown): string[] | null {
        if (!Array.isArray(value)) return null
        return value.map((entry) => String(entry)).filter((entry) => entry.trim().length > 0)
    }

    function applyRestrictedList(field: 'tags' | 'customFields', values: string[] | null | undefined) {
        const list = Array.isArray(values) ? values : []
        const modeKey = field === 'tags' ? 'tagsMode' : 'customFieldsMode'
        const named = list.filter((value) => value !== '*')
        form[modeKey] = list.includes('*') ? 'any' : named.length ? 'list' : 'none'
        form[field] = list.includes('*') ? [] : named
    }

    function applyAgentInstructions(value: AgentInstructionsValue, unsetMode: AgentInstructionsMode) {
        form.agentInstructions = ''
        form.agentInstructionsFile = ''
        if (typeof value === 'string') {
            form.agentInstructionsMode = 'inline'
            form.agentInstructions = value
        } else if (value && typeof value === 'object' && typeof value.file === 'string') {
            form.agentInstructionsMode = 'file'
            form.agentInstructionsFile = value.file
        } else {
            form.agentInstructionsMode = unsetMode
        }
    }

    function populateForm(data: ConfigInspectResult) {
        const effective = data.effective
        form.serverPort = effective.server_port?.toString() ?? ''
        const effectiveDefaultProject = effective.default_project ?? ''
        form.defaultProject = effectiveDefaultProject
        form.attachmentsDir = effective.attachments_dir ?? ''
        form.attachmentsMaxUploadMb = effective.attachments_max_upload_mb?.toString() ?? ''
        form.defaultReporter = effective.default_reporter ?? ''
        form.defaultAssignee = effective.default_assignee ?? ''
        form.defaultTags = [...effective.default_tags]
        form.defaultPriority = effective.default_priority ?? ''
        form.defaultStatus = effective.default_status ?? ''
        form.issueStates = [...effective.issue_states]
        form.issueTypes = [...effective.issue_types]
        form.issuePriorities = [...effective.issue_priorities]
        form.issueDoneStates = null
        applyRestrictedList('tags', effective.tags)
        applyRestrictedList('customFields', effective.custom_fields)
        form.members = [...(effective.members ?? [])]
        form.strictMembers = toToggle(!!effective.strict_members)
        form.autoPopulateMembers = toToggle(effective.auto_populate_members !== false)
        form.reviewReviewers = []
        form.syncReportsDir = effective.sync_reports_dir ?? ''
        form.syncWriteReports = toToggle(effective.sync_write_reports !== false)
        form.agentContextEnabled = toToggle(effective.agent_context_enabled !== false)
        form.agentContextExtension = effective.agent_context_extension ?? ''
        form.agentLogsDir = effective.agent_logs_dir ?? ''
        applyAgentInstructions(effective.agent_instructions, 'builtin')
        form.agentWorktreeEnabled = toToggle(!!effective.agent_worktree?.enabled)
        form.agentWorktreeDir = effective.agent_worktree?.dir ?? ''
        form.agentWorktreeBranchPrefix = effective.agent_worktree?.branch_prefix ?? ''
        form.sprintLength = effective.sprint_defaults?.length ?? ''
        form.sprintOverdueAfter = effective.sprint_defaults?.overdue_after ?? ''
        form.sprintCapacityPoints = effective.sprint_defaults?.capacity_points?.toString() ?? ''
        form.sprintCapacityHours = effective.sprint_defaults?.capacity_hours?.toString() ?? ''
        form.sprintNotificationsEnabled = toToggle(effective.sprint_notifications?.enabled !== false)
        form.webUiPath = data.global_raw?.web_ui_path ?? ''
        form.uiTheme = data.global_raw?.ui_theme ?? ''
        form.uiName = data.global_raw?.ui_name ?? ''
        form.uiLogo = data.global_raw?.ui_logo ?? ''
        form.uiLogoDark = data.global_raw?.ui_logo_dark ?? ''
        form.uiIcon = data.global_raw?.ui_icon ?? ''
        form.uiThemeFile = data.global_raw?.ui_theme_file ?? ''
        form.autoCodeownersAssign = toToggle(!!effective.auto_codeowners_assign)
        form.autoTagsFromPath = toToggle(!!effective.auto_tags_from_path)
        form.autoBranchInferType = toToggle(!!effective.auto_branch_infer_type)
        form.autoBranchInferStatus = toToggle(!!effective.auto_branch_infer_status)
        form.autoBranchInferPriority = toToggle(!!effective.auto_branch_infer_priority)
        form.autoIdentity = toToggle(!!effective.auto_identity)
        form.autoIdentityGit = toToggle(!!effective.auto_identity_git)
        form.scanSignalWords = [...(effective.scan_signal_words ?? [])]
        form.scanTicketPatterns = [...(effective.scan_ticket_patterns ?? [])]
        form.scanEnableTicketWords = toToggle(!!effective.scan_enable_ticket_words)
        form.scanEnableMentions = toToggle(!!effective.scan_enable_mentions)
        form.scanStripAttributes = toToggle(!!effective.scan_strip_attributes)
        form.branchTypeAliases = mapToAliasEntries(effective.branch_type_aliases)
        form.branchStatusAliases = mapToAliasEntries(effective.branch_status_aliases)
        form.branchPriorityAliases = mapToAliasEntries(effective.branch_priority_aliases)
        loadedTagProperties = { ...(effective.tag_properties ?? {}) }
        form.tagColors = tagColorEntries(loadedTagProperties)
        if (!isGlobal.value) {
            const raw = ((data.project_raw as any) || {}) as Record<string, any>
            form.projectName = raw.project_name || currentProject.value?.name || ''
            // Project defaults show overrides only; blank means inherit.
            form.defaultPriority = typeof raw.default_priority === 'string' ? raw.default_priority : ''
            form.defaultStatus = typeof raw.default_status === 'string' ? raw.default_status : ''
            // Explicit project override (kept even when equal to the global
            // list — intent is preserved, never erased to inherit).
            form.issueDoneStates = normalizeDoneStatesRaw(raw.issue_done_states)
            if (raw.attachments_dir !== undefined) {
                form.attachmentsDir = typeof raw.attachments_dir === 'string' ? raw.attachments_dir : ''
            }
            if (raw.attachments_max_upload_mb !== undefined) {
                form.attachmentsMaxUploadMb =
                    raw.attachments_max_upload_mb === null ? '' : String(raw.attachments_max_upload_mb)
            }
            form.autoSetReporter = toTriToggle(raw.auto_set_reporter)
            form.autoAssignOnStatus = toTriToggle(raw.auto_assign_on_status)
            form.scanEnableTicketWords = toTriToggle(raw.scan_enable_ticket_words)
            form.scanEnableMentions = toTriToggle(raw.scan_enable_mentions)
            form.scanStripAttributes = toTriToggle(raw.scan_strip_attributes)
            form.autoCodeownersAssign = toTriToggle(raw.auto_codeowners_assign)
            form.autoTagsFromPath = toTriToggle(raw.auto_tags_from_path)
            form.autoBranchInferType = toTriToggle(raw.auto_branch_infer_type)
            form.autoBranchInferStatus = toTriToggle(raw.auto_branch_infer_status)
            form.autoBranchInferPriority = toTriToggle(raw.auto_branch_infer_priority)
            form.autoIdentity = toTriToggle(raw.auto_identity)
            form.autoIdentityGit = toTriToggle(raw.auto_identity_git)
            // Project scope edits overrides only; absent keys inherit.
            if (Array.isArray(raw.tags)) {
                applyRestrictedList('tags', raw.tags)
            } else {
                form.tagsMode = 'inherit'
                form.tags = []
            }
            if (Array.isArray(raw.custom_fields)) {
                applyRestrictedList('customFields', raw.custom_fields)
            } else {
                form.customFieldsMode = 'inherit'
                form.customFields = []
            }
            form.members = Array.isArray(raw.members) ? [...raw.members] : []
            form.strictMembers = toTriToggle(raw.strict_members)
            form.autoPopulateMembers = toTriToggle(raw.auto_populate_members)
            form.reviewReviewers = Array.isArray(raw.review_reviewers) ? [...raw.review_reviewers] : []
            form.syncReportsDir = typeof raw.sync_reports_dir === 'string' ? raw.sync_reports_dir : ''
            form.syncWriteReports = toTriToggle(raw.sync_write_reports)
            form.agentContextEnabled = toTriToggle(raw.agent_context_enabled)
            form.agentContextExtension =
                typeof raw.agent_context_extension === 'string' ? raw.agent_context_extension : ''
            form.agentLogsDir = typeof raw.agent_logs_dir === 'string' ? raw.agent_logs_dir : ''
            applyAgentInstructions(raw.agent_instructions, 'inherit')
            form.agentWorktreeEnabled = toTriToggle(raw.agent_worktree?.enabled)
            form.agentWorktreeDir = raw.agent_worktree?.dir ?? ''
            form.agentWorktreeBranchPrefix = raw.agent_worktree?.branch_prefix ?? ''
            if (Array.isArray(raw.scan_signal_words)) {
                form.scanSignalWords = [...raw.scan_signal_words]
            }
            if (Array.isArray(raw.scan_ticket_patterns)) {
                form.scanTicketPatterns = [...raw.scan_ticket_patterns]
            }
            if (raw.branch_type_aliases) {
                form.branchTypeAliases = mapToAliasEntries(raw.branch_type_aliases)
            }
            if (raw.branch_status_aliases) {
                form.branchStatusAliases = mapToAliasEntries(raw.branch_status_aliases)
            }
            if (raw.branch_priority_aliases) {
                form.branchPriorityAliases = mapToAliasEntries(raw.branch_priority_aliases)
            }
            // A project edits its own tag colours; the rest are inherited.
            loadedTagProperties = { ...((raw.tag_properties as Record<string, TagProperties> | null) ?? {}) }
            form.tagColors = tagColorEntries(loadedTagProperties)
        } else {
            form.projectName = ''
            form.issueDoneStates = normalizeDoneStatesRaw((data.global_raw as any)?.issue_done_states)
            form.autoSetReporter = toToggle(!!effective.auto_set_reporter)
            form.autoAssignOnStatus = toToggle(!!effective.auto_assign_on_status)
            form.scanEnableTicketWords = toToggle(!!effective.scan_enable_ticket_words)
            form.scanEnableMentions = toToggle(!!effective.scan_enable_mentions)
            form.scanStripAttributes = toToggle(!!effective.scan_strip_attributes)
        }
    }

    function resetForm() {
        if (!baseline.value) return
        applySnapshot(baseline.value)
        clearErrors()
    }

    function buildPayload(): Record<string, string> {
        const base = baseline.value
        if (!base) return {}
        const current = snapshotForm()
        const payload: Record<string, string> = {}

        const addValue = (
            field: keyof ConfigFormState,
            key: string,
            { trim = true, allowEmpty = true }: { trim?: boolean; allowEmpty?: boolean } = {},
        ) => {
            const currentValue = (current[field] as unknown as string) ?? ''
            const baseValue = (base[field] as unknown as string) ?? ''
            const normalize = (val: string) => (trim ? val.trim() : val)
            if (normalize(currentValue) === normalize(baseValue)) return
            if (!allowEmpty && normalize(currentValue) === '') return
            payload[key] = normalize(currentValue)
        }

        const addArray = (field: keyof ConfigFormState, key: string) => {
            const currentArray = current[field] as unknown as string[]
            const baseArray = base[field] as unknown as string[]
            if (arraysEqual(currentArray, baseArray)) return
            payload[key] = normalizeCsv(currentArray)
        }

        const addToggleField = (
            field: ToggleField,
            key: string,
            { allowInherit = false }: { allowInherit?: boolean } = {},
        ) => {
            const currentValue = current[field] as ToggleValue
            const baseValue = base[field] as ToggleValue
            if (currentValue === baseValue) return
            if (currentValue === 'inherit') {
                if (!isGlobal.value && allowInherit) {
                    payload[key] = ''
                }
                return
            }
            payload[key] = currentValue
        }

        const addAliasMap = (
            field: 'branchTypeAliases' | 'branchStatusAliases' | 'branchPriorityAliases',
            key: string,
        ) => {
            const currentEntries = current[field] as unknown as BranchAliasEntry[]
            const baseEntries = base[field] as unknown as BranchAliasEntry[]
            if (aliasEntriesEqual(currentEntries, baseEntries)) return
            const normalized = normalizedAlias(currentEntries)
            if (!Object.keys(normalized).length && !isGlobal.value) {
                payload[key] = ''
                return
            }
            payload[key] = JSON.stringify(normalized)
        }

        if (isGlobal.value) {
            addValue('serverPort', 'server_port', { trim: true, allowEmpty: false })
            addValue('defaultProject', 'default_project', { trim: true, allowEmpty: true })
            addValue('attachmentsDir', 'attachments_dir', { trim: true, allowEmpty: false })
            addValue('attachmentsMaxUploadMb', 'attachments_max_upload_mb', { trim: true, allowEmpty: false })
        } else {
            addValue('projectName', 'project_name', { trim: true, allowEmpty: false })
            addValue('attachmentsDir', 'attachments_dir', { trim: true, allowEmpty: true })
            addValue('attachmentsMaxUploadMb', 'attachments_max_upload_mb', { trim: true, allowEmpty: true })
        }

        addValue('defaultReporter', 'default_reporter', { trim: true })
        addValue('defaultAssignee', 'default_assignee', { trim: true })
        addValue('defaultPriority', 'default_priority', { trim: true })
        addValue('defaultStatus', 'default_status', { trim: true, allowEmpty: true })

        addArray('defaultTags', 'default_tags')
        addArray('issueStates', 'issue_states')
        addArray('issueTypes', 'issue_types')
        addArray('issuePriorities', 'issue_priorities')

        // Done states are tri-state: null (auto/inherit) vs explicit list.
        // Clearing an explicit list sends an empty CSV so the backend resets
        // the project override (or the global automatic inference); an
        // untouched equal-to-global project list stays an explicit override.
        {
            const currentDone = current.issueDoneStates ?? null
            const baseDone = base.issueDoneStates ?? null
            const samePresence = (currentDone === null) === (baseDone === null)
            const sameList = arraysEqual(currentDone ?? [], baseDone ?? [])
            if (!samePresence || !sameList) {
                payload.issue_done_states = currentDone === null ? '' : normalizeCsv(currentDone)
            }
        }

        // Restricted lists: inherit clears the project override, `*` allows
        // any value, and `[]` stores an explicit "none" (a blank value would
        // clear the override in project scope instead).
        const restrictedValue = (mode: ListMode, values: string[]) => {
            switch (mode) {
                case 'inherit':
                    return ''
                case 'any':
                    return '*'
                case 'none':
                    return '[]'
                default:
                    return normalizeCsv(values)
            }
        }
        const addRestricted = (
            modeField: 'tagsMode' | 'customFieldsMode',
            field: 'tags' | 'customFields',
            key: string,
        ) => {
            const next = restrictedValue(current[modeField], current[field])
            if (next !== restrictedValue(base[modeField], base[field])) {
                payload[key] = next
            }
        }
        addRestricted('tagsMode', 'tags', 'tags')
        addRestricted('customFieldsMode', 'customFields', 'custom_fields')
        addArray('members', 'members')
        addToggleField('strictMembers', 'strict_members', { allowInherit: true })
        addToggleField('autoPopulateMembers', 'auto_populate_members', { allowInherit: true })
        if (!isGlobal.value) {
            addArray('reviewReviewers', 'review_reviewers')
        }
        addArray('scanSignalWords', 'scan_signal_words')
        addArray('scanTicketPatterns', 'scan_ticket_patterns')

        addToggleField('autoSetReporter', 'auto_set_reporter', { allowInherit: true })
        addToggleField('autoAssignOnStatus', 'auto_assign_on_status', { allowInherit: true })
        addToggleField('scanEnableTicketWords', 'scan_enable_ticket_words', { allowInherit: true })
        addToggleField('scanEnableMentions', 'scan_enable_mentions', { allowInherit: true })
        addToggleField('scanStripAttributes', 'scan_strip_attributes', { allowInherit: true })

        addToggleField('autoCodeownersAssign', 'auto_codeowners_assign', { allowInherit: true })
        addToggleField('autoTagsFromPath', 'auto_tags_from_path', { allowInherit: true })
        addToggleField('autoBranchInferType', 'auto_branch_infer_type', { allowInherit: true })
        addToggleField('autoBranchInferStatus', 'auto_branch_infer_status', { allowInherit: true })
        addToggleField('autoBranchInferPriority', 'auto_branch_infer_priority', { allowInherit: true })
        addToggleField('autoIdentity', 'auto_identity', { allowInherit: true })
        addToggleField('autoIdentityGit', 'auto_identity_git', { allowInherit: true })

        addValue('syncReportsDir', 'sync_reports_dir', { allowEmpty: !isGlobal.value })
        addToggleField('syncWriteReports', 'sync_write_reports', { allowInherit: true })
        addToggleField('agentContextEnabled', 'agent_context_enabled', { allowInherit: true })
        addValue('agentContextExtension', 'agent_context_extension', { allowEmpty: !isGlobal.value })
        addValue('agentLogsDir', 'agent_logs_dir')
        {
            // One source at a time: inline text and file path share a setting.
            const instructions = (state: ConfigFormState): [string, string] => {
                switch (state.agentInstructionsMode) {
                    case 'inline':
                        return ['agent_instructions', state.agentInstructions.trim()]
                    case 'file':
                        return ['agent_instructions_file', state.agentInstructionsFile.trim()]
                    default:
                        return ['agent_instructions', '']
                }
            }
            const [key, value] = instructions(current)
            const [baseKey, baseValue] = instructions(base)
            if (key !== baseKey || value !== baseValue) {
                payload[key] = value
            }
        }
        addToggleField('agentWorktreeEnabled', 'agent_worktree_enabled', { allowInherit: true })
        addValue('agentWorktreeDir', 'agent_worktree_dir')
        addValue('agentWorktreeBranchPrefix', 'agent_worktree_branch_prefix', { allowEmpty: !isGlobal.value })

        if (isGlobal.value) {
            addValue('sprintLength', 'sprints_defaults_length')
            addValue('sprintOverdueAfter', 'sprints_defaults_overdue_after')
            addValue('sprintCapacityPoints', 'sprints_defaults_capacity_points')
            addValue('sprintCapacityHours', 'sprints_defaults_capacity_hours')
            addToggleField('sprintNotificationsEnabled', 'sprints_notifications_enabled')
            addValue('webUiPath', 'web_ui_path')
            addValue('uiTheme', 'ui_theme')
            addValue('uiName', 'ui_name')
            addValue('uiLogo', 'ui_logo')
            addValue('uiLogoDark', 'ui_logo_dark')
            addValue('uiIcon', 'ui_icon')
            addValue('uiThemeFile', 'ui_theme_file')
        }

        addAliasMap('branchTypeAliases', 'branch_type_aliases')
        addAliasMap('branchStatusAliases', 'branch_status_aliases')
        addAliasMap('branchPriorityAliases', 'branch_priority_aliases')

        {
            const next = JSON.stringify(tagPropertiesFromEntries(current.tagColors))
            if (next !== JSON.stringify(tagPropertiesFromEntries(base.tagColors))) {
                payload.tag_properties = next === '{}' ? '' : next
            }
        }

        return payload
    }

    return {
        form,
        baseline,
        errors,
        isGlobal,
        currentProject,
        tagWildcard,
        customFieldWildcard,
        projectExists,
        tagSuggestions,
        statusOptions,
        priorityOptions,
        typeOptions,
        statusSuggestions,
        prioritySuggestions,
        typeSuggestions,
        effectiveDoneLabels,
        doneStatesMode,
        doneStatesAutomaticLabel,
        doneStatesEffectiveSummary,
        peopleDescription,
        workflowDescription,
        taxonomyDescription,
        projectOverviewDescription,
        automationDescription,
        scanningDescription,
        branchAliasDescription,
        isDirty,
        hasErrors,
        saveDisabled,
        toggleSelectOptions,
        globalToggleSummary,
        provenanceLabel: getProvenanceLabel,
        provenanceClass: getProvenanceClass,
        sourceFor,
        addAliasEntry,
        removeAliasEntry,
        addTagColor,
        removeTagColor,
        clearAliasField,
        validateField,
        validateAll,
        snapshotForm,
        applySnapshot,
        clearErrors,
        populateForm,
        resetForm,
        buildPayload,
    }
}

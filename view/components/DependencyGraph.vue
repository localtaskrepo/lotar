<template>
  <section class="dependency-view" aria-label="Dependency view" :aria-busy="loading">
    <div class="dependency-toolbar">
      <h3>Dependencies</h3>
      <span v-if="graph && !stale" class="muted dependency-count"
        >{{ focalId ? `1 focal task, ${Math.max(0, nodes.length - 1)} connected tasks` : `${nodes.length} tasks` }},
        {{ edges.length }} links</span
      >
      <template v-if="showLayerControls && relationshipKinds === undefined">
        <label><input v-model="related" type="checkbox" /> Related layer</label>
        <label><input v-model="hierarchy" type="checkbox" /> Hierarchy layer</label>
        <label><input v-model="fixes" type="checkbox" /> Fixes layer</label>
        <label><input v-model="duplicates" type="checkbox" /> Duplicate layer</label>
      </template>
      <UiButton v-if="nodes.length" :aria-pressed="showDiagram" @click="showDiagram = !showDiagram">{{
        showDiagram ? 'Hide diagram' : 'Show diagram'
      }}</UiButton>
    </div>
    <div class="dependency-render" :inert="stale ? true : undefined" :aria-hidden="stale || undefined">
      <ul v-if="legendKinds.length" class="dependency-legend" aria-label="Relationship legend">
        <li v-for="kind in legendKinds" :key="kind" :class="`dependency-edge--${kind}`">
          <svg width="36" height="16" aria-hidden="true">
            <path d="M 0 8 H 28" :stroke-dasharray="relationDashes[kind]" stroke="currentColor" fill="none" />
            <path
              v-if="kind !== 'related'"
              :d="relationMarkers[kind]"
              transform="translate(26 3)"
              fill="currentColor"
              stroke="currentColor"
            /></svg
          ><span>{{ relationLabels[kind] }}{{ kind === 'related' ? ' (undirected)' : '' }}</span>
        </li>
      </ul>
      <p v-if="focalId" class="muted">
        Focal task: {{ focalId }}. Other cards are connected tasks, not additional selections.
      </p>
      <p v-else-if="root && graph" role="status">The focal task could not be uniquely identified in this graph.</p>
      <template v-if="graph">
        <p v-if="graph.truncated || !graph.complete" role="status" class="dependency-warning">
          Partial dependency view.
          {{
            graph.truncated
              ? 'The node or depth limit truncated this graph.'
              : 'Some task data could not be read completely.'
          }}
          Not all dependencies or issues may be shown.
        </p>
        <p v-if="!nodes.length">
          No tasks in this dependency scope.<span v-if="!scope.include_isolated">
            Tasks without visible relationships are hidden. Enable Show unlinked tasks in Filters to show them.</span
          >
        </p>
        <p v-else-if="!edges.length" class="muted">
          No connections in these layers. These tasks have no visible dependency order; use the task list or explicitly
          show the diagram.
        </p>
        <p v-if="nodes.length" class="muted dependency-depth-help">
          Depth counts shortest undirected hops through selected relationship kinds from the focus or matching seeds.
          Stages are prerequisite-ordering columns (longest path in the returned subgraph), not distance from the focus.
        </p>
        <div
          v-if="showDiagram && nodes.length"
          :ref="retainDiagramScroll"
          class="dependency-diagram"
          :style="{ minHeight: `min(${retainedHeight}px, 82dvh, 1000px)` }"
          tabindex="0"
          aria-label="Scrollable dependency diagram"
        >
          <div
            v-if="stages.length > 1"
            class="dependency-stages"
            :style="{ width: `${width}px` }"
            aria-label="Dependency stages"
            title="Prerequisite ordering by longest path in the returned subgraph, not hop distance from the focus"
          >
            <span v-for="stage in stages" :key="stage.rank" :style="{ left: `${stage.x}px` }"
              >Stage {{ stage.rank + 1 }}</span
            >
          </div>
          <div class="dependency-canvas" :style="{ width: `${width}px`, height: `${height}px` }">
            <svg
              :viewBox="`0 0 ${width} ${height}`"
              :style="{ width: `${width}px`, height: `${height}px` }"
              role="group"
              aria-label="Dependency graph; the same tasks and connections are listed below"
            >
              <defs>
                <marker
                  v-for="(_, kind) in relationLabels"
                  :id="`${arrowId}-${kind}`"
                  :key="kind"
                  :class="`dependency-edge--${kind}`"
                  viewBox="0 0 10 10"
                  refX="9"
                  refY="5"
                  markerWidth="7"
                  markerHeight="7"
                  orient="auto-start-reverse"
                >
                  <path :d="relationMarkers[kind]" fill="currentColor" stroke="currentColor" />
                </marker>
              </defs>
              <g
                v-for="edge in placedEdges"
                :key="`${edge.kind}:${edge.source}:${edge.target}`"
                :class="[
                  `dependency-edge dependency-edge--${edge.kind}`,
                  { 'dependency-edge--cycle': cycleEdge(edge) },
                ]"
              >
                <path
                  :d="edge.path"
                  :marker-end="edge.kind === 'related' ? undefined : `url(#${arrowId}-${edge.kind})`"
                />
                <title>
                  {{ edge.source }} {{ relationLabels[edge.kind] }} {{ edge.target
                  }}{{ cycleEdge(edge) ? ' / Cycle issue' : '' }}
                </title>
              </g>
            </svg>
            <div
              v-for="node in placedNodes"
              :key="node.id"
              :ref="(el) => observeCard(node.id, el)"
              :style="{ left: `${node.x}px`, top: `${node.y}px` }"
              :data-task-id="node.id"
              :data-focused="node.id === focalId"
              :aria-current="selected === node.id ? 'true' : undefined"
              :data-rank="node.rank"
              :class="[
                'dependency-node',
                {
                  'dependency-node--selected': selected === node.id,
                  'dependency-node--cycle': cycleNodes.has(node.id),
                  'dependency-node--context': node.matches_filter === false,
                },
              ]"
            >
              <strong v-if="node.id === focalId">Focal task</strong>
              <component
                :is="navigable(node) ? 'a' : 'div'"
                :href="navigable(node) ? `/task/${encodeURIComponent(node.id)}` : undefined"
                :aria-label="`${node.id}: ${node.title || stateLabel(node.state)}${cycleNodes.has(node.id) ? ' / Cycle issue' : ''}`"
                @click="navigate(node.id, $event)"
                @keydown.enter="navigate(node.id, $event)"
                @keydown.space="navigate(node.id, $event)"
              >
                <DependencyCardDetails :node="node" :fields="fields" />
              </component>
              <span class="dependency-node-state"
                >{{ stateLabel(node.state) }}{{ node.is_done ? ' / complete' : '' }}</span
              >
              <strong v-if="node.matches_filter === false">Context: outside filters</strong>
              <strong v-if="cycleNodes.has(node.id)" class="dependency-cycle-label"
                >Cycle issue{{ node.cycle ? ` / group ${node.cycleGroup}` : '' }}</strong
              >
            </div>
          </div>
        </div>
        <details class="dependency-lists" :open="!showDiagram">
          <summary>Tasks in this graph ({{ nodes.length }})</summary>
          <ul class="dependency-task-list" aria-label="Dependency tasks">
            <li
              v-for="node in placedNodes"
              :key="node.id"
              :data-task-id="node.id"
              :data-focused="node.id === focalId"
              :aria-current="selected === node.id ? 'true' : undefined"
              :class="{
                'dependency-selected': selected === node.id,
                'dependency-node--context': node.matches_filter === false,
              }"
            >
              <strong v-if="node.id === focalId">Focal task</strong>
              <component
                :is="navigable(node) ? 'a' : 'div'"
                :href="navigable(node) ? `/task/${encodeURIComponent(node.id)}` : undefined"
                :aria-label="`${node.id}: ${node.title || stateLabel(node.state)}`"
                @click="navigate(node.id, $event)"
                ><DependencyCardDetails :node="node" :fields="fields"
              /></component>
              <span>{{ stateLabel(node.state) }}{{ node.is_done ? ' / complete' : '' }}</span>
              <strong v-if="cycleNodes.has(node.id)" class="dependency-cycle-label">Cycle issue</strong>
              <strong v-if="node.matches_filter === false">Context: outside filters</strong>
              <UiButton
                :disabled="stale"
                :aria-pressed="selected === node.id"
                @click="
                  () => {
                    selected = node.id
                    manualSelection = true
                  }
                "
                >Highlight {{ node.id }}</UiButton
              >
            </li>
          </ul>
          <h4>Connections</h4>
          <ul class="dependency-edge-list" aria-label="Dependency relationships">
            <li
              v-for="edge in edges"
              :key="`${edge.kind}:${edge.source}:${edge.target}`"
              :class="`dependency-edge--${edge.kind}`"
            >
              <component
                :is="nodes.some((node) => node.id === edge.source && navigable(node)) ? 'a' : 'span'"
                :href="`/task/${encodeURIComponent(edge.source)}`"
                @click="navigate(edge.source, $event)"
                >{{ edge.source }}</component
              >
              <span class="dependency-relationship-chip"
                ><svg width="36" height="16" aria-hidden="true">
                  <path
                    d="M 0 8 H 28"
                    :stroke-dasharray="relationDashes[edge.kind]"
                    stroke="currentColor"
                    fill="none"
                  />
                  <path
                    v-if="edge.kind !== 'related'"
                    :d="relationMarkers[edge.kind]"
                    transform="translate(26 3)"
                    fill="currentColor"
                    stroke="currentColor"
                  /></svg
                >{{ ` ${relationLabels[edge.kind]} ` }}</span
              >
              <component
                :is="nodes.some((node) => node.id === edge.target && navigable(node)) ? 'a' : 'span'"
                :href="`/task/${encodeURIComponent(edge.target)}`"
                @click="navigate(edge.target, $event)"
                >{{ edge.target }}</component
              >
              <strong v-if="cycleEdge(edge)" class="dependency-cycle-label">Cycle issue</strong>
            </li>
          </ul>
        </details>
        <section v-if="visibleIssues.length" aria-label="Dependency issues">
          <details class="dependency-issue-details">
            <summary>Problems shown in this graph ({{ visibleIssues.length }})</summary>
            <p class="muted">
              Problems detected while loading this view. Validate project or workspace checks the full scope, including
              tasks outside this graph and its depth or node limits.
            </p>
            <ul class="dependency-problem-list" aria-label="Problems in displayed graph">
              <li v-for="(issue, index) in visibleIssues" :key="index" :class="`dependency-problem--${issue.severity}`">
                <header>
                  <strong>{{ issueTitle(issue.code) }}</strong
                  ><span class="dependency-problem-severity">{{
                    issue.severity === 'error' ? 'Error' : issue.severity === 'warning' ? 'Warning' : 'Info'
                  }}</span>
                </header>
                <p>{{ issue.message }}</p>
                <footer>
                  <span class="dependency-problem-action" :title="!issue.repairable ? issue.message : undefined">{{
                    issue.repairable ? 'Safe fix available' : 'Manual review'
                  }}</span>
                  <UiButton
                    v-if="instance?.vnode.props?.onFix && issue.repairable"
                    class="dependency-problem-fix"
                    :disabled="stale || loading || fixBusy || !issue.id"
                    :title="
                      !issue.id
                        ? 'Manual review: this issue has no repair ID. Reload before requesting a fix.'
                        : stale || loading
                          ? 'Wait for the current dependency view before requesting a fix.'
                          : fixBusy
                            ? 'A fix preview or repair is already in progress.'
                            : 'Preview this issue only; confirmation is required before applying changes.'
                    "
                    @click="requestFix(issue)"
                    >Fix</UiButton
                  >
                  <component
                    v-for="id in issue.tasks"
                    :is="nodes.some((node) => node.id === id && navigable(node)) ? 'a' : 'span'"
                    :key="id"
                    :href="
                      nodes.some((node) => node.id === id && navigable(node))
                        ? `/task/${encodeURIComponent(id)}`
                        : undefined
                    "
                    class="dependency-problem-task"
                    @click="navigate(id, $event)"
                    >{{ id }}</component
                  >
                </footer>
              </li>
            </ul>
          </details>
        </section>
      </template>
    </div>
    <p v-if="stale" role="status">
      Previous dependency view (inactive).
      {{ error ? 'The requested view could not be loaded.' : 'Waiting for the requested view.' }}
    </p>
    <p v-if="loading" role="status">Loading dependency graph...</p>
    <p v-if="error" role="alert">{{ error }}</p>
  </section>
</template>

<script setup lang="ts">
import { computed, ref, useId, watch, onBeforeUnmount, getCurrentInstance } from 'vue'
import type {
  RelationshipEdge,
  RelationshipIssue,
  RelationshipKind,
  RelationshipNode,
  RelationshipGraphRequest,
  RelationshipGraphResponse,
  TaskDTO,
} from '../api/types'
import { useRelationshipGraph } from '../composables/useRelationshipGraph'
import { useTaskPanelController } from '../composables/useTaskPanelController'
import UiButton from './UiButton.vue'
import DependencyCardDetails from './DependencyCardDetails.vue'
import { dependencyLayout } from '../utils/dependencyLayout'
import { compareTasks, normalizeSortBy } from '../utils/taskSort'
import { parseTaskId } from '../utils/text'

const props = withDefaults(
  defineProps<{
    root?: string
    project?: string
    depth?: number
    maxNodes?: number
    type?: string
    showLayerControls?: boolean
    showIsolated?: boolean
    filters?: Record<string, string>
    sortBy?: string
    order?: string
    fields?: Partial<Record<string, boolean>>
    relationshipKinds?: RelationshipKind[]
    fixBusy?: boolean
  }>(),
  { depth: 1, maxNodes: 100, type: 'dependency', showLayerControls: true, showIsolated: false, fixBusy: false },
)
const emit = defineEmits<{
  loaded: [graph: RelationshipGraphResponse]
  activate: [id: string]
  fix: [issue: RelationshipIssue]
}>()
const instance = getCurrentInstance()
const related = ref(props.type === 'related' || props.type === 'all')
const hierarchy = ref(props.type === 'hierarchy' || props.type === 'all')
const fixes = ref(props.type === 'fixes' || props.type === 'all')
const duplicates = ref(props.type === 'duplicate' || props.type === 'all')
const selectedKinds = computed<RelationshipKind[]>(
  () =>
    props.relationshipKinds ?? [
      'dependency',
      ...(related.value ? ['related' as const] : []),
      ...(hierarchy.value ? ['hierarchy' as const] : []),
      ...(fixes.value ? ['fixes' as const] : []),
      ...(duplicates.value ? ['duplicate' as const] : []),
    ],
)
const query = computed(() => ({
  task_id: props.root || undefined,
  project: props.project || undefined,
  depth: props.depth,
  max_nodes: props.maxNodes,
  relationship_kinds: [...selectedKinds.value],
  include_isolated: props.showIsolated,
  ...(props.filters || props.sortBy || props.order
    ? {
        filters: {
          ...props.filters,
          ...(props.sortBy ? { sort_by: normalizeSortBy(props.sortBy) || props.sortBy } : {}),
          ...(props.order ? { order: props.order } : {}),
        },
      }
    : {}),
}))
const { graph, error, loading, refresh, acceptedQuery, stale } = useRelationshipGraph(query)
const scope = computed<RelationshipGraphRequest>(() => acceptedQuery.value || query.value)
defineExpose({ refresh, loading })
const relationLabels: Record<RelationshipKind, string> = {
  dependency: 'blocks',
  hierarchy: 'parent of',
  related: 'related',
  fixes: 'fixes',
  duplicate: 'duplicate of',
}
const relationDashes: Record<RelationshipKind, string> = {
  dependency: '',
  hierarchy: '8 3',
  related: '2 4',
  fixes: '10 3 2 3',
  duplicate: '4 3',
}
const relationMarkers: Record<RelationshipKind, string> = {
  dependency: 'M 0 0 L 10 5 L 0 10 z',
  hierarchy: 'M 0 0 L 10 5 L 0 10 L 4 5 z',
  related: '',
  fixes: 'M 0 1 L 5 5 L 0 9 M 5 1 L 10 5 L 5 9',
  duplicate: 'M 0 5 L 5 0 L 10 5 L 5 10 z',
}
watch(graph, (value) => {
  if (value) emit('loaded', value)
})
const { openTaskPanel } = useTaskPanelController()
watch(
  () => props.type,
  (type) => {
    related.value = type === 'related' || type === 'all'
    hierarchy.value = type === 'hierarchy' || type === 'all'
    fixes.value = type === 'fixes' || type === 'all'
    duplicates.value = type === 'duplicate' || type === 'all'
  },
)
const desktop = typeof window !== 'undefined' && window.matchMedia?.('(min-width: 700px)').matches === true
const diagramChoice = ref<boolean | null>(null)
let diagramElement: HTMLElement | undefined
let diagramScroll = { left: 0, top: 0 }
const retainedHeight = ref(0)
watch(
  graph,
  () => {
    if (!diagramElement) return
    retainedHeight.value = Math.max(retainedHeight.value, diagramElement.getBoundingClientRect().height)
  },
  { flush: 'sync' },
)
function retainDiagramScroll(el: unknown) {
  if (diagramElement === el) return
  if (diagramElement) diagramScroll = { left: diagramElement.scrollLeft, top: diagramElement.scrollTop }
  diagramElement = el instanceof HTMLElement ? el : undefined
  if (diagramElement) {
    diagramElement.scrollLeft = diagramScroll.left
    diagramElement.scrollTop = diagramScroll.top
  }
}
const showDiagram = computed({
  get: () => diagramChoice.value ?? (desktop && (edges.value.length > 0 || retainedHeight.value > 0)),
  set: (value) => {
    diagramChoice.value = value
  },
})
const selected = ref('')
const manualSelection = ref(false)
watch(
  () => [props.root, props.project],
  () => {
    selected.value = ''
    manualSelection.value = false
  },
)
const arrowId = `dependency-arrow-${useId()}`
const focalId = computed(() => {
  const rootValue = scope.value.task_id
  if (!rootValue) return ''
  const root = parseTaskId(rootValue)
  const candidates = (graph.value?.nodes || []).filter((node) => {
    if (node.id === rootValue) return true
    const id = parseTaskId(node.id)
    if (!id) return false
    if (root) return root.project === id.project && root.number === id.number
    // Compact aliases retain exact-case project identity and canonical numeric suffixes.
    const suffix = rootValue.slice(id.project.length)
    return rootValue.startsWith(id.project) && /^[0-9]+$/.test(suffix) && BigInt(suffix).toString() === id.number
  })
  return candidates.length === 1 ? candidates[0]!.id : ''
})
const enabledEdges = computed(() =>
  (graph.value?.edges || []).filter((edge) =>
    (scope.value.relationship_kinds ?? selectedKinds.value).includes(edge.kind),
  ),
)
const projectedNodes = computed(() => {
  const inventory = graph.value?.nodes || []
  if (scope.value.task_id && !focalId.value) return []
  const ids = new Set(inventory.map((node) => node.id))
  const seeds = scope.value.task_id
    ? [focalId.value]
    : inventory.filter((node) => node.matches_filter !== false).map((node) => node.id)
  const reached = new Set(seeds)
  let frontier = seeds
  for (let depth = 0; depth < (scope.value.depth ?? 1) && frontier.length; depth++) {
    const next = new Set<string>()
    for (const edge of enabledEdges.value) {
      if (frontier.includes(edge.source) && ids.has(edge.target) && !reached.has(edge.target)) next.add(edge.target)
      if (frontier.includes(edge.target) && ids.has(edge.source) && !reached.has(edge.source)) next.add(edge.source)
    }
    for (const id of next) reached.add(id)
    frontier = [...next]
  }
  return inventory.filter((node) => reached.has(node.id))
})
const searchId = computed(() => {
  const query = scope.value.filters?.q?.trim() || ''
  if (!query) return ''
  const parsed = parseTaskId(query)
  const candidates = projectedNodes.value.filter((node) => {
    const id = parseTaskId(node.id)
    if (!id) return node.id === query
    if (parsed) return parsed.project === id.project && parsed.number === id.number
    if (/^[0-9]+$/.test(query))
      return (!scope.value.project || node.project === scope.value.project) && BigInt(query).toString() === id.number
    const suffix = query.slice(id.project.length)
    return query.startsWith(id.project) && /^[0-9]+$/.test(suffix) && BigInt(suffix).toString() === id.number
  })
  return candidates.length === 1 ? candidates[0]!.id : ''
})
const nodes = computed(() => {
  const inventory = projectedNodes.value
  const ids = new Set(inventory.map((node) => node.id))
  const incident = new Set(
    enabledEdges.value
      .filter((edge) => ids.has(edge.source) && ids.has(edge.target))
      .flatMap((edge) => [edge.source, edge.target]),
  )
  return inventory.filter(
    (node) =>
      incident.has(node.id) ||
      node.id === focalId.value ||
      node.id === searchId.value ||
      ['missing', 'ambiguous', 'unreadable'].includes(node.state) ||
      (scope.value.include_isolated && node.matches_filter !== false),
  )
})
watch([nodes, loading, stale], () => {
  if (graph.value && !loading.value && !stale.value && !nodes.value.length) {
    retainedHeight.value = 0
    diagramScroll = { left: 0, top: 0 }
    cardHeights.value = {}
  }
})
watch(
  () => props.filters?.q,
  () => {
    manualSelection.value = false
    selected.value = searchId.value || focalId.value
  },
)
watch(
  [focalId, nodes, searchId],
  () => {
    if (!graph.value) return
    if (!manualSelection.value || !nodes.value.some((node) => node.id === selected.value))
      selected.value = searchId.value || focalId.value
  },
  { immediate: true },
)
const visibleIssues = computed(() => {
  const ids = new Set(nodes.value.map((node) => node.id))
  return (graph.value?.issues || []).filter((issue) => issue.tasks.every((id) => ids.has(id)))
})
const edges = computed(() => {
  const ids = new Set(nodes.value.map((node) => node.id))
  return enabledEdges.value.filter((edge) => ids.has(edge.source) && ids.has(edge.target))
})
const legendKinds = computed(() =>
  (Object.keys(relationLabels) as RelationshipKind[]).filter((kind) => edges.value.some((edge) => edge.kind === kind)),
)
const cycleIssues = computed(() =>
  visibleIssues.value.filter((issue) => issue.code === 'cycle' || issue.code.endsWith('_cycle')),
)
const cycleNodes = computed(
  () =>
    new Set([
      ...cycleIssues.value.flatMap((issue) => issue.tasks),
      ...placedNodes.value.filter((node) => node.cycle).map((node) => node.id),
    ]),
)
function cycleEdge(edge: RelationshipEdge) {
  const source = placedNodes.value.find((node) => node.id === edge.source)
  const target = placedNodes.value.find((node) => node.id === edge.target)
  if (edge.kind === 'dependency' && source?.cycle && target?.cycle && source.cycleGroup === target.cycleGroup)
    return true
  return (
    ['dependency', 'hierarchy', 'duplicate'].includes(edge.kind) &&
    cycleIssues.value.some(
      (issue) =>
        (issue.code === 'cycle' || issue.code === `${edge.kind}_cycle`) &&
        issue.tasks.includes(edge.source) &&
        issue.tasks.includes(edge.target),
    )
  )
}
function stateLabel(state: RelationshipNode['state']) {
  return (
    {
      active: 'Active',
      external: 'External workspace (read-only)',
      deleted: 'Deleted',
      missing: 'Missing target',
      ambiguous: 'Ambiguous ID',
      unreadable: 'Unreadable task',
    }[state] || state
  )
}
function issueTitle(code: string) {
  const names: Record<string, string> = {
    missing_mirror: 'Missing reciprocal link',
    missing: 'Missing task',
    missing_target: 'Missing task',
    deleted: 'Deleted task',
    deleted_target: 'Deleted task',
    self_link: 'Self relationship',
    parent_conflict: 'Multiple parent claims',
    cycle: 'Circular dependency',
    dependency_cycle: 'Circular dependency',
    hierarchy_cycle: 'Parent-child cycle',
    duplicate_cycle: 'Duplicate loop',
    unreadable: 'Unreadable task',
    ambiguous: 'Ambiguous task ID',
  }
  return names[code] || code.replace(/_/g, ' ')
}
function requestFix(issue: RelationshipIssue) {
  if (!instance?.vnode.props?.onFix || !issue.repairable || !issue.id || stale.value || loading.value || props.fixBusy)
    return
  emit('fix', issue)
}
function navigable(node: RelationshipNode) {
  return node.state === 'active' || node.state === 'external' || node.state === 'deleted'
}
const cardHeights = ref<Record<string, number>>({})
const observedCards = new Map<string, HTMLElement>()
const observer =
  typeof ResizeObserver === 'undefined'
    ? undefined
    : new ResizeObserver((entries) => {
        const next = { ...cardHeights.value }
        let changed = false
        for (const entry of entries) {
          const id = (entry.target as HTMLElement).dataset.taskId
          const height = Math.ceil(entry.target.getBoundingClientRect().height)
          if (id && height > 0 && next[id] !== height) {
            next[id] = height
            changed = true
          }
        }
        if (changed) cardHeights.value = next
      })
function observeCard(id: string, el: unknown) {
  const previous = observedCards.get(id)
  if (previous === el) return
  if (previous) observer?.unobserve(previous)
  if (el instanceof HTMLElement) {
    observedCards.set(id, el)
    observer?.observe(el)
  } else observedCards.delete(id)
}
onBeforeUnmount(() => observer?.disconnect())
function estimatedHeight(node: RelationshipNode) {
  if (cardHeights.value[node.id]) return cardHeights.value[node.id]!
  const values = { ...node.task, id: node.id, title: node.task?.title ?? node.title }
  const lines = Object.entries(values)
    .filter(([key, value]) => props.fields?.[key] !== false && value != null && value !== '')
    .reduce((sum, [, value]) => sum + Math.min(30, Math.max(1, Math.ceil(JSON.stringify(value).length / 24))), 0)
  return Math.min(2400, Math.max(96, 80 + lines * 24))
}
const layout = computed(() =>
  dependencyLayout(
    nodes.value,
    edges.value,
    (a, b) => {
      const sortBy = normalizeSortBy(scope.value.filters?.sort_by) || 'id'
      if (sortBy !== 'id' && !!a.task !== !!b.task) return a.task ? -1 : 1
      // compareTasks reads only fields carried by the compact graph task summary.
      const primary = a.task && b.task ? compareTasks(a.task as TaskDTO, b.task as TaskDTO, sortBy) : 0
      return primary * (scope.value.filters?.order === 'desc' ? -1 : 1) || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)
    },
    estimatedHeight,
  ),
)
const placedNodes = computed(() => layout.value.nodes)
const stages = computed(() =>
  [...new Map(placedNodes.value.map((node) => [node.rank, { rank: node.rank, x: node.x }])).values()].sort(
    (a, b) => a.rank - b.rank,
  ),
)
const width = computed(() => layout.value.width)
const height = computed(() => layout.value.height)
const placedEdges = computed(() => layout.value.edges)
function navigate(id: string, event: MouseEvent | KeyboardEvent) {
  if (stale.value) {
    event.preventDefault()
    event.stopPropagation()
    return
  }
  if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey || ('button' in event && event.button !== 0))
    return
  const node = nodes.value.find((entry) => entry.id === id)
  if (!node || !navigable(node)) return
  event.preventDefault()
  event.stopPropagation()
  selected.value = id
  manualSelection.value = true
  if (instance?.vnode.props?.onActivate) emit('activate', id)
  else openTaskPanel({ taskId: id })
}
</script>

<style scoped>
.dependency-view {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  min-width: 0;
  max-width: 100%;
  gap: 12px;
  border-top: 1px solid var(--border);
  padding-top: 16px;
}
.dependency-render {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  min-width: 0;
  max-width: 100%;
  gap: 12px;
}
.dependency-render[inert] {
  pointer-events: none;
  opacity: 0.55;
}
.dependency-toolbar {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 12px;
}
.dependency-toolbar h3 {
  margin: 0;
  margin-right: auto;
}
.dependency-toolbar label {
  display: inline-flex;
  gap: 4px;
  align-items: center;
}
.dependency-count,
.dependency-legend {
  font-size: 0.85em;
}
.dependency-legend {
  margin: 0;
}
.dependency-lists summary,
.dependency-issue-details summary {
  cursor: pointer;
  color: var(--muted);
  font-size: 0.9em;
}
.dependency-lists[open] summary {
  margin-bottom: 12px;
}
.dependency-problem-list {
  list-style: none;
  margin: 12px 0 0;
  padding: 0;
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(min(100%, 20rem), 1fr));
  gap: 10px;
  align-items: start;
  justify-items: start;
}
.dependency-problem-list > li {
  min-width: 0;
  width: 100%;
  max-width: 24rem;
  box-sizing: border-box;
  border: 1px solid var(--color-border);
  border-left: 3px solid var(--color-accent);
  border-radius: var(--radius-md);
  padding: 12px;
  background: var(--color-surface);
}
.dependency-problem-list > li.dependency-problem--error {
  border-left-color: var(--color-danger);
}
.dependency-problem-list > li.dependency-problem--warning {
  border-left-color: var(--color-warning);
}
.dependency-problem-list header {
  display: flex;
  align-items: start;
  justify-content: space-between;
  gap: 8px;
}
.dependency-problem-list header strong {
  font-size: var(--text-sm);
}
.dependency-problem-severity {
  flex-shrink: 0;
  color: var(--color-accent);
  font-size: var(--text-xs);
  font-weight: 600;
}
.dependency-problem--error .dependency-problem-severity {
  color: var(--color-danger);
}
.dependency-problem--warning .dependency-problem-severity {
  color: var(--color-warning);
}
.dependency-problem-list p {
  margin: 8px 0;
  color: var(--color-muted);
  overflow-wrap: anywhere;
  font-size: var(--text-sm);
}
.dependency-problem-list footer {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 6px;
  font-size: var(--text-xs);
}
.dependency-problem-action {
  margin-right: auto;
  color: var(--color-muted);
}
.dependency-problem-fix {
  padding: 2px 8px;
  font-size: var(--text-xs);
}
.dependency-problem-task {
  color: var(--color-fg);
  background: var(--color-surface-contrast);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  padding: 2px 6px;
  overflow-wrap: anywhere;
}
.dependency-warning {
  border-left: 3px solid var(--color-warning);
  padding: 8px 12px;
}
.dependency-diagram {
  width: fit-content;
  min-width: 0;
  max-width: 100%;
  box-sizing: border-box;
  overflow: auto;
  max-height: min(82dvh, 1000px);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
}
.dependency-canvas {
  position: relative;
}
.dependency-stages {
  position: relative;
  height: 28px;
  color: var(--color-muted);
  font-size: var(--text-sm);
  border-bottom: 1px solid var(--color-border);
}
.dependency-stages span {
  position: absolute;
  top: 6px;
  font-weight: 600;
}
.dependency-canvas > svg {
  position: absolute;
  inset: 0;
  pointer-events: none;
}
.dependency-node {
  position: absolute;
  box-sizing: border-box;
  width: 300px;
  padding: 10px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--surface, var(--color-bg));
  display: grid;
  gap: 4px;
  font-size: 12px;
}
.dependency-node a {
  display: grid;
  gap: 4px;
  min-width: 0;
  overflow-wrap: anywhere;
}
.dependency-node--selected {
  outline: 2px solid var(--accent);
}
.dependency-node--cycle {
  border: 2px dashed var(--color-danger);
}
.dependency-node--context {
  background: color-mix(in srgb, var(--color-bg) 85%, var(--color-muted));
  color: var(--color-muted);
}
.dependency-edge {
  color: var(--muted);
}
.dependency-edge path {
  stroke: currentColor;
  stroke-width: 1.5;
  fill: none;
}
.dependency-edge--dependency {
  color: var(--color-accent);
}
.dependency-edge--hierarchy {
  color: var(--color-chart-5);
  --edge-dash: 8 3;
}
.dependency-edge--related {
  color: var(--color-muted);
  --edge-dash: 2 4;
}
.dependency-edge--fixes {
  color: var(--color-success-strong);
  --edge-dash: 10 3 2 3;
}
.dependency-edge--duplicate {
  color: var(--color-warning-strong);
  --edge-dash: 4 3;
}
.dependency-edge path {
  stroke-dasharray: var(--edge-dash, none);
}
.dependency-legend {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  padding: 0;
  list-style: none;
}
.dependency-legend li {
  display: flex;
  align-items: center;
  gap: 6px;
}
.dependency-legend li > span {
  color: var(--color-fg);
}
.dependency-node-state {
  opacity: 0.7;
}
.dependency-edge--cycle path {
  stroke: var(--color-danger);
  stroke-width: 3;
  stroke-dasharray: 6 3;
}
.dependency-cycle-label {
  font-size: 0.85em;
  font-weight: 700;
}
.dependency-task-list {
  padding: 0;
  list-style: none;
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(min(100%, 17rem), 1fr));
  gap: 12px;
  align-items: start;
}
.dependency-task-list > li {
  min-width: 0;
  width: 100%;
  max-width: 20rem;
  box-sizing: border-box;
  padding: 10px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--surface, var(--color-bg));
  display: grid;
  gap: 6px;
  font-size: 12px;
}
.dependency-selected {
  outline: 2px solid var(--accent);
}
.dependency-edge-list {
  margin: 0;
  padding: 0;
  list-style: none;
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(min(100%, 18rem), 1fr));
  gap: 8px;
  width: 100%;
  max-width: 100%;
  align-items: start;
  justify-items: start;
}
.dependency-edge-list > li {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  width: fit-content;
  max-width: 100%;
  box-sizing: border-box;
  padding: 10px 12px;
  border: 1px solid var(--border);
  border-radius: 8px;
}
.dependency-edge-list a,
.dependency-edge-list > li > span:not(.dependency-relationship-chip) {
  min-width: 0;
  max-width: 100%;
  color: var(--color-fg);
  overflow-wrap: anywhere;
}
.dependency-relationship-chip {
  display: inline-flex;
  align-items: center;
  flex-wrap: wrap;
  max-width: 100%;
  box-sizing: border-box;
  gap: 6px;
  border-radius: 999px;
  padding: 3px 8px;
  background: color-mix(in srgb, currentColor 9%, transparent);
  font-size: 0.8rem;
}
</style>

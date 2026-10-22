<template>
  <UiModal :open="open" aria-label="Board columns" :aria-labelledby="titleId" size="xl" initial-focus="[data-testid='board-columns-apply']" @close="emit('close')">
    <div class="board-columns-dialog" data-testid="board-columns-dialog">
      <header class="board-columns-dialog__header">
        <h2 :id="titleId">Board columns</h2>
        <p class="muted">
          Drag columns to reorder them. Drag a status onto another column to stack it there; each stacked status keeps
          its own section. Saved in this browser for this project.
        </p>
      </header>

      <div ref="boardEl" class="board-columns-dialog__board" @dragend="endDrag" @scroll="repositionOpenMenus">
        <section
          v-for="(column, index) in draft"
          :key="column.id"
          class="board-columns-dialog__column"
          :class="{
            'is-status-target': drag?.kind === 'status' && dropColumn === column.id,
            'is-drop-before': drag?.kind === 'column' && dropColumn === column.id && dropSide === 'before',
            'is-drop-after': drag?.kind === 'column' && dropColumn === column.id && dropSide === 'after',
            'is-dragging': drag?.kind === 'column' && drag.id === column.id,
            'is-hidden': isColumnHidden(column),
          }"
          :data-column-index="index"
          :aria-label="`Column ${index + 1}: ${displayTitle(column)}`"
          @dragover.prevent="onColumnDragOver(column.id, $event)"
          @drop.prevent="onColumnDrop(column.id)"
        >
          <header class="board-columns-dialog__column-head" draggable="true" @dragstart="startColumnDrag(column.id, $event)">
            <button
              type="button"
              class="board-columns-dialog__grip"
              :aria-label="`Reorder column ${displayTitle(column)}`"
              title="Drag to reorder (or use the arrow keys)"
              @keydown.left.prevent="moveColumn(index, -1)"
              @keydown.right.prevent="moveColumn(index, 1)"
            >⠿</button>
            <template v-if="renaming === column.id">
              <input
                :ref="setRenameInput"
                v-model="column.name"
                class="input board-columns-dialog__rename"
                type="text"
                :placeholder="defaultTitle(column)"
                :aria-label="`Name for column ${index + 1}`"
                @keydown.enter.prevent="renaming = null"
                @keydown.escape.stop.prevent="cancelRename(column)"
                @blur="renaming = null"
              />
            </template>
            <template v-else>
              <h3 class="board-columns-dialog__title" :title="displayTitle(column)">{{ displayTitle(column) }}</h3>
              <button
                type="button"
                class="board-columns-dialog__icon-button"
                :aria-label="`Rename column ${displayTitle(column)}`"
                title="Rename column"
                @click="startRename(column)"
              >
                <IconGlyph name="edit" />
              </button>
            </template>
          </header>

          <ul class="board-columns-dialog__statuses">
            <li
              v-for="key in column.statuses"
              :key="key"
              class="board-columns-dialog__status"
              :class="{ 'is-hidden': hidden.has(key), 'is-drop-before': drag?.kind === 'status' && dropStatus === key }"
              :data-layout-status="labelOf(key)"
              draggable="true"
              @dragstart.stop="startStatusDrag(key, $event)"
              @dragover.prevent.stop="onStatusDragOver(column.id, key, $event)"
              @drop.prevent.stop="onStatusDrop(column.id, key)"
            >
              <span class="board-columns-dialog__status-grip" aria-hidden="true">⠿</span>
              <span class="board-columns-dialog__label" :title="labelOf(key)">{{ labelOf(key) }}</span>
              <button
                type="button"
                class="board-columns-dialog__icon-button"
                :aria-pressed="!hidden.has(key)"
                :aria-label="`${hidden.has(key) ? 'Show' : 'Hide'} ${labelOf(key)}`"
                :title="hidden.has(key) ? 'Hidden — click to show' : 'Visible — click to hide'"
                @click="toggleHidden(key)"
              >
                <IconGlyph :name="hidden.has(key) ? 'eye-off' : 'eye'" />
              </button>
              <details class="board-columns-dialog__menu" @toggle="positionMenu" @keydown.escape.stop="closeMenu($event)">
                <summary class="board-columns-dialog__icon-button" :aria-label="`Move ${labelOf(key)}`" title="Move to another column">
                  <IconGlyph name="dots-horizontal" />
                </summary>
                <div class="card board-columns-dialog__menu-list">
                  <button
                    v-if="column.statuses.length > 1"
                    type="button"
                    class="board-columns-dialog__menu-item"
                    @click="closeMenu($event); moveStatus(key, 'new')"
                  >Own column</button>
                  <button
                    v-for="other in draft.filter((entry) => entry.id !== column.id)"
                    :key="other.id"
                    type="button"
                    class="board-columns-dialog__menu-item"
                    @click="closeMenu($event); moveStatus(key, other.id)"
                  >Move to {{ displayTitle(other) }}</button>
                </div>
              </details>
            </li>
          </ul>

          <div class="board-columns-dialog__settings">
            <label title="Maximum cards in this column across all of its statuses">
              <span>WIP limit</span>
              <input v-model.number="column.wipLimit" class="input" type="number" min="0" step="1" placeholder="—" />
            </label>
            <label title="Hide cards that moved into their status more than this many days ago">
              <span>Hide after</span>
              <input v-model.number="column.maxAgeDays" class="input" type="number" min="0" step="1" placeholder="days" aria-label="Hide after (days)" />
            </label>
            <label title="Show only the most recently updated cards per status">
              <span>Max cards</span>
              <input v-model.number="column.maxVisible" class="input" type="number" min="0" step="1" placeholder="—" />
            </label>
          </div>
        </section>

        <div
          class="board-columns-dialog__new-column muted"
          title="Drop a status here to give it its own column at the end"
          :class="{ 'is-status-target': drag?.kind === 'status' && dropColumn === 'new' }"
          @dragover.prevent="drag?.kind === 'status' && (dropColumn = 'new')"
          @drop.prevent="onNewColumnDrop"
        >
          <IconGlyph name="plus" />
          <strong>New column</strong>
          <span class="board-columns-dialog__new-column-hint">Drop a status here</span>
        </div>
      </div>

      <p class="muted board-columns-dialog__hint">
        WIP limit counts every status in the column. "Hide after" and "Max cards" apply to each status; hidden cards
        still exist and stay searchable.
      </p>

      <footer class="board-columns-dialog__footer">
        <UiButton variant="ghost" type="button" @click="resetDraft">Reset to defaults</UiButton>
        <span class="board-columns-dialog__spacer" />
        <UiButton variant="ghost" type="button" @click="emit('close')">Cancel</UiButton>
        <UiButton variant="primary" type="button" data-testid="board-columns-apply" @click="apply">Apply</UiButton>
      </footer>
    </div>
  </UiModal>
</template>

<script setup lang="ts">
import { nextTick, ref, useId, watch } from 'vue'
import type { BoardColumnConfig, BoardLayout, LayoutStatus } from '../composables/useBoardLayout'
import IconGlyph from './IconGlyph.vue'
import UiButton from './UiButton.vue'
import UiModal from './UiModal.vue'

const props = defineProps<{
  open: boolean
  statuses: LayoutStatus[]
  layout: BoardLayout
}>()
const emit = defineEmits<{ (e: 'close'): void; (e: 'apply', layout: BoardLayout): void }>()

interface DraftColumn {
  id: string
  statuses: string[]
  name: string
  wipLimit: number | '' | undefined
  maxAgeDays: number | '' | undefined
  maxVisible: number | '' | undefined
}

type DragState = { kind: 'status'; key: string } | { kind: 'column'; id: string }

const titleId = useId()
const draft = ref<DraftColumn[]>([])
const hidden = ref<Set<string>>(new Set())
const drag = ref<DragState | null>(null)
const dropColumn = ref<string | null>(null)
const dropSide = ref<'before' | 'after'>('before')
const dropStatus = ref<string | null>(null)
const renaming = ref<string | null>(null)
let renameInput: HTMLInputElement | null = null
function setRenameInput(el: unknown) {
  renameInput = el instanceof HTMLInputElement ? el : null
}
let renameOriginal = ''
let nextId = 0

function toDraft(column: BoardColumnConfig): DraftColumn {
  nextId += 1
  return {
    id: `column-${nextId}`,
    statuses: [...column.statuses],
    name: column.name ?? '',
    wipLimit: column.wipLimit,
    maxAgeDays: column.maxAgeDays,
    maxVisible: column.maxVisible,
  }
}

watch(() => props.open, (open) => {
  if (!open) return
  draft.value = props.layout.columns.map(toDraft)
  hidden.value = new Set(props.layout.hidden)
  renaming.value = null
  endDrag()
}, { immediate: true })

const labelOf = (key: string) => props.statuses.find((status) => status.key === key)?.label ?? key
const defaultTitle = (column: DraftColumn) => column.statuses.map(labelOf).join(' · ')
const displayTitle = (column: DraftColumn) => column.name.trim() || defaultTitle(column)
const isColumnHidden = (column: DraftColumn) => column.statuses.every((key) => hidden.value.has(key))

function startRename(column: DraftColumn) {
  renameOriginal = column.name
  renaming.value = column.id
  void nextTick(() => renameInput?.focus())
}

function cancelRename(column: DraftColumn) {
  column.name = renameOriginal
  renaming.value = null
}

const boardEl = ref<HTMLElement | null>(null)

function closeAllMenus(except?: Element) {
  boardEl.value?.querySelectorAll('details[open]').forEach((menu) => {
    if (menu !== except) (menu as HTMLDetailsElement).open = false
  })
}

/** Menus sit above the scrolling column area, aligned to their trigger. */
function placeMenu(menu: HTMLDetailsElement) {
  const trigger = menu.querySelector('summary')?.getBoundingClientRect()
  const list = menu.querySelector<HTMLElement>('.board-columns-dialog__menu-list')
  if (!trigger || !list) return
  const width = Math.max(list.offsetWidth, 200)
  list.style.top = `${Math.round(trigger.bottom + 4)}px`
  list.style.left = `${Math.round(Math.max(8, Math.min(trigger.right - width, window.innerWidth - width - 8)))}px`
}

function positionMenu(event: Event) {
  const menu = event.currentTarget as HTMLDetailsElement
  if (!menu.open) return
  closeAllMenus(menu)
  placeMenu(menu)
}

/** Keep open menus attached to their trigger while the column area scrolls. */
function repositionOpenMenus() {
  boardEl.value?.querySelectorAll<HTMLDetailsElement>('details[open]').forEach(placeMenu)
}

function closeMenu(event: Event) {
  const menu = (event.currentTarget as HTMLElement | null)?.closest('details')
  if (menu) menu.open = false
}

function toggleHidden(key: string) {
  const next = new Set(hidden.value)
  if (next.has(key)) next.delete(key)
  else next.add(key)
  hidden.value = next
}

function moveColumn(index: number, delta: number) {
  const target = index + delta
  if (target < 0 || target >= draft.value.length) return
  const next = [...draft.value]
  const [column] = next.splice(index, 1)
  next.splice(target, 0, column!)
  draft.value = next
}

/** Remove a status from its column; returns the index that column had. */
function detach(key: string): number {
  const index = draft.value.findIndex((column) => column.statuses.includes(key))
  if (index >= 0) draft.value[index]!.statuses = draft.value[index]!.statuses.filter((entry) => entry !== key)
  return index
}

function prune() {
  draft.value = draft.value.filter((column) => column.statuses.length > 0)
}

/** Move a status into another column, or into its own column right after its current one. */
function moveStatus(key: string, targetId: string) {
  const source = draft.value.find((column) => column.statuses.includes(key))
  if (!source || source.id === targetId) return
  if (targetId === 'new') {
    const index = detach(key)
    draft.value.splice(index + 1, 0, toDraft({ statuses: [key] }))
  } else {
    const target = draft.value.find((column) => column.id === targetId)
    if (!target) return
    detach(key)
    target.statuses.push(key)
  }
  prune()
}

// -- Drag and drop ------------------------------------------------------------

function startStatusDrag(key: string, event: DragEvent) {
  drag.value = { kind: 'status', key }
  event.dataTransfer?.setData('text/plain', key)
  if (event.dataTransfer) event.dataTransfer.effectAllowed = 'move'
}

function startColumnDrag(id: string, event: DragEvent) {
  drag.value = { kind: 'column', id }
  event.dataTransfer?.setData('text/plain', id)
  if (event.dataTransfer) event.dataTransfer.effectAllowed = 'move'
}

function endDrag() {
  drag.value = null
  dropColumn.value = null
  dropStatus.value = null
}

/** Column drags land before or after the hovered column, by pointer half. */
function updateDropSide(event: DragEvent) {
  const columnEl = (event.currentTarget as HTMLElement | null)?.closest('.board-columns-dialog__column')
  if (!columnEl) return
  const box = columnEl.getBoundingClientRect()
  dropSide.value = event.clientX > box.left + box.width / 2 ? 'after' : 'before'
}

function onColumnDragOver(id: string, event: DragEvent) {
  if (!drag.value) return
  dropColumn.value = id
  dropStatus.value = null
  if (drag.value.kind === 'column') updateDropSide(event)
}

function onStatusDragOver(columnId: string, key: string, event: DragEvent) {
  if (!drag.value) return
  dropColumn.value = columnId
  if (drag.value.kind === 'column') {
    dropStatus.value = null
    updateDropSide(event)
    return
  }
  dropStatus.value = key
}

/** Move a column before or after another one. */
function placeColumn(movingId: string, targetId: string, side: 'before' | 'after') {
  if (movingId === targetId) return
  const moving = draft.value.find((column) => column.id === movingId)
  if (!moving) return
  const rest = draft.value.filter((column) => column.id !== movingId)
  const at = rest.findIndex((column) => column.id === targetId)
  if (at < 0) return
  rest.splice(side === 'after' ? at + 1 : at, 0, moving)
  draft.value = rest
}

function onColumnDrop(id: string) {
  const state = drag.value
  const side = dropSide.value
  endDrag()
  if (state?.kind === 'status') moveStatus(state.key, id)
  else if (state?.kind === 'column') placeColumn(state.id, id, side)
}

/** Dropping a status on another status places it just before that one. */
function onStatusDrop(columnId: string, beforeKey: string) {
  const state = drag.value
  const side = dropSide.value
  endDrag()
  if (state?.kind === 'column') {
    placeColumn(state.id, columnId, side)
    return
  }
  if (state?.kind !== 'status' || state.key === beforeKey) return
  detach(state.key)
  const target = draft.value.find((column) => column.id === columnId)
  if (target) {
    const at = target.statuses.indexOf(beforeKey)
    target.statuses.splice(at < 0 ? target.statuses.length : at, 0, state.key)
  }
  prune()
}

function onNewColumnDrop() {
  const state = drag.value
  endDrag()
  if (state?.kind !== 'status') return
  detach(state.key)
  draft.value.push(toDraft({ statuses: [state.key] }))
  prune()
}

function resetDraft() {
  draft.value = props.statuses.map((status) => toDraft({ statuses: [status.key] }))
  hidden.value = new Set()
}

function positive(value: number | '' | undefined): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) && value > 0 ? Math.floor(value) : undefined
}

function apply() {
  emit('apply', {
    columns: draft.value.map((column) => ({
      statuses: [...column.statuses],
      name: column.name.trim() || undefined,
      wipLimit: positive(column.wipLimit),
      maxAgeDays: positive(column.maxAgeDays),
      maxVisible: positive(column.maxVisible),
    })),
    hidden: Array.from(hidden.value),
  })
}
</script>

<style scoped>
.board-columns-dialog {
  display: flex;
  flex-direction: column;
  gap: var(--space-3, 0.75rem);
  max-height: min(84vh, 780px);
  min-width: 0;
}

.board-columns-dialog__header h2 {
  margin: 0 0 4px;
  font-size: var(--text-lg, 1.125rem);
}

.board-columns-dialog__header p {
  margin: 0;
  font-size: var(--text-sm, 0.875rem);
}

/* Columns side by side, like the board. */
.board-columns-dialog__board {
  display: flex;
  align-items: stretch;
  gap: var(--space-2, 0.5rem);
  overflow: auto;
  padding: 2px 2px var(--space-2, 0.5rem);
  min-height: 0;
}

.board-columns-dialog__column {
  flex: 1 0 180px;
  max-width: 240px;
}

.board-columns-dialog__column,
.board-columns-dialog__new-column {
  border-radius: var(--radius-md, 8px);
}

.board-columns-dialog__column {
  border: 1px solid var(--border);
  background: var(--bg);
  display: flex;
  flex-direction: column;
  position: relative;
}

.board-columns-dialog__column.is-hidden { opacity: 0.65; }
.board-columns-dialog__column.is-dragging { opacity: 0.4; }
.board-columns-dialog__column.is-status-target,
.board-columns-dialog__new-column.is-status-target {
  border-color: var(--color-accent);
  box-shadow: 0 0 0 2px color-mix(in oklab, var(--color-accent) 25%, transparent);
}
.board-columns-dialog__column.is-drop-before { box-shadow: -3px 0 0 0 var(--color-accent); }
.board-columns-dialog__column.is-drop-after { box-shadow: 3px 0 0 0 var(--color-accent); }

.board-columns-dialog__column-head {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 6px 6px 6px 4px;
  border-bottom: 1px solid var(--border);
  cursor: grab;
  min-height: 40px;
}

.board-columns-dialog__column-head:active { cursor: grabbing; }

.board-columns-dialog__grip,
.board-columns-dialog__icon-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  box-sizing: border-box;
  flex: 0 0 auto;
  width: 28px;
  height: 28px;
  border: none;
  border-radius: var(--radius-sm, 6px);
  background: transparent;
  color: var(--muted);
  cursor: pointer;
  padding: 0;
  font: inherit;
  list-style: none;
}

.board-columns-dialog__grip { cursor: grab; }

.board-columns-dialog__grip:hover,
.board-columns-dialog__icon-button:hover,
.board-columns-dialog__grip:focus-visible,
.board-columns-dialog__icon-button:focus-visible {
  background: color-mix(in oklab, var(--fg) 8%, transparent);
  color: var(--fg);
}

.board-columns-dialog__icon-button::-webkit-details-marker { display: none; }

.board-columns-dialog__title {
  flex: 1 1 auto;
  min-width: 0;
  margin: 0;
  font-size: var(--text-sm, 0.875rem);
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.board-columns-dialog__rename {
  flex: 1 1 auto;
  min-width: 0;
  height: 28px;
  min-height: 0;
  padding: 0 6px;
  font-size: var(--text-sm, 0.875rem);
}

.board-columns-dialog__statuses {
  list-style: none;
  margin: 0;
  padding: 6px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  flex: 1 1 auto;
  min-height: 48px;
}

.board-columns-dialog__status {
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 2px 2px 2px 4px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm, 6px);
  background: var(--bg);
  cursor: grab;
}

.board-columns-dialog__status.is-drop-before { box-shadow: 0 -2px 0 0 var(--color-accent); }

.board-columns-dialog__status.is-hidden .board-columns-dialog__label {
  color: var(--muted);
  text-decoration: line-through;
}

.board-columns-dialog__status-grip {
  color: var(--muted);
  padding: 0 2px;
}

.board-columns-dialog__label {
  flex: 1 1 auto;
  min-width: 0;
  font-weight: 600;
  font-size: var(--text-sm, 0.875rem);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.board-columns-dialog__menu { position: relative; }

.board-columns-dialog__menu-list {
  position: fixed;
  z-index: var(--z-popover, 20);
  display: flex;
  flex-direction: column;
  min-width: 200px;
  max-width: 280px;
  padding: 4px;
  gap: 2px;
}

.board-columns-dialog__menu-item {
  text-align: left;
  border: none;
  background: transparent;
  color: inherit;
  padding: 6px 8px;
  border-radius: var(--radius-sm, 6px);
  cursor: pointer;
  font: inherit;
  font-size: var(--text-sm, 0.875rem);
}

.board-columns-dialog__menu-item:hover,
.board-columns-dialog__menu-item:focus-visible { background: color-mix(in oklab, var(--fg) 8%, transparent); }

.board-columns-dialog__settings {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 6px 8px 8px;
  border-top: 1px solid var(--border);
}

.board-columns-dialog__settings label {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 6px;
  font-size: var(--text-xs, 0.75rem);
  color: var(--muted);
}

.board-columns-dialog__settings input {
  width: 64px;
  height: 26px;
  min-height: 0;
  padding: 0 6px;
}

.board-columns-dialog__new-column-hint { font-size: var(--text-xs, 0.75rem); }

.board-columns-dialog__new-column {
  flex: 0 0 104px;
  border: 1px dashed var(--border);
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: var(--space-3, 0.75rem) 6px;
  text-align: center;
  font-size: var(--text-sm, 0.875rem);
  min-height: 160px;
}

.board-columns-dialog__hint {
  margin: 0;
  font-size: var(--text-xs, 0.75rem);
}

.board-columns-dialog__footer {
  display: flex;
  align-items: center;
  gap: var(--space-2, 0.5rem);
}

.board-columns-dialog__spacer { flex: 1 1 auto; }
</style>

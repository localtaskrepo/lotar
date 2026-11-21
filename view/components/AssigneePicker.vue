<template>
  <div ref="root" class="assignee-picker" :class="{ 'assignee-picker--open': open }">
    <button
      :id="id"
      ref="trigger"
      type="button"
      class="input assignee-picker__trigger"
      aria-haspopup="listbox"
      :aria-expanded="open"
      :aria-controls="open ? listId : undefined"
      :aria-label="ariaLabel ? `${ariaLabel}: ${current.label}` : undefined"
      :data-value="modelValue"
      :disabled="disabled"
      @click="toggle"
      @keydown="onTriggerKey"
    >
      <span class="assignee-picker__glyph" :data-kind="current.kind" aria-hidden="true">
        <IconGlyph v-if="glyphIcon(current.kind)" :name="glyphIcon(current.kind)!" />
        <template v-else>{{ initial(current.label) }}</template>
      </span>
      <span class="assignee-picker__text">
        <span class="assignee-picker__label" :class="{ 'assignee-picker__label--empty': !modelValue }">{{
          current.label
        }}</span>
        <span v-if="current.detail" class="assignee-picker__detail">{{ current.detail }}</span>
      </span>
      <IconGlyph class="assignee-picker__chevron" name="chevron-down" />
    </button>

    <div v-if="open && sheet" class="assignee-picker__backdrop" @click="close(true)"></div>
    <div
      v-if="open"
      ref="popover"
      class="assignee-picker__popover"
      :class="{ 'assignee-picker__popover--sheet': sheet }"
      :style="popoverStyle"
      data-testid="assignee-picker-popover"
    >
      <div class="assignee-picker__search">
        <IconGlyph name="search" class="assignee-picker__search-icon" />
        <input
          ref="search"
          v-model="query"
          class="input assignee-picker__search-input"
          type="text"
          role="combobox"
          aria-autocomplete="list"
          aria-expanded="true"
          :aria-controls="listId"
          :aria-activedescendant="activeKey ? optionId(activeKey) : undefined"
          :aria-label="`Search ${ariaLabel || 'people and agents'}`"
          :placeholder="searchPlaceholder"
          autocomplete="off"
          spellcheck="false"
          data-testid="assignee-picker-search"
          @keydown="onSearchKey"
        />
      </div>
      <div
        :id="listId"
        ref="list"
        class="assignee-picker__list"
        role="listbox"
        :aria-label="ariaLabel || 'People and agents'"
      >
        <p v-if="!options.length" class="assignee-picker__empty">No match</p>
        <template v-for="(group, groupIndex) in groups" :key="group.id">
          <div
            v-if="group.section && group.section !== groups[groupIndex - 1]?.section"
            class="assignee-picker__section"
            role="presentation"
          >
            {{ group.section }}
          </div>
          <div role="group" :aria-label="group.label || undefined" class="assignee-picker__group">
            <div
              v-if="group.label"
              class="assignee-picker__group-label"
              :class="{ 'assignee-picker__group-label--role': group.section }"
              role="presentation"
            >
              {{ group.label }}
            </div>
            <div
              v-for="option in group.options"
              :id="optionId(option.key)"
              :key="option.key"
              role="option"
              class="assignee-picker__option"
              :class="{
                'assignee-picker__option--active': option.key === activeKey,
                'assignee-picker__option--role': group.section,
              }"
              :aria-selected="option.kind !== 'custom' && option.value === modelValue"
              :data-value="option.value"
              :data-kind="option.kind"
              @mousedown.prevent
              @mousemove="activeKey = option.key"
              @click="choose(option)"
            >
              <span class="assignee-picker__glyph" :data-kind="option.kind" aria-hidden="true">
                <IconGlyph v-if="glyphIcon(option.kind)" :name="glyphIcon(option.kind)!" />
                <template v-else>{{ initial(option.label) }}</template>
              </span>
              <span class="assignee-picker__text">
                <span class="assignee-picker__line">
                  <span class="assignee-picker__label">{{ option.label }}</span>
                  <span v-if="option.detail" class="assignee-picker__detail">{{ option.detail }}</span>
                </span>
                <span v-if="option.description" class="assignee-picker__description">{{ option.description }}</span>
              </span>
              <IconGlyph
                v-if="option.kind !== 'custom' && option.value === modelValue"
                class="assignee-picker__check"
                name="check"
              />
            </div>
          </div>
        </template>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onUnmounted, ref, watch, type CSSProperties } from 'vue'
import type { AgentProfileInfo } from '../api/types'
import {
  buildAssigneeGroups,
  readRecentAssignees,
  rememberRecentAssignee,
  type AssigneeOption,
  type AssigneeOptionKind,
  type AssigneeSpecial,
} from '../utils/assigneeOptions'
import IconGlyph from './IconGlyph.vue'

const model = defineModel<string>({ required: true })

const props = withDefaults(
  defineProps<{
    id?: string
    ariaLabel?: string
    people?: string[]
    profiles?: AgentProfileInfo[]
    whoami?: string | null
    specials?: AssigneeSpecial[]
    /** Label of the empty value; leave unset where the field can't be empty. */
    emptyLabel?: string
    /** Shown on the button while the value is empty and there is no empty label. */
    placeholder?: string
    /** False: agents only. */
    includePeople?: boolean
    /** Verb of the row for a typed value ("Assign to"); null: typed values aren't offered. */
    customVerb?: string | null
    searchPlaceholder?: string
    disabled?: boolean
  }>(),
  {
    id: undefined,
    ariaLabel: '',
    people: () => [],
    profiles: () => [],
    whoami: null,
    specials: () => [],
    emptyLabel: undefined,
    placeholder: 'Choose…',
    includePeople: true,
    customVerb: 'Assign to',
    searchPlaceholder: 'Search name, runner, model…',
    disabled: false,
  },
)

const emit = defineEmits<{ (e: 'picked', value: string): void }>()

const uid = `assignee-picker-${Math.random().toString(36).slice(2, 8)}`
const listId = `${uid}-list`
const SHEET_QUERY = '(max-width: 640px)'

const root = ref<HTMLElement | null>(null)
const trigger = ref<HTMLButtonElement | null>(null)
const popover = ref<HTMLElement | null>(null)
const search = ref<HTMLInputElement | null>(null)
const open = ref(false)
const sheet = ref(false)
const query = ref('')
const activeKey = ref<string | null>(null)
const recent = ref<string[]>([])
const popoverStyle = ref<CSSProperties>({})

const source = computed(() => ({
  whoami: props.whoami,
  people: props.people,
  profiles: props.profiles,
  specials: props.specials,
  recent: recent.value,
  current: model.value,
  emptyLabel: props.emptyLabel,
  includePeople: props.includePeople,
  customLabel: props.customVerb ? (typed: string) => `${props.customVerb} “${typed}”` : null,
}))

const groups = computed(() => buildAssigneeGroups(source.value, open.value ? query.value : ''))
const options = computed(() => groups.value.flatMap((group) => group.options))

// What the button shows: the option for the value, whichever group lists it.
const current = computed<Pick<AssigneeOption, 'label' | 'detail' | 'kind'>>(() => {
  const value = model.value ?? ''
  if (!value) {
    return { label: props.emptyLabel ?? props.placeholder, detail: '', kind: 'unassigned' }
  }
  const all = buildAssigneeGroups({ ...source.value, recent: [] }).flatMap((group) => group.options)
  const match = all.find((option) => option.value === value)
  if (!match) return { label: value, detail: '', kind: 'person' }
  return { label: match.label, detail: match.detail || match.description, kind: match.kind }
})

type GlyphIcon = 'agent' | 'user-remove' | 'plus'

function glyphIcon(kind: AssigneeOptionKind): GlyphIcon | null {
  if (kind === 'agent') return 'agent'
  if (kind === 'unassigned') return 'user-remove'
  if (kind === 'custom') return 'plus'
  return null
}

function initial(label: string): string {
  const letter = label.replace(/^@/, '').trim().charAt(0)
  return letter ? letter.toUpperCase() : '?'
}

function optionId(key: string): string {
  return `${uid}-${key.replace(/[^A-Za-z0-9_-]/g, (ch) => `_${ch.charCodeAt(0).toString(16)}`)}`
}

watch(options, (next) => {
  if (!open.value) return
  if (!next.some((option) => option.key === activeKey.value)) activeKey.value = next[0]?.key ?? null
})
watch(query, () => {
  activeKey.value = options.value[0]?.key ?? null
  scrollActiveIntoView()
})

function scrollActiveIntoView() {
  void nextTick(() => {
    const element = activeKey.value ? document.getElementById(optionId(activeKey.value)) : null
    element?.scrollIntoView?.({ block: 'nearest' })
  })
}

function place() {
  sheet.value = typeof window.matchMedia === 'function' && window.matchMedia(SHEET_QUERY).matches
  const rect = trigger.value?.getBoundingClientRect()
  if (sheet.value || !rect) {
    popoverStyle.value = {}
    return
  }
  // Fixed, so a scrolling dialog or panel doesn't clip it; above the button when there's no room below.
  const width = Math.min(Math.max(rect.width, 360), window.innerWidth - 16)
  const left = Math.max(8, Math.min(rect.left, window.innerWidth - width - 8))
  const below = window.innerHeight - rect.bottom - 8
  const above = rect.top - 8
  const style: CSSProperties = { left: `${left}px`, width: `${width}px` }
  if (below >= 280 || below >= above) {
    style.top = `${rect.bottom + 4}px`
    style.maxHeight = `${Math.max(160, below - 4)}px`
  } else {
    style.bottom = `${window.innerHeight - rect.top + 4}px`
    style.maxHeight = `${Math.max(160, above - 4)}px`
  }
  popoverStyle.value = style
}

function openList(initialQuery = '') {
  if (props.disabled) return
  recent.value = readRecentAssignees()
  query.value = initialQuery
  open.value = true
  place()
  const value = model.value ?? ''
  const selected = options.value.find(
    (option) => option.value === value && option.kind !== 'custom' && !option.key.startsWith('recent:'),
  )
  activeKey.value = (initialQuery ? options.value[0] : (selected ?? options.value[0]))?.key ?? null
  void nextTick(() => {
    search.value?.focus()
    scrollActiveIntoView()
  })
}

function close(focusTrigger: boolean) {
  open.value = false
  query.value = ''
  if (focusTrigger) void nextTick(() => trigger.value?.focus())
}

function toggle() {
  if (open.value) close(false)
  else openList()
}

function choose(option: AssigneeOption) {
  const value = option.value
  if (value) recent.value = rememberRecentAssignee(value)
  model.value = value
  emit('picked', value)
  close(true)
}

function onTriggerKey(event: KeyboardEvent) {
  if (['ArrowDown', 'ArrowUp', 'Enter', ' '].includes(event.key)) {
    event.preventDefault()
    openList()
  } else if (event.key.length === 1 && /\S/.test(event.key) && !event.metaKey && !event.ctrlKey && !event.altKey) {
    // Typing on the button starts a search.
    event.preventDefault()
    openList(event.key)
  }
}

function move(step: number | 'first' | 'last') {
  const list = options.value
  if (!list.length) return
  const index = list.findIndex((option) => option.key === activeKey.value)
  let next: number
  if (step === 'first') next = 0
  else if (step === 'last') next = list.length - 1
  else next = index === -1 ? 0 : Math.max(0, Math.min(list.length - 1, index + step))
  activeKey.value = list[next]?.key ?? null
  scrollActiveIntoView()
}

function onSearchKey(event: KeyboardEvent) {
  switch (event.key) {
    case 'ArrowDown':
      move(1)
      break
    case 'ArrowUp':
      move(-1)
      break
    case 'PageDown':
      move(8)
      break
    case 'PageUp':
      move(-8)
      break
    case 'Home':
    case 'End':
      // With text typed they move the caret.
      if (query.value) return
      move(event.key === 'Home' ? 'first' : 'last')
      break
    case 'Enter': {
      const option = options.value.find((entry) => entry.key === activeKey.value)
      if (option) choose(option)
      break
    }
    case 'Escape':
      // Used up here, so it doesn't also close the dialog or panel around it.
      event.stopPropagation()
      close(true)
      break
    case 'Tab':
      close(false)
      return
    default:
      return
  }
  event.preventDefault()
}

function onDocumentPointer(event: PointerEvent) {
  if (open.value && root.value && !root.value.contains(event.target as Node)) close(false)
}
function onViewportChange(event: Event) {
  // Scrolling inside the list itself doesn't move the button.
  if (event.type === 'scroll' && popover.value?.contains(event.target as Node)) return
  place()
}
function listen(on: boolean) {
  const method = on ? 'addEventListener' : 'removeEventListener'
  document[method]('pointerdown', onDocumentPointer as EventListener, true)
  window[method]('resize', onViewportChange)
  window[method]('scroll', onViewportChange, true)
}
watch(open, (isOpen) => listen(isOpen))
onUnmounted(() => listen(false))

defineExpose({ open: openList, close, focus: () => trigger.value?.focus() })
</script>

<style scoped>
.assignee-picker {
  position: relative;
  min-width: 0;
}

.assignee-picker__trigger {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  min-height: 32px;
  padding: 3px 8px 3px 4px;
  text-align: left;
  cursor: pointer;
}

.assignee-picker__trigger:hover:not(:disabled) {
  border-color: var(--color-border-strong);
}

.assignee-picker__trigger:disabled {
  cursor: not-allowed;
  opacity: 0.6;
}

.assignee-picker__glyph {
  display: inline-flex;
  flex: 0 0 auto;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border-radius: 50%;
  font-size: 11px;
  font-weight: 700;
  color: var(--color-accent-strong);
  background: color-mix(in oklab, var(--color-accent) 16%, transparent);
}

.assignee-picker__glyph[data-kind='agent'] {
  border-radius: var(--radius-md);
  color: var(--color-fg);
  background: color-mix(in oklab, var(--color-fg) 10%, transparent);
}

.assignee-picker__glyph[data-kind='unassigned'],
.assignee-picker__glyph[data-kind='custom'],
.assignee-picker__glyph[data-kind='special'] {
  color: var(--color-muted);
  background: transparent;
  border: 1px dashed var(--color-border-strong);
}

.assignee-picker__glyph :deep(svg) {
  width: 14px;
  height: 14px;
}

.assignee-picker__text {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-width: 0;
}

.assignee-picker__trigger .assignee-picker__text {
  flex-direction: row;
  align-items: baseline;
  gap: 8px;
}

.assignee-picker__line {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  column-gap: 8px;
  min-width: 0;
}

/* Runner, model and effort tell variants apart: they wrap below the name rather than lose the effort. */
.assignee-picker__line .assignee-picker__detail {
  max-width: 100%;
}

.assignee-picker__label {
  flex: 0 0 auto;
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-weight: 600;
}

.assignee-picker__label--empty {
  font-weight: 400;
  color: var(--color-muted);
}

.assignee-picker__detail,
.assignee-picker__description {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: var(--text-xs);
  color: var(--color-muted);
}

.assignee-picker__chevron {
  flex-shrink: 0;
  margin-left: auto;
  color: var(--color-muted);
  transition: transform var(--duration-fast) var(--ease-standard);
}

.assignee-picker--open .assignee-picker__chevron {
  transform: rotate(180deg);
}

.assignee-picker__popover {
  position: fixed;
  z-index: var(--z-popover);
  display: flex;
  flex-direction: column;
  min-height: 0;
  max-height: 420px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-popover);
  background: var(--color-surface-contrast);
  box-shadow: var(--shadow-popover);
  color: var(--color-fg);
  overflow: hidden;
}

.assignee-picker__popover--sheet {
  inset: auto 0 0 0;
  max-height: 75vh;
  max-height: 75dvh;
  border-radius: var(--radius-popover) var(--radius-popover) 0 0;
  padding-bottom: env(safe-area-inset-bottom);
}

.assignee-picker__backdrop {
  position: fixed;
  inset: 0;
  z-index: var(--z-popover);
  background: var(--color-dialog-overlay);
}

.assignee-picker__search {
  position: relative;
  flex: 0 0 auto;
  padding: 8px;
  border-bottom: 1px solid var(--color-border);
}

.assignee-picker__search-icon {
  position: absolute;
  top: 50%;
  left: 18px;
  width: 14px;
  height: 14px;
  transform: translateY(-50%);
  color: var(--color-muted);
  pointer-events: none;
}

.assignee-picker__search-input {
  box-sizing: border-box;
  width: 100%;
  padding-left: 30px;
}

.assignee-picker__list {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  overscroll-behavior: contain;
  padding: 4px 6px 6px;
}

.assignee-picker__empty {
  margin: 0;
  padding: 12px 8px;
  color: var(--color-muted);
  font-size: var(--text-sm);
}

.assignee-picker__section,
.assignee-picker__group-label {
  padding: 8px 8px 2px;
  font-size: var(--text-xs);
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--color-muted);
}

.assignee-picker__group-label--role {
  padding-top: 6px;
  padding-left: 12px;
  letter-spacing: 0.02em;
  text-transform: none;
  font-family: var(--font-mono, ui-monospace, monospace);
}

.assignee-picker__option {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 5px 8px;
  border-radius: var(--radius-md);
  cursor: pointer;
}

.assignee-picker__option--role {
  padding-left: 16px;
}

.assignee-picker__option--active {
  background: color-mix(in oklab, var(--color-accent) 12%, transparent);
}

.assignee-picker__check {
  flex-shrink: 0;
  width: 14px;
  height: 14px;
  color: var(--color-accent-strong);
}

@media (max-width: 640px) {
  .assignee-picker__option {
    min-height: 40px;
  }

  .assignee-picker__search-input {
    font-size: 16px;
  }
}
</style>

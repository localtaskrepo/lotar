<template>
  <UiModal :open="open" aria-label="Command palette" size="md" initial-focus="[data-palette-input]" @close="close">
    <div class="palette" data-testid="command-palette">
      <input
        ref="input"
        v-model="query"
        data-palette-input
        data-testid="command-palette-input"
        class="input palette__input"
        type="text"
        role="combobox"
        aria-label="Search commands, pages and tasks"
        aria-autocomplete="list"
        :aria-expanded="items.length > 0"
        :aria-controls="listId"
        :aria-activedescendant="activeItem ? optionId(activeItem) : undefined"
        placeholder="Jump to a page, run a command or find a task…"
        autocomplete="off"
        spellcheck="false"
        @keydown="onInputKeydown"
      />
      <div :id="listId" class="palette__list" role="listbox" aria-label="Results">
        <div v-for="group in groups" :key="group.name" role="group" :aria-labelledby="groupId(group.name)">
          <div :id="groupId(group.name)" class="palette__group" role="presentation">{{ group.name }}</div>
          <div
            v-for="item in group.items"
            :id="optionId(item)"
            :key="item.id"
            class="palette__option"
            :class="{ 'palette__option--active': item === activeItem }"
            role="option"
            :aria-selected="item === activeItem"
            :data-testid="`command-palette-option-${item.id}`"
            @mousemove="activeIndex = items.indexOf(item)"
            @click="run(item)"
          >
            <span class="palette__label">
              <span v-if="item.detail" class="palette__detail">{{ item.detail }}</span>
              {{ item.label }}
            </span>
            <kbd v-if="item.shortcut" class="palette__shortcut">{{ item.shortcut }}</kbd>
          </div>
        </div>
        <p v-if="!items.length" class="palette__empty muted" role="status">
          {{ searchingTasks ? 'Searching tasks…' : 'Nothing matches.' }}
        </p>
      </div>
      <p class="palette__footer muted">
        <span><kbd>↑</kbd> <kbd>↓</kbd> to move</span>
        <span><kbd>Enter</kbd> to run</span>
        <span><kbd>Esc</kbd> to close</span>
      </p>
    </div>
  </UiModal>
</template>

<script setup lang="ts">
import { computed, ref, useId, watch } from 'vue'
import { api } from '../api/client'
import { commandMatchRank, PALETTE_GROUPS } from '../utils/keyboardShortcuts'
import UiModal from './UiModal.vue'

export interface PaletteCommand {
  id: string
  group: (typeof PALETTE_GROUPS)[number]
  /** Only listed while the search is empty (recent tasks). */
  emptyOnly?: boolean
  /** Shown before the label in a muted monospace style, such as a task ID. */
  detail?: string
  label: string
  /** Extra words that should find this command. */
  keywords?: string
  shortcut?: string
  run: () => void
}

interface PaletteItem {
  id: string
  group: string
  label: string
  detail?: string
  shortcut?: string
  run: () => void
}

const props = defineProps<{ open: boolean; commands: PaletteCommand[] }>()
const emit = defineEmits<{ (e: 'close'): void; (e: 'open-task', id: string): void }>()

const TASK_LIMIT = 8
const TASK_DEBOUNCE_MS = 120

const input = ref<HTMLInputElement | null>(null)
const query = ref('')
const activeIndex = ref(0)
const tasks = ref<Array<{ id: string; title: string }>>([])
const searchingTasks = ref(false)
const listId = useId()
const idPrefix = useId()
let taskTimer: ReturnType<typeof setTimeout> | null = null
let taskRequest = 0

const matchedCommands = computed<PaletteItem[]>(() =>
  props.commands
    .filter((command) => !command.emptyOnly || !query.value.trim())
    .map((command) => ({
      command,
      rank: commandMatchRank(query.value, command.label, `${command.detail ?? ''} ${command.keywords ?? ''}`),
    }))
    .filter((entry): entry is { command: PaletteCommand; rank: number } => entry.rank !== null)
    .sort((a, b) => a.rank - b.rank)
    .map(({ command }) => ({ ...command })),
)

const taskItems = computed<PaletteItem[]>(() =>
  tasks.value.map((task) => ({
    id: `task-${task.id}`,
    group: 'Tasks',
    label: task.title,
    detail: task.id,
    run: () => emit('open-task', task.id),
  })),
)

/** Commands first, unless the query looks like a task ID. */
const items = computed<PaletteItem[]>(() => {
  const looksLikeId = /^[A-Za-z][\w-]*-\d+$|^\d+$/.test(query.value.trim())
  const commands = PALETTE_GROUPS.flatMap((group) => matchedCommands.value.filter((item) => item.group === group))
  return looksLikeId ? [...taskItems.value, ...commands] : [...commands, ...taskItems.value]
})

const groups = computed(() => {
  const order: string[] = []
  for (const item of items.value) if (!order.includes(item.group)) order.push(item.group)
  return order.map((name) => ({ name, items: items.value.filter((item) => item.group === name) }))
})

const activeItem = computed(() => items.value[activeIndex.value] ?? null)

function optionId(item: PaletteItem) {
  return `${idPrefix}-${item.id}`
}
function groupId(name: string) {
  return `${idPrefix}-group-${name.replace(/\s+/g, '-').toLowerCase()}`
}

watch(items, () => {
  if (activeIndex.value >= items.value.length) activeIndex.value = 0
})

watch(query, (value) => {
  activeIndex.value = 0
  if (taskTimer) clearTimeout(taskTimer)
  const text = value.trim()
  if (!text) {
    tasks.value = []
    searchingTasks.value = false
    return
  }
  searchingTasks.value = true
  const request = ++taskRequest
  taskTimer = setTimeout(async () => {
    try {
      const found = await api.suggestTasks(text, undefined, TASK_LIMIT)
      if (request === taskRequest) tasks.value = Array.isArray(found) ? found : []
    } catch {
      if (request === taskRequest) tasks.value = []
    } finally {
      if (request === taskRequest) searchingTasks.value = false
    }
  }, TASK_DEBOUNCE_MS)
})

watch(
  () => props.open,
  (open) => {
    if (open) return
    query.value = ''
    tasks.value = []
    activeIndex.value = 0
    taskRequest += 1
    if (taskTimer) clearTimeout(taskTimer)
  },
)

function close() {
  emit('close')
}

function run(item: PaletteItem) {
  close()
  item.run()
}

function move(step: number) {
  const count = items.value.length
  if (!count) return
  activeIndex.value = (activeIndex.value + step + count) % count
  const element = activeItem.value ? document.getElementById(optionId(activeItem.value)) : null
  element?.scrollIntoView?.({ block: 'nearest' })
}

function onInputKeydown(event: KeyboardEvent) {
  if (event.key === 'ArrowDown') {
    event.preventDefault()
    move(1)
  } else if (event.key === 'ArrowUp') {
    event.preventDefault()
    move(-1)
  } else if (event.key === 'Home' && items.value.length) {
    event.preventDefault()
    activeIndex.value = 0
  } else if (event.key === 'End' && items.value.length) {
    event.preventDefault()
    activeIndex.value = items.value.length - 1
  } else if (event.key === 'Enter' && !event.isComposing) {
    event.preventDefault()
    if (activeItem.value) run(activeItem.value)
  }
}
</script>

<style scoped>
.palette {
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 0.5rem);
}

.palette__input {
  width: 100%;
  box-sizing: border-box;
  font-size: var(--text-md, 1rem);
}

.palette__list {
  max-height: min(60vh, 420px);
  overflow-y: auto;
  overscroll-behavior: contain;
}

.palette__group {
  padding: var(--space-2, 0.5rem) var(--space-2, 0.5rem) var(--space-1, 0.25rem);
  color: var(--color-muted);
  font-size: var(--text-xs, 0.75rem);
  font-weight: 600;
  letter-spacing: 0.02em;
  text-transform: uppercase;
}

.palette__option {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3, 0.75rem);
  padding: var(--space-2, 0.5rem);
  border-radius: var(--radius-md, 6px);
  cursor: pointer;
}

.palette__option--active {
  background: color-mix(in oklab, var(--color-accent) 14%, transparent);
}

.palette__label {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.palette__detail {
  margin-right: var(--space-2, 0.5rem);
  color: var(--color-muted);
  font-family: var(--font-mono);
  font-size: var(--text-sm, 0.875rem);
}

.palette__empty {
  margin: 0;
  padding: var(--space-3, 0.75rem) var(--space-2, 0.5rem);
}

.palette__footer {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3, 0.75rem);
  margin: 0;
  padding-top: var(--space-2, 0.5rem);
  border-top: 1px solid var(--color-border);
  font-size: var(--text-xs, 0.75rem);
}
</style>

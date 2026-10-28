<template>
  <div class="field field--full tag-colors" data-testid="config-tag-colors">
    <div class="tag-colors__header">
      <span class="field-label">
        <span>Tag colours</span>
        <span v-if="source" :class="['provenance', provenanceClass(source)]">{{ provenanceLabel(source) }}</span>
      </span>
      <UiButton variant="ghost" type="button" data-testid="tag-colors-add" @click="emit('add')">
        <IconGlyph name="plus" aria-hidden="true" />
        <span>Add colour</span>
      </UiButton>
    </div>

    <p v-if="!entries.length" class="muted tag-colors__empty">No tag colours</p>
    <ul v-else class="tag-colors__rows">
      <li v-for="(entry, index) in entries" :key="`tag-color-${index}`" class="tag-colors__row">
        <UiInput
          v-model="entries[index]!.tag"
          class="tag-colors__name"
          placeholder="Tag"
          :list="listId"
          :aria-label="`Tag ${index + 1}`"
          @blur="emit('validate')"
        />
        <input
          type="color"
          class="tag-colors__picker"
          :value="pickerValue(entry.color)"
          :aria-label="`Colour of ${entry.tag || `tag ${index + 1}`}`"
          @input="setColor(index, ($event.target as HTMLInputElement).value)"
        />
        <UiSelect
          class="tag-colors__named"
          :model-value="namedValue(entry.color)"
          :aria-label="`Named colour of ${entry.tag || `tag ${index + 1}`}`"
          @update:model-value="(value: string) => value && setColor(index, value)"
        >
          <option value="">Custom</option>
          <option v-for="name in colorNames" :key="name" :value="name">{{ label(name) }}</option>
        </UiSelect>
        <span class="tag-colors__preview">
          <span class="tag" v-bind="tagChipAttrs(resolveTagColor(entry.color))">{{ entry.tag.trim() || 'tag' }}</span>
        </span>
        <UiButton
          icon-only
          variant="ghost"
          type="button"
          :aria-label="`Remove colour of ${entry.tag || `tag ${index + 1}`}`"
          title="Remove colour"
          @click="emit('remove', index)"
        >
          <IconGlyph name="close" />
        </UiButton>
      </li>
    </ul>
    <div v-if="uncoloured.length" class="tag-colors__in-use" data-testid="tag-colors-in-use">
      <span class="muted">Tags in use, click one to set its colour:</span>
      <button
        v-for="item in uncoloured"
        :key="item.tag"
        type="button"
        class="tag tag--interactive"
        v-bind="tagChipAttrs(item.color)"
        :title="`Set a colour for ${item.tag}`"
        @click="emit('add', item.tag, item.color)"
      >
        {{ item.tag }}
      </button>
    </div>
    <datalist :id="listId">
      <option v-for="tag in suggestions" :key="tag" :value="tag" />
    </datalist>
    <p class="field-hint">
      {{
        isGlobal
          ? 'Tagged tasks show these colours everywhere. Projects can override them per tag.'
          : 'Overrides the global colour of these tags in this project; other tags keep theirs.'
      }}
    </p>
    <p v-if="inherited.length" class="field-hint tag-colors__inherited">
      <span>Global colours:</span>
      <span v-for="item in inherited" :key="item.tag" class="tag" v-bind="tagChipAttrs(item.color)">{{
        item.tag
      }}</span>
    </p>
    <p v-if="error" class="field-error">{{ error }}</p>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, useId, watch } from 'vue'
import { api } from '../api/client'
import type { ConfigSource, TagProperties } from '../api/types'
import type { TagColorEntry } from '../composables/useConfigForm'
import { autoTagColor, resolveTagColor, TAG_COLOR_NAMES, tagChipAttrs, tagColorOf } from '../utils/tagColors'
import IconGlyph from './IconGlyph.vue'
import UiButton from './UiButton.vue'
import UiInput from './UiInput.vue'
import UiSelect from './UiSelect.vue'

/** Editor for tag colours (DEV-20), stored as tag properties in the config. */
const entries = defineModel<TagColorEntry[]>('entries', { required: true })

const {
  isGlobal,
  project = '',
  suggestions = [],
  inheritedProperties = null,
  error = null,
  source,
  provenanceLabel,
  provenanceClass,
} = defineProps<{
  isGlobal: boolean
  /** Project prefix of the scope; empty for global (every project). */
  project?: string
  /** Known tags offered while typing a name. */
  suggestions?: string[]
  /** Project scope: the global tag properties, shown for reference. */
  inheritedProperties?: Record<string, TagProperties> | null
  error?: string | null
  source?: ConfigSource
  provenanceLabel: (source: ConfigSource | undefined) => string
  provenanceClass: (source: ConfigSource | undefined) => string
}>()

const emit = defineEmits<{
  (e: 'add', tag?: string, color?: string): void
  (e: 'remove', index: number): void
  (e: 'validate'): void
}>()

const listId = `${useId()}-tags`
// `grey` is an alias of `gray`; list each colour once.
const colorNames = Object.keys(TAG_COLOR_NAMES).filter((name) => name !== 'grey')

const inherited = computed(() =>
  isGlobal || !inheritedProperties
    ? []
    : Object.keys(inheritedProperties)
        .map((tag) => ({ tag, color: tagColorOf(inheritedProperties, tag) }))
        .filter((item): item is { tag: string; color: string } => !!item.color)
        .sort((a, b) => a.tag.localeCompare(b.tag)),
)

// Tags on tasks in this scope (DEV-229), so colours can be set for tags that
// are in use but not configured. One request; failures just hide the list.
// Only the newest 1000 tasks are read; a server-side tag listing is a DEV-4
// follow-up.
const tagsInUse = ref<string[]>([])
let tagsRequest = 0
watch(
  () => project,
  async (scope) => {
    const request = ++tagsRequest
    try {
      const response = await api.listTasks({ project: scope || undefined, has: 'tag', limit: 1000 })
      // A newer scope's request superseded this one.
      if (request !== tagsRequest) return
      const seen = new Map<string, string>()
      for (const task of response.tasks ?? []) {
        for (const tag of task.tags ?? []) {
          const trimmed = tag.trim()
          if (trimmed && !seen.has(trimmed.toLowerCase())) seen.set(trimmed.toLowerCase(), trimmed)
        }
      }
      tagsInUse.value = [...seen.values()].sort((a, b) => a.localeCompare(b))
    } catch {
      if (request === tagsRequest) tagsInUse.value = []
    }
  },
  { immediate: true },
)

/** Tags in use without a row here, in the colour they show now. */
const uncoloured = computed(() => {
  const rows = new Set(entries.value.map((entry) => entry.tag.trim().toLowerCase()))
  return tagsInUse.value
    .filter((tag) => !rows.has(tag.toLowerCase()))
    .map((tag) => ({ tag, color: tagColorOf(inheritedProperties, tag) ?? autoTagColor(tag) }))
})

function label(name: string): string {
  return name.charAt(0).toUpperCase() + name.slice(1)
}

/** `<input type="color">` only takes #rrggbb. */
function pickerValue(color: string): string {
  const resolved = resolveTagColor(color) ?? '#0ea5e9'
  return resolved.length === 4 ? `#${[...resolved.slice(1)].map((c) => c + c).join('')}` : resolved
}

function namedValue(color: string): string {
  const name = color.trim().toLowerCase()
  return name === 'grey' ? 'gray' : name in TAG_COLOR_NAMES ? name : ''
}

function setColor(index: number, color: string) {
  const entry = entries.value[index]
  if (!entry) return
  entry.color = color
  emit('validate')
}
</script>

<style scoped>
.tag-colors__header {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2, 0.5rem);
}

.tag-colors__header :deep(.btn) {
  white-space: nowrap;
}

.tag-colors__empty {
  margin: 0;
}

.tag-colors__rows {
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 0.5rem);
  margin: 0;
  padding: 0;
  list-style: none;
}

.tag-colors__row {
  display: grid;
  grid-template-columns: minmax(0, 14rem) 2.5rem minmax(0, 9rem) minmax(0, 1fr) auto;
  align-items: center;
  gap: var(--space-2, 0.5rem);
}

.tag-colors__picker {
  width: 2.5rem;
  height: var(--control-height, 2.25rem);
  padding: 2px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md, 0.375rem);
  background: var(--field-background, var(--color-surface));
  cursor: pointer;
}

.tag-colors__preview {
  min-width: 0;
  overflow: hidden;
}

.tag-colors__in-use {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-1, 0.25rem);
}

.tag-colors__in-use .tag {
  font: inherit;
  font-size: var(--text-xs);
}

.tag-colors__inherited {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-1, 0.25rem);
}

/* Phones: the name on its own line, the colour controls below, then the preview. */
@media (max-width: 560px) {
  .tag-colors__row {
    grid-template-columns: 2.5rem minmax(0, 1fr) auto;
    padding-bottom: var(--space-2, 0.5rem);
    border-bottom: 1px solid var(--color-border);
  }

  .tag-colors__name {
    grid-column: 1 / -1;
  }

  .tag-colors__preview {
    grid-column: 1 / -1;
    grid-row: 3;
  }
}
</style>

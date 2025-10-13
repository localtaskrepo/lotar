<template>
  <div class="task-panel__tags-section">
    <label class="task-panel__tags-label" for="task-panel-tags-input">Tags</label>
    <div class="task-panel__tag-input-wrapper">
      <div class="task-panel__tag-input">
        <UiInput
          id="task-panel-tags-input"
          v-model="tagsInput"
          placeholder="Add tag"
          @input="onTagInputChange"
          @keydown="onTagInputKeydown"
          @focus="onTagInputFocus"
          @blur="onTagInputBlur"
        />
        <UiButton type="button" @click="commitTagInput">Add</UiButton>
      </div>
      <ul
        v-if="tagSuggestionsVisible"
        class="task-panel__tag-suggestions"
        role="listbox"
        aria-label="Tag suggestions"
      >
        <li
          v-for="(entry, suggestionIndex) in tagSuggestionEntries"
          :key="entry.value"
          class="task-panel__tag-suggestions-item"
        >
          <button
            type="button"
            :class="['task-panel__tag-suggestion', { active: tagActiveIndex === suggestionIndex }]"
            role="option"
            :aria-selected="tagActiveIndex === suggestionIndex"
            @mousedown.prevent
            @click.prevent="selectTag(entry.value)"
            @mouseenter="tagActiveIndex = suggestionIndex"
            @focus="tagActiveIndex = suggestionIndex"
          >
            <span class="task-panel__tag-suggestion-label">
              <span
                v-for="(part, partIndex) in entry.parts"
                :key="partIndex"
                :class="['task-panel__tag-suggestion-part', { 'task-panel__tag-suggestion-part--match': part.match }]"
              >
                {{ part.text }}
              </span>
            </span>
          </button>
        </li>
      </ul>
    </div>
    <p v-if="tagSuggestionPrompt" class="task-panel__tag-info">{{ tagSuggestionPrompt }}</p>
    <p v-if="tagHint" class="task-panel__tag-hint">{{ tagHint }}</p>
    <div class="task-panel__tags">
      <span v-for="tag in tags" :key="tag" class="chip">
        {{ tag }}
        <button type="button" class="chip__close" @click="removeTag(tag)">×</button>
      </span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import UiButton from '../UiButton.vue';
import UiInput from '../UiInput.vue';

const props = defineProps<{
  tags: string[]
  configuredTags: string[]
  knownTags: string[]
  allowCustomTags: boolean
}>()

const emit = defineEmits<{
  (e: 'update:tags', value: string[]): void
  (e: 'discoveredTags', value: string[]): void
}>()

const tagsInput = ref('')
const tagHint = ref('')
const tagInputFocused = ref(false)
const tagActiveIndex = ref(-1)
let tagBlurTimer: ReturnType<typeof setTimeout> | null = null
const TAG_SUGGESTION_LIMIT = 8

const normalizedConfigured = computed(() => uniqueNormalizedTags(props.configuredTags))
const normalizedKnown = computed(() => uniqueNormalizedTags(props.knownTags))

const tagCandidates = computed(() =>
  uniqueNormalizedTags([
    ...normalizedConfigured.value,
    ...normalizedKnown.value,
  ]).sort((a, b) => a.localeCompare(b)),
)

const availableTags = computed(() => {
  const lowerExisting = new Set(props.tags.map((tag) => tag.toLowerCase()))
  return tagCandidates.value.filter((tag) => !lowerExisting.has(tag.toLowerCase()))
})

const tagSuggestionList = computed(() => {
  const base = availableTags.value
  if (!base.length) return [] as string[]
  const query = tagsInput.value.trim().toLowerCase()
  if (!tagInputFocused.value && !query) {
    return []
  }
  if (!query) {
    return base.slice(0, TAG_SUGGESTION_LIMIT)
  }
  return base.filter((tag) => tag.toLowerCase().includes(query)).slice(0, TAG_SUGGESTION_LIMIT)
})

const tagSuggestionsVisible = computed(() => tagInputFocused.value && tagSuggestionList.value.length > 0)

const tagSuggestionEntries = computed(() =>
  tagSuggestionList.value.map((tag) => ({
    value: tag,
    parts: highlightTagSuggestion(tag),
  })),
)

const tagSuggestionPrompt = computed(() => {
  if (!tagInputFocused.value) {
    return ''
  }
  if (tagsInput.value.trim()) {
    return ''
  }
  if (!availableTags.value.length) {
    return props.allowCustomTags
      ? 'No configured tags available. Type to add your own.'
      : 'No tags are configured for this project yet.'
  }
  return 'Start typing to filter tags'
})

watch(tagSuggestionList, (list) => {
  tagActiveIndex.value = list.length ? 0 : -1
})

function uniqueNormalizedTags(values: Iterable<string>) {
  const map = new Map<string, string>()
  for (const value of values) {
    const trimmed = value.trim()
    if (!trimmed) continue
    const key = trimmed.toLowerCase()
    if (!map.has(key)) {
      map.set(key, trimmed)
    }
  }
  return Array.from(map.values())
}

function highlightTagSuggestion(tag: string): Array<{ text: string; match: boolean }> {
  const query = tagsInput.value.trim()
  if (!query) {
    return [{ text: tag, match: false }]
  }
  const lowerTag = tag.toLowerCase()
  const lowerQuery = query.toLowerCase()
  const segments: Array<{ text: string; match: boolean }> = []
  let searchStart = 0
  let matchIndex = lowerTag.indexOf(lowerQuery)
  if (matchIndex === -1) {
    return [{ text: tag, match: false }]
  }
  while (matchIndex !== -1) {
    if (matchIndex > searchStart) {
      segments.push({ text: tag.slice(searchStart, matchIndex), match: false })
    }
    const matchEnd = matchIndex + lowerQuery.length
    segments.push({ text: tag.slice(matchIndex, matchEnd), match: true })
    searchStart = matchEnd
    matchIndex = lowerTag.indexOf(lowerQuery, searchStart)
  }
  if (searchStart < tag.length) {
    segments.push({ text: tag.slice(searchStart), match: false })
  }
  return segments
}

function onTagInputChange() {
  const value = tagsInput.value.trim()
  if (!value) {
    tagHint.value = ''
    return
  }
  if (props.allowCustomTags) {
    tagHint.value = ''
    return
  }
  const normalized = value.toLowerCase()
  if (availableTags.value.some((tag) => tag.toLowerCase() === normalized)) {
    tagHint.value = ''
    return
  }
  if (!tagSuggestionList.value.length) {
    tagHint.value = `Tag “${value}” isn’t configured. Update your project tags to allow it.`
  } else {
    tagHint.value = ''
  }
}

function onTagInputFocus() {
  if (tagBlurTimer) {
    clearTimeout(tagBlurTimer)
    tagBlurTimer = null
  }
  tagInputFocused.value = true
}

function onTagInputBlur() {
  if (tagBlurTimer) {
    clearTimeout(tagBlurTimer)
    tagBlurTimer = null
  }
  tagBlurTimer = setTimeout(() => {
    tagInputFocused.value = false
    tagActiveIndex.value = -1
    tagBlurTimer = null
  }, 120)
}

function onTagInputKeydown(event: KeyboardEvent) {
  const suggestions = tagSuggestionList.value
  if (event.key === 'ArrowDown') {
    if (!suggestions.length) return
    event.preventDefault()
    tagActiveIndex.value = suggestions.length
      ? (tagActiveIndex.value + 1 + suggestions.length) % suggestions.length
      : -1
  } else if (event.key === 'ArrowUp') {
    if (!suggestions.length) return
    event.preventDefault()
    tagActiveIndex.value = suggestions.length
      ? (tagActiveIndex.value - 1 + suggestions.length) % suggestions.length
      : -1
  } else if (event.key === 'Enter') {
    if (tagActiveIndex.value >= 0 && suggestions[tagActiveIndex.value]) {
      event.preventDefault()
      selectTag(suggestions[tagActiveIndex.value])
      return
    }
    event.preventDefault()
    commitTagInput()
  } else if (event.key === 'Tab') {
    if (tagActiveIndex.value >= 0 && suggestions[tagActiveIndex.value]) {
      selectTag(suggestions[tagActiveIndex.value])
      event.preventDefault()
    }
  } else if (event.key === 'Escape') {
    tagActiveIndex.value = -1
    tagHint.value = ''
  }
}

function addTag(tag: string): boolean {
  const normalized = tag.trim()
  if (!normalized) {
    return false
  }
  const normalizedLower = normalized.toLowerCase()
  if (props.tags.some((existing) => existing.toLowerCase() === normalizedLower)) {
    tagHint.value = ''
    return false
  }
  const candidates = availableTags.value
  const exactMatch = candidates.find((candidate) => candidate.toLowerCase() === normalizedLower)
  if (!props.allowCustomTags && !exactMatch) {
    tagHint.value = `Tag “${normalized}” isn’t configured. Update your project tags to allow it.`
    return false
  }
  const valueToAdd = exactMatch || normalized
  const next = [...props.tags, valueToAdd]
  emit('update:tags', next)
  emit('discoveredTags', [valueToAdd])
  tagHint.value = ''
  tagActiveIndex.value = tagSuggestionList.value.length ? 0 : -1
  return true
}

function commitTagInput() {
  const value = tagsInput.value.trim()
  if (!value) {
    tagHint.value = ''
    return
  }
  if (addTag(value)) {
    tagsInput.value = ''
  }
}

function selectTag(tag: string) {
  if (addTag(tag)) {
    tagsInput.value = ''
    tagActiveIndex.value = tagSuggestionList.value.length ? 0 : -1
  }
}

function removeTag(tag: string) {
  const lower = tag.toLowerCase()
  const next = props.tags.filter((existing) => existing.toLowerCase() !== lower)
  emit('update:tags', next)
  tagActiveIndex.value = tagSuggestionList.value.length ? 0 : -1
}

onBeforeUnmount(() => {
  if (tagBlurTimer) {
    clearTimeout(tagBlurTimer)
    tagBlurTimer = null
  }
})
</script>



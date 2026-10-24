<template>
  <nav v-if="pageCount > 1" class="ui-pagination" :aria-label="ariaLabel">
    <UiButton
      class="responsive-action"
      variant="ghost"
      type="button"
      aria-label="Previous page"
      title="Previous page"
      :disabled="disabled || currentPage <= 1"
      @click="select(currentPage - 1)"
    >
      <IconGlyph name="chevron-left" />
      <span class="responsive-action__label">Prev</span>
    </UiButton>
    <template v-for="(entry, index) in pageWindow" :key="typeof entry === 'number' ? `page-${entry}` : `gap-${index}`">
      <span v-if="entry === '...'" class="ui-pagination__ellipsis" aria-hidden="true">&hellip;</span>
      <UiButton
        v-else
        class="ui-pagination__page"
        :variant="entry === currentPage ? 'primary' : 'ghost'"
        type="button"
        :aria-label="`Page ${entry}`"
        :aria-current="entry === currentPage ? 'page' : undefined"
        :disabled="disabled"
        @click="select(entry)"
      >
        {{ entry }}
      </UiButton>
    </template>
    <UiButton
      class="responsive-action"
      variant="ghost"
      type="button"
      aria-label="Next page"
      title="Next page"
      :disabled="disabled || currentPage >= pageCount"
      @click="select(currentPage + 1)"
    >
      <span class="responsive-action__label">Next</span>
      <IconGlyph name="chevron-right" />
    </UiButton>
    <span class="ui-pagination__status" aria-live="polite">Page {{ currentPage }} of {{ pageCount }}</span>
  </nav>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import IconGlyph from './IconGlyph.vue'
import UiButton from './UiButton.vue'

const props = withDefaults(
  defineProps<{
    offset: number
    total: number
    pageSize: number
    disabled?: boolean
    ariaLabel: string
  }>(),
  { disabled: false },
)

const emit = defineEmits<{ (e: 'page', page: number): void }>()

const safePageSize = computed(() =>
  Number.isFinite(props.pageSize) && props.pageSize > 0 ? Math.max(1, Math.floor(props.pageSize)) : 1,
)

const pageCount = computed(() => Math.max(1, Math.ceil(Math.max(0, props.total) / safePageSize.value)))

const currentPage = computed(() => {
  const offset = Number.isFinite(props.offset) ? Math.max(0, Math.floor(props.offset)) : 0
  return Math.min(pageCount.value, Math.floor(offset / safePageSize.value) + 1)
})

// Bounded window: first, current +/- 1, last with ellipsis gaps, so even long
// lists render a predictable number of controls that can wrap at 320px.
// Short lists (<= 7 pages) render every page without ellipsis.
const pageWindow = computed<(number | '...')[]>(() => {
  const count = pageCount.value
  const current = currentPage.value
  if (count <= 7) {
    return Array.from({ length: count }, (_, index) => index + 1)
  }
  const entries: (number | '...')[] = [1]
  const start = Math.max(2, current - 1)
  const end = Math.min(count - 1, current + 1)
  if (start > 2) entries.push('...')
  for (let page = start; page <= end; page += 1) entries.push(page)
  if (end < count - 1) entries.push('...')
  entries.push(count)
  return entries
})

function select(page: number) {
  if (props.disabled) return
  if (!Number.isInteger(page)) return
  if (page < 1 || page > pageCount.value) return
  // Deep links may start between page boundaries, so the highlighted page
  // is only a no-op when its offset is already aligned.
  if (page === currentPage.value && props.offset === (page - 1) * safePageSize.value) return
  emit('page', page)
}
</script>

<style scoped>
.ui-pagination {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 4px;
  flex-wrap: wrap;
  min-width: 0;
}

.ui-pagination__ellipsis {
  padding: 0 4px;
}

.ui-pagination__page {
  min-width: var(--control-height);
  padding-inline: 8px;
}

.ui-pagination__status {
  margin-left: 8px;
  font-size: var(--text-sm, 0.875rem);
  color: var(--color-muted);
  white-space: nowrap;
}

@media (max-width: 640px) {
  .ui-pagination__status {
    flex-basis: 100%;
    margin-left: 0;
    text-align: right;
  }
}
</style>

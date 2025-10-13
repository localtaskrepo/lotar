<template>
  <div class="task-panel__tab-panel">
    <header class="task-panel__group-header">
      <h3>References</h3>
      <UiButton type="button" variant="ghost" :disabled="mode !== 'edit'" @click="$emit('reload')">
        Reload
      </UiButton>
    </header>
    <template v-if="mode === 'edit'">
      <div class="task-panel__references" role="region" aria-label="Task references">
        <p v-if="!references.length" class="muted">No references yet</p>
        <ul v-else class="task-panel__references-list">
          <li
            v-for="(reference, index) in references"
            :key="reference.code || reference.link || index"
            :class="[
              'task-panel__reference-item',
              { 'task-panel__reference-item--interactive': !!reference.code }
            ]"
            :tabindex="reference.code ? 0 : undefined"
            @mouseenter="handleReferenceEnter(reference.code, $event)"
            @mouseleave="handleReferenceLeave(reference.code)"
            @focus="handleReferenceEnter(reference.code, $event)"
            @blur="handleReferenceLeave(reference.code)"
          >
            <span class="task-panel__reference-code">{{ reference.code || '—' }}</span>
            <a
              v-if="reference.link"
              class="task-panel__reference-link"
              :href="reference.link"
              target="_blank"
              rel="noreferrer"
            >
              {{ reference.link }}
            </a>
            <span v-else class="task-panel__reference-link muted" aria-hidden="true">—</span>
          </li>
        </ul>
      </div>
      <Teleport to="body">
        <Transition name="fade">
          <div
            v-if="hoveredReferenceCode"
            class="task-panel__reference-preview"
            :style="hoveredReferenceStyle"
            role="dialog"
            aria-live="polite"
            @mouseenter="handleReferencePreviewEnter(hoveredReferenceCode)"
            @mouseleave="handleReferencePreviewLeave(hoveredReferenceCode)"
            :ref="setPreviewElement"
          >
            <div class="task-panel__reference-meta">
              <strong>{{ previewTitle }}</strong>
              <span v-if="hoveredReferenceSnippet">
                Lines {{ hoveredReferenceSnippet.start_line }}–{{ hoveredReferenceSnippet.end_line }}
              </span>
            </div>
            <p v-if="hoveredReferenceError" class="task-panel__reference-error">
              {{ hoveredReferenceError }}
            </p>
            <UiLoader v-else-if="hoveredReferenceLoading" size="sm">Loading reference…</UiLoader>
            <template v-else-if="hoveredReferenceSnippet">
              <div class="task-panel__reference-actions" v-if="hoveredReferenceCanExpand">
                <UiButton
                  v-if="hoveredReferenceCanExpandBefore"
                  variant="ghost"
                  type="button"
                  @click="expandReference('before')"
                >
                  Show earlier lines
                </UiButton>
                <UiButton
                  v-if="hoveredReferenceCanExpandAfter"
                  variant="ghost"
                  type="button"
                  @click="expandReference('after')"
                >
                  Show later lines
                </UiButton>
              </div>
              <div class="task-panel__reference-snippet">
                <div
                  v-for="line in hoveredReferenceSnippet.lines"
                  :key="line.number"
                  class="task-panel__reference-line"
                  :class="{
                    'task-panel__reference-line--highlight': isReferenceLineHighlighted(
                      hoveredReferenceCode,
                      line.number,
                    ),
                  }"
                >
                  <span class="task-panel__reference-line-number">{{ line.number }}</span>
                  <span class="task-panel__reference-line-text">{{ line.text }}</span>
                </div>
              </div>
            </template>
            <p v-else class="muted">No snippet preview available.</p>
          </div>
        </Transition>
      </Teleport>
    </template>
    <p v-else class="task-panel__empty-hint">References appear after the task is created.</p>
  </div>
</template>

<script setup lang="ts">
import { Teleport, Transition, computed, onUnmounted, ref, watch, type ComponentPublicInstance } from 'vue'
import type { ReferenceSnippet } from '../../api/types'
import UiButton from '../UiButton.vue'
import UiLoader from '../UiLoader.vue'

type ReferenceEntry = { code?: string | null; link?: string | null }

const props = defineProps<{
  mode: 'create' | 'edit'
  task: { references?: ReferenceEntry[] | null }
  hoveredReferenceCode: string | null
  hoveredReferenceStyle: Record<string, string>
  hoveredReferenceLoading: boolean
  hoveredReferenceError: string | null
  hoveredReferenceSnippet: ReferenceSnippet | null
  hoveredReferenceCanExpand: boolean
  hoveredReferenceCanExpandBefore: boolean
  hoveredReferenceCanExpandAfter: boolean
  onReferenceEnter: (code?: string | null, event?: Event) => void
  onReferenceLeave: (code?: string | null) => void
  onReferencePreviewEnter: (code?: string | null) => void
  onReferencePreviewLeave: (code?: string | null) => void
  expandReferenceSnippet: (code?: string | null, direction?: 'before' | 'after') => void
  isReferenceLineHighlighted: (code: string, lineNumber: number) => boolean
  setReferencePreviewElement: (el: HTMLElement | null) => void
}>()

defineEmits<{ (e: 'reload'): void }>()

const references = computed(() => props.task?.references?.filter(Boolean) || [])

const previewTitle = computed(() => {
  if (props.hoveredReferenceSnippet?.path) {
    return props.hoveredReferenceSnippet.path
  }
  if (props.hoveredReferenceCode) {
    return props.hoveredReferenceCode
  }
  return 'Reference'
})

const previewElement = ref<HTMLElement | null>(null)

watch(previewElement, (element) => {
  props.setReferencePreviewElement(element)
})

onUnmounted(() => {
  props.setReferencePreviewElement(null)
})

function handleReferenceEnter(code: string | null | undefined, event?: Event) {
  props.onReferenceEnter(code ?? null, event)
}

function handleReferenceLeave(code: string | null | undefined) {
  props.onReferenceLeave(code ?? null)
}

function handleReferencePreviewEnter(code: string | null | undefined) {
  props.onReferencePreviewEnter(code ?? null)
}

function handleReferencePreviewLeave(code: string | null | undefined) {
  props.onReferencePreviewLeave(code ?? null)
}

function expandReference(direction: 'before' | 'after') {
  if (!props.hoveredReferenceCode) return
  props.expandReferenceSnippet(props.hoveredReferenceCode, direction)
}

function setPreviewElement(refValue: Element | ComponentPublicInstance | null) {
  if (!refValue) {
    previewElement.value = null
    return
  }
  if (refValue instanceof HTMLElement) {
    previewElement.value = refValue
    return
  }
  const element = (refValue as ComponentPublicInstance).$el as HTMLElement | null
  previewElement.value = element || null
}
</script>



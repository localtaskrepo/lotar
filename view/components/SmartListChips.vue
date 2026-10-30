<template>
  <div class="row" style="gap: 8px; flex-wrap: wrap; align-items: center">
    <UiButton :variant="isMine ? 'primary' : ''" type="button" @click="toggleMine">Mine</UiButton>
    <UiButton :variant="isUnassigned ? 'primary' : ''" type="button" @click="toggleUnassigned">No assignee</UiButton>
    <UiButton v-if="blocked" :variant="isBlocked ? 'primary' : ''" type="button" @click="toggleBlocked"
      >Blocked</UiButton
    >
    <UiButton v-if="showDueSoonChip" :variant="isDueSoon ? 'primary' : ''" type="button" @click="toggleDueSoon"
      >Due soon</UiButton
    >
    <UiButton :variant="isOverdue ? 'primary' : ''" type="button" @click="toggleOverdue">Overdue</UiButton>
    <UiButton v-if="showRecentChip" :variant="isRecent ? 'primary' : ''" type="button" @click="toggleRecent"
      >Recent</UiButton
    >
    <UiButton :variant="isNoEstimate ? 'primary' : ''" type="button" @click="toggleNoEstimate">No estimate</UiButton>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { listFromCsv } from '../composables/useFilterBuilder'
import UiButton from './UiButton.vue'

const props = defineProps<{
  statuses?: string[]
  priorities?: string[]
  value?: Record<string, string>
  enableDueSoon?: boolean
  enableRecent?: boolean
}>()
const emit = defineEmits<{ (e: 'update:value', v: Record<string, string>): void }>()

function normalize(s: string) {
  return (s || '').toLowerCase().replace(/\s|[_-]/g, '')
}
function isBlockedLike(s: string) {
  return normalize(s).includes('blocked')
}

const blocked = computed(() => (props.statuses || []).find(isBlockedLike) || '')

const isMine = computed(() => (props.value?.assignee || '') === '@me')
const isUnassigned = computed(() => (props.value?.assignee || '') === '__none__')
const isBlocked = computed(() => !!blocked.value && (props.value?.status || '') === blocked.value)
const showDueSoonChip = computed(() => props.enableDueSoon !== false)
const showRecentChip = computed(() => props.enableRecent !== false)

// New meta filters
const isDueSoon = computed(() => (props.value?.due || '') === 'soon')
const isOverdue = computed(() => (props.value?.due || '') === 'overdue')
const isRecent = computed(() => !!(props.value?.recent || ''))
function getNeeds(): string[] {
  return listFromCsv(String(props.value?.needs || ''))
}
const isNoEstimate = computed(() => getNeeds().includes('effort'))

function patch(next: Partial<Record<string, string>>) {
  const base: Record<string, string> = { ...(props.value || {}) }
  for (const [k, v] of Object.entries(next)) {
    if (v === '') delete (base as any)[k]
    else (base as any)[k] = v as string
  }
  emit('update:value', base)
}

function toggleMine() {
  patch({ assignee: isMine.value ? '' : '@me' })
}
function toggleUnassigned() {
  patch({ assignee: isUnassigned.value ? '' : '__none__' })
}
function toggleBlocked() {
  if (!blocked.value) return
  patch({ status: isBlocked.value ? '' : blocked.value })
}

function toggleDueSoon() {
  patch({ due: isDueSoon.value ? '' : 'soon' /* ensure only one of due filters active */ })
}
function toggleOverdue() {
  patch({ due: isOverdue.value ? '' : 'overdue' })
}
function toggleRecent() {
  patch({ recent: isRecent.value ? '' : '7d' })
}
function toggleNoEstimate() {
  const set = new Set(getNeeds())
  if (set.has('effort')) set.delete('effort')
  else set.add('effort')
  const csv = Array.from(set).join(',')
  patch({ needs: csv || '' })
}
</script>

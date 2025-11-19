<template>
  <div class="row" style="gap:8px; flex-wrap: wrap; align-items: center;">
    <button class="btn" :class="{ primary: isMine }" @click="toggleMine">Mine</button>
    <button class="btn" :class="{ primary: isUnassigned }" @click="toggleUnassigned">No assignee</button>
    <button v-if="blocked" class="btn" :class="{ primary: isBlocked }" @click="toggleBlocked">Blocked</button>
    <button v-if="reviewList.length" class="btn" :class="{ primary: isReview }" @click="toggleReview">Review</button>
    <button class="btn" :class="{ primary: isDueSoon }" @click="toggleDueSoon">Due soon</button>
    <button class="btn" :class="{ primary: isOverdue }" @click="toggleOverdue">Overdue</button>
    <button class="btn" :class="{ primary: isRecent }" @click="toggleRecent">Recent</button>
    <button class="btn" :class="{ primary: isNoEstimate }" @click="toggleNoEstimate">No estimate</button>
    <template v-if="customPresets?.length">
      <span class="muted smart-list-chips__label">Custom fields</span>
      <button
        v-for="preset in customPresets"
        :key="preset.expression"
        class="btn ghost smart-list-chips__preset"
        type="button"
        @click="emitPreset(preset.expression)"
      >
        {{ preset.label }}
      </button>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';

interface CustomPreset {
  label: string
  expression: string
}

const props = defineProps<{ statuses?: string[]; priorities?: string[]; value?: Record<string, string>; customPresets?: CustomPreset[] }>()
const emit = defineEmits<{ (e: 'update:value', v: Record<string,string>): void; (e: 'preset', expr: string): void }>()

function normalize(s: string){ return (s || '').toLowerCase().replace(/\s|[_-]/g, '') }
function isBlockedLike(s: string){ return normalize(s).includes('blocked') }
function isReviewLike(s: string){ const v = normalize(s); return v.includes('review') || v.includes('verify') }

const blocked = computed(() => (props.statuses || []).find(isBlockedLike) || '')
const reviewList = computed(() => {
  const found = (props.statuses || []).filter(isReviewLike)
  return found.length ? found : []
})

const isMine = computed(() => (props.value?.assignee || '') === '@me')
const isUnassigned = computed(() => (props.value?.assignee || '') === '__none__')
const isBlocked = computed(() => !!blocked.value && (props.value?.status || '') === blocked.value)
const isReview = computed(() => {
  const want = reviewList.value.join(',')
  return !!want && (props.value?.status || '') === want
})

// New meta filters
const isDueSoon = computed(() => (props.value?.due || '') === 'soon')
const isOverdue = computed(() => (props.value?.due || '') === 'overdue')
const isRecent = computed(() => !!(props.value?.recent || ''))
function getNeeds(): string[] { return String(props.value?.needs || '').split(',').map(s => s.trim()).filter(Boolean) }
const isNoEstimate = computed(() => getNeeds().includes('effort'))

function patch(next: Partial<Record<string,string>>) {
  const base: Record<string,string> = { ...(props.value || {}) }
  for (const [k,v] of Object.entries(next)) {
    if (v === '') delete (base as any)[k]; else (base as any)[k] = v as string
  }
  emit('update:value', base)
}

function toggleMine(){ patch({ assignee: isMine.value ? '' : '@me' }) }
function toggleUnassigned(){ patch({ assignee: isUnassigned.value ? '' : '__none__' }) }
function toggleBlocked(){ if (!blocked.value) return; patch({ status: isBlocked.value ? '' : blocked.value }) }
function toggleReview(){ const want = reviewList.value.join(','); if (!want) return; patch({ status: isReview.value ? '' : want }) }

function toggleDueSoon(){ patch({ due: isDueSoon.value ? '' : 'soon', /* ensure only one of due filters active */ }) }
function toggleOverdue(){ patch({ due: isOverdue.value ? '' : 'overdue' }) }
function toggleRecent(){ patch({ recent: isRecent.value ? '' : '7d' }) }
function toggleNoEstimate(){
  const set = new Set(getNeeds())
  if (set.has('effort')) set.delete('effort'); else set.add('effort')
  const csv = Array.from(set).join(',')
  patch({ needs: csv || '' })
}

function emitPreset(expression: string) {
  emit('preset', expression)
}
</script>

<style scoped>
.smart-list-chips__label {
  font-size: var(--text-xs, 0.75rem);
}

.smart-list-chips__preset {
  font-size: var(--text-xs, 0.75rem);
  padding-inline: var(--space-2);
}
</style>

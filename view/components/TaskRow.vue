<template>
  <li class="row item" @click="$emit('open', task.id)">
    <div class="col">
      <div class="row" style="gap:8px; align-items: baseline;">
        <input v-if="selectable" type="checkbox" v-model="selectedLocal" @click.stop />
  <span class="muted">{{ project }}</span>
  <strong>{{ numeric }}</strong>
        <template v-if="!editingTitle">
          <span>{{ task.title }}</span>
          <UiButton
            icon-only
            variant="ghost"
            type="button"
            aria-label="Edit title"
            title="Edit title"
            @click.stop="startEditTitle"
          >
            <IconGlyph name="edit" />
          </UiButton>
        </template>
        <template v-else>
          <input class="input" v-model="titleDraft" @keyup.enter.prevent="saveTitle" @blur="saveTitle" style="max-width: 420px;" />
          <UiButton variant="ghost" type="button" @click.stop="cancelTitle">Cancel</UiButton>
        </template>
      </div>
      <div class="row" style="gap:8px; font-size:12px; align-items:center; color: var(--muted);">
        <UiButton
          class="status-button"
          variant="ghost"
          type="button"
          :data-status="task.status"
          title="Toggle status"
          @click.stop="cycleStatus"
        >
          {{ task.status || 'status' }}
        </UiButton>
        <span>• {{ task.priority }}</span>
        <span>• {{ task.task_type }}</span>
        <span v-if="task.assignee">• @{{ task.assignee }}</span>
        <span v-if="dueDateLabel">• due {{ dueDateLabel }}</span>
      </div>
      <div class="row" style="gap:6px; flex-wrap: wrap; margin-top: 4px; align-items: center;">
  <span v-for="tag in task.tags" :key="tag" class="chip small">{{ tag }}</span>
        <UiButton variant="ghost" type="button" @click.stop="toggleTagsEdit">
          {{ editingTags ? 'Save tags' : 'Edit tags' }}
        </UiButton>
        <input v-if="editingTags" class="input" v-model="tagsDraft" placeholder="tag1, tag2" style="max-width: 320px;" @keyup.enter.prevent="saveTags" />
      </div>
    </div>
    <div class="row" @click.stop>
      <UiButton variant="ghost" type="button" @click="$emit('assign', task.id)">Assign</UiButton>
      <UiButton variant="ghost" type="button" @click="$emit('unassign', task.id)">Clear</UiButton>
      <UiButton variant="danger" type="button" @click="$emit('delete', task.id)">Delete</UiButton>
    </div>
  </li>
  
</template>
<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import type { TaskDTO } from '../api/types';
import { formatTaskDate } from '../utils/date';
import IconGlyph from './IconGlyph.vue';
import UiButton from './UiButton.vue';
const props = defineProps<{ task: TaskDTO; statuses?: string[]; selectable?: boolean; selected?: boolean }>()
const emit = defineEmits<{ (e:'open', id: string): void; (e:'delete', id: string): void; (e:'update-title', payload: { id: string; title: string }): void; (e:'update-tags', payload: { id: string; tags: string[] }): void; (e:'set-status', payload: { id: string; status: string }): void; (e:'assign', id: string): void; (e:'unassign', id: string): void; (e:'update:selected', v: boolean): void }>()
const project = (props.task.id || '').split('-')[0]
const numeric = (props.task.id || '').split('-').slice(1).join('-')
const dueDateLabel = computed(() => props.task.due_date ? formatTaskDate(props.task.due_date) : '')
// bulk select support
const selectedLocal = ref(!!props.selected)
watch(() => props.selected, (v) => { selectedLocal.value = !!v })
watch(selectedLocal, (v) => emit('update:selected', v))

// Title inline edit
const editingTitle = ref(false)
const titleDraft = ref('')
function startEditTitle(){ titleDraft.value = props.task.title || ''; editingTitle.value = true }
function cancelTitle(){ editingTitle.value = false }
function saveTitle(){ if (!editingTitle.value) return; emit('update-title', { id: props.task.id, title: (titleDraft.value || '').trim() }); editingTitle.value = false }

// Tags inline edit
const editingTags = ref(false)
const tagsDraft = ref('')
watch(() => props.task.tags, (t) => { if (!editingTags.value) tagsDraft.value = (t || []).join(', ') }, { immediate: true })
function toggleTagsEdit(){ if (editingTags.value) { saveTags() } else { tagsDraft.value = (props.task.tags || []).join(', '); editingTags.value = true } }
function saveTags(){ const list = tagsDraft.value.split(',').map(s => s.trim()).filter(Boolean); emit('update-tags', { id: props.task.id, tags: list }); editingTags.value = false }

// Status quick toggle
function cycleStatus(){
  const order = Array.isArray(props.statuses) ? props.statuses.filter((value) => typeof value === 'string' && value.trim().length > 0) : []
  if (!order.length) return
  const normalized = order.map((value) => value.trim())
  const cur = (props.task.status || '').trim()
  const idx = normalized.findIndex((value) => value.toLowerCase() === cur.toLowerCase())
  const next = normalized[(idx + 1) % normalized.length]
  emit('set-status', { id: props.task.id, status: next })
}
</script>
<style scoped>
.item { justify-content: space-between; cursor: pointer; padding: 8px 0; }
.status-button {
  color: var(--color-muted, #6b7280);
  font-weight: 600;
  padding-inline: 0.5rem;
}
.chip.small { font-size: 11px; padding: 2px 6px; background: color-mix(in oklab, var(--bg) 85%, var(--fg)); border-radius: 999px; }
</style>

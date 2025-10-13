<template>
  <div class="task-panel__tab-panel">
    <header class="task-panel__group-header">
      <h3>Relationships</h3>
      <UiButton type="button" variant="ghost" :disabled="mode !== 'edit'" @click="$emit('reload')">Reload</UiButton>
    </header>
    <div class="task-panel__relations">
      <label v-for="rel in relationDefs" :key="rel.key" class="task-panel__relation">
        <span>{{ rel.label }}</span>
        <div class="task-panel__relation-input">
          <UiInput
            :model-value="relationships[rel.key] || ''"
            :placeholder="rel.placeholder"
            @update:modelValue="(value) => handleValueChange(rel.key, value)"
            @keydown="handleKey(rel.key, $event)"
            @focus="() => onRelationInput(rel.key)"
            @blur="() => onRelationBlur(rel.key)"
          />
          <ul v-if="relationSuggestions[rel.key]?.length" class="task-panel__relation-suggest">
            <li
              v-for="(item, idx) in relationSuggestions[rel.key]"
              :key="`${rel.key}-${item.id}`"
              :class="{ active: relationActiveIndex[rel.key] === idx }"
              @mousedown.prevent="() => onPickRelation(rel.key, item.id)"
            >
              <strong>{{ item.id }}</strong>
              <span>{{ item.title }}</span>
            </li>
          </ul>
        </div>
      </label>
    </div>
  </div>
</template>

<script setup lang="ts">
import UiButton from '../UiButton.vue'
import UiInput from '../UiInput.vue'

type RelationDef = {
  key: string
  label: string
  placeholder: string
}

type RelationSuggestion = {
  id: string
  title: string
}

const props = defineProps<{
  mode: 'create' | 'edit'
  relationDefs: RelationDef[]
  relationships: Record<string, string>
  relationSuggestions: Record<string, RelationSuggestion[]>
  relationActiveIndex: Record<string, number>
  onRelationInput: (field: string) => void
  onRelationKey: (field: string, event: KeyboardEvent) => void
  onRelationBlur: (field: string) => void
  onPickRelation: (field: string, id: string) => void
}>()

const emit = defineEmits<{ (e: 'reload'): void; (e: 'update:relationship', field: string, value: string): void }>()

function handleValueChange(field: string, value: string) {
  emit('update:relationship', field, value)
  props.onRelationInput(field)
}

function handleKey(field: string, event: KeyboardEvent) {
  props.onRelationKey(field, event)
}

function onRelationInput(field: string) {
  props.onRelationInput(field)
}

function onRelationBlur(field: string) {
  props.onRelationBlur(field)
}

function onPickRelation(field: string, id: string) {
  props.onPickRelation(field, id)
}
</script>



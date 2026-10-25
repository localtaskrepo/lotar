<template>
  <UiModal :open="open" :aria-labelledby="titleId" size="md" @close="emit('close')">
    <div class="shortcuts" data-testid="keyboard-shortcuts">
      <header class="shortcuts__header">
        <h2 :id="titleId">Keyboard shortcuts</h2>
        <UiButton variant="ghost" icon-only type="button" aria-label="Close" @click="emit('close')">
          <IconGlyph name="close" />
        </UiButton>
      </header>
      <p v-if="!singleKeyEnabled" class="shortcuts__note" role="status">
        Single-key shortcuts are turned off. Turn them on in
        <a href="/preferences" @click.prevent="openPreferences">Preferences</a>; {{ paletteLabel }} always works.
      </p>
      <section v-for="section in sections" :key="section.title">
        <h3>{{ section.title }}</h3>
        <dl class="shortcuts__list">
          <template v-for="row in section.rows" :key="row.label">
            <dt>
              <kbd v-for="(key, index) in row.keys" :key="index">{{ key }}</kbd>
            </dt>
            <dd>{{ row.label }}</dd>
          </template>
        </dl>
      </section>
    </div>
  </UiModal>
</template>

<script setup lang="ts">
import { computed, useId } from 'vue'
import { GO_SHORTCUTS, paletteShortcutLabel } from '../utils/keyboardShortcuts'
import IconGlyph from './IconGlyph.vue'
import UiButton from './UiButton.vue'
import UiModal from './UiModal.vue'

defineProps<{ open: boolean; singleKeyEnabled: boolean }>()
const emit = defineEmits<{ (e: 'close'): void; (e: 'navigate', path: string): void }>()

const titleId = useId()
const paletteLabel = paletteShortcutLabel()

const sections = computed(() => [
  {
    title: 'Anywhere',
    rows: [
      { keys: [paletteLabel], label: 'Command palette: pages, actions and tasks' },
      { keys: ['?'], label: 'Show these shortcuts' },
      { keys: ['/'], label: 'Focus the page search' },
      { keys: ['c'], label: 'New task' },
      { keys: ['Esc'], label: 'Close the open panel or dialog' },
    ],
  },
  {
    title: 'Go to',
    rows: GO_SHORTCUTS.map((entry) => ({ keys: ['g', entry.key], label: entry.label })),
  },
])

function openPreferences() {
  emit('close')
  emit('navigate', '/preferences')
}
</script>

<style scoped>
.shortcuts {
  display: flex;
  flex-direction: column;
  gap: var(--space-3, 0.75rem);
}

.shortcuts__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.shortcuts h2 {
  margin: 0;
  font-size: var(--text-lg, 1.125rem);
}

.shortcuts h3 {
  margin: 0 0 var(--space-2, 0.5rem);
  color: var(--color-muted);
  font-size: var(--text-sm, 0.875rem);
  font-weight: 600;
}

.shortcuts__note {
  margin: 0;
  padding: var(--space-2, 0.5rem) var(--space-3, 0.75rem);
  border-left: 3px solid var(--color-warning);
  background: color-mix(in oklab, var(--color-warning) 10%, transparent);
  font-size: var(--text-sm, 0.875rem);
}

.shortcuts__list {
  display: grid;
  grid-template-columns: max-content 1fr;
  gap: var(--space-2, 0.5rem) var(--space-4, 1rem);
  margin: 0;
  font-size: var(--text-sm, 0.875rem);
}

.shortcuts__list dt {
  display: flex;
  gap: 4px;
}

.shortcuts__list dd {
  margin: 0;
}
</style>

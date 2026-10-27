<template>
  <!-- Each half carries its own theme attributes, so it shows that theme
       whatever the page uses (see view/themes.css). -->
  <span class="theme-preview" aria-hidden="true">
    <span v-for="mode in modes" :key="mode" class="theme-preview__half" :data-theme-name="theme" :data-mode="mode">
      <span class="theme-preview__bar"><span class="theme-preview__dot"></span></span>
      <span class="theme-preview__card">
        <span class="theme-preview__line theme-preview__line--strong"></span>
        <span class="theme-preview__line theme-preview__line--muted"></span>
        <span class="theme-preview__chips">
          <span class="theme-preview__button"></span>
          <span class="theme-preview__status theme-preview__status--success"></span>
          <span class="theme-preview__status theme-preview__status--danger"></span>
        </span>
      </span>
    </span>
  </span>
</template>

<script setup lang="ts">
import type { ThemeId } from '../utils/themes'

defineProps<{ theme: ThemeId }>()

const modes = ['light', 'dark'] as const
</script>

<style scoped>
.theme-preview {
  display: grid;
  grid-template-columns: 1fr 1fr;
  height: 72px;
  overflow: hidden;
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border);
}

.theme-preview__half {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 6px;
  background: var(--color-bg);
  color: var(--color-fg);
}

.theme-preview__bar {
  display: flex;
  align-items: center;
  height: 10px;
  padding: 0 4px;
  border-radius: 3px;
  background: var(--color-surface);
  border: 1px solid var(--color-border);
}

.theme-preview__dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--color-accent);
}

.theme-preview__card {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 4px;
  padding: 5px;
  border-radius: 4px;
  background: var(--color-surface-contrast);
  border: 1px solid var(--color-border);
}

.theme-preview__line {
  height: 4px;
  border-radius: 2px;
}

.theme-preview__line--strong {
  width: 70%;
  background: var(--color-fg);
}

.theme-preview__line--muted {
  width: 50%;
  background: var(--color-muted);
}

.theme-preview__chips {
  display: flex;
  gap: 3px;
  margin-top: auto;
}

.theme-preview__button {
  width: 18px;
  height: 6px;
  border-radius: 3px;
  background: var(--color-accent);
}

.theme-preview__status {
  width: 6px;
  height: 6px;
  border-radius: 50%;
}

.theme-preview__status--success {
  background: var(--color-success);
}

.theme-preview__status--danger {
  background: var(--color-danger);
}
</style>

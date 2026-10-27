<template>
  <!-- Each half carries its own theme attributes, so it shows that theme
       whatever the page uses (see view/themes.css). -->
  <span class="theme-preview" aria-hidden="true">
    <span
      v-for="mode in modes"
      :key="mode"
      class="theme-preview__half"
      :data-theme-name="theme"
      :data-theme-palette="palette ?? undefined"
      :data-mode="mode"
    >
      <span class="theme-preview__bar"
        ><span class="theme-preview__dot"></span><span class="theme-preview__title">Aa</span></span
      >
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
defineProps<{
  /** Skin id (or `custom` for the repository skin). */
  theme: string
  /** Palette id; omitted = the skin's default. */
  palette?: string | null
}>()

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

/* Each half shows the theme's character too: page and header backgrounds,
   card surface, corners, heading font and glow. */
.theme-preview__half {
  display: flex;
  flex-direction: column;
  gap: 5px;
  padding: 6px;
  background: var(--page-background);
  color: var(--color-fg);
}

.theme-preview__bar {
  display: flex;
  align-items: center;
  gap: 4px;
  height: 14px;
  padding: 0 4px;
  border-radius: var(--radius-sm);
  background: var(--topbar-background);
  border: 1px solid var(--topbar-border);
  color: var(--topbar-fg);
}

.theme-preview__title {
  font-family: var(--font-heading);
  font-weight: var(--heading-weight);
  text-transform: var(--heading-transform);
  text-shadow: var(--text-glow);
  font-size: 9px;
  line-height: 1;
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
  border-radius: var(--radius-md);
  background: var(--card-background);
  border: 1px solid var(--card-border);
  box-shadow: var(--card-shadow);
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
  border-radius: var(--radius-sm);
  background: var(--color-accent);
  box-shadow: var(--control-shadow), var(--accent-glow);
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

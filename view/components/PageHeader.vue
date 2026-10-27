<template>
  <header class="app-page-header">
    <div class="app-page-header__row">
      <div class="app-page-header__heading">
        <h1 class="app-page-header__title"><slot name="title" /></h1>
        <p v-if="$slots.subtitle" class="muted app-page-header__subtitle"><slot name="subtitle" /></p>
      </div>
      <div class="app-page-header__controls">
        <!-- View options (Columns, Fields, display toggles) come first. -->
        <slot name="view" />
        <UiButton
          v-if="clearLabel"
          icon-only
          type="button"
          :aria-label="clearLabel"
          :title="clearLabel"
          :disabled="!canClear"
          @click="emit('clear')"
        >
          <IconGlyph name="close" />
        </UiButton>
        <ReloadButton
          v-if="reloadLabel"
          :disabled="reloadDisabled ?? reloading"
          :loading="reloading"
          :label="reloadLabel"
          :title="reloadLabel"
          @click="emit('reload')"
        />
      </div>
    </div>
    <!-- Page controls and actions for pages without a filter bar. -->
    <div v-if="$slots.toolbar" class="app-page-header__toolbar"><slot name="toolbar" /></div>
  </header>
</template>

<script setup lang="ts">
import IconGlyph from './IconGlyph.vue'
import ReloadButton from './ReloadButton.vue'
import UiButton from './UiButton.vue'

/**
 * Shared page header (DEV-186). The title row always ends with view options,
 * then Clear filters, then Reload; page controls and actions go in the second
 * row (the filter bar's actions, or the `toolbar` slot).
 */
withDefaults(
  defineProps<{
    /** Accessible name of the Clear filters button; omit to hide it. */
    clearLabel?: string
    canClear?: boolean
    /** Accessible name of the Reload button; omit to hide it. */
    reloadLabel?: string
    reloading?: boolean
    /** Defaults to disabled while reloading. */
    reloadDisabled?: boolean
  }>(),
  { clearLabel: '', canClear: false, reloadLabel: '', reloading: false, reloadDisabled: undefined },
)

const emit = defineEmits<{ (e: 'clear'): void; (e: 'reload'): void }>()
</script>

<style scoped>
.app-page-header {
  display: flex;
  flex-direction: column;
  gap: var(--space-3, 0.75rem);
}

.app-page-header__row {
  display: flex;
  justify-content: space-between;
  /* Controls line up with the title, not the title plus subtitle. */
  align-items: flex-start;
  flex-wrap: wrap;
  gap: var(--space-2, 0.5rem);
}

.app-page-header__heading {
  display: flex;
  flex-direction: column;
  gap: var(--space-1, 0.25rem);
  min-width: 0;
}

.app-page-header__title {
  margin: 0;
  min-height: var(--control-height);
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  column-gap: 0.3em;
}

.app-page-header__subtitle {
  margin: 0;
}

/* "— Project name" after a page title (DEV-184). A long name moves to its
   own line and ends in an ellipsis rather than pushing the controls away. */
.app-page-header__title :slotted(.app-page-header__project) {
  min-width: 0;
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.app-page-header__controls,
.app-page-header__toolbar {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--space-2, 0.5rem);
}

.app-page-header__controls {
  margin-left: auto;
  justify-content: flex-end;
}

.app-page-header__toolbar {
  justify-content: flex-end;
}
</style>

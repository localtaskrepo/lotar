<template>
  <section class="not-found col" aria-labelledby="not-found-title">
    <p class="not-found__code">404</p>
    <h1 id="not-found-title">This page took a coffee break ☕️</h1>
    <p class="muted not-found__text">
      We asked around but nobody has seen
      <code class="not-found__path">{{ missingPath }}</code
      >. Maybe it sprinted off to an emergency retro.
    </p>
    <div class="not-found__actions">
      <UiButton variant="primary" type="button" @click="goHome">Back to tasks</UiButton>
      <UiButton class="not-found__back" variant="ghost" type="button" @click="goBack">
        Take me to the previous page
      </UiButton>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import UiButton from '../components/UiButton.vue'

const route = useRoute()
const router = useRouter()

const missingPath = computed(() => route.fullPath || route.path || '/')

function goHome() {
  router.push('/')
}

function goBack() {
  if (typeof window !== 'undefined' && window.history.length > 1) {
    router.back()
  } else {
    goHome()
  }
}
</script>

<style scoped>
.not-found {
  width: 100%;
  /* Padding counts inside the width, or the card spills past phones. */
  box-sizing: border-box;
  max-width: 720px;
  text-align: center;
  align-self: center;
  align-items: center;
  gap: var(--space-4);
  padding: clamp(2.5rem, 6vw, 4.75rem) clamp(1.5rem, 4vw, 2.5rem);
  margin: clamp(2rem, 5vw, 3rem) auto clamp(5rem, 12vw, 8rem);
}

/* Centred children size to their content; keep them inside the card on
   phones so long paths and the heading wrap instead of spilling out. */
.not-found > * {
  max-width: 100%;
}

.not-found__code {
  font-size: clamp(3rem, 12vw, 5rem);
  font-weight: 700;
  letter-spacing: 0.1em;
  margin: 0;
  color: var(--color-accent-strong);
}

.not-found h1 {
  margin: 0;
  font-size: clamp(1.75rem, 4vw, 2.75rem);
}

.not-found__text {
  margin: 0;
  max-width: 460px;
}

.not-found__path {
  font-family: var(--font-mono);
  overflow-wrap: anywhere;
  background: color-mix(in oklab, var(--color-surface) 65%, transparent);
  padding: 2px 6px;
  border-radius: var(--radius-sm);
}

.not-found__actions {
  display: flex;
  gap: var(--space-3);
  flex-wrap: wrap;
  justify-content: center;
}

.not-found__back {
  border: 1px solid var(--color-border);
}
</style>

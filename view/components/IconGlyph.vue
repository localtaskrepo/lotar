<template>
  <svg
    class="icon-glyph"
    viewBox="0 0 24 24"
    fill="none"
    aria-hidden="true"
    focusable="false"
  >
    <path
      v-for="(d, index) in activeIcon.paths"
      :key="`${name}-${index}`"
      :d="d"
      :stroke="activeIcon.stroke || 'currentColor'"
      :stroke-width="activeIcon.strokeWidth || 1.8"
      :stroke-linecap="activeIcon.linecap || 'round'"
      :stroke-linejoin="activeIcon.linejoin || 'round'"
      fill="none"
    />
  </svg>
</template>

<script setup lang="ts">
import { computed } from 'vue'

type IconName =
  | 'plus'
  | 'tag'
  | 'check'
  | 'user-add'
  | 'user-remove'
  | 'flag'
  | 'flag-remove'
  | 'trash'
  | 'list'

type IconDef = {
  paths: string[]
  strokeWidth?: number
  linecap?: 'round' | 'square' | 'butt'
  linejoin?: 'round' | 'miter' | 'bevel'
  stroke?: string
}

const ICONS: Record<IconName, IconDef> = {
  plus: {
    paths: ['M12 4v16', 'M4 12h16'],
  },
  tag: {
    paths: ['M5 7V5a2 2 0 012-2h6l6 6-9 9-5-5A2 2 0 015 7z', 'M11 7h.01'],
  },
  check: {
    paths: ['M5 13l4 4L19 7'],
  },
  'user-add': {
    paths: ['M12 12a4 4 0 100-8 4 4 0 000 8z', 'M6 21v-1a6 6 0 0112 0v1', 'M18.5 8h4', 'M20.5 6v4'],
  },
  'user-remove': {
    paths: ['M12 12a4 4 0 100-8 4 4 0 000 8z', 'M6 21v-1a6 6 0 0112 0v1', 'M18 7.5l4 4', 'M22 7.5l-4 4'],
  },
  flag: {
    paths: ['M6 4v16', 'M6 4h11l-2.5 4L17 12H6'],
  },
  'flag-remove': {
    paths: ['M6 4v16', 'M6 4h11l-2.5 4L17 12H6', 'M10 17h8'],
  },
  trash: {
    paths: ['M9 4h6', 'M10 4l1-1h2l1 1', 'M6 7h12', 'M8 7v12h8V7', 'M11 11v6', 'M13 11v6'],
  },
  list: {
    paths: ['M8 7h11', 'M8 12h11', 'M8 17h11', 'M4.5 7h.01', 'M4.5 12h.01', 'M4.5 17h.01'],
  },
}

const props = defineProps<{ name: IconName }>()

const activeIcon = computed(() => ICONS[props.name] ?? ICONS.plus)
</script>

<style scoped>
.icon-glyph {
  width: 1em;
  height: 1em;
  display: inline-block;
}
</style>

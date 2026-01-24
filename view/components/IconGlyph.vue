<template>
  <svg
    class="icon-glyph"
    viewBox="0 0 24 24"
    fill="none"
    aria-hidden="true"
    focusable="false"
    shape-rendering="geometricPrecision"
  >
    <path
      v-for="(d, index) in activeIcon.paths"
      :key="`${name}-${index}`"
      :d="d"
      :stroke="activeIcon.stroke || 'currentColor'"
      :stroke-width="activeIcon.strokeWidth || 1.8"
      :stroke-linecap="activeIcon.linecap || 'round'"
      :stroke-linejoin="activeIcon.linejoin || 'round'"
      vector-effect="non-scaling-stroke"
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
  | 'refresh'
  | 'help'
  | 'close'
  | 'dots-horizontal'
  | 'chevron-left'
  | 'chevron-right'
  | 'chevron-down'
  | 'chevron-up'
  | 'send'
  | 'edit'
  | 'columns'
  | 'file'
  | 'github'
  | 'jira'
  | 'search'
  | 'eye'
  | 'eye-off'

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
  refresh: {
    paths: [
      'M4 12A8 8 0 0112 4h3',
      'M15 4l3 3-3 3',
      'M20 12A8 8 0 0112 20H9',
      'M9 20l-3-3 3-3',
    ],
  },
  help: {
    paths: [
      'M21 12a9 9 0 11-18 0 9 9 0 0118 0z',
      'M8.227 9a3.001 3.001 0 115.546 1.5c-.457.77-1.414 1.28-1.773 2.25',
      'M12 17h.01',
    ],
  },
  close: {
    paths: ['M6 6l12 12', 'M6 18L18 6'],
  },
  'dots-horizontal': {
    paths: ['M6 12h.01', 'M12 12h.01', 'M18 12h.01'],
    strokeWidth: 2.8,
  },
  'chevron-left': {
    paths: ['M15 6l-6 6 6 6'],
  },
  'chevron-right': {
    paths: ['M9 6l6 6-6 6'],
  },
  send: {
    paths: ['M3 3l18 9-18 9 4-9-4-9z', 'M3 12h11'],
    linejoin: 'round',
  },
  edit: {
    paths: ['M5 19h4L20 8l-4-4L5 15v4', 'M15 4l5 5'],
  },
  columns: {
    paths: ['M4 5h5v14H4z', 'M9.5 5h5v14h-5z', 'M15 5h5v14h-5z'],
    linejoin: 'miter',
  },
  file: {
    paths: ['M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z', 'M14 2v6h6'],
  },
  github: {
    paths: [
      'M9 19c-5 1.5-5-2.5-7-3m14 6v-3.87a3.37 3.37 0 0 0-.94-2.61c3.14-.35 6.44-1.54 6.44-7A5.44 5.44 0 0 0 20 4.77 5.07 5.07 0 0 0 19.91 1S18.73.65 16 2.48a13.38 13.38 0 0 0-7 0C6.27.65 5.09 1 5.09 1A5.07 5.07 0 0 0 5 4.77a5.44 5.44 0 0 0-1.5 3.78c0 5.42 3.3 6.61 6.44 7A3.37 3.37 0 0 0 9 18.13V22',
    ],
  },
  jira: {
    paths: ['M5 12l7-7 7 7-7 7-7-7z', 'M9 12l3-3 3 3-3 3-3-3z'],
  },
  search: {
    paths: ['M11 17a6 6 0 100-12 6 6 0 000 12z', 'M21 21l-4.35-4.35'],
  },
  eye: {
    paths: ['M1 12s4-8 11-8 11 8 11 8-4 8-11 8S1 12 1 12z', 'M12 15a3 3 0 100-6 3 3 0 000 6z'],
  },
  'eye-off': {
    paths: ['M17.94 17.94A10.07 10.07 0 0112 20c-7 0-11-8-11-8a18.45 18.45 0 015.06-5.94M9.9 4.24A9.12 9.12 0 0112 4c7 0 11 8 11 8a18.5 18.5 0 01-2.16 3.19m-6.72-1.07a3 3 0 11-4.24-4.24', 'M1 1l22 22'],
  },
  'chevron-down': {
    paths: ['M6 9l6 6 6-6'],
  },
  'chevron-up': {
    paths: ['M6 15l6-6 6 6'],
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

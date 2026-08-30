<template>
  <div
    ref="triggerRef"
    :class="rootClasses"
    @mouseenter="handleTriggerEnter"
    @mouseleave="handleTriggerLeave"
    @focusin="handleTriggerEnter"
    @focusout="handleTriggerLeave"
  >
    <slot />

    <Teleport v-if="teleportToBody" to="body">
      <div
        ref="cardRef"
        :class="cardClasses"
        role="tooltip"
        :style="teleportStyle"
        :aria-hidden="open ? 'false' : 'true'"
        @mouseenter="handleCardEnter"
        @mouseleave="handleCardLeave"
        @focusin="handleCardEnter"
        @focusout="handleCardLeave"
      >
        <TaskHoverCardBody :task="task" :fields="fields" />
      </div>
    </Teleport>

    <div v-else :class="cardClasses" role="tooltip">
      <TaskHoverCardBody :task="task" :fields="fields" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref } from 'vue';
import type { TaskDTO } from '../api/types';
import TaskHoverCardBody from './TaskHoverCardBody.vue';

const props = withDefaults(defineProps<{
  task: TaskDTO
  placement?: 'left' | 'right'
  block?: boolean
  teleportToBody?: boolean
  fields?: Partial<Record<'id' | 'title' | 'status' | 'priority' | 'task_type' | 'reporter' | 'assignee' | 'effort' | 'tags' | 'sprints' | 'due_date' | 'modified' | `custom:${string}`, boolean>>
}>(), {
  placement: 'left',
  block: false,
  teleportToBody: false,
})

const triggerRef = ref<HTMLElement | null>(null)
const cardRef = ref<HTMLElement | null>(null)
const open = ref(false)
const hoveringTrigger = ref(false)
const hoveringCard = ref(false)
let closeTimer: number | null = null

const teleportToBody = computed(() => Boolean(props.teleportToBody))

const teleportPos = ref<{ top: number; left: number }>({ top: 0, left: 0 })

const teleportStyle = computed(() => {
  if (!teleportToBody.value) return undefined
  return {
    top: `${teleportPos.value.top}px`,
    left: `${teleportPos.value.left}px`,
  } as Record<string, string>
})

function clearCloseTimer() {
  if (closeTimer !== null) {
    window.clearTimeout(closeTimer)
    closeTimer = null
  }
}

function scheduleClose() {
  if (!teleportToBody.value) return
  clearCloseTimer()
  closeTimer = window.setTimeout(() => {
    if (!hoveringTrigger.value && !hoveringCard.value) {
      open.value = false
    }
  }, 80)
}

function updateTeleportPosition() {
  if (!teleportToBody.value) return
  const trigger = triggerRef.value
  const card = cardRef.value
  if (!trigger || !card) return

  const margin = 8
  const triggerRect = trigger.getBoundingClientRect()
  const cardRect = card.getBoundingClientRect()

  // Prefer placing beside the trigger so it doesn't cover list items above/below.
  // `placement` here is interpreted as which side has room:
  // - 'left' means "open to the right" (common when trigger is on the left half)
  // - 'right' means "open to the left" (common when trigger is on the right half)
  const preferRight = props.placement === 'left'
  const rightX = triggerRect.right + margin
  const leftX = triggerRect.left - margin - cardRect.width

  let left = preferRight ? rightX : leftX
  if (preferRight && left + cardRect.width > window.innerWidth - margin) {
    left = leftX
  } else if (!preferRight && left < margin) {
    left = rightX
  }
  left = Math.min(Math.max(margin, left), window.innerWidth - margin - cardRect.width)

  let top = triggerRect.top
  top = Math.min(Math.max(margin, top), window.innerHeight - margin - cardRect.height)

  teleportPos.value = { top, left }
}

async function handleTriggerEnter() {
  if (!teleportToBody.value) return
  hoveringTrigger.value = true
  clearCloseTimer()
  open.value = true
  await nextTick()
  updateTeleportPosition()
}

function handleTriggerLeave() {
  if (!teleportToBody.value) return
  hoveringTrigger.value = false
  scheduleClose()
}

function handleCardEnter() {
  if (!teleportToBody.value) return
  hoveringCard.value = true
  clearCloseTimer()
}

function handleCardLeave() {
  if (!teleportToBody.value) return
  hoveringCard.value = false
  scheduleClose()
}

function handleWindowChange() {
  if (!teleportToBody.value) return
  if (!open.value) return
  updateTeleportPosition()
}

onMounted(() => {
  window.addEventListener('resize', handleWindowChange)
  window.addEventListener('scroll', handleWindowChange, true)
})

onUnmounted(() => {
  window.removeEventListener('resize', handleWindowChange)
  window.removeEventListener('scroll', handleWindowChange, true)
  clearCloseTimer()
})

const rootClasses = computed(() => ({
  'task-hover': true,
  'task-hover--block': props.block,
}))

const cardClasses = computed(() => [
  'task-hover-card',
  teleportToBody.value
    ? 'task-hover-card--teleport'
    : (props.placement === 'right' ? 'task-hover-card--right' : 'task-hover-card--left'),
  teleportToBody.value && open.value ? 'is-open' : null,
])
</script>

<style scoped>
.task-hover {
  position: relative;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  min-width: 0;
  min-height: 22px;
}

.task-hover--block {
  display: block;
}

.task-hover-card {
  position: absolute;
  top: calc(100% + 8px);
  z-index: var(--z-tooltip);
  width: max-content;
  min-width: 260px;
  max-width: clamp(260px, 40vw, 360px);
  padding: 12px;
  border-radius: var(--radius-popover);
  border: 1px solid color-mix(in oklab, var(--color-border) 80%, transparent);
  background: var(--color-bg);
  box-shadow: var(--shadow-popover);
  opacity: 0;
  visibility: hidden;
  pointer-events: none;
  transform: translateY(4px);
  transition: opacity var(--duration-fast) var(--ease-standard), visibility var(--duration-fast) var(--ease-standard), transform var(--duration-fast) var(--ease-standard);
  color: var(--color-fg);
}

.task-hover-card--left {
  left: 0;
}

.task-hover-card--right {
  right: 0;
}

.task-hover:hover .task-hover-card,
.task-hover:focus-within .task-hover-card {
  opacity: 1;
  visibility: visible;
  pointer-events: auto;
  transform: translateY(0);
}

.task-hover-card.is-open {
  opacity: 1;
  visibility: visible;
  pointer-events: auto;
  transform: translateY(0);
}

.task-hover-card--teleport {
  position: fixed;
  left: 0;
  right: auto;
  z-index: var(--z-modal-high);
  pointer-events: none;
}

.task-hover-card--teleport.is-open {
  pointer-events: none;
}
</style>

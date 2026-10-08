<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="ui-modal__overlay"
      ref="overlay"
      @pointerdown="recordBackdropStart"
      @click.self="closeFromBackdrop"
    >
      <div
        ref="dialog"
        class="card ui-modal__card"
        :class="sizeClass"
        role="dialog"
        aria-modal="true"
        :aria-label="ariaLabel || 'Dialog'"
        :aria-labelledby="ariaLabelledby"
        :aria-describedby="ariaDescribedby"
        tabindex="-1"
        @keydown="handleModalKeydown"
      >
        <slot />
      </div>
    </div>
  </Teleport>
</template>

<script lang="ts">
interface ModalEntry {
  overlay: HTMLElement
  dialog: HTMLElement
  before: HTMLElement | null
  lastFocus: HTMLElement | null
  focusObserver: MutationObserver | null
  close: () => void
}

// Shared ownership prevents a nested dialog from unlocking or dismissing its parent.
const modalStack: ModalEntry[] = []
const inertBefore = new Map<HTMLElement, boolean>()
let bodyOverflow: { value: string; priority: string } | null = null
let bodyObserver: MutationObserver | null = null

function topModal() {
  return modalStack[modalStack.length - 1]
}

function eligible(element: HTMLElement): boolean {
  if (!element.isConnected || element.matches(':disabled') || element.closest('[hidden], [inert], [aria-hidden="true"]')) return false
  for (let parent: HTMLElement | null = element; parent; parent = parent.parentElement) {
    const style = getComputedStyle(parent)
    if (style.display === 'none' || style.visibility === 'hidden') return false
  }
  return true
}

function focusable(dialog: HTMLElement): HTMLElement[] {
  return Array.from(dialog.querySelectorAll<HTMLElement>(
    'button, input:not([type="hidden"]), select, textarea, a[href], [contenteditable="true"], [tabindex]',
  )).filter(element => element.tabIndex >= 0 && eligible(element))
}

function focusEntry(entry: ModalEntry) {
  const target = entry.lastFocus && eligible(entry.lastFocus)
    ? entry.lastFocus
    : focusable(entry.dialog)[0] || entry.dialog
  target.focus({ preventScroll: true })
}

function handleModalFocus(event: FocusEvent) {
  const entry = topModal()
  if (!entry) return
  if (event.target instanceof HTMLElement && entry.dialog.contains(event.target)) {
    entry.lastFocus = event.target
  } else {
    focusEntry(entry)
  }
}

function handleModalKeydown(event: KeyboardEvent) {
  const entry = topModal()
  if (!entry || event.defaultPrevented) return
  if (event.key === 'Escape') {
    event.preventDefault()
    event.stopImmediatePropagation()
    entry.close()
  } else if (event.key === 'Tab') {
    const controls = focusable(entry.dialog)
    const first = controls[0] || entry.dialog
    const last = controls[controls.length - 1] || entry.dialog
    const active = document.activeElement
    if (!controls.length || !controls.includes(active as HTMLElement) || (event.shiftKey ? active === first : active === last)) {
      event.preventDefault()
      ;(event.shiftKey ? last : first).focus({ preventScroll: true })
    }
  }
}

function refreshModalEnvironment() {
  for (const [element, wasInert] of inertBefore) {
    if (wasInert) element.setAttribute('inert', '')
    else element.removeAttribute('inert')
  }
  inertBefore.clear()
  const top = topModal()
  if (!top) return
  for (const element of Array.from(document.body.children)) {
    if (!(element instanceof HTMLElement) || element.contains(top.overlay)) continue
    inertBefore.set(element, element.hasAttribute('inert'))
    element.setAttribute('inert', '')
  }
  modalStack.forEach((entry, index) => {
    entry.overlay.style.zIndex = `calc(var(--z-modal) + ${index})`
  })
}
</script>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'

const props = withDefaults(defineProps<{
  open: boolean
  ariaLabel?: string
  ariaLabelledby?: string
  ariaDescribedby?: string
  dismissible?: boolean
  initialFocus?: string
  size?: 'sm' | 'md' | 'lg' | 'xl'
}>(), { dismissible: true })

const emit = defineEmits<{
  close: []
}>()

const overlay = ref<HTMLElement | null>(null)
const dialog = ref<HTMLElement | null>(null)
let entry: ModalEntry | null = null
let backdropStarted = false

function recordBackdropStart(event: PointerEvent) {
  backdropStarted = event.target === event.currentTarget
}

function requestClose() {
  if (entry && entry !== topModal()) return
  if (props.dismissible !== false) emit('close')
  else if (entry) focusEntry(entry)
}

function closeFromBackdrop(event: MouseEvent) {
  if (event.detail === 0 || backdropStarted) requestClose()
  backdropStarted = false
}

function activate() {
  // Detached test/render roots must not acquire global document ownership.
  if (entry || !dialog.value?.isConnected || !overlay.value) return
  entry = {
    overlay: overlay.value,
    dialog: dialog.value,
    before: document.activeElement instanceof HTMLElement ? document.activeElement : null,
    lastFocus: null,
    focusObserver: null,
    close: requestClose,
  }
  if (!modalStack.length) {
    bodyOverflow = { value: document.body.style.getPropertyValue('overflow'), priority: document.body.style.getPropertyPriority('overflow') }
    document.body.style.setProperty('overflow', 'hidden')
    document.addEventListener('focusin', handleModalFocus, true)
    document.addEventListener('keydown', handleModalKeydown)
    bodyObserver = new MutationObserver(refreshModalEnvironment)
    bodyObserver.observe(document.body, { childList: true })
  }
  modalStack.push(entry)
  refreshModalEnvironment()
  const activeEntry = entry
  activeEntry.focusObserver = new MutationObserver(() => {
    if (topModal() !== activeEntry || !activeEntry.dialog.isConnected) return
    const focused = document.activeElement
    if (!(focused instanceof HTMLElement) || !activeEntry.dialog.contains(focused) || !eligible(focused)) focusEntry(activeEntry)
  })
  activeEntry.focusObserver.observe(activeEntry.dialog, {
    childList: true, subtree: true, attributes: true,
    attributeFilter: ['disabled', 'hidden', 'inert', 'style', 'class', 'tabindex', 'aria-hidden'],
  })
  const preferred = entry.dialog.querySelector<HTMLElement>(props.initialFocus || '[autofocus], [data-autofocus]')
  if (preferred && eligible(preferred)) preferred.focus({ preventScroll: true })
  else focusEntry(entry)
}

function deactivate() {
  if (!entry) return
  const closing = entry
  closing.focusObserver?.disconnect()
  const wasTop = closing === topModal()
  modalStack.splice(modalStack.indexOf(closing), 1)
  entry = null
  refreshModalEnvironment()
  if (!modalStack.length) {
    bodyObserver?.disconnect()
    bodyObserver = null
    document.removeEventListener('focusin', handleModalFocus, true)
    document.removeEventListener('keydown', handleModalKeydown)
    if (bodyOverflow?.value) document.body.style.setProperty('overflow', bodyOverflow.value, bodyOverflow.priority)
    else document.body.style.removeProperty('overflow')
    bodyOverflow = null
  }
  if (!wasTop) return
  const parent = topModal()
  if (closing.before && eligible(closing.before) && (!parent || parent.dialog.contains(closing.before))) {
    closing.before.focus({ preventScroll: true })
  } else if (parent) {
    focusEntry(parent)
  } else {
    const previousTabindex = document.body.getAttribute('tabindex')
    document.body.setAttribute('tabindex', '-1')
    document.body.focus({ preventScroll: true })
    if (previousTabindex === null) document.body.removeAttribute('tabindex')
    else document.body.setAttribute('tabindex', previousTabindex)
  }
}

watch(() => props.open, open => open ? activate() : deactivate(), { flush: 'post' })
onMounted(() => { if (props.open) activate() })
onBeforeUnmount(deactivate)

const sizeClass = computed(() => {
  switch (props.size) {
    case 'sm': return 'ui-modal__card--sm'
    case 'lg': return 'ui-modal__card--lg'
    case 'xl': return 'ui-modal__card--xl'
    default: return 'ui-modal__card--md'
  }
})
</script>

<style scoped>
.ui-modal__overlay {
  position: fixed;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
  background: var(--color-dialog-overlay);
  z-index: var(--z-modal);
}

.ui-modal__card {
  box-sizing: border-box;
  min-height: 0;
  max-height: calc(100vh - 48px);
  max-height: calc(100dvh - 48px);
  overflow-y: auto;
  overscroll-behavior: contain;
  box-shadow: var(--shadow-dialog);
}

.ui-modal__card--sm {
  width: min(400px, 100%);
}

.ui-modal__card--md {
  width: min(520px, 100%);
}

.ui-modal__card--lg {
  width: min(720px, 100%);
}

.ui-modal__card--xl {
  width: min(980px, 100%);
}

.ui-modal__card--lg,
.ui-modal__card--xl {
  padding: var(--space-5, 2rem);
}
</style>

import { ref } from 'vue'

export type ModifierSource = {
  metaKey?: boolean
  ctrlKey?: boolean
  altKey?: boolean
  key?: string
  code?: string
  type?: string
  getModifierState?: (key: string) => boolean
} | null | undefined

function readModifierState(source: ModifierSource): boolean {
  if (!source) return false
  if (typeof source.getModifierState === 'function') {
    if (source.getModifierState('Meta') || source.getModifierState('Control') || source.getModifierState('Alt')) {
      return true
    }
  }
  if (source.metaKey || source.ctrlKey || source.altKey) {
    return true
  }

  const eventType = typeof source.type === 'string' ? source.type.toLowerCase() : ''
  if (eventType === 'keydown') {
    const key = typeof source.key === 'string' ? source.key.toLowerCase() : ''
    const code = typeof source.code === 'string' ? source.code.toLowerCase() : ''
    if (key === 'meta' || key === 'os' || key === 'super' || code.startsWith('meta')) {
      return true
    }
    if (key === 'control' || key === 'ctrl' || code.startsWith('control')) {
      return true
    }
    if (key === 'alt' || key === 'option' || code.startsWith('alt')) {
      return true
    }
  }

  return false
}

export function useCopyModifier() {
  const copyModifierActive = ref(false)
  let keyHandler: ((event: KeyboardEvent) => void) | null = null
  let focusHandler: (() => void) | null = null

  const updateCopyModifierState = (source: ModifierSource) => {
    copyModifierActive.value = readModifierState(source)
  }

  const resolveCopyModifier = (event?: DragEvent | null): boolean => {
    const activeFromEvent = readModifierState(event)
    if (activeFromEvent) {
      if (!copyModifierActive.value) {
        copyModifierActive.value = true
      }
      return true
    }
    return copyModifierActive.value
  }

  const resetCopyModifier = () => {
    copyModifierActive.value = false
  }

  const bindCopyModifierListeners = () => {
    if (typeof window === 'undefined' || keyHandler) {
      return
    }
    keyHandler = (event: KeyboardEvent) => updateCopyModifierState(event)
    window.addEventListener('keydown', keyHandler, true)
    window.addEventListener('keyup', keyHandler, true)
    focusHandler = () => resetCopyModifier()
    window.addEventListener('focus', focusHandler)
  }

  const unbindCopyModifierListeners = () => {
    if (typeof window === 'undefined') {
      return
    }
    if (keyHandler) {
      window.removeEventListener('keydown', keyHandler, true)
      window.removeEventListener('keyup', keyHandler, true)
      keyHandler = null
    }
    if (focusHandler) {
      window.removeEventListener('focus', focusHandler)
      focusHandler = null
    }
  }

  return {
    copyModifierActive,
    resolveCopyModifier,
    resetCopyModifier,
    bindCopyModifierListeners,
    unbindCopyModifierListeners,
    updateCopyModifierState,
  }
}

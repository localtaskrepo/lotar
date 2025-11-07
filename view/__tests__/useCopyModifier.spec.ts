import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { useCopyModifier } from '../composables/useCopyModifier'

describe('useCopyModifier', () => {
  beforeEach(() => {
    // ensure no lingering key state between tests
    document.dispatchEvent(new KeyboardEvent('keyup'))
  })

  afterEach(() => {
    document.dispatchEvent(new KeyboardEvent('keyup'))
  })

  it('reports active when meta key is pressed', () => {
    const hook = useCopyModifier()
    hook.bindCopyModifierListeners()
    hook.updateCopyModifierState({ metaKey: true })
    expect(hook.copyModifierActive.value).toBe(true)
    hook.resetCopyModifier()
    expect(hook.copyModifierActive.value).toBe(false)
    hook.unbindCopyModifierListeners()
  })

  it('prefers current event modifier state when resolving', () => {
    const hook = useCopyModifier()
    const event = { ctrlKey: true } as any
    const resolved = hook.resolveCopyModifier(event)
    expect(resolved).toBe(true)
    expect(hook.copyModifierActive.value).toBe(true)
    hook.resetCopyModifier()
  })
})

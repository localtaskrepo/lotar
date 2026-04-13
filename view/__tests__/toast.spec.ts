import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { showToast, toasts } from '../components/toast'

describe('showToast', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    toasts.value = []
  })

  afterEach(() => {
    vi.useRealTimers()
    toasts.value = []
  })

  it('adds a toast with message only', () => {
    showToast('hello')
    expect(toasts.value).toHaveLength(1)
    expect(toasts.value[0].message).toBe('hello')
    expect(toasts.value[0].title).toBeUndefined()
  })

  it('adds a toast with title and message', () => {
    showToast('body', 'Title')
    expect(toasts.value[0].title).toBe('Title')
    expect(toasts.value[0].message).toBe('body')
  })

  it('auto-removes toast after default 3 s', () => {
    showToast('temp')
    expect(toasts.value).toHaveLength(1)

    vi.advanceTimersByTime(3000)
    expect(toasts.value).toHaveLength(0)
  })

  it('respects custom duration', () => {
    showToast('long', undefined, 8000)
    vi.advanceTimersByTime(3000)
    expect(toasts.value).toHaveLength(1)

    vi.advanceTimersByTime(5000)
    expect(toasts.value).toHaveLength(0)
  })

  it('assigns unique incrementing ids', () => {
    showToast('a')
    showToast('b')
    expect(toasts.value[0].id).not.toBe(toasts.value[1].id)
    expect(toasts.value[1].id).toBeGreaterThan(toasts.value[0].id)
  })
})

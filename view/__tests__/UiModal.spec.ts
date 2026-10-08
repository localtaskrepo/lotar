import { mount, type VueWrapper } from '@vue/test-utils'
import { nextTick } from 'vue'
import { afterEach, describe, expect, it } from 'vitest'
import UiModal from '../components/UiModal.vue'

const wrappers: VueWrapper[] = []
const initialOverflow = document.body.style.overflow

afterEach(() => {
  for (const wrapper of wrappers.splice(0).reverse()) wrapper.unmount()
  document.body.replaceChildren()
  document.body.style.overflow = initialOverflow
})

function key(target: EventTarget, name: string, shiftKey = false) {
  const event = new KeyboardEvent('keydown', { key: name, shiftKey, bubbles: true, cancelable: true })
  target.dispatchEvent(event)
  return event
}

async function openModal(props: Record<string, unknown> = {}, content = '<button type="button">First</button><button type="button">Last</button>') {
  const wrapper = mount(UiModal, {
    attachTo: document.body,
    props: { open: false, ariaLabel: 'Example dialog', ...props },
    slots: { default: content },
  })
  wrappers.push(wrapper)
  await wrapper.setProps({ open: true })
  const dialog = document.querySelectorAll<HTMLElement>('[role="dialog"]')
  return { wrapper, dialog: dialog[dialog.length - 1]! }
}

describe('UiModal accessibility contract (DEV-69)', () => {
  it('binds its visible title and optional description', async () => {
    const { dialog } = await openModal({ ariaLabelledby: 'test-title', ariaDescribedby: 'test-description' },
      '<h2 id="test-title">Visible title</h2><p id="test-description">Description</p><button>Cancel</button>')
    expect(dialog.getAttribute('aria-labelledby')).toBe('test-title')
    expect(dialog.getAttribute('aria-describedby')).toBe('test-description')
    expect(dialog.getAttribute('aria-modal')).toBe('true')
  })

  it('moves initial focus to the first eligible control', async () => {
    const { dialog } = await openModal({}, '<button disabled>Disabled</button><button hidden>Hidden</button><button>Ready</button>')
    expect(document.activeElement?.textContent).toBe('Ready')
    expect(dialog.contains(document.activeElement)).toBe(true)
  })

  it('prioritizes an explicit safe initial focus', async () => {
    const { dialog } = await openModal({ initialFocus: '[data-cancel]' },
      '<button>Delete</button><button data-cancel>Cancel</button>')
    expect(document.activeElement).toBe(dialog.querySelector('[data-cancel]'))
  })

  it('contains Tab and Shift-Tab at the dynamically computed boundaries', async () => {
    const { dialog } = await openModal()
    const buttons = dialog.querySelectorAll<HTMLButtonElement>('button')
    buttons[1]!.focus()
    expect(key(buttons[1]!, 'Tab').defaultPrevented).toBe(true)
    expect(document.activeElement).toBe(buttons[0])
    key(buttons[0]!, 'Tab', true)
    expect(document.activeElement).toBe(buttons[1])
    buttons[1]!.disabled = true
    key(buttons[0]!, 'Tab')
    expect(document.activeElement).toBe(buttons[0])
  })

  it('uses the dialog itself when there are no eligible controls', async () => {
    const { dialog } = await openModal({}, '<p>Read-only information</p>')
    expect(document.activeElement).toBe(dialog)
    key(dialog, 'Tab')
    expect(document.activeElement).toBe(dialog)
  })

  it('redirects escaped programmatic focus back into the dialog', async () => {
    const outside = document.createElement('button')
    document.body.append(outside)
    const { dialog } = await openModal()
    outside.focus()
    expect(dialog.contains(document.activeElement)).toBe(true)
  })

  it('requests dismissal once on Escape without leaking it to page handlers', async () => {
    const { wrapper, dialog } = await openModal()
    let leaked = 0
    const onWindow = () => { leaked += 1 }
    window.addEventListener('keydown', onWindow)
    try {
      key(dialog.querySelector('button')!, 'Escape')
      expect(wrapper.emitted('close')).toHaveLength(1)
      expect(leaked).toBe(0)
    } finally {
      window.removeEventListener('keydown', onWindow)
    }
  })

  it('blocks Escape and backdrop dismissal while busy', async () => {
    const { wrapper, dialog } = await openModal({ dismissible: false })
    key(dialog, 'Escape')
    document.querySelector('.ui-modal__overlay')!.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    expect(wrapper.emitted('close')).toBeUndefined()
    expect(dialog.contains(document.activeElement)).toBe(true)
    await wrapper.setProps({ dismissible: true })
    key(dialog, 'Escape')
    expect(wrapper.emitted('close')).toHaveLength(1)
  })

  it('dismisses only the top modal and restores focus into its parent', async () => {
    const parent = await openModal({ ariaLabel: 'Parent' })
    const trigger = parent.dialog.querySelectorAll<HTMLButtonElement>('button')[1]!
    trigger.focus()
    const child = await openModal({ ariaLabel: 'Child' })
    key(child.dialog, 'Escape')
    expect(child.wrapper.emitted('close')).toHaveLength(1)
    expect(parent.wrapper.emitted('close')).toBeUndefined()
    await child.wrapper.setProps({ open: false })
    expect(document.activeElement).toBe(trigger)
    expect(document.body.style.overflow).toBe('hidden')
    key(trigger, 'Escape')
    expect(parent.wrapper.emitted('close')).toHaveLength(1)
  })

  it('restores the opener, scroll state and pre-existing inertness on close', async () => {
    document.body.style.setProperty('overflow', 'clip', 'important')
    const opener = document.createElement('button')
    const alreadyInert = document.createElement('section')
    alreadyInert.setAttribute('inert', '')
    document.body.append(opener, alreadyInert)
    opener.focus()
    const { wrapper } = await openModal()
    expect(document.body.style.overflow).toBe('hidden')
    expect(opener.hasAttribute('inert')).toBe(true)
    await wrapper.setProps({ open: false })
    expect(document.activeElement).toBe(opener)
    expect(opener.hasAttribute('inert')).toBe(false)
    expect(alreadyInert.hasAttribute('inert')).toBe(true)
    expect(document.body.style.overflow).toBe('clip')
    expect(document.body.style.getPropertyPriority('overflow')).toBe('important')
  })

  it('cleans up on unmount and tolerates a removed opener', async () => {
    const opener = document.createElement('button')
    document.body.append(opener)
    opener.focus()
    const { wrapper } = await openModal()
    opener.remove()
    wrapper.unmount()
    wrappers.splice(wrappers.indexOf(wrapper), 1)
    expect(document.body.style.overflow).toBe(initialOverflow)
    expect(document.activeElement).toBe(document.body)
  })

  it('makes new background siblings inert while open', async () => {
    const { wrapper } = await openModal()
    const sibling = document.createElement('button')
    document.body.append(sibling)
    await nextTick()
    expect(sibling.hasAttribute('inert')).toBe(true)
    await wrapper.setProps({ open: false })
    expect(sibling.hasAttribute('inert')).toBe(false)
  })

  it('leaves focus and document ownership with the child if its parent unmounts', async () => {
    const parent = await openModal({ ariaLabel: 'Parent' })
    const child = await openModal({ ariaLabel: 'Child' })
    parent.wrapper.unmount()
    wrappers.splice(wrappers.indexOf(parent.wrapper), 1)
    expect(child.dialog.contains(document.activeElement)).toBe(true)
    expect(document.body.style.overflow).toBe('hidden')
    await child.wrapper.setProps({ open: false })
    expect(document.body.style.overflow).toBe(initialOverflow)
    expect(document.activeElement).toBe(document.body)
  })

  it('lets an inner widget consume Escape before dismissing the modal', async () => {
    const { wrapper, dialog } = await openModal()
    const control = dialog.querySelector('button')!
    control.addEventListener('keydown', event => event.preventDefault(), { once: true })
    key(control, 'Escape')
    expect(wrapper.emitted('close')).toBeUndefined()
    key(control, 'Escape')
    expect(wrapper.emitted('close')).toHaveLength(1)
  })

  it('does not dismiss when a pointer begins inside and ends on the backdrop', async () => {
    const { wrapper, dialog } = await openModal()
    const overlay = dialog.closest('.ui-modal__overlay')!
    dialog.dispatchEvent(new Event('pointerdown', { bubbles: true }))
    overlay.dispatchEvent(new MouseEvent('click', { detail: 1, bubbles: true }))
    expect(wrapper.emitted('close')).toBeUndefined()
    overlay.dispatchEvent(new Event('pointerdown', { bubbles: true }))
    overlay.dispatchEvent(new MouseEvent('click', { detail: 1, bubbles: true }))
    expect(wrapper.emitted('close')).toHaveLength(1)
  })

  it('repairs focus when the focused control is disabled during a pending action', async () => {
    const { dialog } = await openModal({ dismissible: false })
    for (const control of dialog.querySelectorAll<HTMLButtonElement>('button')) control.disabled = true
    ;(document.activeElement as HTMLElement).blur()
    await nextTick()
    expect(document.activeElement).toBe(dialog)
  })

  it('repairs focus when an in-dialog subtree becomes inert', async () => {
    const { dialog } = await openModal({}, '<div data-pane><button>First</button></div><button>Available</button>')
    dialog.querySelector('[data-pane]')!.setAttribute('inert', '')
    ;(document.activeElement as HTMLElement).blur()
    await nextTick()
    expect(document.activeElement?.textContent).toBe('Available')
  })

  it('repairs an ignored busy Escape without waiting for a DOM mutation', async () => {
    const { wrapper, dialog } = await openModal({ dismissible: false })
    ;(document.activeElement as HTMLElement).blur()
    key(document, 'Escape')
    expect(wrapper.emitted('close')).toBeUndefined()
    expect(dialog.contains(document.activeElement)).toBe(true)
  })
})

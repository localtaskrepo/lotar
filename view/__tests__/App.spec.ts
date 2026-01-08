import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'
import App from '../App.vue'
import { useTaskPanelController } from '../composables/useTaskPanelController'

describe('App shell', () => {
  it('renders nav links', async () => {
    const router = createRouter({ history: createMemoryHistory(), routes: [{ path: '/', component: { template: '<div />' } }] })
    await router.push('/')
    await router.isReady()
    const wrapper = mount(App, { global: { plugins: [router] } })
    expect(wrapper.text()).toContain('Tasks')
    expect(wrapper.text()).toContain('Insights')
    wrapper.unmount()
  })

  it('prevents TaskPanel and ActivityDrawer overlap', async () => {
    const router = createRouter({ history: createMemoryHistory(), routes: [{ path: '/', component: { template: '<div />' } }] })
    await router.push('/')
    await router.isReady()

    const { state: taskPanelState, openTaskPanel, closeTaskPanel } = useTaskPanelController()
    closeTaskPanel()

    const wrapper = mount(App, {
      global: {
        plugins: [router],
        stubs: {
          TaskPanelHost: true,
          ToastHost: true,
          ActivityDrawer: {
            name: 'ActivityDrawer',
            props: ['open'],
            emits: ['close'],
            template: "<div class='activity-drawer' :data-open=\"open ? 'true' : 'false'\" />",
          },
        },
      },
    })

    // Open task panel, then open Activity: panel should close.
    openTaskPanel({ taskId: 'PRJ-1' })
    await wrapper.vm.$nextTick()
    expect(taskPanelState.open).toBe(true)

    await wrapper.find('button').trigger('click')
    await wrapper.vm.$nextTick()
    expect(taskPanelState.open).toBe(false)
    expect(wrapper.find('.activity-drawer').attributes('data-open')).toBe('true')

    // If Activity is open, opening the task panel should close Activity.
    openTaskPanel({ taskId: 'PRJ-2' })
    await wrapper.vm.$nextTick()
    expect(taskPanelState.open).toBe(true)
    expect(wrapper.find('.activity-drawer').attributes('data-open')).toBe('false')

    wrapper.unmount()
    closeTaskPanel()
  })
})

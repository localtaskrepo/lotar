import { flushPromises, mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import { ref } from 'vue'
import { createMemoryHistory, createRouter } from 'vue-router'
import App from '../App.vue'
import logoLight from '../assets/branding/lotar-logo.svg'
import logoDark from '../assets/branding/lotar-logo-dark.svg'
import { useTaskPanelController } from '../composables/useTaskPanelController'

vi.mock('../composables/useTaskStore', () => ({
  useTaskStore: () => ({
    connectSse: vi.fn(),
    disconnectSse: vi.fn(),
    onTaskError: vi.fn(() => vi.fn()),
    version: ref(0),
    _map: ref(new Map()),
    isHardDeleted: vi.fn(() => false),
  }),
}))

const { state: taskPanelState, openTaskPanel, closeTaskPanel } = useTaskPanelController()

function createTestRouter() {
  return createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div />' } },
      { path: '/insights', component: { template: '<div />' } },
      { path: '/task/:id', component: { template: '<div />' } },
    ],
  })
}

async function mountApp(path: string, query?: Record<string, string | string[]>) {
  const router = createTestRouter()
  await router.push({ path, query })
  await router.isReady()
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
  return wrapper
}

function findActivityButton(wrapper: ReturnType<typeof mount>) {
  const button = wrapper.findAll('button').find((b) => b.text() === 'Activity')
  expect(button, 'Activity button should be rendered').toBeTruthy()
  return button!
}

describe('App shell', () => {
  it('renders supplied light and dark branding with one accessible home link', async () => {
    const wrapper = await mountApp('/insights')
    const brand = wrapper.find('a.brand')
    expect(brand.attributes('aria-label')).toBe('LoTaR home')
    expect(brand.attributes('href')).toBe('/')
    expect(brand.findAll('img')).toHaveLength(2)
    expect(brand.find('.brand__logo--light').attributes('src')).toBe(logoLight)
    expect(brand.find('.brand__logo--dark').attributes('src')).toBe(logoDark)
    for (const image of brand.findAll('img')) expect(image.attributes('alt')).toBe('')
    expect(wrapper.find('[data-testid="global-new-task"]').exists()).toBe(true)
    wrapper.unmount()
  })

  it('returns to Tasks through the branded home link', async () => {
    const router = createTestRouter()
    await router.push('/insights')
    await router.isReady()
    const wrapper = mount(App, { global: { plugins: [router], stubs: { TaskPanelHost: true, ToastHost: true, ActivityDrawer: true } } })
    await wrapper.find('a.brand').trigger('click')
    await router.isReady()
    await flushPromises()
    expect(router.currentRoute.value.path).toBe('/')
    wrapper.unmount()
  })

  it('renders nav links', async () => {
    const wrapper = await mountApp('/')
    expect(wrapper.text()).toContain('Tasks')
    expect(wrapper.text()).toContain('Insights')
    wrapper.unmount()
  })

  it('renders the global new task button on non-Task routes', async () => {
    closeTaskPanel()
    const wrapper = await mountApp('/insights')
    const button = wrapper.find('[data-testid="global-new-task"]')
    expect(button.exists()).toBe(true)
    // Native button semantics keep Enter/Space activation working; jsdom cannot
    // synthesize the keydown-to-click step, so assert the element contract.
    expect(button.element.tagName).toBe('BUTTON')
    expect(button.attributes('type')).toBe('button')
    expect(button.attributes('aria-label')).toBe('New task')
    expect(button.attributes('title')).toBe('New task')
    expect(button.text()).toContain('Task')
    wrapper.unmount()
  })

  it('opens the new task panel with the explicit project query', async () => {
    closeTaskPanel()
    const wrapper = await mountApp('/', { project: 'DEV' })
    await wrapper.find('[data-testid="global-new-task"]').trigger('click')
    await wrapper.vm.$nextTick()
    expect(taskPanelState.open).toBe(true)
    expect(taskPanelState.taskId).toBe('new')
    expect(taskPanelState.initialProject).toBe('DEV')
    wrapper.unmount()
    closeTaskPanel()
  })

  it('uses the first value when the project query is repeated', async () => {
    closeTaskPanel()
    const wrapper = await mountApp('/', { project: ['ABC', 'XYZ'] })
    await wrapper.find('[data-testid="global-new-task"]').trigger('click')
    expect(taskPanelState.initialProject).toBe('ABC')
    wrapper.unmount()
    closeTaskPanel()
  })

  it('uses null project context when none is known instead of a first project', async () => {
    closeTaskPanel()
    const wrapper = await mountApp('/insights')
    await wrapper.find('[data-testid="global-new-task"]').trigger('click')
    expect(taskPanelState.open).toBe(true)
    expect(taskPanelState.taskId).toBe('new')
    expect(taskPanelState.initialProject).toBeNull()
    wrapper.unmount()
    closeTaskPanel()
  })

  it('derives project context from the task detail route id', async () => {
    closeTaskPanel()
    const wrapper = await mountApp('/task/DEV-7')
    await wrapper.find('[data-testid="global-new-task"]').trigger('click')
    expect(taskPanelState.initialProject).toBe('DEV')
    wrapper.unmount()
    closeTaskPanel()

    const precedence = await mountApp('/task/OPS-3', { project: 'QA' })
    await precedence.find('[data-testid="global-new-task"]').trigger('click')
    expect(taskPanelState.initialProject).toBe('QA')
    precedence.unmount()
    closeTaskPanel()
  })

  it('does not reset an already open task panel', async () => {
    closeTaskPanel()
    openTaskPanel({ taskId: 'PRJ-7' })
    const wrapper = await mountApp('/')
    const button = wrapper.find('[data-testid="global-new-task"]')
    expect(button.attributes('disabled')).toBeDefined()
    await button.trigger('click')
    await wrapper.vm.$nextTick()
    expect(taskPanelState.open).toBe(true)
    expect(taskPanelState.taskId).toBe('PRJ-7')
    wrapper.unmount()
    closeTaskPanel()
  })

  it('closes Activity when the global new task panel opens', async () => {
    closeTaskPanel()
    const wrapper = await mountApp('/')
    await findActivityButton(wrapper).trigger('click')
    await wrapper.vm.$nextTick()
    expect(wrapper.find('.activity-drawer').attributes('data-open')).toBe('true')

    await wrapper.find('[data-testid="global-new-task"]').trigger('click')
    await wrapper.vm.$nextTick()
    expect(taskPanelState.open).toBe(true)
    expect(taskPanelState.taskId).toBe('new')
    expect(wrapper.find('.activity-drawer').attributes('data-open')).toBe('false')
    wrapper.unmount()
    closeTaskPanel()
  })

  it('prevents TaskPanel and ActivityDrawer overlap', async () => {
    closeTaskPanel()
    const wrapper = await mountApp('/')

    // Open task panel, then open Activity: panel should close.
    openTaskPanel({ taskId: 'PRJ-1' })
    await wrapper.vm.$nextTick()
    expect(taskPanelState.open).toBe(true)

    await findActivityButton(wrapper).trigger('click')
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

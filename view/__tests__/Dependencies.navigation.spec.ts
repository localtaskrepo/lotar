import { flushPromises, mount } from '@vue/test-utils'
import { defineComponent, h } from 'vue'
import { createMemoryHistory, createRouter, RouterView } from 'vue-router'
import { afterEach, describe, expect, it, vi } from 'vitest'
import Dependencies from '../pages/Dependencies.vue'
import DependencyGraph from '../components/DependencyGraph.vue'
import FilterBar from '../components/FilterBar.vue'
import TaskPanelHost from '../components/TaskPanelHost.vue'
import { useTaskPanelController } from '../composables/useTaskPanelController'

const mocks = vi.hoisted(() => ({
    handlers: new Map<string, () => void>(),
    reconnect: undefined as (() => void) | undefined,
}))
vi.mock('../api/client', () => ({
    api: { listTasks: vi.fn(async () => ({ tasks: [], total: 0 })), suggestTasks: vi.fn(async () => []) },
}))
vi.mock('../composables/useConfig', async () => {
    const { ref } = await import('vue')
    return {
        useConfig: () => ({
            statuses: ref(['Todo', 'Done']),
            priorities: ref(['Low', 'High']),
            types: ref(['Feature', 'Bug']),
            customFields: ref([]),
            tags: ref([]),
            members: ref([]),
            scope: ref('DEV'),
            refresh: vi.fn(async () => {}),
        }),
    }
})
vi.mock('../composables/useSprints', async () => {
    const { ref } = await import('vue')
    return {
        useSprints: () => ({ sprints: ref([]), refresh: vi.fn(async () => {}) }),
        useSprintFilterOptions: () => ref([]),
    }
})
vi.mock('../composables/useProjects', async () => {
    const { ref } = await import('vue')
    return {
        useProjects: () => ({
            projects: ref([
                { prefix: 'DEV', name: 'Development' },
                { prefix: 'OTHER', name: 'Other' },
            ]),
            refresh: vi.fn(async () => {}),
        }),
    }
})
vi.mock('../composables/useSse', () => ({
    useSse: (_path: string, _params: unknown, options: { onReconnect: () => void }) => {
        mocks.reconnect = options.onReconnect
        return { on: (event: string, fn: () => void) => mocks.handlers.set(event, fn), close: vi.fn() }
    },
}))

const wrappers: ReturnType<typeof mount>[] = []
afterEach(() => {
    wrappers.splice(0).forEach((wrapper) => wrapper.unmount())
    useTaskPanelController().closeTaskPanel()
    mocks.handlers.clear()
    localStorage.clear()
    vi.useRealTimers()
    vi.unstubAllGlobals()
})

async function setup(path: string) {
    vi.stubGlobal('EventSource', class {})
    const router = createRouter({
        history: createMemoryHistory(),
        routes: [{ path: '/dependencies', component: Dependencies }],
    })
    await router.push(path)
    await router.isReady()
    let controller!: ReturnType<typeof useTaskPanelController>
    const wrapper = mount(
        defineComponent({
            setup() {
                controller = useTaskPanelController()
                return () => [h(RouterView), h(TaskPanelHost)]
            },
        }),
        {
            global: {
                plugins: [router],
                stubs: {
                    DependencyGraph: { props: ['root', 'filters'], emits: ['activate', 'loaded'], template: '<div />' },
                    TaskPanel: { name: 'TaskPanel', props: ['taskId', 'open'], template: '<div />' },
                },
            },
        },
    )
    wrappers.push(wrapper)
    await flushPromises()
    return { router, controller, wrapper }
}

describe('Dependency page atomic router drill-down', () => {
    it.each([false, true])(
        'preserves filters, identity, callbacks, SSE and history with original panel=%s',
        async (initialPanel) => {
            const { router, controller, wrapper } = await setup(
                `/dependencies?root=DEV-9&q=old%20text&project=DEV&status=Todo&relationships=all&depth=3&sort_by=title&order=asc${initialPanel ? '&taskPanel=DEV-9' : ''}#graph`,
            )
            const original = { ...router.currentRoute.value.query }
            const graph = wrapper.getComponent(DependencyGraph)
            const element = graph.element
            const input = wrapper.get('[data-testid="filter-search"]')
            const inputElement = input.element
            const onUpdated = vi.fn(),
                onClose = vi.fn()
            controller.openTaskPanel({ taskId: 'DEV-9', onUpdated, onClose })
            const push = vi.spyOn(router, 'push')
            const replace = vi.spyOn(router, 'replace')
            let resumePush!: () => void
            const removeGuard = router.beforeEach(
                () =>
                    new Promise<void>((resolve) => {
                        resumePush = resolve
                    }),
            )
            vi.useFakeTimers()
            await input.trigger('focus')
            await input.trigger('blur')
            graph.vm.$emit('activate', 'DEV-1')
            await flushPromises()
            await vi.advanceTimersByTimeAsync(200)
            await flushPromises()
            expect(router.currentRoute.value.query).toEqual(original)
            expect(graph.props('root')).toBe('DEV-1')
            expect(
                wrapper
                    .getComponent(FilterBar)
                    .emitted('update:value')
                    ?.some(([value]) => (value as Record<string, string>).q === 'DEV-1'),
            ).toBe(true)
            expect(replace).not.toHaveBeenCalled()
            removeGuard()
            resumePush()
            await flushPromises()
            vi.useRealTimers()
            expect(push).toHaveBeenCalledTimes(1)
            expect(router.currentRoute.value.query).toEqual({
                ...original,
                root: 'DEV-1',
                q: 'DEV-1',
                taskPanel: 'DEV-1',
            })
            expect(graph.props()).toMatchObject({ root: 'DEV-1', filters: { deletion: 'active', status: 'Todo' } })
            expect(graph.props('filters')).not.toHaveProperty('q')
            expect(controller.state.taskId).toBe('DEV-1')
            expect(wrapper.get('[data-testid="filter-search"]').element).toBe(inputElement)
            expect((inputElement as HTMLInputElement).value).toBe('DEV-1')
            await controller.navigateTask('DEV-9')
            await flushPromises()
            const filterBar = wrapper.getComponent(FilterBar)
            filterBar.vm.$emit('update:value', { ...filterBar.props('value'), priority: 'High' })
            await flushPromises()
            mocks.handlers.get('task_updated')?.()
            mocks.reconnect?.()
            await flushPromises()
            expect(router.currentRoute.value.query).toEqual({
                ...original,
                relationships: 'dependency,hierarchy,related,fixes,duplicate',
                root: 'DEV-9',
                q: 'DEV-9',
                taskPanel: 'DEV-9',
                priority: 'High',
            })
            expect(graph.props()).toMatchObject({ root: 'DEV-9', filters: { status: 'Todo', priority: 'High' } })
            expect(wrapper.getComponent(DependencyGraph).element).toBe(element)
            router.back()
            await flushPromises()
            expect(graph.props('root')).toBe('DEV-1')
            expect(controller.state.taskId).toBe('DEV-1')
            controller.notifyUpdated({ id: 'DEV-1' } as Parameters<typeof controller.notifyUpdated>[0])
            expect(onUpdated).toHaveBeenCalledOnce()
            expect(onClose).not.toHaveBeenCalled()
            router.forward()
            await flushPromises()
            expect(controller.state.taskId).toBe('DEV-9')
            expect(graph.props('root')).toBe('DEV-9')
            router.back()
            await flushPromises()
            expect(controller.state.taskId).toBe('DEV-1')
            wrapper.findComponent({ name: 'TaskPanel' }).vm.$emit('close')
            await flushPromises()
            const { taskPanel: _, ...withoutPanel } = original
            expect(router.currentRoute.value.query).toEqual({ ...withoutPanel, root: 'DEV-1', q: 'DEV-1' })
            expect(onClose).toHaveBeenCalledOnce()
            router.back()
            await flushPromises()
            expect(router.currentRoute.value.query).toEqual(original)
            expect(graph.props('root')).toBe('DEV-9')
            expect(controller.state.open).toBe(initialPanel)
            if (initialPanel) expect(controller.state.taskId).toBe('DEV-9')
            router.forward()
            await flushPromises()
            expect(graph.props('root')).toBe('DEV-1')
            expect(controller.state.open).toBe(false)
            expect(router.currentRoute.value.query).toEqual({ ...withoutPanel, root: 'DEV-1', q: 'DEV-1' })
            expect(router.currentRoute.value.hash).toBe('#graph')
            router.back()
            await flushPromises()
            expect(router.currentRoute.value.query).toEqual(original)
            expect(controller.state.open).toBe(initialPanel)
            expect(wrapper.get('[data-testid="filter-search"]').element).toBe(inputElement)
        },
    )
    it('retains root-only query identity through default hydration but still clears and applies real typed changes', async () => {
        const { router, wrapper } = await setup('/dependencies?root=DEV-9&project=DEV#graph')
        const input = wrapper.get('[data-testid="filter-search"]')
        expect(router.currentRoute.value.query).toEqual({ root: 'DEV-9', project: 'DEV' })
        expect((input.element as HTMLInputElement).value).toBe('DEV-9')
        await wrapper.get('button[aria-label="Clear filters"]').trigger('click')
        await flushPromises()
        expect(router.currentRoute.value.query).not.toHaveProperty('root')
        expect(router.currentRoute.value.query).not.toHaveProperty('q')
        await input.setValue('genuine search')
        await flushPromises()
        expect(router.currentRoute.value.query.q).toBe('genuine search')
        expect(wrapper.getComponent(DependencyGraph).props('filters')).toMatchObject({ q: 'genuine search' })
        await input.setValue('status:Todo changed')
        await input.trigger('keydown', { key: 'Enter' })
        await flushPromises()
        expect(router.currentRoute.value.query).toMatchObject({ q: 'changed', status: 'Todo' })
        expect(wrapper.get('[data-testid="filter-search"]').element).toBe(input.element)
        expect(router.currentRoute.value.hash).toBe('#graph')
    })
})

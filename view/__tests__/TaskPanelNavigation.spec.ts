import { flushPromises, mount } from '@vue/test-utils'
import { defineComponent, h } from 'vue'
import { createMemoryHistory, createRouter, RouterView } from 'vue-router'
import { afterEach, describe, expect, it, vi } from 'vitest'
import TaskPanelHost from '../components/TaskPanelHost.vue'
import TaskDetails from '../pages/TaskDetails.vue'
import { useTaskPanelController } from '../composables/useTaskPanelController'

vi.mock('../api/client', () => ({ api: { listTasks: vi.fn(async () => ({ tasks: [], total: 0 })) } }))

const wrappers: ReturnType<typeof mount>[] = []
afterEach(() => {
    useTaskPanelController().closeTaskPanel()
    wrappers.splice(0).forEach((wrapper) => wrapper.unmount())
})

async function setup(path: string) {
    let controller!: ReturnType<typeof useTaskPanelController>
    const originUnmount = vi.fn()
    const Origin = defineComponent({ unmounted: originUnmount, render: () => h('div', 'Origin graph') })
    const router = createRouter({
        history: createMemoryHistory(),
        routes: [
            { path: '/', component: Origin },
            { path: '/dependencies', component: Origin },
            { path: '/board', component: Origin },
            { path: '/tasks', component: Origin },
            { path: '/task/:id', component: TaskDetails },
        ],
    })
    await router.push(path)
    await router.isReady()
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
                stubs: { TaskPanel: { name: 'TaskPanel', props: ['taskId', 'open'], template: '<div />' } },
            },
        },
    )
    wrappers.push(wrapper)
    await flushPromises()
    return { router, controller, wrapper, originUnmount }
}

describe('Task panel router history', () => {
    it('pushes atomic dependency roots with details and restores the original state', async () => {
        const { router, controller, wrapper, originUnmount } = await setup(
            '/dependencies?root=DEV-9&q=text&project=DEV&status=Todo&depth=3&relationships=all&sort_by=id#graph',
        )
        const original = { ...router.currentRoute.value.query }
        await controller.navigateTask('DEV-1')
        await flushPromises()
        expect(router.currentRoute.value.query).toEqual({ ...original, root: 'DEV-1', q: 'DEV-1', taskPanel: 'DEV-1' })
        expect(controller.state.taskId).toBe('DEV-1')
        await controller.navigateTask('DEV-2')
        await flushPromises()
        router.back()
        await flushPromises()
        expect(router.currentRoute.value.query).toEqual({ ...original, root: 'DEV-1', q: 'DEV-1', taskPanel: 'DEV-1' })
        expect(controller.state.taskId).toBe('DEV-1')
        router.back()
        await flushPromises()
        expect(router.currentRoute.value.query).toEqual(original)
        expect(controller.state.open).toBe(false)
        router.forward()
        await flushPromises()
        expect(controller.state.taskId).toBe('DEV-1')
        wrapper.findComponent({ name: 'TaskPanel' }).vm.$emit('close')
        await flushPromises()
        expect(router.currentRoute.value.query).toEqual({ ...original, root: 'DEV-1', q: 'DEV-1' })
        expect(router.currentRoute.value.hash).toBe('#graph')
        expect(originUnmount).not.toHaveBeenCalled()
    })
    it('returns from detail B to detail A without visiting the list', async () => {
        const { router, controller, originUnmount } = await setup('/task/DEV-1')
        expect(controller.state.taskId).toBe('DEV-1')
        await controller.navigateTask('DEV-2')
        await flushPromises()
        expect(router.currentRoute.value.path).toBe('/task/DEV-2')
        expect(controller.state.taskId).toBe('DEV-2')
        router.back()
        await flushPromises()
        expect(router.currentRoute.value.path).toBe('/task/DEV-1')
        expect(controller.state.taskId).toBe('DEV-1')
        expect(originUnmount).not.toHaveBeenCalled()
        controller.closeTaskPanel()
        await flushPromises()
        expect(router.currentRoute.value.path).toBe('/')
        expect(controller.state.open).toBe(false)
    })

    it.each(['/board', '/tasks'])(
        'keeps %s overlay origin, filters and callbacks through A to B and Back',
        async (path) => {
            const { router, controller, originUnmount, wrapper } = await setup(`${path}?root=DEV-1&project=DEV#graph`)
            const onUpdated = vi.fn(),
                onClose = vi.fn(),
                onCreated = vi.fn()
            controller.openTaskPanel({ taskId: 'DEV-1', onUpdated, onClose, onCreated })
            await controller.navigateTask('DEV-2')
            await flushPromises()
            expect(router.currentRoute.value.path).toBe(path)
            expect(router.currentRoute.value.query).toEqual({ root: 'DEV-1', project: 'DEV', taskPanel: 'DEV-2' })
            expect(router.currentRoute.value.hash).toBe('#graph')
            expect(controller.state.taskId).toBe('DEV-2')
            router.back()
            await flushPromises()
            expect(controller.state.taskId).toBe('DEV-1')
            const task = { id: 'DEV-1' } as Parameters<typeof controller.notifyUpdated>[0]
            controller.notifyUpdated(task)
            controller.notifyCreated(task)
            expect(onUpdated).toHaveBeenCalledExactlyOnceWith(task)
            expect(onCreated).toHaveBeenCalledExactlyOnceWith(task)
            expect(onClose).not.toHaveBeenCalled()
            expect(originUnmount).not.toHaveBeenCalled()
            wrapper.findComponent({ name: 'TaskPanel' }).vm.$emit('close')
            await flushPromises()
            expect(onClose).toHaveBeenCalledOnce()
            expect(router.currentRoute.value.query).toEqual({ root: 'DEV-1', project: 'DEV' })
            controller.notifyUpdated(task)
            expect(onUpdated).toHaveBeenCalledOnce()
        },
    )
})

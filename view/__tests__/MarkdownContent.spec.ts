import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import MarkdownContent from '../components/MarkdownContent.vue'

describe('MarkdownContent', () => {
    it('renders basic markdown', () => {
        const wrapper = mount(MarkdownContent, {
            props: {
                source: 'Hello **world**',
            },
        })

        const strong = wrapper.find('strong')
        expect(strong.exists()).toBe(true)
        expect(strong.text()).toBe('world')
    })

    it('sanitizes unsafe HTML', () => {
        const wrapper = mount(MarkdownContent, {
            props: {
                source: 'Hi <script>alert(1)</script> there',
            },
        })

        expect(wrapper.html()).not.toContain('<script')
        expect(wrapper.text()).toContain('Hi')
        expect(wrapper.text()).toContain('there')
    })
})

const referenceMocks = vi.hoisted(() => ({
    getTask: vi.fn(),
    listProjects: vi.fn(async () => ({ projects: [{ name: 'Dev', prefix: 'DEV' }], total: 1, limit: 500, offset: 0 })),
    navigateTask: vi.fn(async () => {}),
}))

vi.mock('../api/client', () => ({
    api: {
        getTask: referenceMocks.getTask,
        listProjects: referenceMocks.listProjects,
        showConfig: vi.fn(async () => ({})),
    },
}))

vi.mock('../composables/useTaskPanelController', () => ({
    useTaskPanelController: () => ({ navigateTask: referenceMocks.navigateTask }),
}))

describe('MarkdownContent task references', () => {
    beforeEach(() => {
        referenceMocks.getTask.mockReset()
        referenceMocks.navigateTask.mockClear()
    })

    afterEach(() => {
        document.body.innerHTML = ''
    })

    async function mountWithReference(source: string) {
        const wrapper = mount(MarkdownContent, { props: { source }, attachTo: document.body })
        await flushPromises()
        return wrapper
    }

    it('shows the shared hover card with every field, custom fields included', async () => {
        referenceMocks.getTask.mockResolvedValue({
            id: 'DEV-41',
            title: 'Referenced work',
            status: 'InProgress',
            priority: 'High',
            task_type: 'Feature',
            assignee: 'priya',
            tags: ['ui'],
            sprints: [],
            custom_fields: { team: 'Platform' },
            modified: '',
            due_date: '',
        })
        const wrapper = await mountWithReference('Blocked by DEV-41 until Friday')
        const link = wrapper.find('a.task-ref')
        expect(link.attributes('href')).toBe('/task/DEV-41')

        await link.trigger('mouseover')
        await flushPromises()

        const card = document.body.querySelector('.task-hover-card.is-open')
        expect(card?.textContent).toContain('Referenced work')
        expect(card?.textContent).toContain('InProgress')
        expect(card?.textContent).toContain('Platform')
        expect(referenceMocks.getTask).toHaveBeenCalledWith('DEV-41')

        await link.trigger('mouseout')
        expect(document.body.querySelector('.task-hover-card')).toBeNull()
        wrapper.unmount()
    })

    it('opens the task on click without reaching clickable parents', async () => {
        const wrapper = await mountWithReference('See DEV-42')
        const parentClick = vi.fn()
        wrapper.find('.markdown-content').element.parentElement?.addEventListener('click', parentClick)

        await wrapper.find('a.task-ref').trigger('click', { button: 0 })

        expect(referenceMocks.navigateTask).toHaveBeenCalledWith('DEV-42')
        expect(parentClick).not.toHaveBeenCalled()
        wrapper.unmount()
    })

    it('leaves modifier clicks to the browser', async () => {
        const wrapper = await mountWithReference('See DEV-43')
        let defaultPrevented: boolean | null = null
        wrapper.find('.markdown-content').element.addEventListener('click', (event) => {
            defaultPrevented = event.defaultPrevented
            event.preventDefault() // jsdom cannot follow the link
        })
        await wrapper.find('a.task-ref').trigger('click', { button: 0, metaKey: true })
        expect(defaultPrevented).toBe(false)
        expect(referenceMocks.navigateTask).not.toHaveBeenCalled()
        wrapper.unmount()
    })

    it('does not link when references are turned off', async () => {
        const wrapper = mount(MarkdownContent, { props: { source: 'See DEV-44', taskReferences: false } })
        await flushPromises()
        expect(wrapper.find('a.task-ref').exists()).toBe(false)
    })
})

import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import TaskMoveDialog from '../components/TaskMoveDialog.vue'

const api = vi.hoisted(() => ({
    showConfig: vi.fn(),
    moveTask: vi.fn(),
}))
vi.mock('../api/client', () => ({ api }))

const projects = [
    { prefix: 'DEV', name: 'Development' },
    { prefix: 'WEB', name: 'Website' },
    { prefix: 'OPS', name: 'Operations' },
]

function preview(request: Record<string, any>) {
    const to = request.project === 'WEB' ? 'WEB-7' : 'OPS-2'
    const status = request.status ?? (request.project === 'WEB' ? 'Backlog' : 'Todo')
    return {
        from: 'DEV-3',
        to,
        dry_run: request.dry_run,
        changes: status === 'InProgress' ? [] : [{ field: 'status', old: 'InProgress', new: status }],
        task: { id: to, status, task_type: 'Bug', priority: 'High' },
    }
}

describe('TaskMoveDialog (DEV-183)', () => {
    beforeEach(() => {
        api.showConfig.mockImplementation(async (project: string) => ({
            issue_states: project === 'WEB' ? ['Backlog', 'InProgress', 'Shipped'] : ['Todo', 'Done'],
            issue_types: ['Bug', 'Feature'],
            issue_priorities: ['Low', 'High'],
        }))
        api.moveTask.mockImplementation(async (request: Record<string, any>) => preview(request))
    })

    afterEach(() => {
        vi.clearAllMocks()
        document.body.innerHTML = ''
    })

    async function open(initialTarget = 'WEB') {
        const wrapper = mount(TaskMoveDialog, {
            props: { open: true, taskId: 'DEV-3', initialTarget, projects },
            attachTo: document.body,
        })
        await flushPromises()
        return wrapper
    }

    it('previews the new ID and the values that change, then moves', async () => {
        const wrapper = await open()
        const dialog = document.body.querySelector('[data-testid="task-move-dialog"]')!
        const targets = [...dialog.querySelectorAll('[data-testid="task-move-project"] option')].map((o) =>
            o.textContent?.trim(),
        )
        expect(targets.some((label) => label?.includes('DEV'))).toBe(false)
        expect(dialog.querySelector('[data-testid="task-move-new-id"]')?.textContent).toContain('WEB-7')
        expect(dialog.textContent).toContain('Status: InProgress → Backlog')
        expect(api.moveTask).toHaveBeenCalledWith(
            expect.objectContaining({ id: 'DEV-3', project: 'WEB', dry_run: true }),
        )

        const status = dialog.querySelector<HTMLSelectElement>('[data-testid="task-move-status"]')!
        status.value = 'InProgress'
        status.dispatchEvent(new Event('change'))
        await flushPromises()
        expect(api.moveTask).toHaveBeenLastCalledWith(expect.objectContaining({ status: 'InProgress', dry_run: true }))
        expect(dialog.textContent).not.toContain('Status: InProgress')

        dialog.querySelector<HTMLButtonElement>('[data-testid="task-move-submit"]')!.click()
        await flushPromises()
        expect(api.moveTask).toHaveBeenLastCalledWith(expect.objectContaining({ status: 'InProgress', dry_run: false }))
        expect(wrapper.emitted('moved')?.[0]?.[0]).toMatchObject({ to: 'WEB-7' })
        wrapper.unmount()
    })

    it('shows why a move cannot happen and keeps the button off', async () => {
        api.moveTask.mockRejectedValue(new Error("Custom field 'owner' is not allowed in this project."))
        const wrapper = await open('OPS')
        const dialog = document.body.querySelector('[data-testid="task-move-dialog"]')!
        expect(dialog.querySelector('[data-testid="task-move-error"]')?.textContent).toContain("'owner'")
        expect(dialog.querySelector<HTMLButtonElement>('[data-testid="task-move-submit"]')!.disabled).toBe(true)
        wrapper.unmount()
    })
})

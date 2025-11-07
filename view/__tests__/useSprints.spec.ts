import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { defineComponent } from 'vue'

vi.mock('../api/client', () => ({
    api: {
        sprintList: vi.fn(),
    },
}))

describe('useSprints', () => {
    beforeEach(async () => {
        vi.resetModules()
        vi.clearAllMocks()
    })

    async function mountHarness() {
        const exposed: any = {}
        const { useSprints } = await import('../composables/useSprints')
        const Comp = defineComponent({
            setup() {
                const api = useSprints()
                Object.assign(exposed, api)
                return () => null
            },
        })
        const wrapper = mount(Comp)
        return { wrapper, exposed }
    }

    it('loads sprint summaries and exposes active defaults', async () => {
        const { api } = await import('../api/client')
            ; (api.sprintList as any).mockResolvedValue({
                status: 'ok',
                count: 1,
                sprints: [
                    {
                        id: 5,
                        label: 'Iteration 5',
                        display_name: 'Iteration 5',
                        state: 'active',
                        planned_start: '2025-10-01T09:00:00Z',
                        planned_end: null,
                        actual_start: '2025-10-01T09:00:00Z',
                        actual_end: null,
                        computed_end: null,
                        warnings: [],
                    },
                ],
            })
        const { wrapper, exposed } = await mountHarness()
        await exposed.refresh(true)
        expect(exposed.sprints.value).toHaveLength(1)
        expect(exposed.sprints.value[0].id).toBe(5)
        expect(exposed.active.value[0].id).toBe(5)
        expect(exposed.defaultSprintId.value).toBe(5)
        wrapper.unmount()
    })

    it('normalizes missing warnings to an empty array', async () => {
        const { api } = await import('../api/client')
            ; (api.sprintList as any).mockResolvedValue({
                status: 'ok',
                count: 1,
                sprints: [
                    {
                        id: 2,
                        label: 'Iteration 2',
                        display_name: 'Iteration 2',
                        state: 'pending',
                    },
                ],
            })
        const { wrapper, exposed } = await mountHarness()
        await exposed.refresh(true)
        expect(exposed.sprints.value[0].warnings).toEqual([])
        wrapper.unmount()
    })

    it('propagates errors when the API fails', async () => {
        const { api } = await import('../api/client')
            ; (api.sprintList as any)
                .mockResolvedValueOnce({ status: 'ok', count: 0, sprints: [] })
                .mockRejectedValueOnce(new Error('boom'))
        const { wrapper, exposed } = await mountHarness()
        await expect(exposed.refresh(true)).rejects.toThrow('boom')
        wrapper.unmount()
    })
})

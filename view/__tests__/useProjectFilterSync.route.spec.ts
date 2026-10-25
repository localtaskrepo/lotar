import { beforeEach, describe, expect, it, vi } from 'vitest'
import { reactive, ref } from 'vue'

const route = reactive({ path: '/insights', query: {} as Record<string, string> })
const push = vi.fn()
vi.mock('vue-router', () => ({ useRoute: () => route, useRouter: () => ({ push }) }))

const { useProjectFilterSync } = await import('../composables/useFilterBuilder')

describe('useProjectFilterSync URL sync', () => {
    beforeEach(() => {
        push.mockClear()
        route.query = { view: 'team', period: '90d', status: 'todo' }
    })

    it('keeps the listed view-state keys when the project changes (DEV-193)', () => {
        const { onFilterUpdate } = useProjectFilterSync(ref(''), ref({}), {
            routePath: '/insights',
            keepQuery: ['view', 'period', 'from', 'to'],
        })
        onFilterUpdate({ project: 'ATLS' })
        expect(push).toHaveBeenCalledWith({
            path: '/insights',
            query: { view: 'team', period: '90d', project: 'ATLS' },
        })
    })

    it('replaces the whole query when nothing is kept', () => {
        const { onFilterUpdate } = useProjectFilterSync(ref(''), ref({}), { routePath: '/board' })
        onFilterUpdate({ project: 'ATLS' })
        expect(push).toHaveBeenCalledWith({ path: '/board', query: { project: 'ATLS' } })
    })
})

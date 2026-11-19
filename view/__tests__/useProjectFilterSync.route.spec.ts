import { beforeEach, describe, expect, it, vi } from 'vitest'
import { reactive, ref } from 'vue'

const route = reactive({ path: '/insights', query: {} })
const push = vi.fn()
const leaveGuards: Array<() => void> = []
const afterEachHooks: Array<() => void> = []
const afterEach = vi.fn((hook: () => void) => {
    afterEachHooks.push(hook)
    return () => afterEachHooks.splice(afterEachHooks.indexOf(hook), 1)
})
vi.mock('vue-router', () => ({
    onBeforeRouteLeave: (guard: () => void) => leaveGuards.push(guard),
    useRoute: () => route,
    useRouter: () => ({ push, afterEach }),
}))

const { useProjectFilterSync } = await import('../composables/useFilterBuilder')

describe('useProjectFilterSync URL sync', () => {
    beforeEach(() => {
        push.mockClear()
        leaveGuards.length = 0
        afterEachHooks.length = 0
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
    it('does not cancel a move to another page that is still loading (DEV-720)', () => {
        const { onFilterUpdate } = useProjectFilterSync(ref(''), ref({}), { routePath: '/boards' })
        for (const guard of leaveGuards) guard()
        onFilterUpdate({ project: 'KEY' })
        expect(push).not.toHaveBeenCalled()

        // The move ended (for example it was cancelled): the page syncs its URL again.
        for (const hook of [...afterEachHooks]) hook()
        onFilterUpdate({ project: 'ATLS' })
        expect(push).toHaveBeenCalledWith({ path: '/boards', query: { project: 'ATLS' } })
    })
})

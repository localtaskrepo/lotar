import { describe, expect, it, vi } from 'vitest'
import { ref } from 'vue'
import { buildServerFilter, normalizeFilter, useProjectFilterSync } from '../composables/useFilterBuilder'

describe('useProjectFilterSync', () => {
    it('moves the project key out of the filter and into the project ref', () => {
        const project = ref('')
        const filter = ref<Record<string, string>>({})
        const { onFilterUpdate } = useProjectFilterSync(project, filter)

        onFilterUpdate({ project: 'DEV', status: 'todo' })

        expect(project.value).toBe('DEV')
        expect(filter.value).toEqual({ status: 'todo' })
    })

    it('notifies onProjectChange only when the payload carries a project key', () => {
        const project = ref('DEV')
        const filter = ref<Record<string, string>>({})
        const onProjectChange = vi.fn()
        const { onFilterUpdate } = useProjectFilterSync(project, filter, { onProjectChange })

        onFilterUpdate({ status: 'todo' })
        expect(onProjectChange).not.toHaveBeenCalled()

        onFilterUpdate({ project: '  FA  ', status: 'todo' })
        expect(onProjectChange).toHaveBeenCalledWith('FA')
        expect(project.value).toBe('FA')
    })

    it('keeps the filter object identity when nothing changed', () => {
        const project = ref('')
        const filter = ref<Record<string, string>>({ status: 'todo' })
        const { onFilterUpdate } = useProjectFilterSync(project, filter)
        const before = filter.value

        onFilterUpdate({ project: '', status: 'todo' })

        expect(filter.value).toBe(before)
        expect(filter.value).toEqual({ status: 'todo' })
    })

    it('clearFilters resets the filter and delegates to the filter bar', () => {
        const project = ref('')
        const filter = ref<Record<string, string>>({ status: 'todo' })
        const filterBarRef = ref<{ clear?: () => void } | null>(null)
        const clear = vi.fn()
        filterBarRef.value = { clear }
        const { clearFilters } = useProjectFilterSync(project, filter)

        clearFilters(filterBarRef)

        expect(filter.value).toEqual({})
        expect(clear).toHaveBeenCalled()
    })
})

describe('buildServerFilter', () => {
    it('forwards smart filters, order, and a valid sort_by to the server', () => {
        const { serverFilter, normalized } = buildServerFilter(
            {
                q: 'cli',
                status: 'todo,doing',
                assignee: '__none__',
                due: 'today',
                recent: '7d',
                needs: 'effort,due',
                sort_by: 'custom:Risk',
                order: 'asc',
            },
            'DEV',
        )
        expect(serverFilter.project).toBe('DEV')
        expect(serverFilter.q).toBe('cli')
        expect(serverFilter.status).toEqual(['todo', 'doing'])
        expect(serverFilter.assignee).toBe('__none__')
        expect(serverFilter.due).toBe('today')
        expect(serverFilter.recent).toBe('7d')
        expect(serverFilter.needs).toBe('effort,due')
        expect(serverFilter.sort_by).toBe('custom:Risk')
        expect(serverFilter.order).toBe('asc')
        expect(normalized.sort_by).toBe('custom:Risk')
    })

    it('normalizes the field: alias and forwards invalid sort values raw', () => {
        const { serverFilter, normalized } = buildServerFilter({ sort_by: 'field:Risk' }, '')
        expect(serverFilter.sort_by).toBe('custom:Risk')

        // Invalid values reach the server untouched so its strict parser
        // errors instead of the UI silently defaulting the sort.
        const invalid = buildServerFilter({ sort_by: 'bogus' }, '')
        expect(invalid.serverFilter.sort_by).toBe('bogus')
        expect(invalid.normalized.sort_by).toBe('bogus')
        const invalidOrder = buildServerFilter({ order: 'sideways' }, '')
        expect(invalidOrder.serverFilter.order).toBe('sideways')
    })

    it('omits empty smart filters but forwards malformed tokens raw', () => {
        const omitted = buildServerFilter({ due: '', recent: '', needs: '' }, '')
        expect(omitted.serverFilter.due).toBeUndefined()
        expect(omitted.serverFilter.recent).toBeUndefined()
        expect(omitted.serverFilter.needs).toBeUndefined()
        // Backend is strict: malformed values must reach it and error instead
        // of being silently sanitized into a default.
        const malformed = buildServerFilter({ due: 'yesterdayish', recent: '3d', needs: 'banana' }, '')
        expect(malformed.serverFilter.due).toBe('yesterdayish')
        expect(malformed.serverFilter.recent).toBe('3d')
        expect(malformed.serverFilter.needs).toBe('banana')
    })

    it('defaults order to desc', () => {
        const { serverFilter } = buildServerFilter({}, '')
        expect(serverFilter.order).toBe('desc')
    })

    it('keeps unknown keys as extras and forwards them verbatim', () => {
        const { serverFilter, extras } = buildServerFilter({ component: 'ui' }, '')
        expect(extras).toEqual({ component: 'ui' })
        expect((serverFilter as any).component).toBe('ui')
    })
})

describe('normalizeFilter', () => {
    it('round-trips invalid sort_by for server-side rejection', () => {
        const { normalized, extras } = normalizeFilter({ sort_by: 'bogus' })
        expect(normalized.sort_by).toBe('bogus')
        expect(extras.sort_by).toBeUndefined()
    })

    it('keeps invalid order values instead of coercing them', () => {
        const { normalized } = normalizeFilter({ order: 'nope' })
        expect(normalized.order).toBe('nope')
        const absent = normalizeFilter({})
        expect(absent.normalized.order).toBe('desc')
    })

    it('forwards sprints raw when any token is not a positive integer', () => {
        const strict = buildServerFilter({ sprints: '3,7' }, '')
        expect(strict.serverFilter.sprints).toEqual([3, 7])
        const widened = buildServerFilter({ sprints: '3,x,7' }, '')
        expect(widened.serverFilter.sprints).toBe('3,x,7')
        const zero = buildServerFilter({ sprints: '3,0' }, '')
        expect(zero.serverFilter.sprints).toBe('3,0')
    })

    it('normalizes the CLI alias so the URL stays canonical', () => {
        const { normalized } = normalizeFilter({ sort_by: 'due-date' })
        expect(normalized.sort_by).toBe('due-date')
        const built = buildServerFilter({ sort_by: 'due-date' }, '')
        expect(built.serverFilter.sort_by).toBe('due')
    })
})

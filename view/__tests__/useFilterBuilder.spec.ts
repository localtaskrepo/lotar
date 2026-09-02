import { describe, expect, it, vi } from 'vitest'
import { ref } from 'vue'
import { useProjectFilterSync } from '../composables/useFilterBuilder'

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

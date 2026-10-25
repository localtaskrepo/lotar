import { beforeEach, describe, expect, it } from 'vitest'
import { ref } from 'vue'
import type { TaskDTO } from '../api/types'
import { singleSprintId, sortBoardTasks, useBoardSort } from '../composables/useBoardSort'

function task(id: string, fields: Partial<TaskDTO> = {}): TaskDTO {
    return { id, title: id, status: 'Todo', modified: '2026-01-01T00:00:00Z', ...fields } as TaskDTO
}

const ids = (tasks: TaskDTO[]) => tasks.map((t) => t.id)

describe('singleSprintId', () => {
    it('only returns an id for exactly one numeric sprint', () => {
        expect(singleSprintId('3')).toBe(3)
        expect(singleSprintId(' 3 ')).toBe(3)
        expect(singleSprintId('3,4')).toBeNull()
        expect(singleSprintId('@backlog')).toBeNull()
        expect(singleSprintId('0')).toBeNull()
        expect(singleSprintId(undefined)).toBeNull()
    })
})

describe('sortBoardTasks', () => {
    it('orders priority by the configured order, highest first when descending', () => {
        const tasks = [
            task('A-1', { priority: 'Low' }),
            task('A-2', { priority: 'High' }),
            task('A-3', { priority: 'Medium' }),
        ]
        const priorities = ['Low', 'Medium', 'High']
        expect(ids(sortBoardTasks(tasks, 'priority', 'desc', { priorities }))).toEqual(['A-2', 'A-3', 'A-1'])
        expect(ids(sortBoardTasks(tasks, 'priority', 'asc', { priorities }))).toEqual(['A-1', 'A-3', 'A-2'])
    })

    it('breaks priority ties by most recent change', () => {
        const tasks = [
            task('A-1', { priority: 'High', modified: '2026-01-01T00:00:00Z' }),
            task('A-2', { priority: 'High', modified: '2026-03-01T00:00:00Z' }),
        ]
        expect(ids(sortBoardTasks(tasks, 'priority', 'desc', { priorities: ['High'] }))).toEqual(['A-2', 'A-1'])
    })

    it('follows the manual sprint order and puts unordered tasks last by ID', () => {
        const tasks = [
            task('A-10'),
            task('A-2', { sprint_order: { 7: 2 } }),
            task('A-3', { sprint_order: { 7: 1 } }),
            task('A-1', { sprint_order: { 8: 1 } }),
        ]
        expect(ids(sortBoardTasks(tasks, 'sprint', 'desc', { sprintId: 7 }))).toEqual(['A-3', 'A-2', 'A-1', 'A-10'])
    })

    it('uses the shared sort for other keys', () => {
        const tasks = [task('A-1', { title: 'beta' }), task('A-2', { title: 'Alpha' })]
        expect(ids(sortBoardTasks(tasks, 'title', 'asc'))).toEqual(['A-2', 'A-1'])
    })
})

describe('useBoardSort', () => {
    beforeEach(() => localStorage.clear())

    it('defaults to last modified, newest first', () => {
        const sort = useBoardSort(ref('A'), ref({}))
        expect(sort.key.value).toBe('modified')
        expect(sort.dir.value).toBe('desc')
    })

    it('picks a sensible direction per field and saves the choice per project', () => {
        const project = ref('A')
        const sort = useBoardSort(project, ref({}))
        sort.setKey('due')
        expect(sort.dir.value).toBe('asc')
        sort.toggleDir()

        expect(useBoardSort(ref('A'), ref({})).key.value).toBe('due')
        expect(useBoardSort(ref('A'), ref({})).dir.value).toBe('desc')
        expect(useBoardSort(ref('B'), ref({})).key.value).toBe('modified')

        project.value = 'B'
        sort.reload()
        expect(sort.key.value).toBe('modified')
    })

    it('offers sprint order only while one sprint is filtered', () => {
        const filter = ref<Record<string, string>>({ sprints: '7' })
        const sort = useBoardSort(ref('A'), filter)
        expect(sort.options.value.map((o) => o.value)).toContain('sprint')
        sort.setKey('sprint')
        expect(sort.effectiveKey.value).toBe('sprint')
        expect(sort.directional.value).toBe(false)

        filter.value = { sprints: '7,8' }
        expect(sort.options.value.map((o) => o.value)).not.toContain('sprint')
        expect(sort.effectiveKey.value).toBe('modified')
        expect(sort.directional.value).toBe(true)

        filter.value = { sprints: '7' }
        expect(sort.effectiveKey.value).toBe('sprint')
    })
})

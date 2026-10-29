import { describe, expect, it, vi } from 'vitest'
import { computed, nextTick, reactive, ref } from 'vue'
import { useTaskPanelOwnership } from '../composables/useTaskPanelOwnership'

vi.mock('../api/client', () => ({
    api: {
        whoami: vi.fn(async () => ''),
        listTasks: vi.fn(async () => ({ tasks: [{ id: 'A-1', reporter: 'priya', assignee: 'marco', tags: [] }] })),
    },
}))

function setup() {
    const form = reactive<Record<string, any>>({ reporter: '', assignee: '' })
    const ownership = useTaskPanelOwnership({
        form,
        mode: computed(() => 'edit' as const),
        ready: ref(false),
        suppressWatch: ref(true),
        defaults: ref({}),
        members: ref([]),
        mergeKnownTags: () => {},
        updateField: () => {},
        onFieldBlur: () => {},
    })
    return { form, ownership }
}

describe('task panel ownership controls', () => {
    it('leaves the custom input once the people list loads and knows the name', async () => {
        const { form, ownership } = setup()
        form.reporter = 'priya'
        form.assignee = 'marco'
        await nextTick()
        expect(ownership.reporterMode.value).toBe('custom')
        expect(ownership.assigneeMode.value).toBe('custom')

        await ownership.preloadPeople('A')
        await nextTick()
        expect(ownership.reporterMode.value).toBe('select')
        expect(ownership.reporterSelection.value).toBe('priya')
        expect(ownership.assigneeMode.value).toBe('select')
        expect(ownership.assigneeSelection.value).toBe('marco')
    })

    it('keeps a custom input the user chose', async () => {
        const { form, ownership } = setup()
        await ownership.preloadPeople('A')
        form.reporter = 'priya'
        await nextTick()
        ownership.setReporterSelection('__custom')
        expect(ownership.reporterMode.value).toBe('custom')

        ownership.knownUsers.value = [...ownership.knownUsers.value, 'sam']
        await nextTick()
        expect(ownership.reporterMode.value).toBe('custom')
    })
})

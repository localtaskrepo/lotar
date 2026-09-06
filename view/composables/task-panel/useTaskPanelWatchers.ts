import type { ComputedRef, Ref } from 'vue'
import { watch } from 'vue'
import type { TaskPanelFormState } from './useTaskPanelPersistence'

interface UseTaskPanelWatchersOptions {
    open: () => boolean
    taskId: () => string | null | undefined
    mode: ComputedRef<'create' | 'edit'>
    ready: Ref<boolean>
    suppressWatch: Ref<boolean>
    form: TaskPanelFormState
    preloadPeople: (project: string) => void
    ensureProjectsLoaded: () => Promise<void>
    initialize: () => Promise<void>
    onClose: () => void
}

export function useTaskPanelWatchers(options: UseTaskPanelWatchersOptions) {
    watch(
        [() => options.open(), () => options.taskId()],
        async ([isOpen]) => {
            if (!isOpen) {
                options.onClose()
                return
            }
            await options.initialize()
        },
        { immediate: true, flush: 'sync' },
    )
}

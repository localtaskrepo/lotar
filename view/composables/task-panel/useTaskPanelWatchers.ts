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
        () => options.open(),
        async (isOpen) => {
            if (!isOpen) {
                options.onClose()
                return
            }
            await options.ensureProjectsLoaded()
            await options.initialize()
        },
        { immediate: true },
    )

    watch(
        () => options.taskId(),
        async (next, prev) => {
            if (!options.open()) return
            if (next === prev) return
            await options.initialize()
        },
    )

    watch(
        () => options.form.project,
        (project) => {
            if (options.suppressWatch.value) return
            options.preloadPeople(project)
        },
    )
}

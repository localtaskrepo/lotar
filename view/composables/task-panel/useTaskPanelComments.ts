import type { ComputedRef, Ref } from 'vue'
import type { TaskDTO } from '../../api/types'
import { useTaskComments } from '../useTaskComments'

interface UseTaskPanelCommentsOptions {
    mode: ComputedRef<'create' | 'edit'>
    task: TaskDTO
    submitting: Ref<boolean>
    emitUpdated: (task: TaskDTO) => void
}

type CommentApi = ReturnType<typeof useTaskComments>

export type TaskPanelCommentsApi = CommentApi & {
    bindApplyTask: (fn: (task: TaskDTO) => void) => void
}

export function useTaskPanelComments(options: UseTaskPanelCommentsOptions): TaskPanelCommentsApi {
    let applyTaskImpl: (task: TaskDTO) => void = () => {
        /* no-op until bound */
    }

    const commentApi = useTaskComments({
        mode: options.mode,
        task: options.task,
        submitting: options.submitting,
        applyTask: (task) => applyTaskImpl(task),
        emitUpdated: options.emitUpdated,
    })

    const bindApplyTask = (fn: (task: TaskDTO) => void) => {
        applyTaskImpl = fn
    }

    return {
        ...commentApi,
        bindApplyTask,
    }
}

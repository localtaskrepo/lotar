import { reactive, readonly } from 'vue'
import type { TaskDTO } from '../api/types'

type TaskPanelCallback = (task: TaskDTO) => void

type CloseCallback = () => void

interface TaskPanelCallbacks {
    onClose?: CloseCallback | null
    onCreated?: TaskPanelCallback | null
    onUpdated?: TaskPanelCallback | null
}

interface OpenTaskPanelOptions extends TaskPanelCallbacks {
    taskId: string
    initialProject?: string | null
}

interface TaskPanelState {
    open: boolean
    taskId: string | null
    initialProject: string | null
    callbacks: TaskPanelCallbacks | null
}

const state = reactive<TaskPanelState>({
    open: false,
    taskId: null,
    initialProject: null,
    callbacks: null,
})

function openTaskPanel(options: OpenTaskPanelOptions) {
    state.taskId = options.taskId
    state.initialProject = options.initialProject ?? null
    state.callbacks = {
        onClose: options.onClose ?? null,
        onCreated: options.onCreated ?? null,
        onUpdated: options.onUpdated ?? null,
    }
    state.open = true
}

function closeTaskPanel() {
    if (!state.open) {
        return
    }
    state.open = false
    const { onClose } = state.callbacks ?? {}
    state.taskId = null
    state.initialProject = null
    state.callbacks = null
    if (onClose) {
        try {
            onClose()
        } catch (err) {
            console.warn('Task panel onClose callback failed', err)
        }
    }
}

function notifyCreated(task: TaskDTO) {
    try {
        state.callbacks?.onCreated?.(task)
    } catch (err) {
        console.warn('Task panel onCreated callback failed', err)
    }
}

function notifyUpdated(task: TaskDTO) {
    try {
        state.callbacks?.onUpdated?.(task)
    } catch (err) {
        console.warn('Task panel onUpdated callback failed', err)
    }
}

export function useTaskPanelController() {
    return {
        state: readonly(state),
        openTaskPanel,
        closeTaskPanel,
        notifyCreated,
        notifyUpdated,
    }
}

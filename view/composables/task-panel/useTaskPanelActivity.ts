import type { Ref } from 'vue'
import { ref } from 'vue'
import { api } from '../../api/client'
import type { TaskDTO, TaskHistoryEntry } from '../../api/types'

export interface CommitEntry {
    commit: string
    author: string
    email: string
    date: string
    message: string
}

export interface TaskPanelActivityApi {
    changeLog: Ref<TaskHistoryEntry[]>
    commitHistory: Ref<CommitEntry[]>
    commitsLoading: Ref<boolean>
    syncFromTaskHistory: (task: TaskDTO) => void
    resetActivity: () => void
    loadCommitHistory: (taskId: string, limit?: number) => Promise<void>
    refreshCommits: (taskId: string | undefined, limit?: number) => Promise<void>
    formatDate: (value: string) => string
    formatCommit: (value: string) => string
    formatFieldName: (value: string) => string
    formatChangeValue: (value?: string | null) => string
}

const DEFAULT_HISTORY_LIMIT = 8

export function useTaskPanelActivity(): TaskPanelActivityApi {
    const changeLog = ref<TaskHistoryEntry[]>([])
    const commitHistory = ref<CommitEntry[]>([])
    const commitsLoading = ref(false)

    const resetActivity = () => {
        changeLog.value = []
        commitHistory.value = []
    }

    const syncFromTaskHistory = (task: TaskDTO) => {
        const history = Array.isArray(task.history) ? task.history : []
        changeLog.value = history
            .slice()
            .reverse()
            .map((entry) => ({
                ...entry,
                changes: Array.isArray(entry.changes)
                    ? entry.changes.map((change) => ({ ...change }))
                    : [],
            }))
    }

    const loadCommitHistory = async (taskId: string, limit = DEFAULT_HISTORY_LIMIT) => {
        commitsLoading.value = true
        try {
            const items = await api.taskHistory(taskId, limit)
            commitHistory.value = items
        } catch {
            commitHistory.value = []
        } finally {
            commitsLoading.value = false
        }
    }

    const refreshCommits = async (taskId: string | undefined, limit = DEFAULT_HISTORY_LIMIT) => {
        if (!taskId) return
        await loadCommitHistory(taskId, limit)
    }

    const formatDate = (value: string) => {
        try {
            return new Date(value).toLocaleString()
        } catch {
            return value
        }
    }

    const formatCommit = (value: string) => {
        if (!value) return ''
        return value.slice(0, 7)
    }

    const formatFieldName = (value: string) => {
        if (!value) return ''
        return value
            .split(/[_\s]+/)
            .filter(Boolean)
            .map((segment) => segment.charAt(0).toUpperCase() + segment.slice(1))
            .join(' ')
    }

    const formatChangeValue = (value?: string | null) => {
        if (value === undefined || value === null) return ''
        const trimmed = value.trim()
        if (!trimmed.length) return ''
        return trimmed.length > 60 ? `${trimmed.slice(0, 57)}…` : trimmed
    }

    return {
        changeLog,
        commitHistory,
        commitsLoading,
        syncFromTaskHistory,
        resetActivity,
        loadCommitHistory,
        refreshCommits,
        formatDate,
        formatCommit,
        formatFieldName,
        formatChangeValue,
    }
}

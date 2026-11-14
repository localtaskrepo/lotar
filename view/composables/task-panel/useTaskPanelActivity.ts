import type { Ref } from 'vue'
import { formatDateTime } from '../../utils/date'
import { onScopeDispose, ref } from 'vue'
import { api } from '../../api/client'
import { titleCase } from '../../utils/text'

export interface CommitEntry {
    commit: string
    author: string
    email: string
    date: string
    message: string
}

export interface TaskPanelActivityApi {
    commitHistory: Ref<CommitEntry[]>
    commitsLoading: Ref<boolean>
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
    const commitHistory = ref<CommitEntry[]>([])
    const commitsLoading = ref(false)
    let generation = 0

    const resetActivity = () => {
        generation += 1
        commitsLoading.value = false
        commitHistory.value = []
    }
    onScopeDispose(resetActivity)

    const loadCommitHistory = async (taskId: string, limit = DEFAULT_HISTORY_LIMIT) => {
        const request = ++generation
        commitsLoading.value = true
        try {
            const items = await api.taskHistory(taskId, limit)
            if (request !== generation) return
            commitHistory.value = items
        } catch {
            if (request !== generation) return
            commitHistory.value = []
        } finally {
            if (request === generation) commitsLoading.value = false
        }
    }

    const formatDate = (value: string) => formatDateTime(value)

    const refreshCommits = async (taskId: string | undefined, limit = DEFAULT_HISTORY_LIMIT) => {
        if (!taskId) return
        await loadCommitHistory(taskId, limit)
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
            .map((segment) => titleCase(segment))
            .join(' ')
    }

    const formatChangeValue = (value?: string | null) => {
        if (value === undefined || value === null) return ''
        const trimmed = value.trim()
        if (!trimmed.length) return ''
        return trimmed.length > 60 ? `${trimmed.slice(0, 57)}…` : trimmed
    }

    return {
        commitHistory,
        commitsLoading,
        resetActivity,
        loadCommitHistory,
        refreshCommits,
        formatDate,
        formatCommit,
        formatFieldName,
        formatChangeValue,
    }
}

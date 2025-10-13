import type { Ref } from 'vue'
import { nextTick, ref } from 'vue'
import { api } from '../api/client'
import type { TaskDTO } from '../api/types'
import { showToast } from '../components/toast'

interface UseTaskCommentsOptions {
    mode: Ref<'create' | 'edit'>
    task: TaskDTO
    submitting: Ref<boolean>
    applyTask: (task: TaskDTO) => void
    emitUpdated: (task: TaskDTO) => void
}

export function useTaskComments(options: UseTaskCommentsOptions) {
    const newComment = ref('')
    const editingCommentIndex = ref<number | null>(null)
    const editingCommentText = ref('')
    const editingCommentSubmitting = ref(false)
    const editingCommentTextarea = ref<HTMLTextAreaElement | null>(null)

    function setEditingCommentTextarea(el: HTMLTextAreaElement | null) {
        editingCommentTextarea.value = el
    }

    function updateNewComment(value: string) {
        newComment.value = value
    }

    function updateEditingCommentText(value: string) {
        editingCommentText.value = value
    }

    function cancelEditComment() {
        editingCommentIndex.value = null
        editingCommentText.value = ''
        editingCommentSubmitting.value = false
        editingCommentTextarea.value = null
    }

    function resetComments() {
        newComment.value = ''
        cancelEditComment()
    }

    function startEditComment(index: number) {
        const list = options.task.comments
        if (!Array.isArray(list) || !list[index]) return
        editingCommentIndex.value = index
        editingCommentText.value = list[index].text || ''
        editingCommentSubmitting.value = false
        nextTick(() => {
            const el = editingCommentTextarea.value
            if (el) {
                el.focus()
                const len = el.value.length
                try {
                    el.setSelectionRange(len, len)
                } catch {
                    // ignore selection errors
                }
            }
        })
    }

    async function addComment() {
        if (options.mode.value !== 'edit') return
        const id = options.task.id
        if (!id) return
        const trimmed = newComment.value.trim()
        if (!trimmed) return
        options.submitting.value = true
        try {
            const updated = await api.addComment(id, trimmed)
            newComment.value = ''
            Object.assign(options.task, updated)
            options.applyTask(updated)
            options.emitUpdated(updated)
        } catch (error: any) {
            showToast(error?.message || 'Failed to add comment')
        } finally {
            options.submitting.value = false
        }
    }

    async function saveCommentEdit(index?: number) {
        if (options.mode.value !== 'edit') return
        const id = options.task.id
        if (!id) return
        const targetIndex = typeof index === 'number' ? index : editingCommentIndex.value
        if (targetIndex === null || targetIndex === undefined) return
        const list = options.task.comments
        const original = Array.isArray(list) && list[targetIndex] ? list[targetIndex].text || '' : ''
        const trimmed = editingCommentText.value.trim()
        if (!trimmed) {
            cancelEditComment()
            return
        }
        if (trimmed === original) {
            cancelEditComment()
            return
        }
        editingCommentSubmitting.value = true
        try {
            const updated = await api.updateComment(id, targetIndex, trimmed)
            Object.assign(options.task, updated)
            options.applyTask(updated)
            options.emitUpdated(updated)
            cancelEditComment()
        } catch (error: any) {
            showToast(error?.message || 'Failed to update comment')
        } finally {
            editingCommentSubmitting.value = false
        }
    }

    return {
        newComment,
        editingCommentIndex,
        editingCommentText,
        editingCommentSubmitting,
        setEditingCommentTextarea,
        updateNewComment,
        updateEditingCommentText,
        addComment,
        startEditComment,
        cancelEditComment,
        saveCommentEdit,
        resetComments,
    }
}

import { onMounted, onUnmounted, ref } from 'vue'
import { api } from '../../api/client'
import type { TaskDTO } from '../../api/types'
import { dismissToast, showToast } from '../../components/toast'
import { attachmentDisplayName } from '../../utils/taskAttachments'

/** How long Undo stays available before the removal is sent. */
export const UNDO_WINDOW_MS = 6000

interface PendingRemoval {
    taskId: string
    path: string
    timer: ReturnType<typeof setTimeout>
    toastId: number
}

interface Options {
    /** Id of the task the panel shows right now. */
    currentTaskId: () => string
    /** Apply the server's task after a removal of the current task's file. */
    onRemoved: (task: TaskDTO) => void
}

/**
 * Removing an attachment from the panel hides it at once and offers Undo for
 * a few seconds; only then is the removal sent (which may delete the file).
 * Pending removals are sent right away when the panel closes, switches task
 * or the page unloads, so nothing is silently kept.
 */
export function useDeferredAttachmentRemoval(options: Options) {
    const pending = ref<PendingRemoval[]>([])

    function isPending(path: string): boolean {
        return pending.value.some((entry) => entry.path === path && entry.taskId === options.currentTaskId())
    }

    function take(path: string, taskId: string): PendingRemoval | null {
        const entry = pending.value.find((item) => item.path === path && item.taskId === taskId) ?? null
        if (entry) {
            clearTimeout(entry.timer)
            pending.value = pending.value.filter((item) => item !== entry)
        }
        return entry
    }

    async function send(entry: PendingRemoval) {
        try {
            const response = await api.removeTaskAttachment({ id: entry.taskId, stored_path: entry.path })
            if (options.currentTaskId() === entry.taskId) options.onRemoved(response.task)
        } catch (error) {
            console.warn('Failed to remove attachment', { path: entry.path, error })
            showToast(`Could not remove ${attachmentDisplayName(entry.path)}`)
        }
    }

    function remove(path: string) {
        const taskId = options.currentTaskId()
        if (!taskId || isPending(path)) return
        const name = attachmentDisplayName(path)
        const entry: PendingRemoval = {
            taskId,
            path,
            timer: setTimeout(() => {
                const due = take(path, taskId)
                if (due) void send(due)
            }, UNDO_WINDOW_MS),
            toastId: showToast(`Removed ${name}`, undefined, UNDO_WINDOW_MS, {
                label: 'Undo',
                run: () => {
                    take(path, taskId)
                },
            }),
        }
        pending.value = [...pending.value, entry]
    }

    /** Send every pending removal now. */
    function flush() {
        const due = pending.value
        pending.value = []
        for (const entry of due) {
            clearTimeout(entry.timer)
            dismissToast(entry.toastId)
            void send(entry)
        }
    }

    // A closing tab cannot await responses; keepalive lets the requests finish.
    function onPageHide() {
        const due = pending.value
        pending.value = []
        for (const entry of due) {
            clearTimeout(entry.timer)
            void fetch('/api/tasks/attachments/remove', {
                method: 'POST',
                keepalive: true,
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ id: entry.taskId, stored_path: entry.path }),
            }).catch(() => {})
        }
    }

    onMounted(() => window.addEventListener('pagehide', onPageHide))
    onUnmounted(() => {
        window.removeEventListener('pagehide', onPageHide)
        flush()
    })

    return { pending, isPending, remove, flush }
}

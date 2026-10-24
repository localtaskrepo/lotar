import type { ComputedRef, Ref } from 'vue'
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import type { TaskDTO } from '../../api/types'
import { showToast } from '../../components/toast'
import {
    onPreferencesChanged,
    readTaskPanelAutoDetectLinksPreference,
    readTaskPanelShowAttachmentsPreference,
    readTaskPanelShowLinksInAttachmentsPreference,
} from '../../utils/preferences'
import { extractDroppedUrls, isFileDrag, isLinkDrag } from '../../utils/taskAttachments'
import { relationshipMime } from '../../utils/relationships'
import type { TaskPanelReferenceMutationsApi } from './useTaskPanelReferenceMutations'

export interface TaskPanelAttachmentPreferences {
    showAttachments: Ref<boolean>
    showLinksInAttachments: Ref<boolean>
    autoDetectLinks: Ref<boolean>
}

/** Attachment-related panel preferences, kept live while the panel is mounted. */
export function useTaskPanelAttachmentPreferences(): TaskPanelAttachmentPreferences {
    const showAttachments = ref(readTaskPanelShowAttachmentsPreference())
    const showLinksInAttachments = ref(readTaskPanelShowLinksInAttachmentsPreference())
    const autoDetectLinks = ref(readTaskPanelAutoDetectLinksPreference())

    let unsubscribe: null | (() => void) = null
    onMounted(() => {
        unsubscribe = onPreferencesChanged(() => {
            showAttachments.value = readTaskPanelShowAttachmentsPreference()
            showLinksInAttachments.value = readTaskPanelShowLinksInAttachmentsPreference()
            autoDetectLinks.value = readTaskPanelAutoDetectLinksPreference()
        })
    })
    onUnmounted(() => {
        unsubscribe?.()
        unsubscribe = null
    })

    return { showAttachments, showLinksInAttachments, autoDetectLinks }
}

interface UseTaskPanelAttachmentDropOptions {
    task: TaskDTO
    mode: ComputedRef<'create' | 'edit'>
    /** Trimmed id of the task currently loaded into the panel ('' while loading). */
    taskId: ComputedRef<string>
    isDeleted: ComputedRef<boolean>
    /** Panel props: whether it is open and which task target it shows. */
    isOpen: () => boolean
    targetTaskId: () => string | null | undefined
    preferences: TaskPanelAttachmentPreferences
    mutations: TaskPanelReferenceMutationsApi
}

export interface TaskPanelAttachmentDropApi {
    attachmentsDragActive: Ref<boolean>
    attachmentsDropLabel: ComputedRef<string>
    onAttachmentsDragEnter: (event: DragEvent) => void
    onAttachmentsDragOver: (event: DragEvent) => void
    onAttachmentsDragLeave: (event: DragEvent) => void
    onAttachmentsDrop: (event: DragEvent) => Promise<void>
}

/**
 * Whole-panel drop target for files and links. Tracks nested drag depth so
 * the overlay stays up across child elements, and buffers drops that arrive
 * while an edit-mode panel is still loading its task.
 */
export function useTaskPanelAttachmentDrop(options: UseTaskPanelAttachmentDropOptions): TaskPanelAttachmentDropApi {
    const { task, mode, preferences, mutations } = options
    const { attachmentsUploading, addingLinkReferences } = mutations

    const attachmentsDragActive = ref(false)
    const attachmentsDragDepth = ref(0)

    // Drops that arrived while an edit-mode panel was still loading the task
    // (before `task.id` exists); flushed as soon as that task is ready. The
    // buffer is bound to the task target that was loading when the drop landed.
    const pendingDropFiles = ref<File[]>([])
    const pendingDropUrls = ref<string[]>([])
    let pendingDropTaskId: string | null = null

    function clearPendingDrops() {
        pendingDropFiles.value = []
        pendingDropUrls.value = []
        pendingDropTaskId = null
    }

    onUnmounted(clearPendingDrops)

    const attachmentsDropLabel = computed(() => {
        if (attachmentsUploading.value) return 'Uploading attachments'
        if (addingLinkReferences.value) return 'Adding links'
        if (mode.value !== 'edit' || !task.id) {
            return preferences.showLinksInAttachments.value
                ? 'Save the task to attach files or links'
                : 'Save the task to attach files'
        }
        if (preferences.showLinksInAttachments.value) return 'Drop files or links to attach'
        return 'Drop files to attach'
    })

    function resetAttachmentsDragState() {
        attachmentsDragDepth.value = 0
        attachmentsDragActive.value = false
    }

    async function uploadDroppedFiles(files: File[]) {
        try {
            await mutations.uploadFiles(files)
        } finally {
            resetAttachmentsDragState()
        }
    }

    async function addDroppedUrls(urls: string[]) {
        try {
            await mutations.addUrls(urls)
        } finally {
            resetAttachmentsDragState()
        }
    }

    function isAttachmentDrag(event: DragEvent): boolean {
        if (Array.from(event.dataTransfer?.types ?? []).includes(relationshipMime)) return false
        if (!preferences.showAttachments.value) return false
        if (isFileDrag(event)) return true
        if (preferences.showLinksInAttachments.value && isLinkDrag(event)) return true
        return false
    }

    function onAttachmentsDragEnter(event: DragEvent) {
        if (attachmentsUploading.value || addingLinkReferences.value) return
        if (!isAttachmentDrag(event)) return
        event.preventDefault()
        attachmentsDragDepth.value += 1
        attachmentsDragActive.value = true
    }

    function onAttachmentsDragOver(event: DragEvent) {
        if (attachmentsUploading.value || addingLinkReferences.value) return
        if (!isAttachmentDrag(event)) return
        event.preventDefault()
    }

    function onAttachmentsDragLeave(event: DragEvent) {
        if (attachmentsUploading.value || addingLinkReferences.value) return
        if (!attachmentsDragActive.value) return
        event.preventDefault()
        attachmentsDragDepth.value = Math.max(0, attachmentsDragDepth.value - 1)
        if (attachmentsDragDepth.value === 0) {
            attachmentsDragActive.value = false
        }
    }

    async function onAttachmentsDrop(event: DragEvent) {
        if (Array.from(event.dataTransfer?.types ?? []).includes(relationshipMime)) return
        if (attachmentsUploading.value || addingLinkReferences.value) return

        const hasFiles = isFileDrag(event)
        const urls = preferences.showLinksInAttachments.value ? extractDroppedUrls(event) : []
        if (!hasFiles && urls.length === 0) return

        event.preventDefault()
        resetAttachmentsDragState()

        if (options.isDeleted.value) {
            showToast('Restore the task before attaching files or links')
            return
        }

        if (mode.value !== 'edit') {
            showToast(hasFiles ? 'Save the task before attaching files' : 'Save the task before attaching links')
            return
        }

        if (!task.id) {
            // The panel is still initializing this task: `task.id` is assigned only
            // after the per-project config loads, so an eager drop would otherwise
            // be discarded with a misleading "save the task" toast. Buffer the
            // payload for the task target that is loading and flush it once that
            // task is ready. A files-bearing event ignores link payloads, matching
            // the direct drop path where files take priority.
            if (hasFiles) {
                const files = Array.from(event.dataTransfer?.files ?? []).filter((file) => file && file.size >= 0)
                if (!files.length) return
                pendingDropFiles.value = [...pendingDropFiles.value, ...files]
                pendingDropTaskId = options.targetTaskId() ?? null
                showToast('Task is still loading — attachments will upload when it is ready')
                return
            }
            if (urls.length) {
                pendingDropUrls.value = [...pendingDropUrls.value, ...urls]
                pendingDropTaskId = options.targetTaskId() ?? null
                showToast('Task is still loading — attachments will upload when it is ready')
                return
            }
            return
        }

        if (hasFiles) {
            const files = Array.from(event.dataTransfer?.files ?? []).filter((file) => file && file.size >= 0)
            if (!files.length) return
            await uploadDroppedFiles(files)
            return
        }

        if (urls.length) {
            await addDroppedUrls(urls)
        }
    }

    // Flush drops buffered while the panel was still loading the task. `taskId`
    // becomes non-empty exactly when the loaded task fields are applied, and the
    // buffer only uploads when the panel still shows that same task target in
    // edit mode — a swap to another task, create mode, or a closed panel
    // invalidates the buffer instead of uploading into the wrong task.
    watch(options.taskId, (id) => {
        if (!id) return
        const files = pendingDropFiles.value
        const urls = pendingDropUrls.value
        if (!files.length && !urls.length) return
        const belongsToLoadedTask =
            options.isOpen() && mode.value === 'edit' && options.targetTaskId() === id && pendingDropTaskId === id
        clearPendingDrops()
        if (!belongsToLoadedTask) return
        if (files.length) {
            void uploadDroppedFiles(files)
        } else {
            void addDroppedUrls(urls)
        }
    })

    // A task-target swap (the controller opens task B while task A is still
    // loading, or reopens the panel in create mode) invalidates drops buffered
    // for the previous target. Sync flush clears the buffer at the prop change,
    // before the new target's load can complete and arm the flush watcher.
    watch(options.targetTaskId, () => clearPendingDrops(), { flush: 'sync' })

    watch(options.isOpen, (open) => {
        if (!open) clearPendingDrops()
    })

    return {
        attachmentsDragActive,
        attachmentsDropLabel,
        onAttachmentsDragEnter,
        onAttachmentsDragOver,
        onAttachmentsDragLeave,
        onAttachmentsDrop,
    }
}

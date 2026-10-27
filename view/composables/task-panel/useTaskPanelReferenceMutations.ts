import type { ComputedRef, Ref } from 'vue'
import { computed, ref } from 'vue'
import { api } from '../../api/client'
import type { TaskDTO } from '../../api/types'
import { showToast } from '../../components/toast'
import { fileToBase64, normalizeUrlCandidate } from '../../utils/taskAttachments'

interface UseTaskPanelReferenceMutationsOptions {
    task: TaskDTO
    mode: ComputedRef<'create' | 'edit'>
    /** Returns true (and explains why) when the task is soft-deleted. */
    rejectDeletedMutation: (label: string) => boolean
    emitUpdated: (task: TaskDTO) => void
}

export type LinkAddOutcome = 'added' | 'skipped' | 'failed'

/** One file of the upload in progress, for the panel's progress list. */
export interface UploadProgressEntry {
    key: string
    name: string
    /** 0..1 of the bytes sent. */
    fraction: number
    state: 'waiting' | 'uploading' | 'done' | 'failed'
}

export interface TaskPanelReferenceMutationsApi {
    attachments: ComputedRef<NonNullable<TaskDTO['references']>>
    linkReferences: ComputedRef<NonNullable<TaskDTO['references']>>
    attachmentsUploading: Ref<boolean>
    addingLinkReferences: Ref<boolean>
    removingAttachmentPath: Ref<string | null>
    removingLinkUrl: Ref<string | null>
    uploadProgress: Ref<UploadProgressEntry[]>
    applyReferencesFromTaskResponse: (updated: Partial<TaskDTO> | null | undefined) => void
    removeAttachment: (relPath: string) => Promise<void>
    removeLinkReference: (url: string) => Promise<void>
    /** Upload files; resolves with the stored names of the ones that worked. */
    uploadFiles: (files: File[]) => Promise<string[]>
    addUrls: (urls: string[]) => Promise<void>
    attachLinksFromText: (urls: string[]) => Promise<void>
}

/**
 * Managed attachment and link-reference mutations for the task panel. Every
 * server response replaces the panel's references wholesale (see
 * `applyReferencesFromTaskResponse`) and is re-emitted as `updated`.
 */
export function useTaskPanelReferenceMutations(
    options: UseTaskPanelReferenceMutationsOptions,
): TaskPanelReferenceMutationsApi {
    const { task, mode } = options

    const attachmentsUploading = ref(false)
    const addingLinkReferences = ref(false)
    const removingAttachmentPath = ref<string | null>(null)
    const removingLinkUrl = ref<string | null>(null)
    const uploadProgress = ref<UploadProgressEntry[]>([])

    // Managed attachments-store blobs. Repository-file references (`file`) are
    // intentionally excluded: they are never managed blobs (DEV-61).
    const attachments = computed(() => {
        const list = Array.isArray(task.references) ? task.references : []
        return list.filter(
            (entry) => typeof entry?.attachment === 'string' && (entry.attachment || '').trim().length > 0,
        )
    })

    const linkReferences = computed(() => {
        const list = Array.isArray(task.references) ? task.references : []
        return list.filter((entry) => typeof entry?.link === 'string' && (entry.link || '').trim().length > 0)
    })

    function applyReferencesFromTaskResponse(updated: Partial<TaskDTO> | null | undefined) {
        task.references = Array.isArray(updated?.references)
            ? updated!.references!.map((reference) => ({ ...reference }))
            : []
    }

    function applyResponseTask(updated: TaskDTO) {
        Object.assign(task, updated)
        applyReferencesFromTaskResponse(updated)
        options.emitUpdated(updated)
    }

    async function removeAttachment(relPath: string) {
        const path = (relPath || '').trim()
        if (!path) return
        if (attachmentsUploading.value) return
        if (removingAttachmentPath.value) return
        if (options.rejectDeletedMutation('removing attachments')) return

        if (mode.value !== 'edit' || !task.id) {
            showToast('Save the task before removing attachments')
            return
        }

        removingAttachmentPath.value = path
        try {
            const response = await api.removeTaskAttachment({ id: task.id, stored_path: path })
            applyResponseTask(response.task)
            if (response.deleted) {
                showToast('Attachment removed')
            } else if (response.still_referenced) {
                showToast('Attachment removed (file kept: used by other tasks)')
            } else {
                showToast('Attachment removed (file may already be gone)')
            }
        } catch (error: any) {
            console.warn('Failed to remove attachment', { path, error })
            showToast('Failed to remove attachment')
        } finally {
            removingAttachmentPath.value = null
        }
    }

    async function addLinkReference(url: string): Promise<LinkAddOutcome> {
        const normalized = normalizeUrlCandidate(url)
        if (!normalized) return 'skipped'
        if (mode.value !== 'edit' || !task.id) return 'failed'

        const existing = new Set(
            (Array.isArray(task.references) ? task.references : [])
                .map((r) => (typeof r?.link === 'string' ? r.link.trim() : ''))
                .filter(Boolean),
        )
        if (existing.has(normalized)) return 'skipped'

        try {
            const response = await api.addTaskLinkReference({ id: task.id, url: normalized })
            applyResponseTask(response.task)
            return response.added ? 'added' : 'skipped'
        } catch (error: any) {
            console.warn('Failed to add link reference', { url: normalized, error })
            return 'failed'
        }
    }

    async function removeLinkReference(url: string) {
        const normalized = normalizeUrlCandidate(url)
        if (!normalized) return
        if (attachmentsUploading.value) return
        if (addingLinkReferences.value) return
        if (removingLinkUrl.value) return
        if (options.rejectDeletedMutation('removing links')) return

        if (mode.value !== 'edit' || !task.id) {
            showToast('Save the task before removing links')
            return
        }

        removingLinkUrl.value = normalized
        try {
            const response = await api.removeTaskLinkReference({ id: task.id, url: normalized })
            applyResponseTask(response.task)
            showToast(response.removed ? 'Link removed' : 'Link already removed')
        } catch (error: any) {
            console.warn('Failed to remove link reference', { url: normalized, error })
            showToast('Failed to remove link')
        } finally {
            removingLinkUrl.value = null
        }
    }

    async function uploadFiles(files: File[]): Promise<string[]> {
        attachmentsUploading.value = true
        const stored: string[] = []
        uploadProgress.value = files.map((file, index) => ({
            key: `${index}:${file.name}`,
            name: file.name,
            fraction: 0,
            state: 'waiting',
        }))
        try {
            let added = 0
            let skipped = 0
            let failed = 0
            let cleaned = 0
            const warnings: string[] = []

            for (const [index, file] of files.entries()) {
                const progress = uploadProgress.value[index]!
                progress.state = 'uploading'
                try {
                    const base64 = await fileToBase64(file)
                    const response = await api.uploadTaskAttachment(
                        {
                            id: task.id,
                            filename: file.name,
                            content_base64: base64,
                        },
                        (fraction) => {
                            progress.fraction = fraction
                        },
                    )

                    if (response.attached) {
                        added += 1
                    } else {
                        skipped += 1
                    }
                    if (response.metadata_removed) cleaned += 1
                    warnings.push(...(response.warnings ?? []))
                    stored.push(response.stored_path)
                    progress.state = 'done'
                    progress.fraction = 1

                    applyResponseTask(response.task)
                } catch (error: any) {
                    failed += 1
                    progress.state = 'failed'
                    console.warn('Failed to upload attachment', { file: file.name, error })
                }
            }

            for (const warning of warnings) showToast(warning, 'Check this file', 6000)
            if (cleaned > 0) {
                showToast(
                    cleaned === 1
                        ? 'Removed location and camera details from the photo'
                        : `Removed location and camera details from ${cleaned} photos`,
                )
            }

            if (files.length === 1 && added === 0 && skipped === 1 && failed === 0) {
                showToast('Attachment already attached')
                return stored
            }

            if (added > 0 && skipped === 0 && failed === 0) {
                showToast(added === 1 ? 'Attachment added' : 'Attachments added')
            } else {
                const parts = []
                if (added > 0) parts.push(`${added} added`)
                if (skipped > 0) parts.push(`${skipped} already attached`)
                if (failed > 0) parts.push(`${failed} failed`)
                showToast(parts.length ? `Attachments: ${parts.join(', ')}` : 'No attachments uploaded')
            }
            return stored
        } finally {
            attachmentsUploading.value = false
            uploadProgress.value = []
        }
    }

    async function addLinks(urls: string[]) {
        let added = 0
        let skipped = 0
        let failed = 0
        for (const url of urls) {
            const outcome = await addLinkReference(url)
            if (outcome === 'added') added += 1
            else if (outcome === 'skipped') skipped += 1
            else failed += 1
        }
        return { added, skipped, failed }
    }

    /** Explicitly dropped links: always report the outcome. */
    async function addUrls(urls: string[]): Promise<void> {
        addingLinkReferences.value = true
        try {
            const { added, skipped, failed } = await addLinks(urls)

            if (urls.length === 1 && added === 0 && skipped === 1 && failed === 0) {
                showToast('Link already attached')
                return
            }

            if (added > 0 && skipped === 0 && failed === 0) {
                showToast(added === 1 ? 'Link added' : 'Links added')
            } else {
                const parts = []
                if (added > 0) parts.push(`${added} added`)
                if (skipped > 0) parts.push(`${skipped} already attached`)
                if (failed > 0) parts.push(`${failed} failed`)
                showToast(parts.length ? `Links: ${parts.join(', ')}` : 'No links added')
            }
        } finally {
            addingLinkReferences.value = false
        }
    }

    /** Links auto-detected in task text: stay quiet unless something was added. */
    async function attachLinksFromText(urls: string[]): Promise<void> {
        addingLinkReferences.value = true
        try {
            const { added, skipped, failed } = await addLinks(urls)
            if (added > 0) {
                const parts = []
                parts.push(`${added} added`)
                if (skipped > 0) parts.push(`${skipped} already attached`)
                if (failed > 0) parts.push(`${failed} failed`)
                showToast(`Links: ${parts.join(', ')}`)
            }
        } finally {
            addingLinkReferences.value = false
        }
    }

    return {
        attachments,
        linkReferences,
        attachmentsUploading,
        addingLinkReferences,
        removingAttachmentPath,
        removingLinkUrl,
        uploadProgress,
        applyReferencesFromTaskResponse,
        removeAttachment,
        removeLinkReference,
        uploadFiles,
        addUrls,
        attachLinksFromText,
    }
}

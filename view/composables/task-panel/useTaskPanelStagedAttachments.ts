import type { Ref } from 'vue'
import { onUnmounted, ref } from 'vue'
import { api } from '../../api/client'
import type { TaskDTO } from '../../api/types'
import { attachmentKind, fileToBase64, normalizeUrlCandidate } from '../../utils/taskAttachments'

/** A file picked, dropped or pasted while the task is still being created. */
export interface StagedFile {
    key: string
    file: File
    /** Object URL for image thumbnails. */
    previewUrl: string | null
}

export interface StagedUploadResult {
    task: TaskDTO
    failed: string[]
    warnings: string[]
}

export interface TaskPanelStagedAttachmentsApi {
    stagedFiles: Ref<StagedFile[]>
    stagedLinks: Ref<string[]>
    stageFiles: (files: File[]) => StagedFile[]
    stageLinks: (urls: string[]) => void
    unstageFile: (key: string) => void
    unstageLink: (url: string) => void
    clearStaged: () => void
    /** Placeholder target a description uses for a staged file until it is uploaded. */
    stagedTarget: (key: string) => string
    /** Upload everything staged to the new task and fix description links. */
    uploadStaged: (created: TaskDTO) => Promise<StagedUploadResult>
}

/**
 * Attachments added while creating a task (DEV-244). Files and links are kept
 * in the browser until the task exists, then uploaded to it. A screenshot
 * pasted into the description links to `attachment:staged-N` meanwhile; the
 * link is pointed at the stored file once it is uploaded.
 */
export function useTaskPanelStagedAttachments(): TaskPanelStagedAttachmentsApi {
    const stagedFiles = ref<StagedFile[]>([])
    const stagedLinks = ref<string[]>([])
    let counter = 0

    const stagedTarget = (key: string) => `staged-${key}`

    function stageFiles(files: File[]): StagedFile[] {
        const added = files.map((file) => ({
            key: String(++counter),
            file,
            previewUrl:
                attachmentKind(file.name) === 'image' && typeof URL.createObjectURL === 'function'
                    ? URL.createObjectURL(file)
                    : null,
        }))
        stagedFiles.value = [...stagedFiles.value, ...added]
        return added
    }

    function stageLinks(urls: string[]) {
        const next = [...stagedLinks.value]
        for (const url of urls) {
            const normalized = normalizeUrlCandidate(url)
            if (normalized && !next.includes(normalized)) next.push(normalized)
        }
        stagedLinks.value = next
    }

    function release(entry: StagedFile) {
        if (entry.previewUrl && typeof URL.revokeObjectURL === 'function') URL.revokeObjectURL(entry.previewUrl)
    }

    function unstageFile(key: string) {
        const entry = stagedFiles.value.find((item) => item.key === key)
        if (entry) release(entry)
        stagedFiles.value = stagedFiles.value.filter((item) => item.key !== key)
    }

    function unstageLink(url: string) {
        stagedLinks.value = stagedLinks.value.filter((item) => item !== url)
    }

    function clearStaged() {
        stagedFiles.value.forEach(release)
        stagedFiles.value = []
        stagedLinks.value = []
    }

    onUnmounted(clearStaged)

    async function uploadStaged(created: TaskDTO): Promise<StagedUploadResult> {
        let task = created
        const failed: string[] = []
        const warnings: string[] = []
        const storedByKey = new Map<string, string>()
        for (const entry of stagedFiles.value) {
            try {
                const response = await api.uploadTaskAttachment({
                    id: created.id,
                    filename: entry.file.name,
                    content_base64: await fileToBase64(entry.file),
                })
                storedByKey.set(entry.key, response.stored_path)
                warnings.push(...(response.warnings ?? []))
                task = response.task
            } catch (error) {
                console.warn('Failed to upload attachment', { file: entry.file.name, error })
                failed.push(entry.file.name)
            }
        }
        for (const url of stagedLinks.value) {
            try {
                task = (await api.addTaskLinkReference({ id: created.id, url })).task
            } catch (error) {
                console.warn('Failed to add link', { url, error })
                failed.push(url)
            }
        }
        const description = task.description ?? ''
        if (storedByKey.size && description.includes('attachment:staged-')) {
            const fixed = description.replace(/\(attachment:staged-(\d+)\)/g, (match, key: string) => {
                const stored = storedByKey.get(key)
                return stored ? `(attachment:${stored})` : match
            })
            if (fixed !== description) {
                try {
                    task = await api.updateTask(created.id, { description: fixed })
                } catch (error) {
                    console.warn('Failed to link pasted attachments in the description', error)
                }
            }
        }
        clearStaged()
        return { task, failed, warnings }
    }

    return {
        stagedFiles,
        stagedLinks,
        stageFiles,
        stageLinks,
        unstageFile,
        unstageLink,
        clearStaged,
        stagedTarget,
        uploadStaged,
    }
}

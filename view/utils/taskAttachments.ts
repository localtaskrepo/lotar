import { projectPrefixOfTaskId } from './text'

// Pure helpers for managed attachments and link references shown in the task
// panel. Repository-file references (`file`) are never managed blobs (DEV-61)
// and are not handled here.

/** Content hash embedded in a stored attachment name, if any. */
export function extractAttachmentHash(relPath: string): string | null {
    const cleaned = (relPath || '').trim()
    if (!cleaned) return null
    const parts = cleaned.split('/')
    const leaf = parts[parts.length - 1] || cleaned

    const lastDot = leaf.lastIndexOf('.')
    const base = lastDot > 0 ? leaf.slice(0, lastDot) : leaf
    const ext = lastDot > 0 ? leaf.slice(lastDot + 1) : ''

    if (ext.length === 32 && /^[0-9a-f]{32}$/i.test(ext)) {
        return ext
    }

    const m = base.match(/^(.*)[.-]([0-9a-f]{32})$/i)
    return m ? (m[2] ?? null) : null
}

/** Original file name of a stored attachment, without the content hash. */
export function attachmentDisplayName(relPath: string): string {
    const cleaned = (relPath || '').trim()
    if (!cleaned) return 'attachment'
    const parts = cleaned.split('/')
    const leaf = parts[parts.length - 1] || cleaned

    const dot = leaf.lastIndexOf('.')
    const stem = dot > 0 ? leaf.slice(0, dot) : leaf
    const ext = dot > 0 ? leaf.slice(dot) : ''

    const hashMatch = stem.match(/^(.*)[.-]([0-9a-f]{32})$/i)
    if (hashMatch) {
        const displayStem = (hashMatch[1] || '').trim()
        return `${displayStem || 'attachment'}${ext}`
    }

    return leaf
}

/** Download URL for a stored attachment, scoped to the task's project store. */
export function attachmentUrl(relPath: string, taskId: string | null | undefined): string {
    const stored = (typeof relPath === 'string' ? relPath : '').trim()
    if (!stored) return '/api/attachments/get?path='

    const project = projectPrefixOfTaskId((taskId || '').trim()) || ''

    const hash = extractAttachmentHash(stored)
    const display = attachmentDisplayName(stored)
    if (hash) {
        const qs = project ? `?${new URLSearchParams({ project }).toString()}` : ''
        return `/api/attachments/h/${encodeURIComponent(hash)}/${encodeURIComponent(display)}${qs}`
    }

    const params = new URLSearchParams({ path: stored })
    if (project) params.set('project', project)
    return `/api/attachments/get?${params.toString()}`
}

/** Workspace path of a stored attachment for hover titles. */
export function attachmentHoverTitle(relPath: string, attachmentsDir: string | null | undefined): string {
    const cleaned = (relPath || '').trim()
    if (!cleaned) return ''

    const configured = (attachmentsDir || '').trim()
    if (configured.startsWith('/')) {
        return `${configured.replace(/\/+$/, '')}/${cleaned.replace(/^\/+/, '')}`
    }

    const dir = (configured || '@attachments').replace(/^\/+/, '').replace(/\/+$/, '')
    const leaf = cleaned.replace(/^\/+/, '')
    return `.tasks/${dir}/${leaf}`
}

export function isImageAttachment(relPath: string): boolean {
    const name = attachmentDisplayName(relPath).toLowerCase()
    return (
        name.endsWith('.png') ||
        name.endsWith('.jpg') ||
        name.endsWith('.jpeg') ||
        name.endsWith('.gif') ||
        name.endsWith('.webp') ||
        name.endsWith('.svg')
    )
}

export function linkDisplayName(rawUrl: string): string {
    const cleaned = (rawUrl || '').trim()
    if (!cleaned) return 'link'
    try {
        const url = new URL(cleaned)
        const suffix = `${url.pathname || ''}${url.search || ''}${url.hash || ''}`
        return `${url.hostname}${suffix || ''}`
    } catch {
        return cleaned
    }
}

/** Accept only absolute http(s) URLs, trimming wrappers and trailing punctuation. */
export function normalizeUrlCandidate(candidate: string): string | null {
    let value = (candidate || '').trim()
    if (!value) return null

    if ((value.startsWith('<') && value.endsWith('>')) || (value.startsWith('(') && value.endsWith(')'))) {
        value = value.slice(1, -1).trim()
    }

    // Strip common trailing punctuation after paste.
    value = value.replace(/[),.;!?]+$/, '')

    if (!/^https?:\/\//i.test(value)) return null
    try {
        new URL(value)
    } catch {
        return null
    }
    return value
}

/** Unique normalized http(s) URLs mentioned in free text, in order. */
export function extractUrlsFromText(text: string): string[] {
    const raw = (text || '').trim()
    if (!raw) return []

    const matches = raw.match(/https?:\/\/[^\s<>"]+/gi) ?? []
    const out: string[] = []
    const seen = new Set<string>()
    for (const match of matches) {
        const normalized = normalizeUrlCandidate(match)
        if (!normalized) continue
        const key = normalized.trim()
        if (seen.has(key)) continue
        seen.add(key)
        out.push(key)
    }
    return out
}

export function extractDroppedUrls(event: DragEvent): string[] {
    const dt = event.dataTransfer
    if (!dt) return []

    const candidates: string[] = []
    const uriList = dt.getData('text/uri-list')
    if (uriList) {
        for (const line of uriList.split(/\r?\n/)) {
            const trimmed = line.trim()
            if (!trimmed || trimmed.startsWith('#')) continue
            candidates.push(trimmed)
        }
    }

    // Firefox can expose URL drags as a single string with a newline-separated title.
    const moz = dt.getData('text/x-moz-url')
    if (moz) {
        const parts = moz
            .split(/\r?\n/)
            .map((p) => p.trim())
            .filter(Boolean)
        if (parts[0]) candidates.push(parts[0])
    }

    return candidates
        .map((c) => normalizeUrlCandidate(c))
        .filter((c): c is string => typeof c === 'string' && c.length > 0)
}

export function isFileDrag(event: DragEvent): boolean {
    const dt = event.dataTransfer
    if (!dt) return false

    const items = Array.from(dt.items ?? [])
    if (items.some((item) => item.kind === 'file')) return true

    const types = Array.from(dt.types ?? [])
    return types.includes('Files') || types.includes('public.file-url') || types.includes('application/x-moz-file')
}

export function isLinkDrag(event: DragEvent): boolean {
    const dt = event.dataTransfer
    if (!dt) return false
    const types = Array.from(dt.types ?? [])
    return (
        types.includes('text/uri-list') ||
        types.includes('text/x-moz-url') ||
        // Safari sometimes exposes URL drags as plain text; we only treat as link on drop.
        types.includes('public.url')
    )
}

export function fileToBase64(file: File): Promise<string> {
    return new Promise((resolve, reject) => {
        const reader = new FileReader()
        reader.onerror = () => reject(new Error('Failed to read file'))
        reader.onload = () => {
            const result = typeof reader.result === 'string' ? reader.result : ''
            const marker = 'base64,'
            const idx = result.indexOf(marker)
            if (idx >= 0) {
                resolve(result.slice(idx + marker.length))
                return
            }
            reject(new Error('Failed to encode file'))
        }
        reader.readAsDataURL(file)
    })
}

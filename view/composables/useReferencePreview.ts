import { computed, nextTick, onMounted, onUnmounted, reactive, ref, watch } from 'vue'
import { api } from '../api/client'
import type { ReferenceSnippet } from '../api/types'

interface ReferencePreviewEntry {
    loading: boolean
    error: string | null
    snippet: ReferenceSnippet | null
    requestedBefore: number
    requestedAfter: number
}

const REFERENCE_SNIPPET_DEFAULT_CONTEXT = 6
const REFERENCE_SNIPPET_STEP = 4
const REFERENCE_SNIPPET_MAX_CONTEXT = 20
const REFERENCE_PREVIEW_DISMISS_DELAY = 400

function clampSnippetContext(value?: number) {
    if (typeof value !== 'number' || Number.isNaN(value)) {
        return REFERENCE_SNIPPET_DEFAULT_CONTEXT
    }
    if (value < 0) return 0
    if (value > REFERENCE_SNIPPET_MAX_CONTEXT) return REFERENCE_SNIPPET_MAX_CONTEXT
    return Math.floor(value)
}

export function useReferencePreview() {
    const referencePreviewState = reactive<Record<string, ReferencePreviewEntry>>({})
    const hoveredReferenceCode = ref<string | null>(null)
    const hoveredReferenceAnchor = ref<HTMLElement | null>(null)
    const hoveredReferenceAnchorRect = ref<{ top: number; left: number; right: number; bottom: number; width: number; height: number } | null>(null)
    const referencePreviewElement = ref<HTMLElement | null>(null)
    const referencePreviewRect = ref<{ width: number; height: number } | null>(null)

    let referenceAnchorRaf: number | null = null
    let referencePreviewMeasureRaf: number | null = null
    let referenceDismissTimer: ReturnType<typeof setTimeout> | null = null

    const hoveredReferenceEntry = computed(() => {
        const code = hoveredReferenceCode.value
        if (!code) return undefined
        return referencePreviewState[code]
    })

    const hoveredReferenceSnippet = computed(() => hoveredReferenceEntry.value?.snippet || null)
    const hoveredReferenceLoading = computed(() => !!hoveredReferenceEntry.value?.loading)
    const hoveredReferenceError = computed(() => hoveredReferenceEntry.value?.error || null)

    const hoveredReferenceCanExpandBefore = computed(() => {
        const entry = hoveredReferenceEntry.value
        if (!entry || !entry.snippet) return false
        if (entry.loading) return false
        if (!entry.snippet.has_more_before) return false
        return entry.requestedBefore < REFERENCE_SNIPPET_MAX_CONTEXT
    })

    const hoveredReferenceCanExpandAfter = computed(() => {
        const entry = hoveredReferenceEntry.value
        if (!entry || !entry.snippet) return false
        if (entry.loading) return false
        if (!entry.snippet.has_more_after) return false
        return entry.requestedAfter < REFERENCE_SNIPPET_MAX_CONTEXT
    })

    const hoveredReferenceCanExpand = computed(
        () => hoveredReferenceCanExpandBefore.value || hoveredReferenceCanExpandAfter.value,
    )

    function updateReferenceAnchorRect(immediate = false) {
        if (typeof window === 'undefined') return
        const run = () => {
            referenceAnchorRaf = null
            if (!hoveredReferenceCode.value) {
                hoveredReferenceAnchorRect.value = null
                return
            }
            const element = hoveredReferenceAnchor.value
            if (!element) {
                hoveredReferenceAnchorRect.value = null
                return
            }
            const rect = element.getBoundingClientRect()
            hoveredReferenceAnchorRect.value = {
                top: rect.top,
                left: rect.left,
                right: rect.right,
                bottom: rect.bottom,
                width: rect.width,
                height: rect.height,
            }
        }
        if (immediate) {
            run()
            return
        }
        if (referenceAnchorRaf !== null) return
        referenceAnchorRaf = window.requestAnimationFrame(run)
    }

    function scheduleReferencePreviewMeasure(immediate = false) {
        if (typeof window === 'undefined') return
        const run = () => {
            referencePreviewMeasureRaf = null
            const element = referencePreviewElement.value
            if (!element) {
                referencePreviewRect.value = null
                return
            }
            const rect = element.getBoundingClientRect()
            const width = rect.width
            const height = rect.height
            const previous = referencePreviewRect.value
            if (previous && previous.width === width && previous.height === height) {
                return
            }
            referencePreviewRect.value = { width, height }
        }
        if (immediate) {
            run()
            return
        }
        if (referencePreviewMeasureRaf !== null) {
            cancelAnimationFrame(referencePreviewMeasureRaf)
        }
        referencePreviewMeasureRaf = window.requestAnimationFrame(run)
    }

    const hoveredReferenceStyle = computed<Record<string, string>>(() => {
        if (!hoveredReferenceCode.value || typeof window === 'undefined') {
            return {
                top: '-9999px',
                left: '-9999px',
                width: '0px',
                maxHeight: '0px',
            }
        }
        const viewportWidth = window.innerWidth || 0
        const viewportHeight = window.innerHeight || 0
        const rect = hoveredReferenceAnchorRect.value
        const previewRectValue = referencePreviewRect.value
        const GAP = 12
        const HORIZONTAL_PADDING = 16
        const width = Math.min(560, Math.max(260, viewportWidth - HORIZONTAL_PADDING * 2))
        let left = rect ? rect.left - GAP - width : viewportWidth - width - HORIZONTAL_PADDING
        const minLeft = HORIZONTAL_PADDING
        const maxLeft = Math.max(minLeft, viewportWidth - width - HORIZONTAL_PADDING)
        if (Number.isFinite(left)) {
            left = Math.min(Math.max(left, minLeft), maxLeft)
        } else {
            left = minLeft
        }
        const desiredHeight = Math.min(520, Math.max(240, viewportHeight - GAP * 2))
        const measuredHeight = previewRectValue?.height ?? desiredHeight
        const minTop = GAP
        const maxTop = Math.max(minTop, viewportHeight - GAP - Math.min(measuredHeight, desiredHeight))
        let top = rect ? rect.top : minTop
        if (!Number.isFinite(top)) {
            top = minTop
        }
        top = Math.min(Math.max(top, minTop), maxTop)
        let availableSpace = Math.max(0, viewportHeight - top - GAP)
        if (availableSpace <= 0) {
            top = Math.max(minTop, viewportHeight - GAP - measuredHeight)
            availableSpace = Math.max(0, viewportHeight - top - GAP)
        }
        let maxHeight = availableSpace > 0 ? Math.min(measuredHeight, availableSpace) : Math.min(measuredHeight, viewportHeight - GAP * 2)
        if (availableSpace >= 160) {
            maxHeight = Math.max(maxHeight, 160)
        }
        if (maxHeight <= 0) {
            maxHeight = Math.min(measuredHeight, Math.max(120, viewportHeight - GAP * 2))
        }
        return {
            top: `${Math.round(top)}px`,
            left: `${Math.round(left)}px`,
            width: `${Math.round(width)}px`,
            maxHeight: `${Math.round(maxHeight)}px`,
        }
    })

    watch(hoveredReferenceCode, (code) => {
        if (!code) {
            hoveredReferenceAnchor.value = null
            hoveredReferenceAnchorRect.value = null
            return
        }
        nextTick(() => {
            updateReferenceAnchorRect(true)
            scheduleReferencePreviewMeasure(true)
        })
    })

    function clearReferenceDismissTimer() {
        if (referenceDismissTimer) {
            clearTimeout(referenceDismissTimer)
            referenceDismissTimer = null
        }
    }

    function scheduleReferenceDismiss() {
        clearReferenceDismissTimer()
        referenceDismissTimer = setTimeout(() => {
            hoveredReferenceCode.value = null
            setReferenceAnchor(null)
        }, REFERENCE_PREVIEW_DISMISS_DELAY)
    }

    function resolveReferenceAnchor(event?: Event) {
        const element = event?.currentTarget
        return element instanceof HTMLElement ? element : null
    }

    function setReferenceAnchor(element: HTMLElement | null) {
        if (hoveredReferenceAnchor.value === element) {
            updateReferenceAnchorRect(true)
            return
        }
        hoveredReferenceAnchor.value = element
        if (!element) {
            hoveredReferenceAnchorRect.value = null
            return
        }
        updateReferenceAnchorRect(true)
    }

    function ensureReferencePreview(code: string, before?: number, after?: number) {
        if (!code) return
        const entry =
            referencePreviewState[code] ||
            (referencePreviewState[code] = {
                loading: false,
                error: null,
                snippet: null,
                requestedBefore: REFERENCE_SNIPPET_DEFAULT_CONTEXT,
                requestedAfter: REFERENCE_SNIPPET_DEFAULT_CONTEXT,
            })
        const targetBefore = clampSnippetContext(typeof before === 'number' ? before : entry.requestedBefore)
        const targetAfter = clampSnippetContext(typeof after === 'number' ? after : entry.requestedAfter)
        if (entry.loading && entry.requestedBefore === targetBefore && entry.requestedAfter === targetAfter) {
            return
        }
        if (
            entry.snippet &&
            entry.requestedBefore === targetBefore &&
            entry.requestedAfter === targetAfter &&
            !entry.loading
        ) {
            return
        }
        entry.requestedBefore = targetBefore
        entry.requestedAfter = targetAfter
        fetchReferenceSnippet(code, targetBefore, targetAfter)
    }

    function fetchReferenceSnippet(code: string, before: number, after: number) {
        if (!code) return
        const entry =
            referencePreviewState[code] ||
            (referencePreviewState[code] = {
                loading: false,
                error: null,
                snippet: null,
                requestedBefore: REFERENCE_SNIPPET_DEFAULT_CONTEXT,
                requestedAfter: REFERENCE_SNIPPET_DEFAULT_CONTEXT,
            })
        entry.loading = true
        entry.error = null
        entry.requestedBefore = clampSnippetContext(before)
        entry.requestedAfter = clampSnippetContext(after)
        api
            .referenceSnippet(code, {
                before: entry.requestedBefore,
                after: entry.requestedAfter,
            })
            .then((snippet) => {
                entry.snippet = snippet
                entry.error = null
            })
            .catch((error: any) => {
                entry.error = error?.message || 'Failed to load snippet'
            })
            .finally(() => {
                entry.loading = false
                nextTick(() => scheduleReferencePreviewMeasure(true))
            })
    }

    function onReferenceEnter(code?: string | null, event?: Event) {
        if (!code) {
            hoveredReferenceCode.value = null
            setReferenceAnchor(null)
            return
        }
        clearReferenceDismissTimer()
        hoveredReferenceCode.value = code
        setReferenceAnchor(resolveReferenceAnchor(event) || hoveredReferenceAnchor.value)
        ensureReferencePreview(code)
    }

    function onReferenceLeave(code?: string | null) {
        if (!code) {
            scheduleReferenceDismiss()
            return
        }
        if (hoveredReferenceCode.value === code) {
            scheduleReferenceDismiss()
        }
    }

    function onReferencePreviewEnter(code?: string | null) {
        if (!code) return
        clearReferenceDismissTimer()
        hoveredReferenceCode.value = code
        setReferenceAnchor(hoveredReferenceAnchor.value)
    }

    function onReferencePreviewLeave(code?: string | null) {
        if (!code) return
        if (hoveredReferenceCode.value === code) {
            scheduleReferenceDismiss()
        }
    }

    function expandReferenceSnippet(code?: string | null, direction: 'before' | 'after' = 'after') {
        if (!code) return
        const entry = referencePreviewState[code]
        if (!entry) {
            ensureReferencePreview(code)
            return
        }
        const currentBefore = entry.requestedBefore ?? REFERENCE_SNIPPET_DEFAULT_CONTEXT
        const currentAfter = entry.requestedAfter ?? REFERENCE_SNIPPET_DEFAULT_CONTEXT
        let targetBefore = currentBefore
        let targetAfter = currentAfter
        if (direction === 'before') {
            if (!entry.snippet?.has_more_before) return
            if (currentBefore >= REFERENCE_SNIPPET_MAX_CONTEXT) return
            targetBefore = Math.min(currentBefore + REFERENCE_SNIPPET_STEP, REFERENCE_SNIPPET_MAX_CONTEXT)
        } else {
            if (!entry.snippet?.has_more_after) return
            if (currentAfter >= REFERENCE_SNIPPET_MAX_CONTEXT) return
            targetAfter = Math.min(currentAfter + REFERENCE_SNIPPET_STEP, REFERENCE_SNIPPET_MAX_CONTEXT)
        }
        ensureReferencePreview(code, targetBefore, targetAfter)
    }

    function isReferenceLineHighlighted(code: string, lineNumber: number) {
        const entry = referencePreviewState[code]
        if (!entry?.snippet) return false
        const { highlight_start, highlight_end } = entry.snippet
        return lineNumber >= highlight_start && lineNumber <= highlight_end
    }

    function setReferencePreviewElement(el: HTMLElement | null) {
        if (referencePreviewElement.value === el) {
            return
        }
        referencePreviewElement.value = el
        if (!el) {
            referencePreviewRect.value = null
            return
        }
        scheduleReferencePreviewMeasure(true)
    }

    function handleReferenceViewportChange() {
        updateReferenceAnchorRect()
        scheduleReferencePreviewMeasure()
    }

    function resetReferencePreviews() {
        hoveredReferenceCode.value = null
        setReferenceAnchor(null)
        clearReferenceDismissTimer()
        Object.keys(referencePreviewState).forEach((key) => delete referencePreviewState[key])
    }

    function cleanupRafs() {
        if (referenceAnchorRaf !== null && typeof window !== 'undefined') {
            window.cancelAnimationFrame(referenceAnchorRaf)
            referenceAnchorRaf = null
        }
        if (referencePreviewMeasureRaf !== null && typeof window !== 'undefined') {
            window.cancelAnimationFrame(referencePreviewMeasureRaf)
            referencePreviewMeasureRaf = null
        }
    }

    onMounted(() => {
        if (typeof window === 'undefined') return
        window.addEventListener('resize', handleReferenceViewportChange)
        window.addEventListener('scroll', handleReferenceViewportChange, true)
    })

    onUnmounted(() => {
        if (typeof window !== 'undefined') {
            window.removeEventListener('resize', handleReferenceViewportChange)
            window.removeEventListener('scroll', handleReferenceViewportChange, true)
        }
        clearReferenceDismissTimer()
        cleanupRafs()
    })

    return {
        hoveredReferenceCode,
        hoveredReferenceStyle,
        hoveredReferenceLoading,
        hoveredReferenceError,
        hoveredReferenceSnippet,
        hoveredReferenceCanExpand,
        hoveredReferenceCanExpandBefore,
        hoveredReferenceCanExpandAfter,
        onReferenceEnter,
        onReferenceLeave,
        onReferencePreviewEnter,
        onReferencePreviewLeave,
        expandReferenceSnippet,
        isReferenceLineHighlighted,
        setReferencePreviewElement,
        resetReferencePreviews,
    }
}

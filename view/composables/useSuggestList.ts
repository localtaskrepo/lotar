import { computed, ref, watch, type ComputedRef, type Ref } from 'vue'

export interface SuggestPart {
    text: string
    match: boolean
}

export interface SuggestEntry {
    value: string
    parts: SuggestPart[]
}

export interface SuggestKeydownHandlers {
    /** Called on Enter; receives the highlighted entry, or undefined to commit raw input. */
    onCommit: (active: string | undefined) => void
    /** Called on Escape after the active index is reset. */
    onEscape?: () => void
}

export interface UseSuggestListOptions {
    /** Full candidate list (already excluding selected values). */
    candidates: ComputedRef<string[]> | Ref<string[]> | (() => string[])
    /** The active query text used for substring filtering and highlighting. */
    query: () => string
    /** Whether the composer is active (focused); inactive composers show nothing. */
    active: () => boolean
    limit?: number
}

/**
 * Shared suggestion-list machinery: substring filtering with a size limit,
 * match highlighting, active-index management, and arrow/enter/tab/escape
 * keyboard navigation. Selection semantics stay with the caller via
 * `onCommit`/`onEscape` callbacks.
 */
export function useSuggestList(options: UseSuggestListOptions) {
    const limit = options.limit ?? 8
    const activeIndex = ref(-1)

    const resolveCandidates = () =>
        typeof options.candidates === 'function'
            ? options.candidates()
            : options.candidates.value

    const list = computed(() => {
        const base = resolveCandidates()
        if (!options.active() || !base.length) return [] as string[]
        const query = options.query().trim().toLowerCase()
        if (!query) return base.slice(0, limit)
        return base.filter((value) => value.toLowerCase().includes(query)).slice(0, limit)
    })

    const visible = computed(() => options.active() && list.value.length > 0)

    const entries = computed<SuggestEntry[]>(() =>
        list.value.map((value) => ({
            value,
            parts: highlightSuggestParts(value, options.query()),
        })),
    )

    watch(list, (next) => {
        activeIndex.value = next.length ? 0 : -1
    })

    function handleKeydown(event: KeyboardEvent, handlers: SuggestKeydownHandlers) {
        const suggestions = list.value
        if (event.key === 'ArrowDown') {
            if (!suggestions.length) return
            event.preventDefault()
            activeIndex.value = (activeIndex.value + 1 + suggestions.length) % suggestions.length
        } else if (event.key === 'ArrowUp') {
            if (!suggestions.length) return
            event.preventDefault()
            activeIndex.value = (activeIndex.value - 1 + suggestions.length) % suggestions.length
        } else if (event.key === 'Enter') {
            const active = activeIndex.value >= 0 ? suggestions[activeIndex.value] : undefined
            event.preventDefault()
            handlers.onCommit(active)
        } else if (event.key === 'Tab') {
            const active = activeIndex.value >= 0 ? suggestions[activeIndex.value] : undefined
            if (active) {
                event.preventDefault()
                handlers.onCommit(active)
            }
        } else if (event.key === 'Escape') {
            activeIndex.value = -1
            handlers.onEscape?.()
        }
    }

    return { activeIndex, list, entries, visible, handleKeydown }
}

/** Split a label into highlight segments around case-insensitive query matches. */
export function highlightSuggestParts(label: string, query: string): SuggestPart[] {
    const trimmed = query.trim()
    if (!trimmed) {
        return [{ text: label, match: false }]
    }
    const lowerLabel = label.toLowerCase()
    const lowerQuery = trimmed.toLowerCase()
    const segments: SuggestPart[] = []
    let searchStart = 0
    let matchIndex = lowerLabel.indexOf(lowerQuery)
    if (matchIndex === -1) {
        return [{ text: label, match: false }]
    }
    while (matchIndex !== -1) {
        if (matchIndex > searchStart) {
            segments.push({ text: label.slice(searchStart, matchIndex), match: false })
        }
        const matchEnd = matchIndex + lowerQuery.length
        segments.push({ text: label.slice(matchIndex, matchEnd), match: true })
        searchStart = matchEnd
        matchIndex = lowerLabel.indexOf(lowerQuery, searchStart)
    }
    if (searchStart < label.length) {
        segments.push({ text: label.slice(searchStart), match: false })
    }
    return segments
}

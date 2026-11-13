import { ref, type Ref } from 'vue'
import { storageGetJson, storageSetJson } from '../../utils/storage'

/** Which authors' comments the Comments tab hides, kept across tasks (DEV-502). */
export const COMMENT_FILTER_KEY = 'lotar.preferences.taskPanel.commentFilter'
/** Which authors and field groups the History tab hides, kept across tasks (DEV-502). */
export const HISTORY_FILTER_KEY = 'lotar.preferences.taskPanel.historyFilter'

export interface ActivityFilter {
    hiddenAuthors: Ref<Set<string>>
    hiddenFields: Ref<Set<string>>
    toggleAuthor: (author: string) => void
    toggleField: (field: string) => void
    showAll: () => void
}

function readStrings(value: unknown): Set<string> {
    return new Set(Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : [])
}

/** Hidden authors and fields for one tab, read from and written to the viewer's browser. */
export function useActivityFilter(key: string): ActivityFilter {
    const stored = storageGetJson(key) as { authors?: unknown; fields?: unknown } | null
    const hiddenAuthors = ref(readStrings(stored?.authors))
    const hiddenFields = ref(readStrings(stored?.fields))

    const persist = () => {
        storageSetJson(key, { authors: [...hiddenAuthors.value], fields: [...hiddenFields.value] })
    }

    const toggle = (set: Ref<Set<string>>, value: string) => {
        const next = new Set(set.value)
        if (next.has(value)) next.delete(value)
        else next.add(value)
        set.value = next
        persist()
    }

    return {
        hiddenAuthors,
        hiddenFields,
        toggleAuthor: (author) => toggle(hiddenAuthors, author),
        toggleField: (field) => toggle(hiddenFields, field),
        showAll: () => {
            hiddenAuthors.value = new Set()
            hiddenFields.value = new Set()
            persist()
        },
    }
}

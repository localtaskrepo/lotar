import { reactive } from 'vue'
import { api } from '../api/client'

// Repository path for each file name looked up so far (null: names no file),
// keyed by the directory it was written in and the name. Shared by every
// rendered markdown block; reading it inside a computed re-renders that block
// once its lookup answers.
const resolved = reactive(new Map<string, string | null>())
const queued = new Map<string, Set<string>>()
// Keys queued or in flight, so a re-render meanwhile does not ask again.
const requested = new Set<string>()
let flushTimer: ReturnType<typeof setTimeout> | null = null

// The server takes at most this many references per request.
const BATCH = 500

function key(base: string, reference: string) {
    return `${base}\n${reference}`
}

/**
 * Repository path of the file `reference` names, written in a file inside
 * directory `base` ('' for ticket text). Undefined until the lookup answers:
 * names written in one tick are looked up in one request.
 */
export function lookupFile(reference: string, base = ''): string | null | undefined {
    const entry = key(base, reference)
    if (resolved.has(entry)) return resolved.get(entry)
    if (requested.has(entry)) return undefined
    requested.add(entry)
    let names = queued.get(base)
    if (!names) {
        names = new Set()
        queued.set(base, names)
    }
    names.add(reference)
    flushTimer ??= setTimeout(flush, 0)
    return undefined
}

function flush() {
    flushTimer = null
    const batches = [...queued.entries()]
    queued.clear()
    for (const [base, names] of batches) {
        const references = [...names]
        for (let start = 0; start < references.length; start += BATCH) {
            void resolveBatch(base, references.slice(start, start + BATCH))
        }
    }
}

async function resolveBatch(base: string, references: string[]) {
    let paths = new Map<string, string>()
    try {
        const response = await api.resolveFiles({ base: base || undefined, references })
        paths = new Map(response.items.map((item) => [item.reference, item.path]))
    } catch {
        // Unlinked names are the same as before the viewer existed.
    }
    for (const reference of references) {
        const entry = key(base, reference)
        resolved.set(entry, paths.get(reference) ?? null)
        requested.delete(entry)
    }
}

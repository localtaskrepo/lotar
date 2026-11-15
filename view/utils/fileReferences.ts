/** A repository file and the lines a reference points at. */
export interface FileLocation {
    path: string
    line?: number
    endLine?: number
}

// `path:12`, `path:12-20`, `path:12:5` (line and column), `path#L12`,
// `path#L12-L20`, `path#12-20`.
const LOCATION_SUFFIX = /(?::(\d+)(?:-(\d+)|:\d+)?|#L?(\d+)(?:-L?(\d+))?)$/

// File names in prose end in an extension, so `e.g.` or `v0.15.0` mostly stay
// out; whether a candidate names a file is up to the server.
const PATH_CHARS = String.raw`[\p{Alphabetic}\p{Number}_.@+~-]`
const TEXT_CANDIDATE = new RegExp(
    String.raw`(?<![\p{Alphabetic}\p{Number}_./@~-])/?${PATH_CHARS}+(?:/${PATH_CHARS}+)*\.\p{Alphabetic}[\p{Alphabetic}\p{Number}]{0,9}` +
        String.raw`(?::\d+(?:-\d+)?|#L\d+(?:-L?\d+)?)?(?![\p{Alphabetic}\p{Number}_/@~-])`,
    'gu',
)

// Inline code holding nothing but a path: one with a directory or an extension.
const CODE_PATH = new RegExp(String.raw`^/?${PATH_CHARS}+(?:/${PATH_CHARS}+)*/?$`, 'u')
const EXTENSION = /\.\p{Alphabetic}[\p{Alphabetic}\p{Number}]{0,9}$/u

/** Split `path:12-20` or `path#L12` into the path and its lines. */
export function parseFileLocation(text: string): FileLocation {
    const trimmed = text.trim()
    const match = LOCATION_SUFFIX.exec(trimmed)
    if (!match) return { path: trimmed }
    const path = trimmed.slice(0, match.index)
    const start = Number(match[1] ?? match[3])
    const end = Number(match[2] ?? match[4])
    if (!path || !start) return { path: trimmed }
    return end > start ? { path, line: start, endLine: end } : { path, line: start }
}

/** The location as written in a `file` query value: `path`, `path:12`, `path:12-20`. */
export function formatFileLocation(location: FileLocation): string {
    if (!location.line) return location.path
    if (location.endLine && location.endLine > location.line)
        return `${location.path}:${location.line}-${location.endLine}`
    return `${location.path}:${location.line}`
}

/** The location inline code names, or null when it is not a path. */
export function codeFileCandidate(text: string): FileLocation | null {
    const trimmed = text.trim()
    if (!trimmed || trimmed.length > 300 || trimmed.includes('://') || trimmed.startsWith('-')) return null
    const location = parseFileLocation(trimmed)
    if (!CODE_PATH.test(location.path) || location.path.endsWith('/')) return null
    if (!location.path.includes('/') && !EXTENSION.test(location.path)) return null
    return location
}

/** File-name candidates in a run of prose, with their offsets. */
export function textFileCandidates(text: string): Array<{ index: number; text: string; location: FileLocation }> {
    if (!text.includes('.')) return []
    const found: Array<{ index: number; text: string; location: FileLocation }> = []
    for (const match of text.matchAll(TEXT_CANDIDATE)) {
        found.push({ index: match.index, text: match[0], location: parseFileLocation(match[0]) })
    }
    return found
}

/** Directory part of a repository path, `''` for top-level files. */
export function fileDirectory(path: string): string {
    const slash = path.lastIndexOf('/')
    return slash < 0 ? '' : path.slice(0, slash)
}

import type { HLJSApi, LanguageFn } from 'highlight.js'

// Grammars load on first use, so the app bundle carries none of them.
type Loader = () => Promise<{ default: LanguageFn }>
const LANGUAGES: Record<string, Loader> = {
    bash: () => import('highlight.js/lib/languages/bash'),
    css: () => import('highlight.js/lib/languages/css'),
    diff: () => import('highlight.js/lib/languages/diff'),
    dockerfile: () => import('highlight.js/lib/languages/dockerfile'),
    go: () => import('highlight.js/lib/languages/go'),
    ini: () => import('highlight.js/lib/languages/ini'),
    javascript: () => import('highlight.js/lib/languages/javascript'),
    json: () => import('highlight.js/lib/languages/json'),
    markdown: () => import('highlight.js/lib/languages/markdown'),
    python: () => import('highlight.js/lib/languages/python'),
    rust: () => import('highlight.js/lib/languages/rust'),
    sql: () => import('highlight.js/lib/languages/sql'),
    swift: () => import('highlight.js/lib/languages/swift'),
    typescript: () => import('highlight.js/lib/languages/typescript'),
    xml: () => import('highlight.js/lib/languages/xml'),
    yaml: () => import('highlight.js/lib/languages/yaml'),
}

const BY_EXTENSION: Record<string, string> = {
    bash: 'bash',
    sh: 'bash',
    zsh: 'bash',
    css: 'css',
    scss: 'css',
    diff: 'diff',
    patch: 'diff',
    go: 'go',
    ini: 'ini',
    toml: 'ini',
    cfg: 'ini',
    js: 'javascript',
    mjs: 'javascript',
    cjs: 'javascript',
    jsx: 'javascript',
    json: 'json',
    jsonc: 'json',
    md: 'markdown',
    markdown: 'markdown',
    py: 'python',
    rs: 'rust',
    sql: 'sql',
    swift: 'swift',
    ts: 'typescript',
    mts: 'typescript',
    cts: 'typescript',
    tsx: 'typescript',
    html: 'xml',
    xml: 'xml',
    svg: 'xml',
    plist: 'xml',
    vue: 'xml',
    yml: 'yaml',
    yaml: 'yaml',
}

// Grammars a language embeds: script and style blocks in HTML and Vue files.
const EMBEDDED: Record<string, string[]> = { xml: ['javascript', 'css'] }

const BY_NAME: Record<string, string> = { dockerfile: 'dockerfile', containerfile: 'dockerfile' }

// Highlighting a huge file would stall the page; past this it stays plain.
const MAX_HIGHLIGHT_CHARS = 400_000

let core: Promise<HLJSApi> | null = null

/** The highlight.js language for a repository path, or null for plain text. */
export function languageForPath(path: string): string | null {
    const name = path.slice(path.lastIndexOf('/') + 1).toLowerCase()
    if (BY_NAME[name]) return BY_NAME[name]
    const dot = name.lastIndexOf('.')
    return dot > 0 ? (BY_EXTENSION[name.slice(dot + 1)] ?? null) : null
}

/**
 * The text as one HTML string per line: highlighted when the language is
 * known, escaped otherwise. Tokens that span lines (block comments, strings)
 * are closed at each line end and reopened on the next line.
 */
export async function highlightLines(text: string, language: string | null): Promise<string[]> {
    if (!language || !LANGUAGES[language] || text.length > MAX_HIGHLIGHT_CHARS) return text.split('\n').map(escapeHtml)
    core ??= import('highlight.js/lib/core').then((module) => module.default)
    const hljs = await core
    for (const name of [...(EMBEDDED[language] ?? []), language]) {
        const load = LANGUAGES[name]
        if (load && !hljs.getLanguage(name)) hljs.registerLanguage(name, (await load()).default)
    }
    return splitHighlightedLines(hljs.highlight(text, { language, ignoreIllegals: true }).value)
}

export function escapeHtml(text: string): string {
    return text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')
}

/** Split highlight.js output (escaped text and `<span>` tags only) into balanced lines. */
export function splitHighlightedLines(html: string): string[] {
    const lines: string[] = []
    const open: string[] = []
    let line = ''
    let last = 0
    for (const match of html.matchAll(/<span[^>]*>|<\/span>|\n/g)) {
        line += html.slice(last, match.index)
        last = match.index + match[0].length
        if (match[0] === '\n') {
            lines.push(line + '</span>'.repeat(open.length))
            line = open.join('')
        } else if (match[0] === '</span>') {
            open.pop()
            line += match[0]
        } else {
            open.push(match[0])
            line += match[0]
        }
    }
    lines.push(line + html.slice(last))
    return lines
}

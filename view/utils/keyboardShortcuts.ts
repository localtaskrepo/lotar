// Keyboard shortcut definitions shared by the global handler, the command
// palette and the shortcuts reference (DEV-158).

/** `g` then a letter jumps to a page. */
export const GO_SHORTCUTS: Array<{ key: string; path: string; label: string }> = [
    { key: 't', path: '/', label: 'Tasks' },
    { key: 's', path: '/sprints', label: 'Sprints' },
    { key: 'b', path: '/boards', label: 'Boards' },
    { key: 'c', path: '/calendar', label: 'Calendar' },
    { key: 'd', path: '/dependencies', label: 'Dependencies' },
    { key: 'i', path: '/insights', label: 'Insights' },
    { key: 'a', path: '/agents', label: 'Agents' },
    { key: 'u', path: '/automations', label: 'Automations' },
    { key: 'y', path: '/sync', label: 'Sync' },
    { key: 'n', path: '/scan', label: 'Scan' },
    { key: 'o', path: '/config', label: 'Config' },
    { key: 'p', path: '/preferences', label: 'Preferences' },
]

/** How long after `g` the second key still counts. */
export const GO_SEQUENCE_TIMEOUT_MS = 1500

export function isMacPlatform(): boolean {
    if (typeof navigator === 'undefined') return false
    const platform =
        (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData?.platform ||
        navigator.platform ||
        navigator.userAgent
    return /mac|iphone|ipad|ipod/i.test(platform)
}

/** The palette shortcut as shown to the user: `⌘K` on Apple devices, `Ctrl K` elsewhere. */
export function paletteShortcutLabel(): string {
    return isMacPlatform() ? '⌘K' : 'Ctrl K'
}

/** Ctrl+K, or Cmd+K on Apple devices, without other modifiers. */
export function isPaletteChord(event: KeyboardEvent): boolean {
    if (event.key.toLowerCase() !== 'k' || event.altKey || event.shiftKey) return false
    return isMacPlatform() ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey
}

/** True while the user is typing: single-key shortcuts must not fire there. */
export function isEditableTarget(target: EventTarget | null): boolean {
    if (!(target instanceof HTMLElement)) return false
    if (target.isContentEditable) return true
    const tag = target.tagName
    if (tag === 'TEXTAREA' || tag === 'SELECT') return true
    if (tag !== 'INPUT') return false
    const type = (target as HTMLInputElement).type
    return !['checkbox', 'radio', 'button', 'submit', 'reset', 'range', 'color', 'file'].includes(type)
}

/**
 * Whether a command matches the palette query: every query word must appear
 * in the label or its keywords (case-insensitive). Returns a rank for
 * ordering (lower is better), or null when it does not match.
 */
export function commandMatchRank(query: string, label: string, keywords = ''): number | null {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean)
    if (!words.length) return 0
    const lowerLabel = label.toLowerCase()
    const haystack = `${lowerLabel} ${keywords.toLowerCase()}`
    if (!words.every((word) => haystack.includes(word))) return null
    const phrase = words.join(' ')
    if (lowerLabel.startsWith(phrase)) return 0
    if (lowerLabel.split(/\s+/).some((word) => word.startsWith(words[0]!))) return 1
    return 2
}

import { onMounted, onUnmounted } from 'vue'
import { GO_SEQUENCE_TIMEOUT_MS, GO_SHORTCUTS, isEditableTarget, isPaletteChord } from '../utils/keyboardShortcuts'

export interface GlobalShortcutHandlers {
    openPalette: () => void
    openHelp: () => void
    newTask: () => void
    navigate: (path: string) => void
    /** Single-key shortcuts are on (a preference; Ctrl/Cmd+K always works). */
    singleKeyEnabled: () => boolean
    /** Something already owns the keyboard, such as an open dialog or task panel. */
    busy: () => boolean
    /** The palette cannot open now; it may open over the task panel, unlike single keys. */
    paletteBlocked: () => boolean
}

/**
 * App-wide keyboard shortcuts (DEV-158): Ctrl/Cmd+K anywhere; `?`, `c` and
 * `g` + letter only while not typing and when nothing else is open.
 */
export function useGlobalShortcuts(handlers: GlobalShortcutHandlers) {
    let goPending = false
    let goTimer: ReturnType<typeof setTimeout> | null = null

    function resetGo() {
        goPending = false
        if (goTimer) clearTimeout(goTimer)
        goTimer = null
    }

    function onKeydown(event: KeyboardEvent) {
        if (event.defaultPrevented || event.isComposing) return
        if (isPaletteChord(event)) {
            if (handlers.paletteBlocked()) return
            event.preventDefault()
            resetGo()
            handlers.openPalette()
            return
        }
        if (event.ctrlKey || event.metaKey || event.altKey) return
        if (!handlers.singleKeyEnabled() || isEditableTarget(event.target) || handlers.busy()) {
            resetGo()
            return
        }
        const key = event.key
        if (goPending) {
            const target = GO_SHORTCUTS.find((entry) => entry.key === key.toLowerCase())
            resetGo()
            if (target) {
                event.preventDefault()
                handlers.navigate(target.path)
            }
            return
        }
        if (key === '?') {
            event.preventDefault()
            handlers.openHelp()
        } else if (key === 'c' && !event.shiftKey) {
            event.preventDefault()
            handlers.newTask()
        } else if (key === 'g' && !event.shiftKey) {
            goPending = true
            goTimer = setTimeout(resetGo, GO_SEQUENCE_TIMEOUT_MS)
        }
    }

    onMounted(() => window.addEventListener('keydown', onKeydown))
    onUnmounted(() => {
        window.removeEventListener('keydown', onKeydown)
        resetGo()
    })

    return { onKeydown }
}

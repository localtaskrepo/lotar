import { onMounted, onUnmounted } from 'vue'
import { isEditableTarget } from '../utils/keyboardShortcuts'
import { readSingleKeyShortcutsPreference } from '../utils/preferences'
import { useTaskPanelController } from './useTaskPanelController'

/**
 * `j` / `k` move focus to the next or previous item of a task list or board
 * (DEV-198); the item's own Enter handler opens it. Like the other
 * single-key shortcuts this never fires while typing, while a dialog or the
 * task panel is open, or when single-key shortcuts are turned off.
 */
export function useListKeyNavigation(itemSelector: string) {
    const { state: panel } = useTaskPanelController()

    function items(): HTMLElement[] {
        return [...document.querySelectorAll<HTMLElement>(itemSelector)].filter(
            (element) => element.offsetParent !== null || element.getClientRects().length > 0,
        )
    }

    function onKeydown(event: KeyboardEvent) {
        if (event.defaultPrevented || event.isComposing) return
        if (event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) return
        if (event.key !== 'j' && event.key !== 'k') return
        if (!readSingleKeyShortcutsPreference() || isEditableTarget(event.target)) return
        if (panel.open || document.querySelector('[aria-modal="true"]')) return
        const list = items()
        if (!list.length) return
        event.preventDefault()
        const active = document.activeElement
        const current = list.findIndex((element) => element === active || element.contains(active))
        const next =
            current === -1
                ? event.key === 'j'
                    ? 0
                    : list.length - 1
                : Math.min(list.length - 1, Math.max(0, current + (event.key === 'j' ? 1 : -1)))
        const target = list[next]!
        target.focus({ preventScroll: true })
        target.scrollIntoView?.({ block: 'nearest' })
    }

    onMounted(() => window.addEventListener('keydown', onKeydown))
    onUnmounted(() => window.removeEventListener('keydown', onKeydown))
}

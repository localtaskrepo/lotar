import { onMounted, onUnmounted } from 'vue'

interface UseTaskPanelHotkeysOptions {
    isOpen: () => boolean
    onClose: () => void
}

export function useTaskPanelHotkeys(options: UseTaskPanelHotkeysOptions) {
    const closeListener = (event: KeyboardEvent) => {
        if (event.key !== 'Escape' || !options.isOpen()) return
        // DEV-263: an inner control (suggestion list, nested dialog) already
        // used this Escape; it must not also close the panel.
        if (event.defaultPrevented) return
        options.onClose()
    }

    onMounted(() => {
        if (typeof window !== 'undefined') {
            window.addEventListener('keydown', closeListener)
        }
    })

    onUnmounted(() => {
        if (typeof window !== 'undefined') {
            window.removeEventListener('keydown', closeListener)
        }
    })

    return {
        closeListener,
    }
}

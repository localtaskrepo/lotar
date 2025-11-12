import { onMounted, onUnmounted } from 'vue'

interface UseTaskPanelHotkeysOptions {
    isOpen: () => boolean
    onClose: () => void
}

export function useTaskPanelHotkeys(options: UseTaskPanelHotkeysOptions) {
    const closeListener = (event: KeyboardEvent) => {
        if (event.key === 'Escape' && options.isOpen()) {
            options.onClose()
        }
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

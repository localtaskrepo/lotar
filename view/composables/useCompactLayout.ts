import { computed, ref, type ComputedRef } from 'vue'
import {
    NARROW_LAYOUT_KEY,
    onPreferencesChanged,
    readNarrowLayoutPreference,
    type NarrowLayout,
} from '../utils/preferences'

/** Widths at or below this get the compact Board and Tasks layout (DEV-419); matches the CSS breakpoint. */
export const NARROW_QUERY = '(max-width: 640px)'

const narrow = ref(false)
const preference = ref<NarrowLayout>('compact')
const compact = computed(() => narrow.value && preference.value === 'compact')
let started = false

function start() {
    if (started || typeof window === 'undefined') return
    started = true
    preference.value = readNarrowLayoutPreference()
    onPreferencesChanged((key) => {
        if (key === NARROW_LAYOUT_KEY) preference.value = readNarrowLayoutPreference()
    })
    const query = typeof window.matchMedia === 'function' ? window.matchMedia(NARROW_QUERY) : null
    if (!query) return
    narrow.value = query.matches
    query.addEventListener('change', (event) => {
        narrow.value = event.matches
    })
}

/** True on a narrow screen unless Preferences chose the classic layout. Shared by every caller. */
export function useCompactLayout(): ComputedRef<boolean> {
    start()
    return compact
}

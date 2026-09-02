import { computed, ref, watch } from 'vue'

const STORAGE_KEY = 'lotar.preferences.hiddenTabs'

export interface NavTabDefinition {
    label: string
    path: string
}

/** All main tabs, in nav order. */
export const NAV_TABS: NavTabDefinition[] = [
    { label: 'Tasks', path: '/' },
    { label: 'Sprints', path: '/sprints' },
    { label: 'Boards', path: '/boards' },
    { label: 'Calendar', path: '/calendar' },
    { label: 'Insights', path: '/insights' },
    { label: 'Agents', path: '/agents' },
    { label: 'Automations', path: '/automations' },
    { label: 'Sync', path: '/sync' },
    { label: 'Scan', path: '/scan' },
    { label: 'Config', path: '/config' },
]

/** Tabs that cannot be hidden (the preferences UI itself must stay reachable). */
const ALWAYS_VISIBLE = new Set(['/preferences'])

function readHidden(): Set<string> {
    try {
        const raw = typeof localStorage !== 'undefined' ? localStorage.getItem(STORAGE_KEY) : null
        if (!raw) return new Set()
        const parsed = JSON.parse(raw)
        if (!Array.isArray(parsed)) return new Set()
        return new Set(parsed.filter((p): p is string => typeof p === 'string'))
    } catch {
        return new Set()
    }
}

const hiddenTabs = ref<Set<string>>(readHidden())

watch(hiddenTabs, (value) => {
    try {
        if (typeof localStorage !== 'undefined') {
            localStorage.setItem(STORAGE_KEY, JSON.stringify(Array.from(value)))
        }
    } catch {
        // ignore persistence errors
    }
}, { deep: true })

export function useNavTabs() {
    const visibleTabs = computed(() =>
        NAV_TABS.filter((tab) => !hiddenTabs.value.has(tab.path)),
    )

    function isTabVisible(path: string): boolean {
        return ALWAYS_VISIBLE.has(path) || !hiddenTabs.value.has(path)
    }

    function setTabHidden(path: string, hidden: boolean) {
        if (ALWAYS_VISIBLE.has(path)) return
        const next = new Set(hiddenTabs.value)
        if (hidden) next.add(path)
        else next.delete(path)
        hiddenTabs.value = next
    }

    function toggleTab(path: string) {
        setTabHidden(path, !hiddenTabs.value.has(path))
    }

    return { visibleTabs, isTabVisible, setTabHidden, toggleTab, hiddenTabs }
}

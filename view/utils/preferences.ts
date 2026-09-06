import { storageGet, storageGetFlag, storageSet, storageSetFlag } from './storage'

export type StartupDestination = 'tasks' | 'sprints' | 'boards' | 'calendar' | 'insights' | 'config' | 'remember'
export type FixedStartupDestination = Exclude<StartupDestination, 'remember'>

const STARTUP_DESTINATION_KEY = 'lotar.preferences.startupDestination'
const LAST_VISITED_SECTION_KEY = 'lotar.preferences.lastVisitedSection'

const TASK_PANEL_SHOW_ATTACHMENTS_KEY = 'lotar.preferences.taskPanel.showAttachments'
const TASK_PANEL_SHOW_LINKS_IN_ATTACHMENTS_KEY = 'lotar.preferences.taskPanel.showLinksInAttachments'
const TASK_PANEL_AUTO_DETECT_LINKS_KEY = 'lotar.preferences.taskPanel.autoDetectLinks'

const TASKS_PAGE_SIZE_KEY = 'lotar.preferences.tasks.pageSize'
export const DEFAULT_TASKS_PAGE_SIZE = 50
const TASKS_PAGE_SIZE_CHOICES = new Set([25, 50, 100, 200])

const PREFERENCES_EVENT = 'lotar:preferences-changed'

function emitPreferencesChanged(key: string) {
    if (typeof window === 'undefined') return
    try {
        window.dispatchEvent(new CustomEvent(PREFERENCES_EVENT, { detail: { key } }))
    } catch {
        // ignore event failures
    }
}

export function onPreferencesChanged(handler: (key: string) => void): () => void {
    if (typeof window === 'undefined') return () => { }
    const listener = (event: Event) => {
        const detail = (event as CustomEvent<any>)?.detail
        const key = typeof detail?.key === 'string' ? detail.key : ''
        handler(key)
    }
    window.addEventListener(PREFERENCES_EVENT, listener as any)
    return () => window.removeEventListener(PREFERENCES_EVENT, listener as any)
}

const DEFAULT_FIXED_DESTINATION: FixedStartupDestination = 'tasks'
export const DEFAULT_STARTUP_DESTINATION: StartupDestination = DEFAULT_FIXED_DESTINATION

const DESTINATION_PATHS: Record<FixedStartupDestination, string> = {
    tasks: '/',
    sprints: '/sprints',
    boards: '/boards',
    calendar: '/calendar',
    insights: '/insights',
    config: '/config',
}

const VALID_DESTINATIONS: StartupDestination[] = ['tasks', 'sprints', 'boards', 'calendar', 'insights', 'config', 'remember']
const STARTUP_PATH_CHOICES = new Set(Object.values(DESTINATION_PATHS))

export const STARTUP_DESTINATION_OPTIONS: Array<{ value: StartupDestination; label: string }> = [
    { value: 'tasks', label: 'Tasks list' },
    { value: 'sprints', label: 'Sprints overview' },
    { value: 'boards', label: 'Boards view' },
    { value: 'calendar', label: 'Calendar' },
    { value: 'insights', label: 'Insights' },
    { value: 'config', label: 'Config dashboard' },
    { value: 'remember', label: 'Remember where I left off' },
]

export function readStartupDestination(): StartupDestination {
    const stored = storageGet(STARTUP_DESTINATION_KEY) as StartupDestination | null
    if (stored && VALID_DESTINATIONS.includes(stored)) {
        return stored
    }
    return DEFAULT_STARTUP_DESTINATION
}

export function storeStartupDestination(destination: StartupDestination) {
    storageSet(STARTUP_DESTINATION_KEY, destination)
}

export function readLastVisitedStartupRoute(): string | null {
    const stored = storageGet(LAST_VISITED_SECTION_KEY)
    if (stored && isStartupRoutePath(stored)) {
        return stored
    }
    return null
}

export function storeLastVisitedStartupRoute(path: string) {
    if (!isStartupRoutePath(path)) return
    storageSet(LAST_VISITED_SECTION_KEY, path)
}

function readBooleanPreference(key: string, defaultValue: boolean): boolean {
    return storageGetFlag(key, defaultValue)
}

function storeBooleanPreference(key: string, value: boolean) {
    storageSetFlag(key, value)
    emitPreferencesChanged(key)
}

export function readTasksPageSizePreference(): number {
    const raw = storageGet(TASKS_PAGE_SIZE_KEY)
    if (raw === null) return DEFAULT_TASKS_PAGE_SIZE
    const parsed = Number.parseInt(raw, 10)
    if (!Number.isFinite(parsed)) return DEFAULT_TASKS_PAGE_SIZE
    if (TASKS_PAGE_SIZE_CHOICES.has(parsed)) return parsed
    return DEFAULT_TASKS_PAGE_SIZE
}

export function storeTasksPageSizePreference(value: number) {
    const normalized = TASKS_PAGE_SIZE_CHOICES.has(value) ? value : DEFAULT_TASKS_PAGE_SIZE
    storageSet(TASKS_PAGE_SIZE_KEY, String(normalized))
    emitPreferencesChanged(TASKS_PAGE_SIZE_KEY)
}

export function readTaskPanelShowAttachmentsPreference(): boolean {
    return readBooleanPreference(TASK_PANEL_SHOW_ATTACHMENTS_KEY, true)
}

export function storeTaskPanelShowAttachmentsPreference(value: boolean) {
    storeBooleanPreference(TASK_PANEL_SHOW_ATTACHMENTS_KEY, value)
}

export function readTaskPanelShowLinksInAttachmentsPreference(): boolean {
    return readBooleanPreference(TASK_PANEL_SHOW_LINKS_IN_ATTACHMENTS_KEY, true)
}

export function storeTaskPanelShowLinksInAttachmentsPreference(value: boolean) {
    storeBooleanPreference(TASK_PANEL_SHOW_LINKS_IN_ATTACHMENTS_KEY, value)
}

export function readTaskPanelAutoDetectLinksPreference(): boolean {
    return readBooleanPreference(TASK_PANEL_AUTO_DETECT_LINKS_KEY, true)
}

export function storeTaskPanelAutoDetectLinksPreference(value: boolean) {
    storeBooleanPreference(TASK_PANEL_AUTO_DETECT_LINKS_KEY, value)
}

export function resolveStartupPath(destination: StartupDestination, lastVisited: string | null): string {
    if (destination === 'remember') {
        if (lastVisited && isStartupRoutePath(lastVisited)) {
            return lastVisited
        }
        return DESTINATION_PATHS[DEFAULT_FIXED_DESTINATION]
    }
    const fixedDestination = destination as FixedStartupDestination
    return DESTINATION_PATHS[fixedDestination] ?? DESTINATION_PATHS[DEFAULT_FIXED_DESTINATION]
}

export function getStartupRedirectPath(currentPath: string): string | null {
    if (typeof window === 'undefined') return null
    const preference = readStartupDestination()
    const lastVisited = readLastVisitedStartupRoute()
    const target = resolveStartupPath(preference, lastVisited)
    if (target === currentPath) return null
    return target
}

export function isStartupRoutePath(path: string | null | undefined): path is string {
    if (!path) return false
    return STARTUP_PATH_CHOICES.has(path)
}

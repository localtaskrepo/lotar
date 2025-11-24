export type StartupDestination = 'tasks' | 'sprints' | 'boards' | 'calendar' | 'insights' | 'config' | 'remember'
export type FixedStartupDestination = Exclude<StartupDestination, 'remember'>

const STARTUP_DESTINATION_KEY = 'lotar.preferences.startupDestination'
const LAST_VISITED_SECTION_KEY = 'lotar.preferences.lastVisitedSection'

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
    if (typeof window === 'undefined') return DEFAULT_STARTUP_DESTINATION
    try {
        const stored = localStorage.getItem(STARTUP_DESTINATION_KEY) as StartupDestination | null
        if (stored && VALID_DESTINATIONS.includes(stored)) {
            return stored
        }
    } catch (err) {
        console.warn('Unable to read startup destination preference', err)
    }
    return DEFAULT_STARTUP_DESTINATION
}

export function storeStartupDestination(destination: StartupDestination) {
    if (typeof window === 'undefined') return
    try {
        localStorage.setItem(STARTUP_DESTINATION_KEY, destination)
    } catch (err) {
        console.warn('Unable to persist startup destination preference', err)
    }
}

export function readLastVisitedStartupRoute(): string | null {
    if (typeof window === 'undefined') return null
    try {
        const stored = localStorage.getItem(LAST_VISITED_SECTION_KEY)
        if (stored && isStartupRoutePath(stored)) {
            return stored
        }
    } catch (err) {
        console.warn('Unable to read last visited section', err)
    }
    return null
}

export function storeLastVisitedStartupRoute(path: string) {
    if (typeof window === 'undefined' || !isStartupRoutePath(path)) return
    try {
        localStorage.setItem(LAST_VISITED_SECTION_KEY, path)
    } catch (err) {
        console.warn('Unable to persist last visited section', err)
    }
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

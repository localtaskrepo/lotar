import { reactive, readonly } from 'vue'
import { api } from '../api/client'
import type { BrandingResponse } from '../api/types'
import { setRepositoryTheme } from '../utils/theme'
import { useSse } from './useSse'

/** Repository branding (DEV-218), shared by the header, tab and settings. */
export interface BrandingState {
    loaded: boolean
    theme: string | null
    themeSource: 'home' | 'repository' | null
    repositoryTheme: string | null
    name: string | null
    logo: string | null
    logoDark: string | null
    icon: string | null
    problems: string[]
}

export const DEFAULT_APP_NAME = 'LoTaR'

const state = reactive<BrandingState>({
    loaded: false,
    theme: null,
    themeSource: null,
    repositoryTheme: null,
    name: null,
    logo: null,
    logoDark: null,
    icon: null,
    problems: [],
})

let watching = false
let inflight: Promise<void> | null = null

/** Apply a server response: theme default, tab title and icon. */
export function applyBranding(response: BrandingResponse) {
    state.loaded = true
    state.theme = response.theme ?? null
    state.themeSource = response.theme_source ?? null
    state.repositoryTheme = response.repository_theme ?? null
    state.name = response.name?.trim() || null
    state.logo = response.logo ?? null
    state.logoDark = response.logo_dark ?? null
    state.icon = response.icon ?? null
    state.problems = response.problems ?? []
    setRepositoryTheme(state.theme)
    if (typeof document === 'undefined') return
    document.title = state.name ?? DEFAULT_APP_NAME
    applyIcon(state.icon)
}

// The built-in icons from index.html, kept to restore them when the
// repository icon is removed.
let builtinIcons: Array<{ link: HTMLLinkElement; href: string; type: string }> | null = null

function applyIcon(url: string | null) {
    const links = Array.from(document.querySelectorAll<HTMLLinkElement>('link[rel~="icon"]'))
    if (!builtinIcons) builtinIcons = links.map((link) => ({ link, href: link.href, type: link.type }))
    for (const { link, href, type } of builtinIcons) {
        link.href = url ?? href
        // The repository icon can be any image type; let the browser sniff it.
        if (url) link.removeAttribute('type')
        else if (type) link.type = type
    }
}

export async function refreshBranding(): Promise<void> {
    if (inflight) return inflight
    inflight = (async () => {
        try {
            applyBranding(await api.branding())
        } catch {
            // Keep the built-in look when branding cannot be loaded.
            state.loaded = true
        } finally {
            inflight = null
        }
    })()
    return inflight
}

/** Load branding once and follow config changes from then on. */
export function useBranding() {
    if (!state.loaded && !inflight) void refreshBranding()
    if (!watching && typeof EventSource !== 'undefined') {
        watching = true
        const sse = useSse(
            '/api/events',
            { kinds: 'config_updated', ready: true },
            { onReconnect: () => void refreshBranding() },
        )
        sse.on('config_updated', () => void refreshBranding())
    }
    return readonly(state)
}

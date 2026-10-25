import type { RouteLocationNormalized, RouterScrollBehavior } from 'vue-router'

type ScrollPosition = { left: number; top: number }

/**
 * Wait until the page is tall enough to reach `top` (pages load their data
 * after the route changes), giving up after `timeoutMs`.
 */
export function whenScrollable(top: number, timeoutMs = 1500, interval = 50): Promise<void> {
    return new Promise((resolve) => {
        const started = Date.now()
        const check = () => {
            const reachable = document.documentElement.scrollHeight - window.innerHeight >= top
            if (reachable || Date.now() - started >= timeoutMs) resolve()
            else window.setTimeout(check, interval)
        }
        check()
    })
}

/**
 * Back/forward returns to where the page was left; a new page starts at the
 * top. Query-only changes on the same page (filters, project switches) and
 * hash links keep the current position so the page can handle them.
 */
export const scrollBehavior: RouterScrollBehavior = async (
    to: RouteLocationNormalized,
    from: RouteLocationNormalized,
    savedPosition: ScrollPosition | null,
) => {
    if (savedPosition) {
        await whenScrollable(savedPosition.top)
        return savedPosition
    }
    if (to.hash || to.path === from.path) return false
    return { top: 0 }
}

import { createApp } from 'vue'
import { createRouter, createWebHistory } from 'vue-router'
import App from './App.vue'
import { scrollBehavior } from './utils/routerScroll'
// DEV-274: only the default route ships in the entry chunk; every other page
// (and what only it uses, such as chart.js) loads on first visit.
import TasksList from './pages/TasksList.vue'
import './styles.css'
import './themes.css'
import { getStartupRedirectPath, storeLastVisitedStartupRoute } from './utils/preferences'
import { initializeThemeFromStorage } from './utils/theme'

if (typeof window !== 'undefined') {
    initializeThemeFromStorage()
    // After an upgrade, an open tab still names the old content-hashed page
    // chunks, which the server no longer has. Reload once to pick up the new
    // build instead of leaving the navigation dead.
    window.addEventListener('vite:preloadError', (event) => {
        const key = 'lotar.chunkReloadAt'
        let last: number
        try {
            last = Number(sessionStorage.getItem(key)) || 0
            sessionStorage.setItem(key, String(Date.now()))
        } catch {
            // Without session storage a retry loop cannot be ruled out: keep the error.
            return
        }
        // A second failure right after the reload is a real error, not a stale tab.
        if (Date.now() - last < 10_000) return
        event.preventDefault()
        window.location.reload()
    })
}

const router = createRouter({
    history: createWebHistory(),
    // oxlint-disable-next-line typescript/no-deprecated -- vue-router 5 replaces it with the experimental useScrollRestoration(); keep the stable option until that API is stable.
    scrollBehavior,
    routes: [
        { path: '/', component: TasksList },
        { path: '/sprints', component: () => import('./pages/SprintsList.vue') },
        { path: '/boards', component: () => import('./pages/Board.vue') },
        { path: '/calendar', component: () => import('./pages/Calendar.vue') },
        { path: '/insights', component: () => import('./pages/ProjectInsights.vue') },
        { path: '/dependencies', component: () => import('./pages/Dependencies.vue') },
        { path: '/agents', component: () => import('./pages/AgentJobs.vue') },
        { path: '/automations', component: () => import('./pages/AutomationView.vue') },
        { path: '/sync', component: () => import('./pages/SyncHub.vue') },
        { path: '/scan', component: () => import('./pages/ScanView.vue') },
        { path: '/task/:id', component: () => import('./pages/TaskDetails.vue') },
        { path: '/config', component: () => import('./pages/ConfigView.vue') },
        { path: '/preferences', component: () => import('./pages/Preferences.vue') },
        {
            path: '/sprints/backlog',
            redirect: (to) => ({ path: '/sprints', hash: '#backlog', query: to.query }),
        },
        { path: '/:pathMatch(.*)*', component: () => import('./pages/NotFound.vue') },
    ],
})

let startupRouteHandled = false
router.beforeEach((to, from, next) => {
    if (!startupRouteHandled) {
        startupRouteHandled = true
        if (to.path === '/') {
            const redirect = getStartupRedirectPath(to.path)
            if (redirect) {
                next(redirect)
                return
            }
        }
    }
    next()
})

router.afterEach((to) => {
    storeLastVisitedStartupRoute(to.path)
})

createApp(App).use(router).mount('#app')

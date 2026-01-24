import { createApp } from 'vue'
import { createRouter, createWebHistory } from 'vue-router'
import App from './App.vue'
import Board from './pages/Board.vue'
import Calendar from './pages/Calendar.vue'
import ConfigView from './pages/ConfigView.vue'
import NotFound from './pages/NotFound.vue'
import Preferences from './pages/Preferences.vue'
import ProjectInsights from './pages/ProjectInsights.vue'
import ScanView from './pages/ScanView.vue'
import SprintsList from './pages/SprintsList.vue'
import SyncHub from './pages/SyncHub.vue'
import TaskDetails from './pages/TaskDetails.vue'
import TasksList from './pages/TasksList.vue'
import './styles.css'
import { getStartupRedirectPath, storeLastVisitedStartupRoute } from './utils/preferences'
import { initializeThemeFromStorage } from './utils/theme'

if (typeof window !== 'undefined') {
  initializeThemeFromStorage()
}

const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', component: TasksList },
    { path: '/sprints', component: SprintsList },
    { path: '/boards', component: Board },
    { path: '/calendar', component: Calendar },
    { path: '/insights', component: ProjectInsights },
    { path: '/sync', component: SyncHub },
    { path: '/scan', component: ScanView },
    { path: '/task/:id', component: TaskDetails },
    { path: '/config', component: ConfigView },
    { path: '/preferences', component: Preferences },
    {
      path: '/sprints/backlog',
      redirect: (to) => ({ path: '/sprints', hash: '#backlog', query: to.query }),
    },
    { path: '/:pathMatch(.*)*', component: NotFound },
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

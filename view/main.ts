import { createApp } from 'vue'
import { createRouter, createWebHistory } from 'vue-router'
import App from './App.vue'
import Board from './pages/Board.vue'
import Calendar from './pages/Calendar.vue'
import ConfigView from './pages/ConfigView.vue'
import Preferences from './pages/Preferences.vue'
import ProjectInsights from './pages/ProjectInsights.vue'
import ProjectsList from './pages/ProjectsList.vue'
import TaskDetails from './pages/TaskDetails.vue'
import TaskStatistics from './pages/TaskStatistics.vue'
import TasksList from './pages/TasksList.vue'
import './styles.css'
import { initializeThemeFromStorage } from './utils/theme'

if (typeof window !== 'undefined') {
  initializeThemeFromStorage()
}

const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', component: TasksList },
    { path: '/boards', component: Board },
    { path: '/calendar', component: Calendar },
    { path: '/projects', component: ProjectsList },
    { path: '/statistics', component: TaskStatistics },
    { path: '/insights', component: ProjectInsights },
    { path: '/task/:id', component: TaskDetails },
    { path: '/config', component: ConfigView },
    { path: '/preferences', component: Preferences },
  ],
})

createApp(App).use(router).mount('#app')

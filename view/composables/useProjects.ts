import { computed, getCurrentScope, onScopeDispose, ref } from 'vue'
import type { ApiClient } from '../api/client'
import { api } from '../api/client'
import type { ProjectDTO, ProjectStatsDTO } from '../api/types'
import { createResource } from './useResource'

type ProjectsChangeListener = () => void | Promise<void>

const projectsChangeListeners = new Set<ProjectsChangeListener>()

/**
 * Ask every live useProjects instance with an already-loaded snapshot to
 * refresh, so mounted project selectors pick up mutations (e.g. a project
 * created in ConfigView) without a browser reload. Instances that never
 * loaded stay lazy and fetch on demand. Resolves once every listener has
 * settled; one failing listener cannot break the others.
 */
export async function notifyProjectsChanged(): Promise<void> {
  const listeners = [...projectsChangeListeners]
  await Promise.all(
    listeners.map(async (listener) => {
      try {
        await listener()
      } catch (error) {
        console.warn('useProjects: projects change listener failed', error)
      }
    }),
  )
}

export function createUseProjects(client: ApiClient) {
  const total = ref(0)
  const limit = ref(0)
  const offset = ref(0)

  const projectsResource = createResource<ProjectDTO[], []>(
    async () => {
      const pageSize = 500
      const maxPages = 1000

      const collected: ProjectDTO[] = []
      let currentOffset = 0
      let expectedTotal = 0
      let pages = 0

      while (pages < maxPages) {
        pages += 1
        const response = await client.listProjects({ limit: pageSize, offset: currentOffset })

        if (pages === 1) {
          total.value = response.total ?? 0
          limit.value = response.limit ?? pageSize
          offset.value = response.offset ?? 0
        }

        expectedTotal = response.total ?? expectedTotal
        const batch = Array.isArray(response.projects) ? response.projects : []
        if (batch.length === 0) {
          break
        }
        collected.push(...batch)
        currentOffset += batch.length

        if (expectedTotal && collected.length >= expectedTotal) {
          break
        }
      }

      if (pages >= maxPages) {
        console.warn('useProjects: maxPages reached while paging projects')
      }

      return collected
    },
    { initialValue: [] },
  )

  const statsResource = createResource<ProjectStatsDTO | null, [string]>(
    async (project: string) => client.projectStats(project),
    { initialValue: null, throwOnError: true },
  )

  const projects = computed<ProjectDTO[]>(() => projectsResource.data.value ?? [])
  const stats = computed<ProjectStatsDTO | null>(() => statsResource.data.value ?? null)
  const loading = projectsResource.loading
  const error = computed(() => projectsResource.error.value?.message ?? null)

  async function refresh() {
    await projectsResource.refresh()
  }

  async function loadStats(project: string) {
    const result = await statsResource.refresh(project)
    return result
  }

  function handleProjectsChanged() {
    if (projectsResource.status.value === 'idle') return
    return refresh()
  }

  projectsChangeListeners.add(handleProjectsChanged)
  if (getCurrentScope()) {
    onScopeDispose(() => {
      projectsChangeListeners.delete(handleProjectsChanged)
    })
  }

  return {
    projects,
    stats,
    loading,
    error,
    total,
    limit,
    offset,
    status: projectsResource.status,
    ready: projectsResource.ready,
    refresh,
    loadStats,
  }
}

export const useProjects = () => createUseProjects(api)

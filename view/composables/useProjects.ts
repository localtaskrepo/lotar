import { computed } from 'vue'
import type { ApiClient } from '../api/client'
import { api } from '../api/client'
import type { ProjectDTO, ProjectStatsDTO } from '../api/types'
import { createResource } from './useResource'

export function createUseProjects(client: ApiClient) {
  const projectsResource = createResource<ProjectDTO[], []>(
    async () => client.listProjects(),
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

  return {
    projects,
    stats,
    loading,
    error,
    status: projectsResource.status,
    ready: projectsResource.ready,
    refresh,
    loadStats,
  }
}

export const useProjects = () => createUseProjects(api)

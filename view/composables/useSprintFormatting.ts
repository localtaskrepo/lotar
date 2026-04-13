import { computed, type Ref } from 'vue';
import type { SprintListItem } from '../api/types';

export function useSprintFormatting(sprints: Ref<SprintListItem[]>) {
  const sprintLookup = computed<Record<number, { label: string; state?: string }>>(() => {
    const lookup: Record<number, { label: string; state?: string }> = {}
    for (const sprint of sprints.value) {
      const base = sprint.display_name || sprint.label || `Sprint ${sprint.id}`
      lookup[sprint.id] = {
        label: `#${sprint.id} ${base}`.trim(),
        state: sprint.state,
      }
    }
    return lookup
  })

  function sprintLabel(id: number) {
    const entry = sprintLookup.value[id]
    const raw = (entry?.label || `#${id}`).trim()
    const firstSpace = raw.indexOf(' ')
    if (firstSpace === -1) return raw
    const head = raw.slice(0, firstSpace)
    const tail = raw.slice(firstSpace + 1).trim()
    if (!tail) return head
    return `${head} ${tail}`
  }

  function sprintStateClass(id: number) {
    const state = sprintLookup.value[id]?.state?.toLowerCase()
    if (!state) return 'sprint--unknown'
    return `sprint--${state}`
  }

  function sprintTooltip(id: number) {
    return sprintLabel(id)
  }

  type SprintState = SprintListItem['state']
  const sprintStateColors: Record<SprintState | 'default', string> = {
    pending: 'var(--color-muted)',
    active: 'var(--color-accent)',
    overdue: 'var(--color-danger)',
    complete: 'var(--color-success)',
    default: 'var(--color-muted)',
  }

  function sprintColorForState(state?: string | null): string {
    const normalized = (state || '').toLowerCase() as SprintState
    return sprintStateColors[normalized] || sprintStateColors.default
  }

  return { sprintLookup, sprintLabel, sprintStateClass, sprintTooltip, sprintColorForState }
}

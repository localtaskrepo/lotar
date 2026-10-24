import type { RelationshipField, RelationshipMutationRequest, TaskRelationships } from '../api/types'

export const relationshipMime = 'application/x-lotar-relationship'
export const relationshipCategories = [
  { key: 'depends_on', label: 'Depends on', description: 'Prerequisites of this task' },
  { key: 'blocks', label: 'Blocks', description: 'Tasks waiting for this task' },
  { key: 'related', label: 'Related', description: 'Associated tasks, without dependency direction' },
  { key: 'children', label: 'Children', description: 'Tasks beneath this task' },
  { key: 'fixes', label: 'Fixes', description: 'Tasks fixed by this task' },
  { key: 'parent', label: 'Parent', description: 'The single task above this task' },
  { key: 'duplicate_of', label: 'Duplicate of', description: 'The single original task' },
] as const

export type RelationshipCategory = RelationshipField
export type RelationshipIntent = { action: 'add' | 'remove' | 'change_kind'; kind: RelationshipCategory; target: string; from_kind?: RelationshipCategory; expected_current?: string }
export function relationshipMutation(task: string, intent: RelationshipIntent): RelationshipMutationRequest {
  return { source: task, target: intent.target, kind: intent.kind, operation: intent.action, ...(intent.from_kind ? { from_kind: intent.from_kind } : {}), ...(intent.expected_current ? { expected_current: intent.expected_current } : {}) }
}
export function singletonRelationship(kind: string) { return kind === 'parent' || kind === 'duplicate_of' }
export function relationshipEntries(relationships: TaskRelationships | Record<string, string>, kind: RelationshipCategory): string[] {
  const raw = relationships[kind]
  const entries = Array.isArray(raw) ? raw : typeof raw === 'string' ? raw.split(',') : []
  return [...new Set(entries.map((id) => id.trim()).filter(Boolean))]
}

export function relationshipMoveDescription(from: RelationshipCategory, to: RelationshipCategory): string {
  if ((from === 'depends_on' && to === 'blocks') || (from === 'blocks' && to === 'depends_on')) {
    return 'This reverses dependency direction: prerequisite and dependent exchange roles.'
  }
  if ((from === 'parent' && to === 'children') || (from === 'children' && to === 'parent')) {
    return 'This reverses hierarchy direction: parent and child exchange roles.'
  }
  return 'Only this relationship kind changes. Other relationship kinds are preserved.'
}

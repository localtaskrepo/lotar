import { describe, expect, it } from 'vitest'
import TaskHoverCardSource from '../components/TaskHoverCard.vue?raw'

function extractStyle(source: string): string {
  const start = source.indexOf('<style')
  if (start === -1) return ''
  const openEnd = source.indexOf('>', start)
  if (openEnd === -1) return ''
  const close = source.indexOf('</style>', openEnd)
  if (close === -1) return ''
  return source.slice(openEnd + 1, close)
}

describe('TaskHoverCard styles', () => {
  it('keeps the task id on a single line', () => {
    const style = extractStyle(TaskHoverCardSource)
    expect(style).toMatch(/\.task-hover-card__id\s*\{[^}]*white-space\s*:\s*nowrap\s*;?/s)
  })
})

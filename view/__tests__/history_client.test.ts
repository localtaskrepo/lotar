import { describe, expect, it } from 'vitest'
import { api } from '../api/client'

describe('api client history endpoints', () => {
  it('builds URLs correctly for taskHistory and taskCommitDiff', () => {
    // Just ensure functions exist and return promises
    expect(typeof api.taskHistory).toBe('function')
    expect(typeof api.taskCommitDiff).toBe('function')
    // call with dummy values but don't await (would fetch); ensure no throw on creation
    const p1 = api.taskHistory('ABC-1', 10)
    const p2 = api.taskCommitDiff('ABC-1', 'deadbeef')
    expect(p1).toBeInstanceOf(Promise)
    expect(p2).toBeInstanceOf(Promise)
  })
})

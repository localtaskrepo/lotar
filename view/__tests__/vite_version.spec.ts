import { describe, expect, it } from 'vitest'

describe('VITE_CARGO_VERSION define', () => {
  it('is defined from vite.config.ts', () => {
    const v = (import.meta as any).env?.VITE_CARGO_VERSION
    expect(typeof v).toBe('string')
    expect(v.length).toBeGreaterThan(0)
  })
})

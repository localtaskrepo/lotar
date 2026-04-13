import { describe, expect, it } from 'vitest'
import { formatMember } from '../utils/member'

describe('formatMember', () => {
  it('returns empty string for empty input', () => {
    expect(formatMember('')).toBe('')
  })

  it('returns bare username as-is (no @ prepended)', () => {
    expect(formatMember('alice')).toBe('alice')
  })

  it('returns @-prefixed directives as-is', () => {
    expect(formatMember('@copilot')).toBe('@copilot')
  })

  it('returns email addresses as-is', () => {
    expect(formatMember('user@example.com')).toBe('user@example.com')
  })

  it('returns null/undefined as empty string', () => {
    expect(formatMember(null)).toBe('')
    expect(formatMember(undefined)).toBe('')
  })

  it('returns dotted username as-is', () => {
    expect(formatMember('john.doe')).toBe('john.doe')
  })
})

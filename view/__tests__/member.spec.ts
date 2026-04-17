import { describe, expect, it } from 'vitest'
import { formatMember, memberColor, memberInitials } from '../utils/member'

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

describe('memberInitials', () => {
  it('returns ? for empty/null/undefined', () => {
    expect(memberInitials('')).toBe('?')
    expect(memberInitials(null)).toBe('?')
    expect(memberInitials(undefined)).toBe('?')
  })

  it('extracts first two chars for single-word names', () => {
    expect(memberInitials('alice')).toBe('AL')
    expect(memberInitials('bob')).toBe('BO')
  })

  it('extracts first+last initials for dotted names', () => {
    expect(memberInitials('john.doe')).toBe('JD')
  })

  it('extracts first+last initials for hyphenated names', () => {
    expect(memberInitials('jane-smith')).toBe('JS')
  })

  it('uses email local part', () => {
    expect(memberInitials('alice@example.com')).toBe('AL')
  })

  it('strips @ prefix for directives', () => {
    expect(memberInitials('@copilot')).toBe('CO')
  })
})

describe('memberColor', () => {
  it('returns muted CSS var for empty input', () => {
    expect(memberColor('')).toBe('var(--muted)')
    expect(memberColor(null)).toBe('var(--muted)')
  })

  it('returns a valid hsl() string for a name', () => {
    const color = memberColor('alice')
    expect(color).toMatch(/^hsl\(\d+, 55%, 42%\)$/)
  })

  it('returns deterministic results (same input → same output)', () => {
    expect(memberColor('bob')).toBe(memberColor('bob'))
  })

  it('returns different colours for different names', () => {
    expect(memberColor('alice')).not.toBe(memberColor('bob'))
  })
})

/**
 * Format a member value (assignee/reporter) for display.
 *
 * Values starting with `@` are directives (e.g. `@copilot`) and are shown as-is.
 * Bare usernames and emails are also returned as-is — no `@` is prepended.
 */
export function formatMember(value: string | null | undefined): string {
  if (!value) return ''
  return value
}

/**
 * Extract 1–2 character initials from a member name for badge display.
 */
export function memberInitials(value: string | null | undefined): string {
  const name = (value || '').trim()
  if (!name) return '?'
  // Strip leading @ for directives
  const clean = name.startsWith('@') ? name.slice(1) : name
  // For emails, use local part
  const local = clean.includes('@') ? clean.split('@')[0] : clean
  const parts = local.split(/[\s._-]+/).filter(Boolean)
  if (parts.length >= 2) return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase()
  return local.slice(0, 2).toUpperCase()
}

/**
 * Deterministic HSL colour derived from a member name (for avatar badges).
 * Returns a CSS `hsl(…)` string.
 */
export function memberColor(value: string | null | undefined): string {
  const name = (value || '').trim().toLowerCase()
  if (!name) return 'var(--muted)'
  let hash = 0
  for (let i = 0; i < name.length; i++) {
    hash = name.charCodeAt(i) + ((hash << 5) - hash)
  }
  const hue = ((hash % 360) + 360) % 360
  return `hsl(${hue}, 55%, 42%)`
}

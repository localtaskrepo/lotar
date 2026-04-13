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

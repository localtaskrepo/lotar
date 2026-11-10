/** The `message` of a thrown value, or '' when it has none (callers add their own fallback). */
export function errorMessage(error: unknown): string {
    if (error && typeof error === 'object' && 'message' in error && typeof error.message === 'string') {
        return error.message
    }
    return ''
}

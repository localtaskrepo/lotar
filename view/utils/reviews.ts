import type { CommentReview, TaskComment } from '../api/types'

/** DEV-489: the newest review comment, if any. */
export function latestReview(comments: readonly TaskComment[] | null | undefined): TaskComment | null {
    return [...(comments ?? [])].reverse().find((comment) => comment.review) ?? null
}

/** The commit as people read it: its first 7 characters. */
export function shortCommit(commit: string | undefined): string {
    return (commit ?? '').slice(0, 7)
}

/** The badge text: "Review passed" or "Review blocked". */
export function verdictLabel(review: CommentReview): string {
    return review.verdict === 'pass' ? 'Review passed' : 'Review blocked'
}

/**
 * The reviewer's summary without what the server writes around it in prose
 * ("Review passed at <commit>." first, "\n\nFindings:\n- ..." last), since
 * the UI shows the verdict as a badge and the findings as a list.
 */
export function reviewSummary(comment: TaskComment): string {
    let text = comment.text || ''
    if (comment.review?.findings?.length) {
        const cut = text.lastIndexOf('\n\nFindings:\n')
        if (cut >= 0) text = text.slice(0, cut)
    }
    const head = text.match(/^Review (passed|blocked)( at \S+)?\.(\n\n|$)/)
    return head ? text.slice(head[0].length) : text
}

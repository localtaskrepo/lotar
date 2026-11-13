import type { CommentAnswer, TaskComment } from '../api/types'

/** DEV-488: question comments no answer points to, oldest first. */
export function openQuestions(comments: readonly TaskComment[] | null | undefined): TaskComment[] {
    const list = comments ?? []
    return list.filter((comment) => comment.question && !answerTo(list, comment))
}

/** The newest open question: the one an answer without `to` answers. */
export function lastOpenQuestion(comments: readonly TaskComment[] | null | undefined): TaskComment | null {
    const open = openQuestions(comments)
    return open[open.length - 1] ?? null
}

/** The answer comment pointing to `question`, if any. */
export function answerTo(comments: readonly TaskComment[], question: TaskComment): TaskComment | null {
    return comments.find((comment) => comment.answer?.to === question.date) ?? null
}

/**
 * The question without the options the server appends to its text in prose
 * ("\n\nOptions:\n- key: label"), since the UI shows them as buttons.
 */
export function questionPrompt(comment: TaskComment): string {
    const text = comment.text || ''
    const cut = text.lastIndexOf('\n\nOptions:\n')
    return cut >= 0 ? text.slice(0, cut) : text
}

/** The chosen options' labels, for an answer shown under its question. */
export function answerLabels(question: TaskComment, answer: CommentAnswer): string[] {
    const options = question.question?.options ?? []
    return (answer.choice ?? []).map((key) => options.find((option) => option.key === key)?.label ?? key)
}

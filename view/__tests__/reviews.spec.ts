import { describe, expect, it } from 'vitest'
import type { TaskComment } from '../api/types'
import { latestReview, reviewSummary, shortCommit, verdictLabel } from '../utils/reviews'

const review = (date: string, verdict: 'pass' | 'block'): TaskComment => ({
    date,
    text: `Review ${verdict === 'pass' ? 'passed' : 'blocked'} at abc1234def.\n\nOne problem.\n\nFindings:\n- [blocking] src/a.rs:7: data loss`,
    review: {
        verdict,
        commit: 'abc1234def',
        findings: [{ summary: 'data loss', file: 'src/a.rs', line: 7, blocking: true }],
    },
})

describe('review comments (DEV-489)', () => {
    it('finds the newest review', () => {
        const plain: TaskComment = { date: '2026-11-13T12:00:00Z', text: 'later note' }
        const comments = [review('2026-11-13T10:00:00Z', 'block'), review('2026-11-13T11:00:00Z', 'pass'), plain]
        expect(latestReview(comments)?.review?.verdict).toBe('pass')
        expect(latestReview([plain])).toBeNull()
        expect(latestReview(null)).toBeNull()
    })

    it('strips the verdict line and findings the server writes in prose', () => {
        expect(reviewSummary(review('2026-11-13T10:00:00Z', 'block'))).toBe('One problem.')
        expect(reviewSummary({ date: 'd', text: 'Review passed.', review: { verdict: 'pass' } })).toBe('')
        expect(reviewSummary({ date: 'd', text: 'Hand-written text', review: { verdict: 'pass' } })).toBe(
            'Hand-written text',
        )
    })

    it('labels verdicts and shortens commits', () => {
        expect(verdictLabel({ verdict: 'block' })).toBe('Review blocked')
        expect(verdictLabel({ verdict: 'pass' })).toBe('Review passed')
        expect(shortCommit('abc1234def')).toBe('abc1234')
    })
})

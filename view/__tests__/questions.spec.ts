import { describe, expect, it } from 'vitest'
import type { TaskComment } from '../api/types'
import { answerLabels, lastOpenQuestion, openQuestions, questionPrompt } from '../utils/questions'

const question = (date: string): TaskComment => ({
    date,
    text: 'Rework or close?\n\nOptions:\n- rework: Rework it (recommended)\n- close: Close it',
    question: {
        options: [
            { key: 'rework', label: 'Rework it' },
            { key: 'close', label: 'Close it', set: { status: 'Done' } },
        ],
        recommended: 'rework',
    },
})

describe('question comments (DEV-488)', () => {
    it('finds questions no answer points to, newest last', () => {
        const first = question('2026-11-13T10:00:00Z')
        const second = question('2026-11-13T11:00:00Z')
        const answer: TaskComment = {
            date: '2026-11-13T12:00:00Z',
            text: 'Answer: close',
            answer: { to: second.date, choice: ['close'] },
        }
        expect(openQuestions([first, second, answer])).toEqual([first])
        expect(lastOpenQuestion([first, second])).toBe(second)
        expect(lastOpenQuestion([{ date: 'x', text: 'plain' }])).toBeNull()
        expect(answerLabels(second, answer.answer!)).toEqual(['Close it'])
    })

    it('shows the question without the options the server writes in prose', () => {
        expect(questionPrompt(question('d'))).toBe('Rework or close?')
        expect(questionPrompt({ date: 'd', text: 'No options here' })).toBe('No options here')
    })
})

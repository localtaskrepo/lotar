import { mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'

vi.mock('../api/client', () => ({ api: {} }))

import OpenQuestion from '../components/OpenQuestion.vue'

const question = { date: '2026-01-02T00:00:00Z', text: 'Ship it?', question: { options: [{ key: 'y', label: 'Yes' }] } }

describe('OpenQuestion (DEV-692)', () => {
    it("shows a list row's open_question, which has no comments", () => {
        const wrapper = mount(OpenQuestion, { props: { task: { id: 'A-1', open_question: question } } })
        expect(wrapper.text()).toContain('Ship it?')
    })

    it('prefers the comments of a whole task', () => {
        const wrapper = mount(OpenQuestion, {
            props: { task: { id: 'A-1', comments: [], open_question: question } },
        })
        expect(wrapper.text()).not.toContain('Ship it?')
    })
})

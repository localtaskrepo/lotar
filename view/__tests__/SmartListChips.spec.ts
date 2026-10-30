import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import SmartListChips from '../components/SmartListChips.vue'

describe('SmartListChips', () => {
    it('does not render a Review chip even when review-like statuses exist', () => {
        const wrapper = mount(SmartListChips, {
            props: {
                value: {},
                statuses: ['Todo', 'InProgress', 'NeedsReview', 'Done'],
                priorities: [],
            },
        })

        const buttons = wrapper.findAll('button')
        const reviewBtn = buttons.find((btn) => btn.text().trim() === 'Review')
        expect(reviewBtn).toBeUndefined()
    })
})

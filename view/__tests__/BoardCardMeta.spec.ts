import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import BoardCardMeta from '../components/BoardCardMeta.vue'

const sprint = { label: (id: number) => `#${id}`, stateClass: () => '', tooltip: (id: number) => `Sprint #${id}` }

function mountCard(relationships: Record<string, unknown>, fields: Record<string, boolean>) {
    const task = {
        id: 'DEV-1',
        status: 'Todo',
        tags: [],
        sprints: [],
        modified: '2026-10-01T00:00:00Z',
        relationships,
    } as any
    return mount(BoardCardMeta, {
        props: { task, fields, hasHeader: true, dueInfo: { label: '', overdue: false }, modifiedInfo: '', sprint },
    })
}

describe('BoardCardMeta relationships (DEV-418)', () => {
    it('lists related IDs per kind, blocks first, while the field is shown', () => {
        const card = mountCard(
            { depends_on: ['DEV-9'], blocks: ['DEV-2', 'DEV-5'], related: ['other:OPS-4'] },
            { relationships: true },
        )
        const chips = card.findAll('.relation-chip')
        expect(chips.map((chip) => chip.attributes('aria-label'))).toEqual([
            'Blocks DEV-2, DEV-5',
            'Depends on DEV-9',
            'Related other:OPS-4',
        ])
        expect(chips[0]!.find('.relation-chip__label').text()).toBe('Blocks')
        expect(chips[0]!.findAll('.relation-chip__id').map((id) => id.text())).toEqual(['DEV-2', 'DEV-5'])
        expect(chips[0]!.classes()).toContain('relation-chip--blocks')
    })

    it('shortens long lists and names every ID in the tooltip', () => {
        const children = ['DEV-2', 'DEV-3', 'DEV-4', 'DEV-5', 'DEV-6']
        const chip = mountCard({ children }, { relationships: true }).find('.relation-chip')
        expect(chip.findAll('.relation-chip__id').map((id) => id.text())).toEqual(['DEV-2', 'DEV-3', 'DEV-4'])
        expect(chip.find('.relation-chip__more').text()).toBe('+2')
        expect(chip.attributes('title')).toBe(`Children ${children.join(', ')}`)
    })

    it('shows nothing when the field is off or the task has no relationships', () => {
        expect(
            mountCard({ blocks: ['DEV-2'] }, { relationships: false })
                .find('.relation-chip')
                .exists(),
        ).toBe(false)
        expect(mountCard({}, { relationships: true }).find('.task-meta__relations').exists()).toBe(false)
    })
})

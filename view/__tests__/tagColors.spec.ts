import { describe, expect, it } from 'vitest'
import { resolveTagColor, tagChipAttrs, tagColorOf } from '../utils/tagColors'
import { recordTagProperties, tagPropertiesFor } from '../utils/tagPropertiesStore'

describe('tag colours (DEV-20)', () => {
    it('accepts hex and named colours and ignores anything else', () => {
        expect(resolveTagColor('#0EA5E9')).toBe('#0ea5e9')
        expect(resolveTagColor('#abc')).toBe('#abc')
        expect(resolveTagColor(' Teal ')).toBe('#14b8a6')
        expect(resolveTagColor('grey')).toBe(resolveTagColor('gray'))
        for (const bad of ['chartreuse', '#12345', 'rgb(0,0,0)', '', null, 7]) {
            expect(resolveTagColor(bad)).toBeNull()
        }
    })

    it('finds a tag exactly first, then ignoring case', () => {
        const properties = { UI: { color: 'red' }, ui: { color: 'blue' }, Api: { color: 'green' } }
        expect(tagColorOf(properties, 'ui')).toBe('#3b82f6')
        expect(tagColorOf(properties, 'api')).toBe('#22c55e')
        expect(tagColorOf(properties, 'docs')).toBeNull()
        expect(tagColorOf({ ui: { color: 'nope' } }, 'ui')).toBeNull()
        expect(tagColorOf(undefined, 'ui')).toBeNull()
    })

    it('only coloured tags get the tint class and colour variable', () => {
        expect(tagChipAttrs('#ef4444')).toEqual({ class: 'tag--colored', style: { '--tag-color': '#ef4444' } })
        expect(tagChipAttrs(null)).toEqual({})
    })

    it('keeps each project its own tag properties from config responses', () => {
        recordTagProperties('ALPHA', { tag_properties: { ui: { color: 'red' } } })
        recordTagProperties('BETA', { tags: ['*'] })
        expect(tagPropertiesFor('ALPHA')).toEqual({ ui: { color: 'red' } })
        expect(tagPropertiesFor('BETA')).toEqual({})
        expect(tagPropertiesFor('GAMMA')).toBeUndefined()
    })
})

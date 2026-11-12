import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import TaskPanelCustomFieldsSection from '../components/task-panel/TaskPanelCustomFieldsSection.vue'

describe('task panel custom fields', () => {
    it('suggests the versions a version field already holds, in version order', () => {
        const wrapper = mount(TaskPanelCustomFieldsSection, {
            props: {
                customFields: { affects: '', product: '' },
                customFieldKeys: { affects: 'affects', product: 'product' },
                newFieldKey: '',
                newFieldValue: '',
                allowNewFields: false,
                versionFields: ['affects'],
                knownValues: { affects: ['0.15.0', '0.9', 'soon', '0.15'], product: ['web'] },
            },
        })
        const rows = wrapper.findAll('.task-panel__custom-row')
        const versionInput = rows[0]!.findAll('input')[1]!
        const listId = versionInput.attributes('list')
        expect(listId).toBeTruthy()
        const options = wrapper.findAll(`datalist[id="${listId}"] option`).map((option) => option.attributes('value'))
        expect(options).toEqual(['0.9', '0.15', '0.15.0'])
        expect(rows[1]!.findAll('input')[1]!.attributes('list')).toBeUndefined()
    })
})

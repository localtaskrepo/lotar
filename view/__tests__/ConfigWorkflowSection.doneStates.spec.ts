import { vi, describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import { defineComponent, h, ref } from 'vue'
import ConfigWorkflowSection from '../components/ConfigWorkflowSection.vue'

vi.mock('../components/ChipListField.vue', () => ({
    default: {
        props: ['modelValue', 'suggestions', 'placeholder', 'addLabel', 'composerLabel', 'emptyLabel'],
        emits: ['update:modelValue'],
        template: '<div class="chip-stub" :data-count="modelValue.length" @click="$emit(\'update:modelValue\', modelValue.length ? [] : [\'Closed\'])" />',
    },
}))

vi.mock('../components/ConfigGroup.vue', () => ({
    default: { props: ['title', 'description'], template: '<section><slot /></section>' },
}))

function mountSection(initial: string[] | null) {
    const model = ref<string[] | null>(initial)
    const host = defineComponent({
        setup() {
            return () =>
                h(ConfigWorkflowSection, {
                    description: 'Workflow',
                    issueStates: [],
                    issueTypes: [],
                    issuePriorities: [],
                    issueDoneStates: model.value,
                    'onUpdate:issueDoneStates': (value: string[] | null) => {
                        model.value = value
                    },
                    statusSuggestions: ['Todo', 'Doing', 'Done', 'Closed'],
                    typeSuggestions: [],
                    prioritySuggestions: [],
                    issueDoneStatesError: null,
                    issueDoneStatesSource: 'global',
                    automaticLabel: 'Inherit global (Currently Done)',
                    effectiveSummary: 'Effective now: Done (inferred)',
                    effectiveDoneLabels: ['Done'],
                    provenanceLabel: (source: string | undefined) => source ?? '',
                    provenanceClass: () => '',
                })
        },
    })
    const wrapper = mount(host)
    const section = wrapper.findComponent(ConfigWorkflowSection)
    const doneField = () => wrapper.findAll('.field').filter((f) => f.find('select.done-states-mode').exists())[0]
    return {
        wrapper,
        section,
        model,
        modeSelect: () => wrapper.find('select.done-states-mode'),
        chips: () => doneField()?.find('.chip-stub') ?? wrapper.find('.chip-stub-none'),
    }
}

describe('ConfigWorkflowSection done states control', () => {
    it('shows automatic mode for a null model without rendering chips', () => {
        const ctx = mountSection(null)
        expect((ctx.modeSelect().element as HTMLSelectElement).value).toBe('automatic')
        expect(ctx.chips().exists()).toBe(false)
        expect(ctx.wrapper.text()).toContain('Inherit global (Currently Done)')
        expect(ctx.wrapper.text()).toContain('Effective now: Done (inferred)')
        // Provenance chip renders the source next to the label.
        expect(ctx.wrapper.text().toLowerCase()).toContain('global')
    })

    it('shows explicit mode with chips for a non-null model', () => {
        const ctx = mountSection(['Done', 'Closed'])
        expect((ctx.modeSelect().element as HTMLSelectElement).value).toBe('explicit')
        expect(ctx.chips().exists()).toBe(true)
        expect(ctx.chips().attributes('data-count')).toBe('2')
    })

    it('seeds from the effective labels when switching to explicit', async () => {
        const ctx = mountSection(null)
        await ctx.modeSelect().setValue('explicit')
        expect(ctx.model.value).toEqual(['Done'])
        expect((ctx.modeSelect().element as HTMLSelectElement).value).toBe('explicit')
        expect(ctx.chips().exists()).toBe(true)
    })

    it('clearing the last chip resets to automatic/inherit instead of explicit-empty', async () => {
        const ctx = mountSection(['Done'])
        await ctx.chips().trigger('click')
        expect(ctx.model.value).toBeNull()
        expect(ctx.chips().exists()).toBe(false)
        expect(ctx.section.emitted('validate')?.flat()).toContain('issue_done_states')
    })

    it('switching back to automatic nulls the model', async () => {
        const ctx = mountSection(['Done'])
        await ctx.modeSelect().setValue('automatic')
        expect(ctx.model.value).toBeNull()
        expect(ctx.chips().exists()).toBe(false)
    })
})

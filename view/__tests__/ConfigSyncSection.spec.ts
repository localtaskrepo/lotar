import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import type { SyncRemoteConfig } from '../api/types'
import ConfigSyncSection from '../components/ConfigSyncSection.vue'

const stubs = {
    ConfigGroup: {
        template: '<section><slot /></section>',
        props: ['title', 'description'],
    },
    UiButton: {
        template: '<button type="button" @click="$emit(\'click\')"><slot /></button>',
        props: ['disabled', 'variant'],
    },
}

const entries: Array<{ name: string; remote: SyncRemoteConfig }> = [
    {
        name: 'jira-home',
        remote: {
            provider: 'jira',
            project: 'DEMO',
            repo: null,
            filter: null,
            auth_profile: null,
            mapping: {},
        },
    },
]

describe('ConfigSyncSection', () => {
    it('renders empty state when no remotes exist', () => {
        const wrapper = mount(ConfigSyncSection, {
            global: { stubs },
            props: { entries: [] },
        })

        expect(wrapper.text()).toContain('No sync remotes configured')
    })

    it('emits actions for pull, push, and check', async () => {
        const wrapper = mount(ConfigSyncSection, {
            global: { stubs },
            props: { entries },
        })

        const buttons = wrapper.findAll('button')
        const byLabel = (label: string) => buttons.find((btn) => btn.text() === label)

        await byLabel('Pull')?.trigger('click')
        await byLabel('Push')?.trigger('click')
        await byLabel('Check')?.trigger('click')

        expect(wrapper.emitted('pull')).toBeTruthy()
        expect(wrapper.emitted('push')).toBeTruthy()
        expect(wrapper.emitted('check')).toBeTruthy()
    })

    it('renders last check summary', () => {
        const wrapper = mount(ConfigSyncSection, {
            global: { stubs },
            props: {
                entries,
                checkStates: {
                    'jira-home': {
                        checkedAt: '2026-01-16T12:00:00Z',
                        result: {
                            status: 'ok',
                            direction: 'pull',
                            provider: 'jira',
                            remote: 'jira-home',
                            project: 'DEMO',
                            dry_run: true,
                            summary: { created: 1, updated: 2, skipped: 3, failed: 0 },
                            warnings: [],
                            info: [],
                            run_id: 'run-1',
                            report: null,
                            report_entries: [],
                        },
                    },
                },
            },
        })

        expect(wrapper.text()).toContain('Last check')
        expect(wrapper.text()).toContain('1 created')
        expect(wrapper.text()).toContain('2 updated')
    })
})

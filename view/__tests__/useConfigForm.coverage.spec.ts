import { describe, expect, it } from 'vitest'
import { ref } from 'vue'
import type {
    ConfigInspectResult,
    GlobalConfigRaw,
    ProjectConfigRaw,
    ProjectDTO,
    ResolvedConfigDTO,
} from '../api/types'
import { useConfigForm } from '../composables/useConfigForm'

// DEV-146: every settable config field has a bespoke control, and list
// restrictions distinguish inherit / any (*) / only these / none ([]).

function resolved(overrides: Partial<ResolvedConfigDTO> = {}): ResolvedConfigDTO {
    return {
        review_reviewers: [],
        server_port: 8080,
        default_project: 'TEST',
        attachments_dir: '@attachments',
        attachments_max_upload_mb: 10,
        sync_reports_dir: '@reports',
        sync_write_reports: true,
        default_assignee: null,
        default_reporter: null,
        default_tags: [],
        default_priority: 'Medium',
        default_status: 'Todo',
        issue_states: ['Todo', 'Done'],
        issue_types: ['Feature'],
        issue_priorities: ['Low', 'Medium', 'High'],
        issue_done_states: null,
        effective_done_states: ['Done'],
        done_states_mode: 'inferred',
        task_calendar_day: '2026-10-23',
        tags: ['*'],
        custom_fields: ['*'],
        auto_set_reporter: true,
        auto_assign_on_status: true,
        auto_codeowners_assign: true,
        auto_tags_from_path: true,
        auto_branch_infer_type: true,
        auto_branch_infer_status: true,
        auto_branch_infer_priority: true,
        auto_identity: true,
        auto_identity_git: true,
        scan_signal_words: [],
        scan_ticket_patterns: [],
        scan_enable_ticket_words: true,
        scan_enable_mentions: true,
        scan_strip_attributes: true,
        branch_type_aliases: {},
        branch_status_aliases: {},
        branch_priority_aliases: {},
        members: ['alice', 'bob'],
        strict_members: false,
        auto_populate_members: true,
        agent_context_enabled: true,
        agent_context_extension: '.context',
        agent_worktree: { enabled: false, branch_prefix: 'agent/' },
        sprint_defaults: {},
        sprint_notifications: { enabled: true },
        ...overrides,
    }
}

function inspectFor(
    project: string,
    projectRaw: ProjectConfigRaw | null,
    global: Partial<ResolvedConfigDTO> = {},
): ConfigInspectResult {
    const globalResolved = resolved(global)
    return {
        effective: project ? resolved({ ...global, ...(projectRaw as Partial<ResolvedConfigDTO>) }) : globalResolved,
        global_effective: globalResolved,
        global_raw: { web_ui_path: null } as unknown as GlobalConfigRaw,
        auth_profiles: {},
        project_raw: projectRaw,
        has_global_file: true,
        project_exists: !!projectRaw,
        sources: {},
    }
}

function load(project: string, inspect: ConfigInspectResult) {
    const form = useConfigForm({
        project: ref(project),
        projects: ref([{ prefix: 'TEST', name: 'Test' } as ProjectDTO]),
        inspectData: ref(inspect),
    })
    form.populateForm(inspect)
    form.baseline.value = form.snapshotForm()
    return form
}

describe('useConfigForm restricted lists (DEV-146)', () => {
    it('turns custom fields off for one project with an explicit empty list', () => {
        const form = load('TEST', inspectFor('TEST', { project_name: 'Test' }))
        expect(form.form.customFieldsMode).toBe('inherit')
        expect(form.form.tagsMode).toBe('inherit')

        form.form.customFieldsMode = 'none'
        expect(form.buildPayload()).toEqual({ custom_fields: '[]' })
    })

    it('reads an explicit empty project list as none and restores inheritance with a blank value', () => {
        const form = load('TEST', inspectFor('TEST', { project_name: 'Test', custom_fields: [] }))
        expect(form.form.customFieldsMode).toBe('none')

        form.form.customFieldsMode = 'inherit'
        expect(form.buildPayload()).toEqual({ custom_fields: '' })
    })

    it('stores the wildcard for any value and a CSV for listed values', () => {
        const form = load('', inspectFor('', null, { tags: ['frontend'], custom_fields: [] }))
        expect(form.form.tagsMode).toBe('list')
        expect(form.form.tags).toEqual(['frontend'])
        expect(form.form.customFieldsMode).toBe('none')

        form.form.tagsMode = 'any'
        form.form.customFieldsMode = 'list'
        form.form.customFields = ['Category', 'Environment']
        expect(form.buildPayload()).toEqual({ tags: '*', custom_fields: 'Category,Environment' })
    })

    it('requires at least one value in "only these" mode', () => {
        const form = load('', inspectFor('', null))
        form.form.customFieldsMode = 'list'
        form.validateField('custom_fields')
        expect(form.errors.custom_fields).toMatch(/at least one custom field/)
    })
})

describe('useConfigForm newly represented fields (DEV-146)', () => {
    it('sends global members, sync, agent, sprint and web UI settings with their setter names', () => {
        const form = load('', inspectFor('', null))
        expect(form.form.members).toEqual(['alice', 'bob'])
        expect(form.form.agentInstructionsMode).toBe('builtin')

        form.form.members = ['alice', 'bob', 'carol']
        form.form.strictMembers = 'true'
        form.form.syncWriteReports = 'false'
        form.form.agentContextExtension = '.ctx'
        form.form.agentInstructionsMode = 'file'
        form.form.agentInstructionsFile = 'docs/agents.md'
        form.form.agentWorktreeEnabled = 'true'
        form.form.sprintLength = '2w'
        form.form.sprintCapacityPoints = '30'
        form.form.sprintNotificationsEnabled = 'false'
        form.form.webUiPath = '/srv/ui'

        expect(form.buildPayload()).toEqual({
            members: 'alice,bob,carol',
            strict_members: 'true',
            sync_write_reports: 'false',
            agent_context_extension: '.ctx',
            agent_instructions_file: 'docs/agents.md',
            agent_worktree_enabled: 'true',
            sprints_defaults_length: '2w',
            sprints_defaults_capacity_points: '30',
            sprints_notifications_enabled: 'false',
            web_ui_path: '/srv/ui',
        })
    })

    it('edits project overrides only and clears them back to inherit', () => {
        const form = load(
            'TEST',
            inspectFor('TEST', {
                project_name: 'Test',
                members: ['carol'],
                auto_codeowners_assign: false,
                agent_instructions: 'Use the project guide.',
                review_reviewers: ['dana'],
            }),
        )
        expect(form.form.members).toEqual(['carol'])
        expect(form.form.autoCodeownersAssign).toBe('false')
        expect(form.form.autoTagsFromPath).toBe('inherit')
        expect(form.form.agentInstructionsMode).toBe('inline')
        expect(form.form.agentInstructions).toBe('Use the project guide.')
        expect(form.form.reviewReviewers).toEqual(['dana'])
        expect(form.form.agentWorktreeEnabled).toBe('inherit')

        form.form.members = []
        form.form.autoCodeownersAssign = 'inherit'
        form.form.autoTagsFromPath = 'false'
        form.form.agentInstructionsMode = 'inherit'
        form.form.reviewReviewers = ['dana', 'erin']

        expect(form.buildPayload()).toEqual({
            members: '',
            review_reviewers: 'dana,erin',
            auto_codeowners_assign: '',
            auto_tags_from_path: 'false',
            agent_instructions: '',
        })
    })

    it('rejects negative or fractional sprint capacities', () => {
        const form = load('', inspectFor('', null))
        form.form.sprintCapacityHours = '-3'
        form.validateField('sprints_defaults_capacity_hours')
        expect(form.errors.sprints_defaults_capacity_hours).toMatch(/whole number/)
        form.form.sprintCapacityHours = '12'
        form.validateField('sprints_defaults_capacity_hours')
        expect(form.errors.sprints_defaults_capacity_hours).toBeNull()
    })
})

describe('useConfigForm sprint duration validation (DEV-111)', () => {
    it('matches the server grammar for lengths and overdue grace periods', () => {
        const form = load('', inspectFor('', null))
        const check = (field: 'sprints_defaults_length' | 'sprints_defaults_overdue_after', value: string) => {
            if (field === 'sprints_defaults_length') form.form.sprintLength = value
            else form.form.sprintOverdueAfter = value
            form.validateField(field)
            return form.errors[field]
        }
        for (const ok of ['', '2w', '14d', '3 weeks']) expect(check('sprints_defaults_length', ok)).toBeNull()
        for (const bad of ['12h', '0d', '-2w', 'soon', '600w'])
            expect(check('sprints_defaults_length', bad)).toBeTruthy()
        for (const ok of ['', '12h', '0d', '2 hours']) expect(check('sprints_defaults_overdue_after', ok)).toBeNull()
        for (const bad of ['later', '87601h']) expect(check('sprints_defaults_overdue_after', bad)).toBeTruthy()
    })
})

describe('useConfigForm tag colours (DEV-20)', () => {
    it('edits global tag colours and keeps other tag properties', () => {
        const form = load(
            '',
            inspectFor('', null, {
                tag_properties: { ui: { color: 'sky', icon: 'brush' }, api: { color: '#ff0000' } },
            }),
        )
        expect(form.form.tagColors).toEqual([
            { tag: 'api', color: '#ff0000' },
            { tag: 'ui', color: 'sky' },
        ])
        expect(form.buildPayload()).toEqual({})

        form.form.tagColors[1]!.color = '#22c55e'
        form.form.tagColors.splice(0, 1)
        form.addTagColor('docs')
        form.form.tagColors[1]!.color = 'violet'
        expect(JSON.parse(form.buildPayload().tag_properties!)).toEqual({
            docs: { color: 'violet' },
            ui: { color: '#22c55e', icon: 'brush' },
        })
    })

    it('edits only the project overrides and clears them with a blank value', () => {
        const form = load(
            'TEST',
            inspectFor(
                'TEST',
                { project_name: 'Test', tag_properties: { ui: { color: 'green' } } },
                {
                    tag_properties: { ui: { color: 'blue' }, api: { color: 'red' } },
                },
            ),
        )
        expect(form.form.tagColors).toEqual([{ tag: 'ui', color: 'green' }])
        form.removeTagColor(0)
        expect(form.buildPayload()).toEqual({ tag_properties: '' })
    })

    it('rejects unknown colours, nameless rows and duplicates', () => {
        const form = load('', inspectFor('', null))
        form.addTagColor('ui')
        form.form.tagColors[0]!.color = 'chartreuse'
        form.validateField('tag_properties')
        expect(form.errors.tag_properties).toMatch(/not a colour/)
        form.form.tagColors[0]!.color = 'red'
        form.addTagColor('UI')
        form.validateField('tag_properties')
        expect(form.errors.tag_properties).toMatch(/more than one colour/)
        form.form.tagColors[1]!.tag = ''
        form.validateField('tag_properties')
        expect(form.errors.tag_properties).toMatch(/Name the tag/)
    })
})

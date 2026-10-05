import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { ref } from 'vue'
import type { ConfigInspectResult, GlobalConfigRaw, ProjectDTO, ResolvedConfigDTO } from '../api/types'
import { useConfigForm } from '../composables/useConfigForm'

function resolved(overrides: Partial<ResolvedConfigDTO> = {}): ResolvedConfigDTO {
    return {
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
        issue_states: ['Todo', 'Doing', 'Done', 'Closed', 'Shipped'],
        issue_types: ['Feature'],
        issue_priorities: ['Low', 'Medium', 'High'],
        issue_done_states: null,
        effective_done_states: ['Done'],
        done_states_mode: 'inferred',
        task_calendar_day: '2026-10-02',
        tags: [],
        custom_fields: [],
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
        ...overrides,
    }
}

function globalRaw(overrides: Partial<GlobalConfigRaw> = {}): GlobalConfigRaw {
    return {
        server_port: 8080,
        default_project: 'TEST',
        attachments_dir: '@attachments',
        attachments_max_upload_mb: 10,
        sync_reports_dir: '@reports',
        sync_write_reports: true,
        issue_states: ['Todo', 'Doing', 'Done', 'Closed', 'Shipped'],
        issue_types: ['Feature'],
        issue_priorities: ['Low', 'Medium', 'High'],
        issue_done_states: null,
        tags: [],
        default_assignee: null,
        default_reporter: null,
        default_tags: [],
        auto_set_reporter: true,
        auto_assign_on_status: true,
        auto_identity: true,
        auto_identity_git: true,
        auto_codeowners_assign: true,
        auto_tags_from_path: true,
        auto_branch_infer_type: true,
        auto_branch_infer_status: true,
        auto_branch_infer_priority: true,
        default_priority: 'Medium',
        default_status: 'Todo',
        custom_fields: [],
        scan_signal_words: [],
        scan_ticket_patterns: [],
        scan_enable_ticket_words: true,
        scan_enable_mentions: true,
        scan_strip_attributes: true,
        branch_type_aliases: {},
        branch_status_aliases: {},
        branch_priority_aliases: {},
        ...overrides,
    }
}

const projects: ProjectDTO[] = [
    { prefix: 'TEST', name: 'Test project' } as ProjectDTO,
]

function makeForm(project: string, inspect: ConfigInspectResult) {
    return useConfigForm({
        project: ref(project),
        projects: ref(projects),
        inspectData: ref(inspect),
    })
}

describe('useConfigForm done states (DEV-21)', () => {
    let form: ReturnType<typeof useConfigForm>

    function snapshot() {
        form.baseline.value = form.snapshotForm()
    }

    beforeEach(() => {
        // Keep tests deterministic regardless of the local day.
    })

    afterEach(() => {
        form?.resetForm()
    })

    it('populates the global scope from global_raw and keeps automatic as null', () => {
        const inspect: ConfigInspectResult = {
            effective: resolved(),
            global_effective: resolved(),
            global_raw: globalRaw(),
            auth_profiles: {},
            project_raw: null,
            has_global_file: true,
            project_exists: false,
            sources: { issue_done_states: 'built_in' },
        }
        form = makeForm('', inspect)
        form.populateForm(inspect)
        snapshot()

        expect(form.form.issueDoneStates).toBeNull()
        expect(form.effectiveDoneLabels.value).toEqual(['Done'])
        expect(form.doneStatesMode.value).toBe('inferred')
        expect(form.doneStatesAutomaticLabel.value).toContain('Automatic')
        expect(form.doneStatesAutomaticLabel.value).toContain('Done')
        expect(form.doneStatesEffectiveSummary.value).toContain('Effective now: Done')
        expect(form.doneStatesEffectiveSummary.value).toContain('inferred')
    })

    it('populates an explicit global list from global_raw', () => {
        const inspect: ConfigInspectResult = {
            effective: resolved({
                issue_done_states: ['Done', 'Closed'],
                effective_done_states: ['Done', 'Closed'],
                done_states_mode: 'explicit',
            }),
            global_effective: resolved(),
            global_raw: globalRaw({ issue_done_states: ['Done', 'Closed'] }),
            auth_profiles: {},
            project_raw: null,
            has_global_file: true,
            project_exists: false,
            sources: { issue_done_states: 'global' },
        }
        form = makeForm('', inspect)
        form.populateForm(inspect)
        snapshot()

        expect(form.form.issueDoneStates).toEqual(['Done', 'Closed'])
        expect(form.buildPayload()).toEqual({})
    })

    it('inherits (null) when the project has no override', () => {
        const inspect: ConfigInspectResult = {
            effective: resolved(),
            global_effective: resolved(),
            global_raw: globalRaw(),
            auth_profiles: {},
            project_raw: { project_name: 'Test project' },
            has_global_file: true,
            project_exists: true,
            sources: { issue_done_states: 'global' },
        }
        form = makeForm('TEST', inspect)
        form.populateForm(inspect)
        snapshot()

        expect(form.form.issueDoneStates).toBeNull()
        expect(form.doneStatesAutomaticLabel.value).toContain('Inherit global')
    })

    it('preserves an explicit project list even when it equals the global list', () => {
        const inspect: ConfigInspectResult = {
            effective: resolved({
                issue_done_states: ['Done'],
                effective_done_states: ['Done'],
                done_states_mode: 'explicit',
            }),
            global_effective: resolved({ effective_done_states: ['Done'] }),
            global_raw: globalRaw({ issue_done_states: ['Done'] }),
            auth_profiles: {},
            project_raw: { project_name: 'Test project', issue_done_states: ['Done'] },
            has_global_file: true,
            project_exists: true,
            sources: { issue_done_states: 'project' },
        }
        form = makeForm('TEST', inspect)
        form.populateForm(inspect)
        snapshot()

        // Explicit-over-override intent is kept, not erased to inherit.
        expect(form.form.issueDoneStates).toEqual(['Done'])
        expect(form.buildPayload()).toEqual({})
    })

    describe('validation', () => {
        function projectForm() {
            const inspect: ConfigInspectResult = {
                effective: resolved(),
                global_effective: resolved(),
                global_raw: globalRaw(),
                auth_profiles: {},
                project_raw: { project_name: 'Test project' },
                has_global_file: true,
                project_exists: true,
                sources: {},
            }
            const f = makeForm('TEST', inspect)
            f.populateForm(inspect)
            f.baseline.value = f.snapshotForm()
            return f
        }

        it('rejects an explicit empty list (one-or-more rule)', () => {
            form = projectForm()
            form.form.issueDoneStates = []
            form.validateField('issue_done_states')
            expect(form.errors.issue_done_states).toMatch(/at least one/)
            expect(form.validateAll()).toBe(false)
        })

        it('rejects duplicates and statuses outside the configured workflow', () => {
            form = projectForm()
            form.form.issueDoneStates = ['Done', 'done']
            form.validateField('issue_done_states')
            expect(form.errors.issue_done_states).toMatch(/more than once/)

            form.form.issueDoneStates = ['Done', 'Bogus']
            form.validateField('issue_done_states')
            expect(form.errors.issue_done_states).toMatch(/not a configured status/)

            form.form.issueDoneStates = null
            form.validateField('issue_done_states')
            expect(form.errors.issue_done_states).toBeNull()
        })

        it('accepts a valid subset in any casing', () => {
            form = projectForm()
            form.form.issueDoneStates = ['closed', 'Shipped']
            form.validateField('issue_done_states')
            expect(form.errors.issue_done_states).toBeNull()
        })
    })

    describe('payload diffing', () => {
        function projectForm() {
            const inspect: ConfigInspectResult = {
                effective: resolved(),
                global_effective: resolved(),
                global_raw: globalRaw(),
                auth_profiles: {},
                project_raw: { project_name: 'Test project' },
                has_global_file: true,
                project_exists: true,
                sources: {},
            }
            const f = makeForm('TEST', inspect)
            f.populateForm(inspect)
            f.baseline.value = f.snapshotForm()
            return f
        }

        it('sends a CSV when going automatic -> explicit', () => {
            form = projectForm()
            form.form.issueDoneStates = ['Done', 'Closed']
            expect(form.buildPayload().issue_done_states).toBe('Done,Closed')
        })

        it('sends an empty CSV when clearing an explicit override back to inherit/automatic', () => {
            form = projectForm()
            form.form.issueDoneStates = ['Done']
            form.baseline.value = form.snapshotForm()
            form.form.issueDoneStates = null
            expect(form.buildPayload().issue_done_states).toBe('')
        })

        it('omits the key entirely for untouched values', () => {
            form = projectForm()
            form.form.defaultStatus = 'Doing'
            expect(form.buildPayload().issue_done_states).toBeUndefined()
        })
    })
})

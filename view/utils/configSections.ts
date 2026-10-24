import type { ConfigFormState } from '../composables/useConfigForm'

/** Sections of the Config page, in menu order (DEV-19). */
export type ConfigSectionId =
    'general' | 'defaults' | 'workflow' | 'fields' | 'people' | 'automation' | 'git' | 'storage' | 'agents' | 'sprints'

export interface ConfigSectionDefinition {
    id: ConfigSectionId
    label: string
    summary: string
    /** Only shown in global scope (no project overrides exist). */
    globalOnly?: boolean
    /** Form fields edited in this section; drives the unsaved-changes marker. */
    fields: ReadonlyArray<keyof ConfigFormState>
    /** Validation error keys shown in this section; drives the error marker. */
    errorKeys: readonly string[]
}

export const CONFIG_SECTIONS: readonly ConfigSectionDefinition[] = [
    {
        id: 'general',
        label: 'General',
        summary: 'Server settings and the project name.',
        fields: ['serverPort', 'webUiPath', 'projectName'],
        errorKeys: ['server_port', 'project_name'],
    },
    {
        id: 'defaults',
        label: 'New tasks',
        summary: 'Values applied when a task is created without them.',
        fields: [
            'defaultProject',
            'defaultStatus',
            'defaultPriority',
            'defaultReporter',
            'defaultAssignee',
            'defaultTags',
        ],
        errorKeys: [
            'default_project',
            'default_status',
            'default_priority',
            'default_reporter',
            'default_assignee',
            'default_tags',
        ],
    },
    {
        id: 'workflow',
        label: 'Workflow',
        summary: 'Statuses, types, priorities and which statuses count as done.',
        fields: ['issueStates', 'issueTypes', 'issuePriorities', 'issueDoneStates'],
        errorKeys: ['issue_states', 'issue_types', 'issue_priorities', 'issue_done_states'],
    },
    {
        id: 'fields',
        label: 'Tags & fields',
        summary: 'Which tags and custom fields tasks may use.',
        fields: ['tagsMode', 'tags', 'customFieldsMode', 'customFields'],
        errorKeys: ['tags', 'custom_fields'],
    },
    {
        id: 'people',
        label: 'People',
        summary: 'Members, membership rules and reviewers.',
        fields: ['members', 'strictMembers', 'autoPopulateMembers', 'reviewReviewers'],
        errorKeys: ['members', 'review_reviewers'],
    },
    {
        id: 'automation',
        label: 'Automation',
        summary: 'Automatic behaviors and automation rules.',
        fields: [
            'autoSetReporter',
            'autoAssignOnStatus',
            'autoCodeownersAssign',
            'autoTagsFromPath',
            'autoBranchInferType',
            'autoBranchInferStatus',
            'autoBranchInferPriority',
            'autoIdentity',
            'autoIdentityGit',
        ],
        errorKeys: [],
    },
    {
        id: 'git',
        label: 'Git & scanning',
        summary: 'Source scanning and branch name aliases.',
        fields: [
            'scanSignalWords',
            'scanTicketPatterns',
            'scanEnableTicketWords',
            'scanEnableMentions',
            'scanStripAttributes',
            'branchTypeAliases',
            'branchStatusAliases',
            'branchPriorityAliases',
        ],
        errorKeys: [
            'scan_signal_words',
            'scan_ticket_patterns',
            'branch_type_aliases',
            'branch_status_aliases',
            'branch_priority_aliases',
        ],
    },
    {
        id: 'storage',
        label: 'Files & sync',
        summary: 'Attachment storage and sync reports.',
        fields: ['attachmentsDir', 'attachmentsMaxUploadMb', 'syncReportsDir', 'syncWriteReports'],
        errorKeys: ['attachments_dir', 'attachments_max_upload_mb', 'sync_reports_dir'],
    },
    {
        id: 'agents',
        label: 'Agents',
        summary: 'Agent job context, instructions and worktrees.',
        fields: [
            'agentContextEnabled',
            'agentContextExtension',
            'agentLogsDir',
            'agentInstructionsMode',
            'agentInstructions',
            'agentInstructionsFile',
            'agentWorktreeEnabled',
            'agentWorktreeDir',
            'agentWorktreeBranchPrefix',
        ],
        errorKeys: ['agent_context_extension', 'agent_instructions', 'agent_worktree_branch_prefix'],
    },
    {
        id: 'sprints',
        label: 'Sprints',
        summary: 'Defaults for new sprints.',
        globalOnly: true,
        fields: [
            'sprintLength',
            'sprintOverdueAfter',
            'sprintCapacityPoints',
            'sprintCapacityHours',
            'sprintNotificationsEnabled',
        ],
        errorKeys: [
            'sprints_defaults_length',
            'sprints_defaults_overdue_after',
            'sprints_defaults_capacity_points',
            'sprints_defaults_capacity_hours',
        ],
    },
]

export function sectionsForScope(isGlobal: boolean): ConfigSectionDefinition[] {
    return CONFIG_SECTIONS.filter((section) => isGlobal || !section.globalOnly)
}

/** The requested section when it exists in this scope, else the first one. */
export function resolveSection(requested: unknown, isGlobal: boolean): ConfigSectionId {
    const available = sectionsForScope(isGlobal)
    const match = available.find((section) => section.id === requested)
    return (match ?? available[0]!).id
}

/** Sections whose fields differ from the loaded baseline. */
export function dirtySections(current: ConfigFormState, baseline: ConfigFormState | null): Set<ConfigSectionId> {
    const dirty = new Set<ConfigSectionId>()
    if (!baseline) return dirty
    for (const section of CONFIG_SECTIONS) {
        if (section.fields.some((field) => JSON.stringify(current[field]) !== JSON.stringify(baseline[field]))) {
            dirty.add(section.id)
        }
    }
    return dirty
}

/** Sections that currently show a validation error. */
export function errorSections(errors: Record<string, string | null | undefined>): Set<ConfigSectionId> {
    const failing = new Set<ConfigSectionId>()
    for (const section of CONFIG_SECTIONS) {
        if (section.errorKeys.some((key) => errors[key])) {
            failing.add(section.id)
        }
    }
    return failing
}

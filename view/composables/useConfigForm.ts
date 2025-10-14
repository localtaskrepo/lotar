import { computed, reactive, ref, type ComputedRef, type Ref } from 'vue'
import type { ConfigInspectResult, ConfigSource, ProjectDTO } from '../api/types'

export type ToggleValue = 'inherit' | 'true' | 'false'

export interface BranchAliasEntry {
    key: string
    value: string
}

export interface ConfigFormState {
    serverPort: string
    defaultPrefix: string
    projectName: string
    defaultReporter: string
    defaultAssignee: string
    defaultTags: string[]
    defaultPriority: string
    defaultStatus: string
    issueStates: string[]
    issueTypes: string[]
    issuePriorities: string[]
    tags: string[]
    customFields: string[]
    autoSetReporter: ToggleValue
    autoAssignOnStatus: ToggleValue
    autoCodeownersAssign: ToggleValue
    autoTagsFromPath: ToggleValue
    autoBranchInferType: ToggleValue
    autoBranchInferStatus: ToggleValue
    autoBranchInferPriority: ToggleValue
    autoIdentity: ToggleValue
    autoIdentityGit: ToggleValue
    scanSignalWords: string[]
    scanTicketPatterns: string[]
    scanEnableTicketWords: ToggleValue
    scanEnableMentions: ToggleValue
    scanStripAttributes: ToggleValue
    branchTypeAliases: BranchAliasEntry[]
    branchStatusAliases: BranchAliasEntry[]
    branchPriorityAliases: BranchAliasEntry[]
}

export type ToggleField =
    | 'autoSetReporter'
    | 'autoAssignOnStatus'
    | 'autoCodeownersAssign'
    | 'autoTagsFromPath'
    | 'autoBranchInferType'
    | 'autoBranchInferStatus'
    | 'autoBranchInferPriority'
    | 'autoIdentity'
    | 'autoIdentityGit'
    | 'scanEnableTicketWords'
    | 'scanEnableMentions'
    | 'scanStripAttributes'

type AliasField = 'branchTypeAliases' | 'branchStatusAliases' | 'branchPriorityAliases'

type AliasFieldKeyMap = Record<AliasField, 'branch_type_aliases' | 'branch_status_aliases' | 'branch_priority_aliases'>

type UseConfigFormOptions = {
    project: Ref<string>
    projects: Ref<ProjectDTO[]>
    inspectData: Ref<ConfigInspectResult | null>
    saving?: Ref<boolean>
}

type ToggleOption = {
    value: ToggleValue
    label: string
}

type UseConfigFormReturn = {
    form: ConfigFormState
    baseline: Ref<ConfigFormState | null>
    errors: Record<string, string | null>
    isGlobal: ComputedRef<boolean>
    currentProject: ComputedRef<ProjectDTO | null>
    tagWildcard: ComputedRef<boolean>
    customFieldWildcard: ComputedRef<boolean>
    projectExists: ComputedRef<boolean>
    tagSuggestions: ComputedRef<string[]>
    statusOptions: ComputedRef<string[]>
    priorityOptions: ComputedRef<string[]>
    typeOptions: ComputedRef<string[]>
    statusSuggestions: ComputedRef<string[]>
    prioritySuggestions: ComputedRef<string[]>
    typeSuggestions: ComputedRef<string[]>
    peopleDescription: ComputedRef<string>
    workflowDescription: ComputedRef<string>
    taxonomyDescription: ComputedRef<string>
    projectOverviewDescription: ComputedRef<string>
    automationDescription: ComputedRef<string>
    scanningDescription: ComputedRef<string>
    branchAliasDescription: ComputedRef<string>
    isDirty: ComputedRef<boolean>
    hasErrors: ComputedRef<boolean>
    saveDisabled: ComputedRef<boolean>
    toggleSelectOptions: (field: ToggleField) => ToggleOption[]
    globalToggleSummary: (field: ToggleField) => string
    provenanceLabel: (source: ConfigSource | undefined) => string
    provenanceClass: (source: ConfigSource | undefined) => string
    sourceFor: (field: string) => ConfigSource | undefined
    addAliasEntry: (field: AliasField) => void
    removeAliasEntry: (field: AliasField, index: number) => void
    clearAliasField: (field: AliasField) => void
    validateField: (field: string) => void
    validateAll: () => boolean
    snapshotForm: () => ConfigFormState
    applySnapshot: (snapshot: ConfigFormState) => void
    clearErrors: () => void
    populateForm: (data: ConfigInspectResult) => void
    resetForm: () => void
    buildPayload: () => Record<string, string>
}

export function useConfigForm({ project, projects, inspectData, saving }: UseConfigFormOptions): UseConfigFormReturn {
    const savingRef = saving ?? ref(false)

    const form = reactive<ConfigFormState>({
        serverPort: '',
        defaultPrefix: '',
        projectName: '',
        defaultReporter: '',
        defaultAssignee: '',
        defaultTags: [],
        defaultPriority: '',
        defaultStatus: '',
        issueStates: [],
        issueTypes: [],
        issuePriorities: [],
        tags: [],
        customFields: [],
        autoSetReporter: 'inherit',
        autoAssignOnStatus: 'inherit',
        autoCodeownersAssign: 'true',
        autoTagsFromPath: 'true',
        autoBranchInferType: 'true',
        autoBranchInferStatus: 'true',
        autoBranchInferPriority: 'true',
        autoIdentity: 'true',
        autoIdentityGit: 'true',
        scanSignalWords: [],
        scanTicketPatterns: [],
        scanEnableTicketWords: 'inherit',
        scanEnableMentions: 'inherit',
        scanStripAttributes: 'inherit',
        branchTypeAliases: [],
        branchStatusAliases: [],
        branchPriorityAliases: [],
    })

    const baseline = ref<ConfigFormState | null>(null)
    const errors = reactive<Record<string, string | null>>({})

    const isGlobal = computed(() => project.value === '')
    const currentProject = computed(() => projects.value.find((p) => p.prefix === project.value) || null)

    const tagWildcard = computed(() => inspectData.value?.effective.tags.includes('*') ?? false)
    const customFieldWildcard = computed(() => inspectData.value?.effective.custom_fields.includes('*') ?? false)
    const projectExists = computed(() => inspectData.value?.project_exists ?? false)

    const tagSuggestions = computed(() =>
        Array.from(new Set([...(inspectData.value?.effective.tags ?? []), ...(inspectData.value?.effective.default_tags ?? [])]))
            .filter((value) => value && value !== '*')
    )

    function dedupeCaseInsensitive(values: string[]): string[] {
        const seen = new Set<string>()
        const result: string[] = []
        for (const raw of values) {
            if (typeof raw !== 'string') continue
            const trimmed = raw.trim()
            if (!trimmed || trimmed === '*') continue
            const key = trimmed.toLowerCase()
            if (seen.has(key)) continue
            seen.add(key)
            result.push(trimmed)
        }
        return result
    }

    function findDuplicateValue(values: string[]): string | null {
        const seen = new Set<string>()
        for (const raw of values) {
            if (typeof raw !== 'string') continue
            const trimmed = raw.trim()
            if (!trimmed) continue
            const key = trimmed.toLowerCase()
            if (seen.has(key)) {
                return trimmed
            }
            seen.add(key)
        }
        return null
    }

    function toToggle(value: boolean): ToggleValue {
        return value ? 'true' : 'false'
    }

    function toTriToggle(value: boolean | undefined | null): ToggleValue {
        if (value === undefined || value === null) return 'inherit'
        return value ? 'true' : 'false'
    }

    function mapToAliasEntries(map: Record<string, string> | null | undefined): BranchAliasEntry[] {
        if (!map) return []
        return Object.entries(map)
            .map(([key, value]) => ({ key, value }))
            .sort((a, b) => a.key.localeCompare(b.key))
    }

    function normalizedAlias(entries: BranchAliasEntry[]): Record<string, string> {
        const normalized: Record<string, string> = {}
        for (const entry of entries) {
            const key = entry.key?.trim()
            const value = entry.value?.trim()
            if (!key || !value) continue
            normalized[key.toLowerCase()] = value
        }
        return normalized
    }

    function aliasEntriesEqual(a: BranchAliasEntry[], b: BranchAliasEntry[]): boolean {
        const left = normalizedAlias(a)
        const right = normalizedAlias(b)
        const leftKeys = Object.keys(left).sort()
        const rightKeys = Object.keys(right).sort()
        if (leftKeys.length !== rightKeys.length) return false
        for (let i = 0; i < leftKeys.length; i += 1) {
            if (leftKeys[i] !== rightKeys[i]) return false
            if ((left[leftKeys[i]] ?? '') !== (right[rightKeys[i]] ?? '')) {
                return false
            }
        }
        return true
    }

    function validateAliasEntries(entries: BranchAliasEntry[], label: string): string | null {
        const seen = new Set<string>()
        for (const entry of entries) {
            const key = entry.key?.trim() ?? ''
            const value = entry.value?.trim() ?? ''
            if (!key && !value) {
                continue
            }
            if (!key) {
                return `Add an alias token for the ${label}.`
            }
            if (!value) {
                return `Provide a target value for alias "${key}".`
            }
            if (key.length > 50) {
                return `Alias token "${key}" is too long (max 50 characters).`
            }
            if (value.length > 100) {
                return `Alias value "${value}" is too long (max 100 characters).`
            }
            const lowered = key.toLowerCase()
            if (seen.has(lowered)) {
                return `Alias token "${key}" is duplicated.`
            }
            seen.add(lowered)
        }
        return null
    }

    const globalWorkflow = computed(() => inspectData.value?.global_effective)

    const statusOptions = computed(() => {
        const own = form.issueStates
        if (isGlobal.value || own.length > 0) {
            return dedupeCaseInsensitive(own)
        }
        return dedupeCaseInsensitive(globalWorkflow.value?.issue_states ?? [])
    })

    const priorityOptions = computed(() => {
        const own = form.issuePriorities
        if (isGlobal.value || own.length > 0) {
            return dedupeCaseInsensitive(own)
        }
        return dedupeCaseInsensitive(globalWorkflow.value?.issue_priorities ?? [])
    })

    const typeOptions = computed(() => {
        const own = form.issueTypes
        if (isGlobal.value || own.length > 0) {
            return dedupeCaseInsensitive(own)
        }
        return dedupeCaseInsensitive(globalWorkflow.value?.issue_types ?? [])
    })

    const statusSuggestions = computed(() => statusOptions.value)
    const prioritySuggestions = computed(() => priorityOptions.value)
    const typeSuggestions = computed(() => typeOptions.value)

    const peopleDescription = computed(() =>
        isGlobal.value
            ? 'Used when tasks omit reporter/assignee information. @me resolves to the current identity.'
            : 'Overrides apply only to this project. Leave blank to inherit global fallbacks.',
    )

    const workflowDescription = computed(() =>
        isGlobal.value
            ? 'Statuses map to board columns, while types and priorities power creation menus and validation.'
            : 'Override only the pieces you need. Empty lists fall back to the global workflow.',
    )

    const taxonomyDescription = computed(() =>
        isGlobal.value
            ? 'Control shared taxonomy defaults such as tags plus any custom fields you rely on across projects.'
            : 'Trim the list to limit this project or leave it empty to inherit global taxonomy and custom fields.',
    )

    const projectOverviewDescription = computed(() =>
        projectExists.value
            ? 'Project overrides stored on disk. Rename the project or adjust default workflow values here.'
            : 'No project config file yet. Enter values to create one; otherwise the project inherits everything.',
    )

    const automationDescription = computed(() =>
        isGlobal.value
            ? 'Toggle automatic behaviors for all projects. Changes take effect immediately for new commands.'
            : 'Override automation only when this project needs different defaults. Choose inherit to reuse the global setting.',
    )

    const scanningDescription = computed(() =>
        isGlobal.value
            ? 'Tune repository scanning: control signal words, ticket detection, and attribute scrubbing.'
            : 'Project overrides apply when scanning this project’s files. Leave values empty or set to inherit to use global behavior.',
    )

    const branchAliasDescription = computed(() =>
        isGlobal.value
            ? 'Map branch name tokens to workflow metadata so new tasks pick sensible defaults.'
            : 'Project-specific aliases replace global mappings. Clear the list to fall back to global behavior.',
    )

    function provenanceLabel(source: ConfigSource | undefined): string {
        if (!source) return ''
        switch (source) {
            case 'project':
                return 'Project override'
            case 'global':
                return 'Global default'
            case 'built_in':
                return 'Built-in'
            default:
                return ''
        }
    }

    function provenanceClass(source: ConfigSource | undefined): string {
        return source ? `source-${source}` : ''
    }

    function sourceFor(field: string): ConfigSource | undefined {
        return inspectData.value?.sources?.[field]
    }

    function snapshotForm(): ConfigFormState {
        return {
            serverPort: form.serverPort,
            defaultPrefix: form.defaultPrefix,
            projectName: form.projectName,
            defaultReporter: form.defaultReporter,
            defaultAssignee: form.defaultAssignee,
            defaultTags: [...form.defaultTags],
            defaultPriority: form.defaultPriority,
            defaultStatus: form.defaultStatus,
            issueStates: [...form.issueStates],
            issueTypes: [...form.issueTypes],
            issuePriorities: [...form.issuePriorities],
            tags: [...form.tags],
            customFields: [...form.customFields],
            autoSetReporter: form.autoSetReporter,
            autoAssignOnStatus: form.autoAssignOnStatus,
            autoCodeownersAssign: form.autoCodeownersAssign,
            autoTagsFromPath: form.autoTagsFromPath,
            autoBranchInferType: form.autoBranchInferType,
            autoBranchInferStatus: form.autoBranchInferStatus,
            autoBranchInferPriority: form.autoBranchInferPriority,
            autoIdentity: form.autoIdentity,
            autoIdentityGit: form.autoIdentityGit,
            scanSignalWords: [...form.scanSignalWords],
            scanTicketPatterns: [...form.scanTicketPatterns],
            scanEnableTicketWords: form.scanEnableTicketWords,
            scanEnableMentions: form.scanEnableMentions,
            scanStripAttributes: form.scanStripAttributes,
            branchTypeAliases: form.branchTypeAliases.map((entry) => ({ ...entry })),
            branchStatusAliases: form.branchStatusAliases.map((entry) => ({ ...entry })),
            branchPriorityAliases: form.branchPriorityAliases.map((entry) => ({ ...entry })),
        }
    }

    function applySnapshot(snapshot: ConfigFormState) {
        form.serverPort = snapshot.serverPort
        form.defaultPrefix = snapshot.defaultPrefix
        form.projectName = snapshot.projectName
        form.defaultReporter = snapshot.defaultReporter
        form.defaultAssignee = snapshot.defaultAssignee
        form.defaultTags = [...snapshot.defaultTags]
        form.defaultPriority = snapshot.defaultPriority
        form.defaultStatus = snapshot.defaultStatus
        form.issueStates = [...snapshot.issueStates]
        form.issueTypes = [...snapshot.issueTypes]
        form.issuePriorities = [...snapshot.issuePriorities]
        form.tags = [...snapshot.tags]
        form.customFields = [...snapshot.customFields]
        form.autoSetReporter = snapshot.autoSetReporter
        form.autoAssignOnStatus = snapshot.autoAssignOnStatus
        form.autoCodeownersAssign = snapshot.autoCodeownersAssign
        form.autoTagsFromPath = snapshot.autoTagsFromPath
        form.autoBranchInferType = snapshot.autoBranchInferType
        form.autoBranchInferStatus = snapshot.autoBranchInferStatus
        form.autoBranchInferPriority = snapshot.autoBranchInferPriority
        form.autoIdentity = snapshot.autoIdentity
        form.autoIdentityGit = snapshot.autoIdentityGit
        form.scanSignalWords = [...snapshot.scanSignalWords]
        form.scanTicketPatterns = [...snapshot.scanTicketPatterns]
        form.scanEnableTicketWords = snapshot.scanEnableTicketWords
        form.scanEnableMentions = snapshot.scanEnableMentions
        form.scanStripAttributes = snapshot.scanStripAttributes
        form.branchTypeAliases = snapshot.branchTypeAliases.map((entry) => ({ ...entry }))
        form.branchStatusAliases = snapshot.branchStatusAliases.map((entry) => ({ ...entry }))
        form.branchPriorityAliases = snapshot.branchPriorityAliases.map((entry) => ({ ...entry }))
    }

    function deepEqual(a: ConfigFormState, b: ConfigFormState): boolean {
        return JSON.stringify(a) === JSON.stringify(b)
    }

    const isDirty = computed(() => {
        if (!baseline.value) return false
        return !deepEqual(snapshotForm(), baseline.value)
    })

    const hasErrors = computed(() => Object.values(errors).some(Boolean))
    const saveDisabled = computed(() => savingRef.value || !isDirty.value || hasErrors.value)

    function clearErrors() {
        for (const key of Object.keys(errors)) {
            errors[key] = null
        }
    }

    function normalizeCsv(values: string[]): string {
        return values
            .map((v) => v.trim())
            .filter((v) => v.length > 0)
            .join(',')
    }

    function arraysEqual(a: string[], b: string[]): boolean {
        if (a.length !== b.length) return false
        return normalizeCsv(a) === normalizeCsv(b)
    }

    const aliasFieldKey: AliasFieldKeyMap = {
        branchTypeAliases: 'branch_type_aliases',
        branchStatusAliases: 'branch_status_aliases',
        branchPriorityAliases: 'branch_priority_aliases',
    }

    function addAliasEntry(field: AliasField) {
        form[field].push({ key: '', value: '' })
    }

    function removeAliasEntry(field: AliasField, index: number) {
        form[field].splice(index, 1)
        validateField(aliasFieldKey[field])
    }

    function clearAliasField(field: AliasField) {
        form[field] = []
        validateField(aliasFieldKey[field])
    }

    function globalToggleValue(field: ToggleField): boolean | null {
        const global = inspectData.value?.global_effective
        if (!global) return null
        switch (field) {
            case 'autoSetReporter':
                return global.auto_set_reporter
            case 'autoAssignOnStatus':
                return global.auto_assign_on_status
            case 'autoCodeownersAssign':
                return global.auto_codeowners_assign
            case 'autoTagsFromPath':
                return global.auto_tags_from_path
            case 'autoBranchInferType':
                return global.auto_branch_infer_type
            case 'autoBranchInferStatus':
                return global.auto_branch_infer_status
            case 'autoBranchInferPriority':
                return global.auto_branch_infer_priority
            case 'autoIdentity':
                return global.auto_identity
            case 'autoIdentityGit':
                return global.auto_identity_git
            case 'scanEnableTicketWords':
                return global.scan_enable_ticket_words
            case 'scanEnableMentions':
                return global.scan_enable_mentions
            case 'scanStripAttributes':
                return global.scan_strip_attributes
            default:
                return null
        }
    }

    function inheritToggleLabel(field: ToggleField, base: string): string {
        const value = globalToggleValue(field)
        if (value === null) return base
        return `${base} (Currently ${value ? 'On' : 'Off'})`
    }

    function toggleSelectOptions(field: ToggleField): ToggleOption[] {
        return [
            { value: 'inherit', label: inheritToggleLabel(field, 'Inherit global') },
            { value: 'true', label: 'Enabled' },
            { value: 'false', label: 'Disabled' },
        ]
    }

    function globalToggleSummary(field: ToggleField): string {
        const value = globalToggleValue(field)
        if (value === null) return ''
        return value ? 'On' : 'Off'
    }

    function validateField(field: string) {
        switch (field) {
            case 'server_port': {
                if (!isGlobal.value) {
                    errors.server_port = null
                    return
                }
                const value = form.serverPort.trim()
                if (!value) {
                    errors.server_port = 'Server port is required.'
                    return
                }
                const port = Number(value)
                if (!Number.isInteger(port) || port < 1024 || port > 65535) {
                    errors.server_port = 'Enter a port between 1024 and 65535.'
                } else {
                    errors.server_port = null
                }
                break
            }
            case 'default_prefix': {
                const value = form.defaultPrefix.trim()
                if (!value) {
                    errors.default_prefix = null
                    return
                }
                if (!/^[A-Za-z0-9_-]+$/.test(value)) {
                    errors.default_prefix = 'Only letters, numbers, hyphen, or underscore allowed.'
                } else if (value.length > 20) {
                    errors.default_prefix = 'Keep prefixes under 20 characters.'
                } else {
                    errors.default_prefix = null
                }
                break
            }
            case 'project_name': {
                if (isGlobal.value) {
                    errors.project_name = null
                    return
                }
                const value = form.projectName.trim()
                if (!value) {
                    errors.project_name = 'Project name cannot be empty.'
                } else if (value.length > 100) {
                    errors.project_name = 'Project name must be under 100 characters.'
                } else {
                    errors.project_name = null
                }
                break
            }
            case 'default_reporter':
            case 'default_assignee': {
                const value = field === 'default_reporter' ? form.defaultReporter.trim() : form.defaultAssignee.trim()
                errors[field] = value.length > 100 ? 'Keep names under 100 characters.' : null
                break
            }
            case 'default_tags': {
                const values = form.defaultTags
                const invalid = values.find((value) => value.trim().length > 50)
                errors.default_tags = invalid ? `"${invalid}" is too long (max 50 characters).` : null
                break
            }
            case 'tags': {
                const values = form.tags
                const invalid = values.find((value) => value.trim().length > 50)
                errors.tags = invalid ? `"${invalid}" is too long (max 50 characters).` : null
                break
            }
            case 'custom_fields': {
                const values = form.customFields
                const invalid = values.find((value) => value.trim().length > 50)
                errors.custom_fields = invalid ? `"${invalid}" is too long (max 50 characters).` : null
                break
            }
            case 'scan_signal_words': {
                const values = form.scanSignalWords.map((value) => value.trim()).filter(Boolean)
                const invalid = values.find((value) => value.length > 50)
                if (invalid) {
                    errors.scan_signal_words = `"${invalid}" is too long (max 50 characters).`
                    break
                }
                const duplicate = findDuplicateValue(values)
                errors.scan_signal_words = duplicate ? `"${duplicate}" appears more than once.` : null
                break
            }
            case 'scan_ticket_patterns': {
                const values = form.scanTicketPatterns.map((value) => value.trim()).filter(Boolean)
                const invalid = values.find((value) => value.length > 200)
                if (invalid) {
                    errors.scan_ticket_patterns = `"${invalid}" is too long (max 200 characters).`
                    break
                }
                const duplicate = findDuplicateValue(values)
                errors.scan_ticket_patterns = duplicate ? `"${duplicate}" appears more than once.` : null
                break
            }
            case 'default_priority': {
                const value = form.defaultPriority.trim()
                if (!value) {
                    errors.default_priority = null
                    return
                }
                const options = priorityOptions.value
                if (!options.length) {
                    errors.default_priority = 'Define priorities before choosing a default.'
                    return
                }
                const ok = options.some((opt) => opt.toLowerCase() === value.toLowerCase())
                errors.default_priority = ok ? null : 'Select a priority from the configured list.'
                break
            }
            case 'default_status': {
                const value = form.defaultStatus.trim()
                if (!value) {
                    errors.default_status = null
                    return
                }
                const options = statusOptions.value
                if (!options.length) {
                    errors.default_status = 'Define statuses before choosing a default.'
                    return
                }
                const ok = options.some((opt) => opt.toLowerCase() === value.toLowerCase())
                errors.default_status = ok ? null : 'Select a status from the configured workflow.'
                break
            }
            case 'issue_states': {
                const values = form.issueStates.map((val) => val.trim()).filter((val) => val.length > 0)
                if (isGlobal.value && values.length === 0) {
                    errors.issue_states = 'At least one status is required.'
                    return
                }
                if (!isGlobal.value && values.length === 0) {
                    errors.issue_states = null
                    validateField('default_status')
                    return
                }
                const duplicate = findDuplicateValue(values)
                if (duplicate) {
                    errors.issue_states = `"${duplicate}" appears more than once.`
                    return
                }
                const tooLong = values.find((val) => val.length > 50)
                if (tooLong) {
                    errors.issue_states = `"${tooLong}" is too long (max 50 characters).`
                    return
                }
                errors.issue_states = null
                validateField('default_status')
                break
            }
            case 'issue_types': {
                const values = form.issueTypes.map((val) => val.trim()).filter((val) => val.length > 0)
                if (!isGlobal.value && values.length === 0) {
                    errors.issue_types = null
                    return
                }
                if (values.length === 0) {
                    errors.issue_types = 'Provide at least one issue type.'
                    return
                }
                const duplicate = findDuplicateValue(values)
                if (duplicate) {
                    errors.issue_types = `"${duplicate}" appears more than once.`
                    return
                }
                const tooLong = values.find((val) => val.length > 50)
                if (tooLong) {
                    errors.issue_types = `"${tooLong}" is too long (max 50 characters).`
                    return
                }
                errors.issue_types = null
                break
            }
            case 'issue_priorities': {
                const values = form.issuePriorities.map((val) => val.trim()).filter((val) => val.length > 0)
                if (!isGlobal.value && values.length === 0) {
                    errors.issue_priorities = null
                    validateField('default_priority')
                    return
                }
                if (values.length === 0) {
                    errors.issue_priorities = 'Provide at least one priority.'
                    return
                }
                const duplicate = findDuplicateValue(values)
                if (duplicate) {
                    errors.issue_priorities = `"${duplicate}" appears more than once.`
                    return
                }
                const tooLong = values.find((val) => val.length > 50)
                if (tooLong) {
                    errors.issue_priorities = `"${tooLong}" is too long (max 50 characters).`
                    return
                }
                errors.issue_priorities = null
                validateField('default_priority')
                break
            }
            case 'branch_type_aliases': {
                errors.branch_type_aliases = validateAliasEntries(form.branchTypeAliases, 'branch type')
                break
            }
            case 'branch_status_aliases': {
                errors.branch_status_aliases = validateAliasEntries(form.branchStatusAliases, 'branch status')
                break
            }
            case 'branch_priority_aliases': {
                errors.branch_priority_aliases = validateAliasEntries(form.branchPriorityAliases, 'branch priority')
                break
            }
            default:
                break
        }
    }

    function validateAll(): boolean {
        const fields: string[] = [
            'default_reporter',
            'default_assignee',
            'default_tags',
            'default_priority',
            'default_status',
            'issue_states',
            'issue_types',
            'issue_priorities',
            'tags',
            'custom_fields',
            'scan_signal_words',
            'scan_ticket_patterns',
            'branch_type_aliases',
            'branch_status_aliases',
            'branch_priority_aliases',
        ]
        if (isGlobal.value) {
            fields.push('server_port', 'default_prefix')
        } else {
            fields.push('project_name')
        }
        fields.forEach((field) => validateField(field))
        return !hasErrors.value
    }

    function populateForm(data: ConfigInspectResult) {
        const effective = data.effective
        form.serverPort = effective.server_port?.toString() ?? ''
        form.defaultPrefix = effective.default_prefix ?? ''
        form.defaultReporter = effective.default_reporter ?? ''
        form.defaultAssignee = effective.default_assignee ?? ''
        form.defaultTags = [...effective.default_tags]
        form.defaultPriority = effective.default_priority ?? ''
        form.defaultStatus = effective.default_status ?? ''
        form.issueStates = [...effective.issue_states]
        form.issueTypes = [...effective.issue_types]
        form.issuePriorities = [...effective.issue_priorities]
        form.tags = (effective.tags ?? []).filter((value) => value !== '*')
        form.customFields = (effective.custom_fields ?? []).filter((value) => value !== '*')
        form.autoCodeownersAssign = toToggle(!!effective.auto_codeowners_assign)
        form.autoTagsFromPath = toToggle(!!effective.auto_tags_from_path)
        form.autoBranchInferType = toToggle(!!effective.auto_branch_infer_type)
        form.autoBranchInferStatus = toToggle(!!effective.auto_branch_infer_status)
        form.autoBranchInferPriority = toToggle(!!effective.auto_branch_infer_priority)
        form.autoIdentity = toToggle(!!effective.auto_identity)
        form.autoIdentityGit = toToggle(!!effective.auto_identity_git)
        form.scanSignalWords = [...(effective.scan_signal_words ?? [])]
        form.scanTicketPatterns = [...(effective.scan_ticket_patterns ?? [])]
        form.scanEnableTicketWords = toToggle(!!effective.scan_enable_ticket_words)
        form.scanEnableMentions = toToggle(!!effective.scan_enable_mentions)
        form.scanStripAttributes = toToggle(!!effective.scan_strip_attributes)
        form.branchTypeAliases = mapToAliasEntries(effective.branch_type_aliases)
        form.branchStatusAliases = mapToAliasEntries(effective.branch_status_aliases)
        form.branchPriorityAliases = mapToAliasEntries(effective.branch_priority_aliases)
        if (!isGlobal.value) {
            const raw = ((data.project_raw as any) || {}) as Record<string, any>
            form.projectName = raw.project_name || currentProject.value?.name || ''
            form.autoSetReporter = toTriToggle(raw.auto_set_reporter)
            form.autoAssignOnStatus = toTriToggle(raw.auto_assign_on_status)
            form.scanEnableTicketWords = toTriToggle(raw.scan_enable_ticket_words)
            form.scanEnableMentions = toTriToggle(raw.scan_enable_mentions)
            form.scanStripAttributes = toTriToggle(raw.scan_strip_attributes)
            if (Array.isArray(raw.scan_signal_words)) {
                form.scanSignalWords = [...raw.scan_signal_words]
            }
            if (Array.isArray(raw.scan_ticket_patterns)) {
                form.scanTicketPatterns = [...raw.scan_ticket_patterns]
            }
            if (raw.branch_type_aliases) {
                form.branchTypeAliases = mapToAliasEntries(raw.branch_type_aliases)
            }
            if (raw.branch_status_aliases) {
                form.branchStatusAliases = mapToAliasEntries(raw.branch_status_aliases)
            }
            if (raw.branch_priority_aliases) {
                form.branchPriorityAliases = mapToAliasEntries(raw.branch_priority_aliases)
            }
        } else {
            form.projectName = ''
            form.autoSetReporter = toToggle(!!effective.auto_set_reporter)
            form.autoAssignOnStatus = toToggle(!!effective.auto_assign_on_status)
            form.scanEnableTicketWords = toToggle(!!effective.scan_enable_ticket_words)
            form.scanEnableMentions = toToggle(!!effective.scan_enable_mentions)
            form.scanStripAttributes = toToggle(!!effective.scan_strip_attributes)
        }
    }

    function resetForm() {
        if (!baseline.value) return
        applySnapshot(baseline.value)
        clearErrors()
    }

    function buildPayload(): Record<string, string> {
        const base = baseline.value
        if (!base) return {}
        const current = snapshotForm()
        const payload: Record<string, string> = {}

        const addValue = (
            field: keyof ConfigFormState,
            key: string,
            { trim = true, allowEmpty = true }: { trim?: boolean; allowEmpty?: boolean } = {},
        ) => {
            const currentValue = (current[field] as unknown as string) ?? ''
            const baseValue = (base[field] as unknown as string) ?? ''
            const normalize = (val: string) => (trim ? val.trim() : val)
            if (normalize(currentValue) === normalize(baseValue)) return
            if (!allowEmpty && normalize(currentValue) === '') return
            payload[key] = normalize(currentValue)
        }

        const addArray = (field: keyof ConfigFormState, key: string) => {
            const currentArray = current[field] as unknown as string[]
            const baseArray = base[field] as unknown as string[]
            if (arraysEqual(currentArray, baseArray)) return
            payload[key] = normalizeCsv(currentArray)
        }

        const addToggleField = (field: ToggleField, key: string, { allowInherit = false }: { allowInherit?: boolean } = {}) => {
            const currentValue = current[field] as ToggleValue
            const baseValue = base[field] as ToggleValue
            if (currentValue === baseValue) return
            if (currentValue === 'inherit') {
                if (!isGlobal.value && allowInherit) {
                    payload[key] = ''
                }
                return
            }
            payload[key] = currentValue
        }

        const addAliasMap = (
            field: 'branchTypeAliases' | 'branchStatusAliases' | 'branchPriorityAliases',
            key: string,
        ) => {
            const currentEntries = current[field] as unknown as BranchAliasEntry[]
            const baseEntries = base[field] as unknown as BranchAliasEntry[]
            if (aliasEntriesEqual(currentEntries, baseEntries)) return
            const normalized = normalizedAlias(currentEntries)
            if (!Object.keys(normalized).length && !isGlobal.value) {
                payload[key] = ''
                return
            }
            payload[key] = JSON.stringify(normalized)
        }

        if (isGlobal.value) {
            addValue('serverPort', 'server_port', { trim: true, allowEmpty: false })
            addValue('defaultPrefix', 'default_prefix', { trim: true, allowEmpty: true })
        } else {
            addValue('projectName', 'project_name', { trim: true, allowEmpty: false })
        }

        addValue('defaultReporter', 'default_reporter', { trim: true })
        addValue('defaultAssignee', 'default_assignee', { trim: true })
        addValue('defaultPriority', 'default_priority', { trim: true })
        addValue('defaultStatus', 'default_status', { trim: true, allowEmpty: true })

        addArray('defaultTags', 'default_tags')
        addArray('issueStates', 'issue_states')
        addArray('issueTypes', 'issue_types')
        addArray('issuePriorities', 'issue_priorities')
        addArray('tags', 'tags')
        addArray('customFields', 'custom_fields')
        addArray('scanSignalWords', 'scan_signal_words')
        addArray('scanTicketPatterns', 'scan_ticket_patterns')

        addToggleField('autoSetReporter', 'auto_set_reporter', { allowInherit: true })
        addToggleField('autoAssignOnStatus', 'auto_assign_on_status', { allowInherit: true })
        addToggleField('scanEnableTicketWords', 'scan_enable_ticket_words', { allowInherit: true })
        addToggleField('scanEnableMentions', 'scan_enable_mentions', { allowInherit: true })
        addToggleField('scanStripAttributes', 'scan_strip_attributes', { allowInherit: true })

        if (isGlobal.value) {
            addToggleField('autoCodeownersAssign', 'auto_codeowners_assign')
            addToggleField('autoTagsFromPath', 'auto_tags_from_path')
            addToggleField('autoBranchInferType', 'auto_branch_infer_type')
            addToggleField('autoBranchInferStatus', 'auto_branch_infer_status')
            addToggleField('autoBranchInferPriority', 'auto_branch_infer_priority')
            addToggleField('autoIdentity', 'auto_identity')
            addToggleField('autoIdentityGit', 'auto_identity_git')
        }

        addAliasMap('branchTypeAliases', 'branch_type_aliases')
        addAliasMap('branchStatusAliases', 'branch_status_aliases')
        addAliasMap('branchPriorityAliases', 'branch_priority_aliases')

        return payload
    }

    return {
        form,
        baseline,
        errors,
        isGlobal,
        currentProject,
        tagWildcard,
        customFieldWildcard,
        projectExists,
        tagSuggestions,
        statusOptions,
        priorityOptions,
        typeOptions,
        statusSuggestions,
        prioritySuggestions,
        typeSuggestions,
        peopleDescription,
        workflowDescription,
        taxonomyDescription,
        projectOverviewDescription,
        automationDescription,
        scanningDescription,
        branchAliasDescription,
        isDirty,
        hasErrors,
        saveDisabled,
        toggleSelectOptions,
        globalToggleSummary,
        provenanceLabel,
        provenanceClass,
        sourceFor,
        addAliasEntry,
        removeAliasEntry,
        clearAliasField,
        validateField,
        validateAll,
        snapshotForm,
        applySnapshot,
        clearErrors,
        populateForm,
        resetForm,
        buildPayload,
    }
}

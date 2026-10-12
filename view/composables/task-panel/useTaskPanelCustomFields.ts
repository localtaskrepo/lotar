import type { ComputedRef, Ref } from 'vue'
import { reactive } from 'vue'

interface UseTaskPanelCustomFieldsOptions {
    mode: ComputedRef<'create' | 'edit'>
    ready: Ref<boolean>
    suppressWatch: Ref<boolean>
    configCustomFields: Ref<string[]>
    applyPatch: (patch: Record<string, unknown>) => Promise<void>
}

interface CustomFieldsMap {
    [key: string]: string
}

interface CustomFieldKeysMap {
    [key: string]: string
}

interface NewFieldState {
    key: string
    value: string
}

export interface TaskPanelCustomFieldsApi {
    customFields: CustomFieldsMap
    customFieldKeys: CustomFieldKeysMap
    newField: NewFieldState
    buildCustomFields: () => Record<string, unknown>
    commitCustomFields: () => Promise<void>
    addField: () => Promise<void>
    removeField: (key: string) => Promise<void>
    resetCustomFields: () => void
    ensureConfiguredCustomFields: () => void
    applyDefaultsFromConfig: (defaults: Record<string, unknown>) => void
    applyTaskCustomFields: (values: Record<string, unknown>) => void
    updateCustomFieldKey: (key: string, value: string) => void
    updateCustomFieldValue: (key: string, value: string) => void
    updateNewFieldKey: (value: string) => void
    updateNewFieldValue: (value: string) => void
}

export function useTaskPanelCustomFields(options: UseTaskPanelCustomFieldsOptions): TaskPanelCustomFieldsApi {
    const customFields = reactive<CustomFieldsMap>({})
    const customFieldKeys = reactive<CustomFieldKeysMap>({})
    const newField = reactive<NewFieldState>({ key: '', value: '' })
    // Raw (non-string) JSON values loaded from the task, keyed by field name.
    // Untouched values are resent verbatim so replace-all custom_fields
    // patches never degrade numbers/booleans/objects to their string form.
    const rawCustomFieldValues: Record<string, unknown> = {}
    // DEV-102: the task's persisted custom_fields snapshot (raw values) as
    // last applied from the server. Blur commits that would resend exactly
    // this map are skipped; null means "no baseline yet" (defer to server).
    let appliedTaskCustomFields: Record<string, unknown> | null = null

    const displayValue = (value: unknown): string =>
        value === undefined || value === null ? '' : String(value)

    /**
     * Structural deep equality for custom-field maps: preserves raw types
     * (string "5" differs from number 5, boolean true differs from "true"),
     * compares arrays order-sensitively, objects key-order-insensitively,
     * and nested values recursively — never a JSON.stringify comparison,
     * which would falsely differ on key order alone.
     */
    const customFieldValuesEqual = (a: unknown, b: unknown): boolean => {
        if (a === b) return true
        if (Array.isArray(a) && Array.isArray(b)) {
            return a.length === b.length && a.every((value, index) => customFieldValuesEqual(value, b[index]))
        }
        if (a !== null && b !== null && typeof a === 'object' && typeof b === 'object') {
            const aKeys = Object.keys(a as Record<string, unknown>)
            const bKeys = Object.keys(b as Record<string, unknown>)
            return aKeys.length === bKeys.length && aKeys.every((key) =>
                Object.prototype.hasOwnProperty.call(b, key) &&
                customFieldValuesEqual((a as Record<string, unknown>)[key], (b as Record<string, unknown>)[key]))
        }
        return false
    }

    const buildCustomFields = () => {
        const out: Record<string, unknown> = {}
        Object.entries(customFields).forEach(([key, value]) => {
            const target = (customFieldKeys[key] || key || '').trim()
            if (!target) {
                return
            }
            const raw = rawCustomFieldValues[target]
            out[target] = raw !== undefined && displayValue(raw) === value ? raw : value
        })
        return out
    }

    const commitCustomFields = async () => {
        if (options.mode.value !== 'edit' || !options.ready.value || options.suppressWatch.value) {
            return
        }
        const next = buildCustomFields()
        // DEV-102: blurring an untouched custom-field row must not patch.
        // Skip only when the would-be replace-all map structurally equals
        // the task's persisted values; without a baseline (task not yet
        // applied) the patch is deferred to the server.
        if (appliedTaskCustomFields && customFieldValuesEqual(next, appliedTaskCustomFields)) {
            return
        }
        await options.applyPatch({ custom_fields: next })
    }

    const addField = async () => {
        const key = newField.key.trim()
        if (!key || customFields[key] !== undefined) {
            return
        }
        customFields[key] = newField.value
        customFieldKeys[key] = key
        newField.key = ''
        newField.value = ''
        await commitCustomFields()
    }

    const removeField = async (key: string) => {
        delete customFields[key]
        delete customFieldKeys[key]
        delete rawCustomFieldValues[key]
        await commitCustomFields()
    }

    const resetCustomFields = () => {
        Object.keys(customFields).forEach((key) => delete customFields[key])
        Object.keys(customFieldKeys).forEach((key) => delete customFieldKeys[key])
        Object.keys(rawCustomFieldValues).forEach((key) => delete rawCustomFieldValues[key])
        appliedTaskCustomFields = null
        newField.key = ''
        newField.value = ''
    }

    const ensureConfiguredCustomFields = () => {
        const existingKeys = Object.keys(customFields)
        const lowerToActual = new Map<string, string>()
        existingKeys.forEach((key) => {
            lowerToActual.set(key.toLowerCase(), key)
            if (!customFieldKeys[key]) {
                customFieldKeys[key] = key
            }
        })

        const configured = Array.isArray(options.configCustomFields.value)
            ? options.configCustomFields.value.filter((key: string) => key && key !== '*')
            : []

        configured.forEach((raw) => {
            const trimmed = (raw || '').trim()
            if (!trimmed) return
            const lower = trimmed.toLowerCase()
            if (lowerToActual.has(lower)) {
                const existing = lowerToActual.get(lower)
                if (existing && !customFieldKeys[existing]) {
                    customFieldKeys[existing] = existing
                }
                return
            }
            customFields[trimmed] = ''
            customFieldKeys[trimmed] = trimmed
            lowerToActual.set(lower, trimmed)
        })
    }

    const applyDefaultsFromConfig = (defaults: Record<string, unknown>) => {
        Object.entries(defaults).forEach(([key, value]) => {
            const trimmedKey = (key || '').trim()
            if (!trimmedKey) return
            if (customFields[trimmedKey] === undefined) {
                customFields[trimmedKey] = value === undefined || value === null ? '' : String(value)
            }
            if (!customFieldKeys[trimmedKey]) {
                customFieldKeys[trimmedKey] = trimmedKey
            }
        })
    }

    const applyTaskCustomFields = (values: Record<string, unknown>) => {
        resetCustomFields()
        // Baseline for the DEV-102 blur no-op guard: snapshot the persisted
        // raw values (deep copy) before the editor state is derived from them.
        appliedTaskCustomFields = structuredClone(values ?? {})
        Object.entries(values).forEach(([rawKey, value]) => {
            const targetKey = (rawKey || '').trim()
            if (!targetKey) return
            customFields[targetKey] = displayValue(value)
            customFieldKeys[targetKey] = targetKey
            if (value !== null && value !== undefined && typeof value !== 'string') {
                rawCustomFieldValues[targetKey] = value
            }
        })
        ensureConfiguredCustomFields()
    }

    const updateCustomFieldKey = (key: string, value: string) => {
        customFieldKeys[key] = value
    }

    const updateCustomFieldValue = (key: string, value: string) => {
        customFields[key] = value
    }

    const updateNewFieldKey = (value: string) => {
        newField.key = value
    }

    const updateNewFieldValue = (value: string) => {
        newField.value = value
    }

    return {
        customFields,
        customFieldKeys,
        newField,
        buildCustomFields,
        commitCustomFields,
        addField,
        removeField,
        resetCustomFields,
        ensureConfiguredCustomFields,
        applyDefaultsFromConfig,
        applyTaskCustomFields,
        updateCustomFieldKey,
        updateCustomFieldValue,
        updateNewFieldKey,
        updateNewFieldValue,
    }
}

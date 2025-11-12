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
    buildCustomFields: () => Record<string, string>
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

    const buildCustomFields = () => {
        const out: Record<string, string> = {}
        Object.entries(customFields).forEach(([key, value]) => {
            const target = (customFieldKeys[key] || key || '').trim()
            if (target) {
                out[target] = value
            }
        })
        return out
    }

    const commitCustomFields = async () => {
        if (options.mode.value !== 'edit' || !options.ready.value || options.suppressWatch.value) {
            return
        }
        await options.applyPatch({ custom_fields: buildCustomFields() })
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
        await commitCustomFields()
    }

    const resetCustomFields = () => {
        Object.keys(customFields).forEach((key) => delete customFields[key])
        Object.keys(customFieldKeys).forEach((key) => delete customFieldKeys[key])
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
        Object.entries(values).forEach(([rawKey, value]) => {
            const targetKey = (rawKey || '').trim()
            if (!targetKey) return
            const strValue = value === undefined || value === null ? '' : String(value)
            customFields[targetKey] = strValue
            customFieldKeys[targetKey] = targetKey
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

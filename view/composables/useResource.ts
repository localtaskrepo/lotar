import { computed, ref, type ComputedRef, type Ref } from 'vue'

export type ResourceStatus = 'idle' | 'loading' | 'ready' | 'error'

export interface ResourceOptions<T, Args extends any[]> {
    initialValue?: T
    throwOnError?: boolean
    onError?: (error: Error, args: Args) => void
    transform?: (value: T) => T
}

export interface ResourceState<T, Args extends any[]> {
    data: Ref<T | undefined>
    status: Ref<ResourceStatus>
    error: Ref<Error | null>
    loading: ComputedRef<boolean>
    ready: ComputedRef<boolean>
    lastArgs: Ref<Args | null>
    refresh: (...args: Args) => Promise<T | undefined>
    reset: () => void
    set: (value: T | undefined) => void
    mutate: (updater: (value: T | undefined) => T | undefined) => void
}

function normalizeError(err: unknown): Error {
    if (err instanceof Error) return err
    return new Error(typeof err === 'string' ? err : JSON.stringify(err))
}

export function createResource<T, Args extends any[]>(
    loader: (...args: Args) => Promise<T>,
    options: ResourceOptions<T, Args> = {},
): ResourceState<T, Args> {
    const data = ref<T | undefined>(options.initialValue as T | undefined) as Ref<T | undefined>
    const status = ref<ResourceStatus>('idle') as Ref<ResourceStatus>
    const error = ref<Error | null>(null)
    const lastArgs = ref<Args | null>(null as Args | null) as Ref<Args | null>
    const loading = computed(() => status.value === 'loading')
    const ready = computed(() => status.value === 'ready')

    async function refresh(...args: Args): Promise<T | undefined> {
        status.value = 'loading'
        error.value = null
        lastArgs.value = args

        try {
            const result = await loader(...args)
            const transformed = options.transform ? options.transform(result) : result
            data.value = transformed
            status.value = 'ready'
            return transformed
        } catch (err: unknown) {
            const normalized = normalizeError(err)
            error.value = normalized
            status.value = 'error'
            if (options.onError) {
                try {
                    options.onError(normalized, args)
                } catch (callbackError) {
                    console.warn('createResource onError callback failed', callbackError)
                }
            }
            if (options.throwOnError) {
                throw normalized
            }
            return undefined
        }
    }

    function reset() {
        data.value = options.initialValue as T | undefined
        status.value = 'idle'
        error.value = null
        lastArgs.value = null
    }

    function set(value: T | undefined) {
        data.value = value
        status.value = 'ready'
    }

    function mutate(updater: (value: T | undefined) => T | undefined) {
        data.value = updater(data.value)
        status.value = 'ready'
    }

    return {
        data,
        status,
        error,
        loading,
        ready,
        lastArgs,
        refresh,
        reset,
        set,
        mutate,
    }
}

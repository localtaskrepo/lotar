// Lookups whose value must exist by construction. They throw a clear error instead of a `!` that would let
// `undefined` flow on (the shared lint standard bans `!` outside tests).

/** The value stored under `key`; throws when there is none. */
export function mustGet<K, V>(map: ReadonlyMap<K, V>, key: K, what = 'entry'): V {
    const value = map.get(key)
    if (value === undefined) throw new Error(`No ${what} for ${String(key)}`)
    return value
}

/** The item at `index`; throws when it is out of range. For arrays whose items are never undefined. */
export function itemAt<T>(items: ArrayLike<T>, index: number, what = 'item'): T {
    const value = items[index]
    if (value === undefined) throw new Error(`No ${what} at index ${index} of ${items.length}`)
    return value
}

/** `value` itself; throws when it is null or undefined. */
export function required<T>(value: T | null | undefined, what: string): T {
    if (value === undefined || value === null) throw new Error(`Missing ${what}`)
    return value
}

/** `[value]`, or `[]` when it is null or undefined. In a template, `v-for="x in asList(f())"` binds a narrowed `x`
 * where `v-if="f()"` can't narrow the second call. */
export function asList<T>(value: T | null | undefined): T[] {
    return value === undefined || value === null ? [] : [value]
}

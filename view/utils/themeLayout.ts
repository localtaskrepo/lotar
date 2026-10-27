/**
 * Column count for `count` cards when at most `fit` fit side by side, chosen
 * so the last row is full or more than half full (never a lone card).
 * Returns 1 when no such layout exists; callers then show a list.
 */
export function balancedColumns(count: number, fit: number): number {
    for (let columns = Math.min(count, fit); columns >= 2; columns -= 1) {
        const rest = count % columns
        if (rest === 0 || rest > columns / 2) return columns
    }
    return 1
}

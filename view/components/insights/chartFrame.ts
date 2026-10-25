import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'

/** Categorical series colours in fixed order; extra series use the muted colour. */
export const SERIES_COLORS = ['var(--viz-1)', 'var(--viz-2)', 'var(--viz-3)', 'var(--viz-4)']
export const seriesColor = (index: number) => SERIES_COLORS[index] ?? 'var(--viz-muted)'

/** Clean ticks from zero: steps of 1, 2 or 5 times a power of ten, about four gridlines. */
export function niceTicks(max: number, integer = true): number[] {
    if (max <= 0) return [0, 1]
    const raw = max / 4
    const power = 10 ** Math.floor(Math.log10(raw))
    let step = [1, 2, 5, 10].map((m) => m * power).find((candidate) => candidate >= raw) ?? power * 10
    if (integer) step = Math.max(1, Math.round(step))
    const out: number[] = [0]
    while (out[out.length - 1]! < max) out.push(out[out.length - 1]! + step)
    return out
}

/** Show every nth x label so labels about 7px per character never collide. */
export function labelStride(labels: string[], spacing: number): number {
    const longest = Math.max(1, ...labels.map((label) => label.length))
    return Math.max(1, Math.ceil((longest * 7 + 12) / Math.max(1, spacing)))
}

/** Place a tooltip right of `rightEdge`, or left of `leftEdge` when it would overflow `width`. */
export function tooltipX(rightEdge: number, leftEdge: number, tooltipWidth: number, width: number): number {
    return rightEdge + tooltipWidth <= width ? rightEdge : Math.max(0, leftEdge - tooltipWidth)
}

/**
 * Shared chart state: the measured host width, the hovered index (cleared when
 * the labels change) and the rendered tooltip width for placement.
 */
export function useChartFrame(labels: () => string[], initialTooltipWidth: number) {
    const host = ref<HTMLElement | null>(null)
    const tooltip = ref<HTMLElement | null>(null)
    const width = ref(600)
    const tooltipWidth = ref(initialTooltipWidth)
    const active = ref<number | null>(null)
    let observer: ResizeObserver | null = null

    onMounted(() => {
        const measure = () => {
            const measured = host.value?.clientWidth
            if (measured) width.value = Math.max(240, Math.floor(measured))
        }
        measure()
        if (typeof ResizeObserver !== 'undefined' && host.value) {
            observer = new ResizeObserver(measure)
            observer.observe(host.value)
        }
    })
    onBeforeUnmount(() => observer?.disconnect())

    // A new period or filter replaces the labels: drop a stale hover.
    watch(
        () => labels().join('|'),
        () => (active.value = null),
    )
    watch(active, async (value) => {
        if (value === null) return
        await nextTick()
        if (tooltip.value) tooltipWidth.value = tooltip.value.offsetWidth
    })

    return { host, tooltip, width, tooltipWidth, active }
}

const HEX_REGEX = /^[0-9a-fA-F]+$/

function clamp(value: number, min = 0, max = 1) {
    return Math.min(max, Math.max(min, value))
}

export function normalizeHex(input: string | null | undefined): string | null {
    if (!input) return null
    let hex = input.trim()
    if (!hex) return null
    if (hex.startsWith('#')) hex = hex.slice(1)
    if (hex.length === 3) {
        hex = hex
            .split('')
            .map((c) => c + c)
            .join('')
    }
    if (hex.length !== 6 || !HEX_REGEX.test(hex)) return null
    return `#${hex.toLowerCase()}`
}

export function hexToRgb(hex: string): { r: number; g: number; b: number } | null {
    const normalized = normalizeHex(hex)
    if (!normalized) return null
    const value = normalized.slice(1)
    const r = parseInt(value.slice(0, 2), 16)
    const g = parseInt(value.slice(2, 4), 16)
    const b = parseInt(value.slice(4, 6), 16)
    return { r, g, b }
}

export function rgbToHex(r: number, g: number, b: number): string {
    const toHex = (n: number) => clamp(Math.round(n), 0, 255).toString(16).padStart(2, '0')
    return `#${toHex(r)}${toHex(g)}${toHex(b)}`
}

export function mixHex(color: string, target: string, amount: number): string {
    const a = hexToRgb(color)
    const b = hexToRgb(target)
    if (!a || !b) return normalizeHex(color) || color
    const mix = clamp(amount, 0, 1)
    const r = a.r * (1 - mix) + b.r * mix
    const g = a.g * (1 - mix) + b.g * mix
    const bl = a.b * (1 - mix) + b.b * mix
    return rgbToHex(r, g, bl)
}

function linearize(channel: number): number {
    const c = channel / 255
    return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4)
}

export function relativeLuminance(hex: string): number {
    const rgb = hexToRgb(hex)
    if (!rgb) return 0
    const r = linearize(rgb.r)
    const g = linearize(rgb.g)
    const b = linearize(rgb.b)
    return 0.2126 * r + 0.7152 * g + 0.0722 * b
}

export function getContrastingColor(hex: string): '#000000' | '#ffffff' {
    const luminance = relativeLuminance(hex)
    return luminance > 0.55 ? '#000000' : '#ffffff'
}

export function withAlpha(hex: string, alpha: number): string {
    const rgb = hexToRgb(hex)
    if (!rgb) return hex
    const a = clamp(alpha, 0, 1)
    return `rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, ${a})`
}

export function darken(hex: string, amount: number): string {
    return mixHex(hex, '#000000', amount)
}

export function lighten(hex: string, amount: number): string {
    return mixHex(hex, '#ffffff', amount)
}

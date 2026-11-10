// Types shared by the insights components and insightsContext.ts. They live here, not in the .vue files, so
// tools without SFC support (the linter's type checker) can resolve them.

export interface BarRow {
    key: string
    label: string
    value: number
    /** Text shown instead of the raw value (e.g. "3.5d"). */
    display?: string
    hint?: string
    /** Needs attention: shows a flag icon next to the label. */
    alert?: boolean
    /** De-emphasised (e.g. "unassigned", "none"). */
    muted?: boolean
    /** Set false for rows with no matching drill-down. */
    clickable?: boolean
}

export interface StatDelta {
    /** e.g. "12% vs previous 30 days" */
    text: string
    direction: 'up' | 'down' | 'flat'
    /** Whether the change is good news, bad news or neither. */
    tone: 'good' | 'bad' | 'neutral'
}

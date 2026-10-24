import { describe, expect, it } from 'vitest'
import source from '../components/FilterBar.vue?raw'

const style = source.slice(source.indexOf('<style scoped>'), source.indexOf('</style>'))

function declarations(selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  const matches = [...style.matchAll(new RegExp(`${escaped}\\s*\\{([^}]*)\\}`, 'g'))]
  expect(matches, `${selector} must have one layout contract, not conflicting breakpoint overrides`).toHaveLength(1)
  return matches[0]![1]!
}

describe('FilterBar responsive layout contracts', () => {
  it('uses the shared button-height token for the search border box', () => {
    const search = declarations('.filter-bar__search-input')
    expect(search).toMatch(/box-sizing:\s*border-box/)
    expect(search).toMatch(/height:\s*var\(--control-height\)/)
    expect(search).toMatch(/min-height:\s*var\(--control-height\)/)
  })
  it('allocates readable equal field tracks from available panel width rather than viewport breakpoints', () => {
    expect(declarations('.filter-bar__fields')).toContain('repeat(auto-fit, minmax(min(100%, 12rem), 1fr))')
    // Quick picks must not reserve a proportional sidebar at wide widths.
    expect(style).not.toMatch(/grid-template-columns:\s*minmax\(240px/)
    expect(style).not.toMatch(/@media/)
    expect(style).toContain('@container filter-controls (max-width: 40rem)')
  })

  it('allows native selects and multiselect triggers to shrink inside equal field tracks', () => {
    const controls = declarations('.filter-bar__field .input')
    expect(controls).toMatch(/width:\s*100%/)
    expect(controls).toMatch(/min-width:\s*0/)
    expect(controls).toMatch(/box-sizing:\s*border-box/)
    expect(declarations('.filter-bar__dropdown')).toMatch(/min-width:\s*0/)
  })

  it('keeps rightmost popovers within their field without clipping or truncating choices', () => {
    const popover = declarations('.filter-bar__menu-popover')
    expect(popover).toMatch(/width:\s*100%/)
    expect(popover).toMatch(/min-width:\s*0/)
    expect(popover).toMatch(/box-sizing:\s*border-box/)
    expect(popover).toMatch(/overflow:\s*auto/)
    expect(declarations('.filter-bar__menu-item span')).toMatch(/overflow-wrap:\s*anywhere/)
    expect(declarations('.filter-bar__menu-actions')).toMatch(/flex-wrap:\s*wrap/)
  })
})

import { describe, expect, it } from 'vitest'
import { dependencyLayout } from '../utils/dependencyLayout'
import type { RelationshipEdge } from '../api/types'

const nodes = (...ids: string[]) => ids.map(id => ({ id }))
const edge = (source: string, target: string): RelationshipEdge => ({ source, target, kind: 'dependency', provenance: [] })
describe('dependency ranks', () => {
  it('routes reverse non-dependency edges between the correct 300px ports', () => {
    const result = dependencyLayout(nodes('a', 'b'), [edge('a', 'b'), { ...edge('b', 'a'), kind: 'hierarchy' }])
    const reverse = result.edges.find(edge => edge.kind === 'hierarchy')!
    const a = result.nodes.find(node => node.id === 'a')!, b = result.nodes.find(node => node.id === 'b')!
    expect(reverse.controls[0]).toBe(b.x)
    expect(reverse.controls[6]).toBe(a.x + 300)
    expect(reverse.controls[2]).toBeGreaterThan(a.x + 300)
    expect(reverse.controls[4]).toBeLessThan(b.x)
  })
  it('reserves only the lane space needed before the next rank', () => {
    const kinds = ['dependency', 'hierarchy', 'related', 'fixes', 'duplicate'] as const
    const edges = [...kinds.flatMap(kind => [{ ...edge('a', 'b'), kind }, { ...edge('b', 'a'), kind }]), edge('b', 'c')]
    const result = dependencyLayout(nodes('a', 'b', 'c'), edges)
    const c = result.nodes.find(node => node.id === 'c')!
    for (const routed of result.edges.filter(edge => edge.target !== 'c')) {
      expect(Math.max(routed.controls[2]!, routed.controls[4]!)).toBeLessThan(c.x - 16)
    }
    expect(result.width).toBe(c.x + 300 + 16)
  })
  it.each(['self', 'same-rank', 'reverse', 'deep'])('contains every control and marker with five parallel %s lanes', scenario => {
    const kinds = ['dependency', 'hierarchy', 'related', 'fixes', 'duplicate'] as const
    const edges: RelationshipEdge[] = kinds.map(kind => ({ ...edge('a', scenario === 'self' ? 'a' : 'b'), kind }))
    if (scenario === 'same-rank' || scenario === 'reverse') edges.push(edge('b', 'a'))
    if (scenario === 'deep') for (let i = 0; i < 30; i++) edges.push(edge(i ? `n${i - 1}` : 'b', `n${i}`))
    const result = dependencyLayout(nodes('a', 'b', ...Array.from({ length: scenario === 'deep' ? 30 : 0 }, (_, i) => `n${i}`)), edges, undefined, n => n.id === 'a' ? 48 : 700)
    for (const routed of result.edges) {
      for (let i = 0; i < routed.controls.length; i += 2) {
        expect(routed.controls[i]).toBeGreaterThanOrEqual(8)
        expect(routed.controls[i]).toBeLessThanOrEqual(result.width - 16)
        expect(routed.controls[i + 1]).toBeGreaterThanOrEqual(8)
        expect(routed.controls[i + 1]).toBeLessThanOrEqual(result.height - 16)
      }
      const source = result.nodes.find(node => node.id === routed.source)!
      const target = result.nodes.find(node => node.id === routed.target)!
      expect([source.x, source.x + 300]).toContain(routed.controls[0])
      expect([target.x, target.x + 300]).toContain(routed.controls[6])
    }
    const right = Math.max(...result.nodes.map(node => node.x + 300), ...result.edges.flatMap(e => e.controls.filter((_, i) => i % 2 === 0)))
    expect(result.width).toBe(right + 16)
  })
  it('stacks variable card heights without overlap within ranks and between components', () => {
    const result = dependencyLayout(nodes('a', 'b', 'c', 'x'), [edge('a', 'b'), edge('a', 'c')], undefined, node => node.id === 'b' ? 900 : 280)
    const b = result.nodes.find(node => node.id === 'b')!
    const c = result.nodes.find(node => node.id === 'c')!
    const x = result.nodes.find(node => node.id === 'x')!
    expect(c.y).toBeGreaterThan(b.y + b.height)
    expect(x.y).toBeGreaterThan(c.y + c.height)
    expect(result.height).toBeGreaterThan(x.y + x.height)
    expect(result.nodes.map(node => node.rank)).toEqual([0, 1, 1, 0])
  })
  it('bounds invalid and too-small estimates while preserving large measured cards', () => {
    const result = dependencyLayout(nodes('a', 'b', 'c'), [], undefined, node => ({ a: Number.NaN, b: -4, c: 4000 })[node.id]!)
    expect(result.nodes.map(node => node.height)).toEqual([116, 48, 4000])
    expect(result.height).toBeGreaterThan(4000)
  })
  it('lays out arbitrarily deep prerequisites before dependents', () => {
    const result = dependencyLayout(nodes('e', 'c', 'a', 'd', 'b'), [edge('a', 'b'), edge('b', 'c'), edge('c', 'd'), edge('d', 'e')])
    expect(result.nodes.map(node => [node.id, node.rank])).toEqual([['a', 0], ['b', 1], ['c', 2], ['d', 3], ['e', 4]])
    expect(result.width).toBeGreaterThan(1000)
  })
  it('keeps diamonds on equivalent ranks and sorting within ranks', () => {
    const result = dependencyLayout(nodes('a', 'b', 'c', 'd'), [edge('a', 'b'), edge('a', 'c'), edge('b', 'd'), edge('c', 'd')], (a, b) => b.id.localeCompare(a.id))
    expect(result.nodes.map(node => [node.id, node.rank])).toEqual([['a', 0], ['c', 1], ['b', 1], ['d', 2]])
  })
  it('condenses cycles, marks self loops and ranks outgoing edges after the cycle', () => {
    const result = dependencyLayout(nodes('a', 'b', 'c', 'x'), [edge('a', 'b'), edge('b', 'a'), edge('b', 'c'), edge('x', 'x')])
    expect(result.nodes.filter(node => node.cycle).map(node => node.id)).toEqual(['a', 'b', 'x'])
    expect(result.nodes.find(node => node.id === 'c')!.rank).toBe(1)
    expect(result.nodes.find(node => node.id === 'a')!.cycleGroup).toBe('a')
  })
  it('keeps disconnected components stable independently of input order or sort', () => {
    const edges = [edge('a', 'b'), edge('x', 'y')]
    const a = dependencyLayout(nodes('y', 'b', 'x', 'a', 'z'), edges)
    const b = dependencyLayout(nodes('z', 'a', 'x', 'b', 'y'), [...edges].reverse())
    expect(a).toEqual(b)
    expect(a.nodes.map(node => node.component)).toEqual([0, 0, 1, 1, 2])
    expect(a.nodes.find(node => node.id === 'x')!.y).toBeGreaterThan(a.nodes.find(node => node.id === 'a')!.y)
  })
})

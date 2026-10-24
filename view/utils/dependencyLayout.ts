import type { RelationshipEdge } from '../api/types'

/** Condense dependency cycles before assigning longest-path prerequisite ranks. */
export function dependencyLayout<T extends { id: string }>(nodes: readonly T[], edges: readonly RelationshipEdge[], compare: (a: T, b: T) => number = (a, b) => a.id.localeCompare(b.id), heightOf: (node: T) => number = () => 116) {
  const byId = new Map(nodes.map(node => [node.id, node]))
  const ids = [...byId.keys()].sort()
  const next = new Map(ids.map(id => [id, [] as string[]]))
  const neighbors = new Map(ids.map(id => [id, new Set<string>()]))
  for (const edge of edges) {
    if (!byId.has(edge.source) || !byId.has(edge.target)) continue
    neighbors.get(edge.source)!.add(edge.target)
    neighbors.get(edge.target)!.add(edge.source)
    if (edge.kind === 'dependency') next.get(edge.source)!.push(edge.target)
  }
  let index = 0
  const indices = new Map<string, number>(), low = new Map<string, number>()
  const stack: string[] = [], active = new Set<string>(), groups: string[][] = []
  function visit(id: string) {
    indices.set(id, index); low.set(id, index++)
    stack.push(id); active.add(id)
    for (const target of next.get(id)!.sort()) {
      if (!indices.has(target)) { visit(target); low.set(id, Math.min(low.get(id)!, low.get(target)!)) }
      else if (active.has(target)) low.set(id, Math.min(low.get(id)!, indices.get(target)!))
    }
    if (low.get(id) !== indices.get(id)) return
    const group: string[] = []
    let member: string
    do { member = stack.pop()!; active.delete(member); group.push(member) } while (member !== id)
    groups.push(group.sort())
  }
  ids.forEach(id => { if (!indices.has(id)) visit(id) })
  const groupOf = new Map(groups.flatMap((group, i) => group.map(id => [id, i] as const)))
  const ranks = groups.map(() => 0), incoming = groups.map(() => new Set<number>())
  for (const [source, targets] of next) for (const target of targets) {
    const a = groupOf.get(source)!, b = groupOf.get(target)!
    if (a !== b) incoming[b]!.add(a)
  }
  const ranked = new Set<number>()
  function rank(group: number): number {
    if (!ranked.has(group)) {
      ranks[group] = Math.max(0, ...[...incoming[group]!].map(parent => rank(parent) + 1))
      ranked.add(group)
    }
    return ranks[group]!
  }
  groups.forEach((_, i) => rank(i))
  const components: string[][] = [], seen = new Set<string>()
  for (const id of ids) {
    if (seen.has(id)) continue
    const component: string[] = [], pending = [id]
    while (pending.length) {
      const current = pending.pop()!
      if (seen.has(current)) continue
      seen.add(current); component.push(current)
      pending.push(...neighbors.get(current)!)
    }
    components.push(component)
  }
  for (const component of components) {
    component.sort((a, b) => ranks[groupOf.get(a)!]! - ranks[groupOf.get(b)!]! || compare(byId.get(a)!, byId.get(b)!) || a.localeCompare(b))
  }
  components.sort((a, b) => compare(byId.get(a[0]!)!, byId.get(b[0]!)!) || a[0]!.localeCompare(b[0]!))
  const rankWidths = new Map<number, number>()
  const pairCounts = new Map<string, number>()
  for (const edge of edges) {
    if (!byId.has(edge.source) || !byId.has(edge.target)) continue
    const level = ranks[groupOf.get(edge.source)!]!
    if (level !== ranks[groupOf.get(edge.target)!]) continue
    const pair = [edge.source, edge.target].sort().join('\0')
    const count = (pairCounts.get(pair) || 0) + 1
    pairCounts.set(pair, count)
    rankWidths.set(level, Math.max(rankWidths.get(level) || 400, 340 + (count - 1) * 20 + 32))
  }
  const rankX = [16]
  for (let level = 0; level < Math.max(0, ...ranks); level++) rankX.push(rankX[level]! + (rankWidths.get(level) || 400))
  let offset = 16
  const placed: Array<T & { x: number; y: number; height: number; rank: number; component: number; cycle: boolean; cycleGroup: string }> = []
  components.forEach((component, componentIndex) => {
    const rows = new Map<number, number>()
    for (const id of component) {
      const group = groupOf.get(id)!, level = ranks[group]!, row = rows.get(level) || 0
      const measured = heightOf(byId.get(id)!)
      const height = Number.isFinite(measured) ? Math.max(48, measured) : 116
      rows.set(level, row + height + 32)
      placed.push({ ...byId.get(id)!, x: rankX[level]!, y: offset + row, height, rank: level, component: componentIndex, cycle: groups[group]!.length > 1 || next.get(id)!.includes(id), cycleGroup: groups[group]![0]! })
    }
    offset += Math.max(...rows.values()) + 24
  })
  const positions = new Map(placed.map(node => [node.id, node]))
  const orderedEdges = [...edges].sort((a, b) => a.source.localeCompare(b.source) || a.target.localeCompare(b.target) || a.kind.localeCompare(b.kind))
  let right = Math.max(0, ...placed.map(node => node.x + 300))
  let bottom = Math.max(0, ...placed.map(node => node.y + node.height))
  const routed = orderedEdges.flatMap(edge => {
    const source = positions.get(edge.source), target = positions.get(edge.target)
    if (!source || !target) return []
    const siblings = orderedEdges.filter(other => [other.source, other.target].sort().join('\0') === [edge.source, edge.target].sort().join('\0'))
    const lane = siblings.indexOf(edge), fraction = (lane + 1) / (siblings.length + 1)
    const y1 = source.y + source.height * fraction, y2 = target.y + target.height * fraction
    const forward = target.x > source.x
    const x1 = source.x + (forward || source.x === target.x ? 300 : 0)
    const x2 = target.x + (forward ? 0 : 300)
    // Keep all controls inside the canvas, including five parallel self/cycle lanes.
    const bend = source.x + 340 + lane * 20
    const endY = y2 + (source.id === target.id ? Math.min(24, source.height * (1 - fraction) / 2) : 0)
    const controls = source.x === target.x
      ? [x1, y1, bend, Math.max(8, y1 - 40), bend, y2 + 40, x2, endY]
      : [x1, y1, x1 + (forward ? 30 : -30), y1, x2 + (forward ? -30 : 30), y2, x2, y2]
    right = Math.max(right, controls[0]!, controls[2]!, controls[4]!, controls[6]!)
    bottom = Math.max(bottom, controls[1]!, controls[3]!, controls[5]!, controls[7]!)
    return [{ ...edge, path: `M ${controls[0]} ${controls[1]} C ${controls[2]} ${controls[3]}, ${controls[4]} ${controls[5]}, ${controls[6]} ${controls[7]}`, controls }]
  })
  return { nodes: placed, edges: routed, width: right + 16, height: Math.max(148, bottom + 16) }
}

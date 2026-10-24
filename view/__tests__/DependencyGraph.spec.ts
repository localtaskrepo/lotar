import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import DependencyGraph from '../components/DependencyGraph.vue'
import type { RelationshipGraphResponse, RelationshipNode } from '../api/types'
import graphSource from '../components/DependencyGraph.vue?raw'
import cardSource from '../components/DependencyCardDetails.vue?raw'

const mocks = vi.hoisted(() => ({
    graph: vi.fn(),
    getTask: vi.fn(),
    openTaskPanel: vi.fn(),
    on: vi.fn(),
    close: vi.fn(),
    reconnect: undefined as (() => void) | undefined,
}))
vi.mock('../api/client', () => ({ api: { relationshipGraph: mocks.graph, getTask: mocks.getTask } }))
vi.mock('../composables/useTaskPanelController', () => ({
    useTaskPanelController: () => ({ openTaskPanel: mocks.openTaskPanel }),
}))
vi.mock('../composables/useCompletionPolicy', () => ({
    ensureCompletionPolicy: () => Promise.resolve(),
    useCompletionPolicy: () => ({ isTaskOverdue: () => false }),
}))
vi.mock('../composables/useSse', () => ({
    useSse: (_path: string, _params: unknown, opts: { onReconnect: () => void }) => {
        mocks.reconnect = opts.onReconnect
        return { on: mocks.on, off: vi.fn(), close: mocks.close }
    },
}))

const node = (id: string, state: RelationshipNode['state'] = 'active'): RelationshipNode => ({
    id,
    title: state === 'active' ? `Title ${id}` : null,
    status: state === 'active' ? 'Todo' : null,
    state,
    project: 'DEV',
    is_done: false,
    matches_filter: true,
})
function graph(extra: Partial<RelationshipGraphResponse> = {}): RelationshipGraphResponse {
    return {
        nodes: [node('DEV-8'), node('OTHER-2')],
        edges: [
            {
                source: 'OTHER-2',
                target: 'DEV-8',
                kind: 'dependency',
                provenance: [{ task_id: 'DEV-8', field: 'depends_on', value: 'OTHER-2' }],
            },
        ],
        issues: [],
        complete: true,
        truncated: false,
        revision: 'r1',
        ...extra,
    }
}
const edge = (
    source: string,
    target: string,
    kind: RelationshipGraphResponse['edges'][number]['kind'] = 'dependency',
) => ({ source, target, kind, provenance: [] })
const dev54Graph = () =>
    graph({
        nodes: [54, 55, 59, 60, 64, 82, 23, 28, 53].map((id) => node(`DEV-${id}`)),
        edges: [
            ...[55, 59, 60, 64, 82].map((id) => edge('DEV-54', `DEV-${id}`)),
            edge('DEV-54', 'DEV-23', 'related'),
            edge('DEV-54', 'DEV-28', 'related'),
            edge('DEV-53', 'DEV-54', 'hierarchy'),
        ],
    })
const visibleIds = (wrapper: ReturnType<typeof mount>) =>
    wrapper
        .findAll('.dependency-task-list > li')
        .map((row) => row.attributes('data-task-id'))
        .sort()
beforeEach(() => {
    vi.clearAllMocks()
    mocks.graph.mockResolvedValue(graph())
    vi.stubGlobal('EventSource', class {})
    vi.stubGlobal(
        'matchMedia',
        vi.fn(() => ({ matches: false })),
    )
})
afterEach(() => {
    vi.useRealTimers()
    vi.unstubAllGlobals()
})

describe('Bounded dependency view', () => {
    it('emits only the identified safe issue for parent preview, never applies a repair or guesses a cycle edge', async () => {
        const safe = {
            id: 'opaque-safe-id',
            code: 'missing_mirror',
            severity: 'warning' as const,
            message: 'Missing mirror',
            tasks: ['DEV-8'],
            repairable: true,
        }
        mocks.graph.mockResolvedValue(
            graph({
                issues: [
                    safe,
                    {
                        ...safe,
                        id: 'opaque-cycle-id',
                        code: 'dependency_cycle',
                        repairable: false,
                        message: 'Choose an edge manually.',
                    },
                    { ...safe, id: undefined },
                    {
                        ...safe,
                        id: 'opaque-external-id',
                        repairable: false,
                        message: 'External workspace is read-only.',
                    },
                ],
            }),
        )
        const fix = vi.fn()
        const wrapper = mount(DependencyGraph, { props: { onFix: fix } })
        await flushPromises()
        const cards = wrapper.findAll('.dependency-problem-list > li')
        expect(cards[1]!.text()).toContain('Manual review')
        expect(cards[1]!.find('button').exists()).toBe(false)
        expect(cards[3]!.find('button').exists()).toBe(false)
        expect(cards[3]!.get('.dependency-problem-action').attributes('title')).toContain('read-only')
        expect(cards[2]!.get('button').attributes('disabled')).toBeDefined()
        expect(cards[2]!.get('button').attributes('title')).toContain('no repair ID')
        await cards[0]!.get('button').trigger('click')
        expect(fix).toHaveBeenCalledExactlyOnceWith(safe)
        expect(mocks.graph).toHaveBeenCalledOnce()
        expect(mocks.getTask).not.toHaveBeenCalled()
        await wrapper.setProps({ fixBusy: true })
        await cards[0]!.get('button').trigger('click')
        expect(cards[0]!.get('button').attributes('disabled')).toBeDefined()
        expect(fix).toHaveBeenCalledOnce()
        let resolve!: (value: RelationshipGraphResponse) => void
        mocks.graph.mockImplementationOnce(
            () =>
                new Promise((done) => {
                    resolve = done
                }),
        )
        await wrapper.setProps({ fixBusy: false, relationshipKinds: ['related'] })
        expect(cards[0]!.get('button').attributes('disabled')).toBeDefined()
        await cards[0]!.get('button').trigger('click')
        expect(fix).toHaveBeenCalledOnce()
        resolve(graph())
        await flushPromises()
        wrapper.unmount()
    })
    it('keeps accepted kinds, isolates and geometry while replacement scope loads, including on failure', async () => {
        vi.stubGlobal(
            'matchMedia',
            vi.fn(() => ({ matches: true })),
        )
        mocks.graph.mockResolvedValue(graph({ nodes: [node('DEV-8'), node('OTHER-2'), node('DEV-9')] }))
        const wrapper = mount(DependencyGraph, { props: { showIsolated: true } })
        await flushPromises()
        const diagram = wrapper.get('.dependency-diagram').element
        const canvas = wrapper.get('.dependency-canvas').attributes('style')
        let reject!: (reason: Error) => void
        mocks.graph.mockImplementationOnce(
            () =>
                new Promise((_done, fail) => {
                    reject = fail
                }),
        )
        await wrapper.setProps({ relationshipKinds: ['related'], showIsolated: false })
        expect(mocks.graph).toHaveBeenLastCalledWith(
            expect.objectContaining({ relationship_kinds: ['related'], include_isolated: false }),
        )
        expect(visibleIds(wrapper)).toEqual(['DEV-8', 'DEV-9', 'OTHER-2'])
        expect(wrapper.get('.dependency-diagram').element).toBe(diagram)
        expect(wrapper.get('.dependency-canvas').attributes('style')).toBe(canvas)
        expect(wrapper.get('.dependency-render').attributes('inert')).toBeDefined()
        reject(new Error('Unavailable'))
        await flushPromises()
        expect(visibleIds(wrapper)).toEqual(['DEV-8', 'DEV-9', 'OTHER-2'])
        expect(wrapper.text()).toContain('Previous dependency view (inactive)')
        wrapper.unmount()
    })
    it('removes a settled empty plot and retained height across same-root SSE refreshes, preserving diagram choice', async () => {
        vi.useFakeTimers()
        vi.stubGlobal(
            'matchMedia',
            vi.fn(() => ({ matches: true })),
        )
        const wrapper = mount(DependencyGraph, { props: { root: 'DEV-8' } })
        await flushPromises()
        await wrapper
            .findAll('button')
            .find((button) => button.text() === 'Hide diagram')!
            .trigger('click')
        await wrapper
            .findAll('button')
            .find((button) => button.text() === 'Show diagram')!
            .trigger('click')
        const diagram = wrapper.get('.dependency-diagram').element as HTMLElement
        vi.spyOn(diagram, 'getBoundingClientRect').mockReturnValue({ height: 600 } as DOMRect)
        let resolve!: (value: RelationshipGraphResponse) => void
        mocks.graph.mockImplementationOnce(
            () =>
                new Promise((done) => {
                    resolve = done
                }),
        )
        const refresh = wrapper.vm.refresh()
        await flushPromises()
        expect(wrapper.get('.dependency-diagram').element).toBe(diagram)
        resolve(graph({ nodes: [], edges: [] }))
        await refresh
        await flushPromises()
        expect(wrapper.text()).toContain('No tasks in this dependency scope')
        expect(wrapper.find('.dependency-canvas').exists()).toBe(false)
        expect(wrapper.findAll('button').some((button) => /diagram/.test(button.text()))).toBe(false)
        mocks.graph.mockResolvedValue(graph({ nodes: [], edges: [] }))
        mocks.on.mock.calls.find(([name]) => name === 'task_updated')![1]({ data: '{}' })
        await vi.advanceTimersByTimeAsync(120)
        await flushPromises()
        expect(wrapper.find('.dependency-diagram').exists()).toBe(false)
        mocks.graph.mockResolvedValue(graph({ nodes: [node('DEV-8')], edges: [] }))
        await wrapper.vm.refresh()
        await flushPromises()
        expect(wrapper.find('.dependency-diagram').exists()).toBe(true)
        expect(wrapper.get('.dependency-diagram').attributes('style')).toContain('min-height: min(0px')
        wrapper.unmount()
    })
    it('shows four prerequisite stages within two undirected hops of the third task and explains the distinction', async () => {
        vi.stubGlobal(
            'matchMedia',
            vi.fn(() => ({ matches: true })),
        )
        mocks.graph.mockResolvedValue(
            graph({
                nodes: [1, 2, 3, 4].map((id) => node(`DEV-${id}`)),
                edges: [edge('DEV-1', 'DEV-2'), edge('DEV-2', 'DEV-3'), edge('DEV-3', 'DEV-4')],
            }),
        )
        const wrapper = mount(DependencyGraph, { props: { root: 'DEV-3', depth: 2 } })
        await flushPromises()
        expect(visibleIds(wrapper)).toEqual(['DEV-1', 'DEV-2', 'DEV-3', 'DEV-4'])
        expect(wrapper.findAll('.dependency-stages span').map((stage) => stage.text())).toEqual([
            'Stage 1',
            'Stage 2',
            'Stage 3',
            'Stage 4',
        ])
        expect(wrapper.get('.dependency-depth-help').text()).toContain('shortest undirected hops')
        expect(wrapper.get('.dependency-depth-help').text()).toContain('longest path in the returned subgraph')
        expect(wrapper.get('.dependency-stages').attributes('title')).toContain('not hop distance')
        wrapper.unmount()
    })
    it('presents visible problems as severity-marked cards with task actions and repair guidance', async () => {
        mocks.graph.mockResolvedValue(
            graph({
                issues: [
                    {
                        code: 'dependency_cycle',
                        severity: 'error',
                        message: 'These tasks require one another.',
                        tasks: ['DEV-8', 'OTHER-2'],
                        repairable: false,
                    },
                    {
                        code: 'missing_mirror',
                        severity: 'warning',
                        message: 'The reciprocal stored link is missing.',
                        tasks: ['DEV-8'],
                        repairable: true,
                    },
                ],
            }),
        )
        const activate = vi.fn()
        const wrapper = mount(DependencyGraph, { props: { onActivate: activate } })
        await flushPromises()
        const cards = wrapper.findAll('.dependency-problem-list > li')
        expect(cards).toHaveLength(2)
        expect(cards[0]!.classes()).toContain('dependency-problem--error')
        expect(cards[0]!.get('header').text()).toContain('Circular dependency')
        expect(cards[0]!.get('.dependency-problem-severity').text()).toBe('Error')
        expect(cards[0]!.get('p').text()).toBe('These tasks require one another.')
        expect(cards[0]!.text()).toContain('Manual review')
        expect(cards[1]!.text()).toContain('Missing reciprocal link')
        expect(cards[1]!.text()).toContain('Warning')
        expect(cards[1]!.text()).toContain('Safe fix available')
        expect(wrapper.find('.dependency-problem-list button').exists()).toBe(false)
        await cards[1]!.get('a[href="/task/DEV-8"]').trigger('click')
        expect(activate).toHaveBeenCalledExactlyOnceWith('DEV-8')
        wrapper.unmount()
    })
    it('contains many-stage intrinsic widths at every component grid boundary', () => {
        for (const selector of ['dependency-view', 'dependency-render']) {
            expect(graphSource).toMatch(
                new RegExp(
                    `\\.${selector} \\{[^}]*grid-template-columns: minmax\\(0, 1fr\\);\\s*min-width: 0;\\s*max-width: 100%`,
                ),
            )
        }
        expect(graphSource).toMatch(
            /\.dependency-diagram \{[^}]*width: fit-content;\s*min-width: 0;\s*max-width: 100%;[^}]*overflow: auto/,
        )
        expect(graphSource).not.toContain('width: max-content')
    })
    it('keeps same-query refresh visible without a flash and ignores completion after unmount', async () => {
        vi.useFakeTimers()
        vi.stubGlobal(
            'matchMedia',
            vi.fn(() => ({ matches: true })),
        )
        const onLoaded = vi.fn()
        const wrapper = mount(DependencyGraph, { props: { onLoaded } })
        await flushPromises()
        const diagram = wrapper.get('.dependency-diagram').element
        let resolve!: (value: RelationshipGraphResponse) => void
        mocks.graph.mockImplementationOnce(
            () =>
                new Promise((done) => {
                    resolve = done
                }),
        )
        mocks.on.mock.calls.find(([name]) => name === 'task_updated')![1]({ data: '{}' })
        await vi.advanceTimersByTimeAsync(120)
        await flushPromises()
        expect(wrapper.get('.dependency-diagram').element).toBe(diagram)
        expect(wrapper.get('.dependency-render').attributes('inert')).toBeUndefined()
        expect(wrapper.find('.dependency-count').exists()).toBe(true)
        expect(wrapper.text()).not.toContain('Previous dependency view')
        const loaded = onLoaded.mock.calls.length
        wrapper.unmount()
        resolve(graph({ revision: 'late' }))
        await flushPromises()
        expect(onLoaded).toHaveBeenCalledTimes(loaded)
        expect(mocks.close).toHaveBeenCalledOnce()
        await vi.advanceTimersByTimeAsync(120)
        expect(mocks.graph).toHaveBeenCalledTimes(2)
    })
    it('hides enabled-kind isolates by default, preserves diagnostics and exact typed matches, and includes only matching isolates on opt-in', async () => {
        mocks.graph.mockResolvedValue(
            graph({
                nodes: [
                    node('DEV-8'),
                    node('OTHER-2'),
                    node('DEV-9'),
                    node('DEV-3', 'deleted'),
                    node('DEV-4', 'missing'),
                    { ...node('CONTEXT-1'), matches_filter: false },
                ],
                edges: [edge('DEV-8', 'OTHER-2', 'related')],
            }),
        )
        const wrapper = mount(DependencyGraph, { props: { relationshipKinds: [] } })
        await flushPromises()
        expect(visibleIds(wrapper)).toEqual(['DEV-4'])
        await wrapper.setProps({ showIsolated: true })
        await flushPromises()
        expect(visibleIds(wrapper)).toEqual(['DEV-3', 'DEV-4', 'DEV-8', 'DEV-9', 'OTHER-2'])
        await wrapper.setProps({ showIsolated: false, filters: { q: 'DEV0009' } })
        await flushPromises()
        expect(visibleIds(wrapper)).toEqual(['DEV-4', 'DEV-9'])
        await wrapper.setProps({ filters: {}, relationshipKinds: ['related'] })
        await flushPromises()
        expect(visibleIds(wrapper)).toEqual(['DEV-4', 'DEV-8', 'OTHER-2'])
        expect(mocks.graph.mock.calls.every(([query]) => !('showIsolated' in query))).toBe(true)
        wrapper.unmount()
    })
    it('retains accepted geometry inertly during delayed roots, ignores stale replies, and keeps viewport height without blank canvas extents', async () => {
        vi.stubGlobal(
            'matchMedia',
            vi.fn(() => ({ matches: true })),
        )
        const activate = vi.fn()
        const wrapper = mount(DependencyGraph, { props: { root: 'DEV-8', onActivate: activate } })
        await flushPromises()
        const diagram = wrapper.get('.dependency-diagram').element as HTMLElement
        vi.spyOn(diagram, 'getBoundingClientRect').mockReturnValue({ height: 600 } as DOMRect)
        Object.defineProperties(diagram, { clientWidth: { value: 400 }, clientHeight: { value: 600 } })
        diagram.scrollLeft = 90
        diagram.scrollTop = 120
        const before = wrapper.get('.dependency-canvas').attributes('style')
        let resolveB!: (value: RelationshipGraphResponse) => void
        let resolveC!: (value: RelationshipGraphResponse) => void
        mocks.graph.mockImplementationOnce(
            () =>
                new Promise((resolve) => {
                    resolveB = resolve
                }),
        )
        await wrapper.setProps({ root: 'DEV-9' })
        expect(wrapper.get('.dependency-canvas').attributes('style')).toBe(before)
        expect(wrapper.get('.dependency-diagram').element).toBe(diagram)
        expect(diagram.scrollTop).toBe(120)
        expect(diagram.scrollLeft).toBe(90)
        expect(wrapper.get('.dependency-render').attributes('inert')).toBeDefined()
        expect(wrapper.find('.dependency-count').exists()).toBe(false)
        expect(wrapper.text()).toContain('Focal task: DEV-8')
        await wrapper.get('.dependency-task-list a').trigger('click')
        expect(activate).not.toHaveBeenCalled()
        mocks.graph.mockImplementationOnce(
            () =>
                new Promise((resolve) => {
                    resolveC = resolve
                }),
        )
        await wrapper.setProps({ root: 'DEV-10' })
        resolveB(graph({ nodes: [node('DEV-9')], edges: [] }))
        await flushPromises()
        expect(wrapper.text()).toContain('Focal task: DEV-8')
        resolveC(graph({ nodes: [node('DEV-10')], edges: [] }))
        await flushPromises()
        expect(wrapper.get('.dependency-diagram').element).toBe(diagram)
        expect(wrapper.get('.dependency-render').attributes('inert')).toBeUndefined()
        expect(wrapper.get('.dependency-count').text()).toContain('1 focal task')
        expect(wrapper.get('.dependency-diagram').attributes('style')).toContain(
            'min-height: min(600px, 82dvh, 1000px)',
        )
        expect(wrapper.get('.dependency-canvas').attributes('style')).not.toContain('min-height')
        expect(wrapper.get('.dependency-canvas').attributes('style')).not.toContain('min-width')
        wrapper.unmount()
    })
    it('does not add the stage header to retained canvas height on every refresh', async () => {
        vi.stubGlobal(
            'matchMedia',
            vi.fn(() => ({ matches: true })),
        )
        mocks.graph.mockImplementation(async () => graph())
        const wrapper = mount(DependencyGraph)
        await flushPromises()
        const diagram = wrapper.get('.dependency-diagram').element as HTMLElement
        const canvas = wrapper.get('.dependency-canvas').element as HTMLElement
        const header = wrapper.get('.dependency-stages').element as HTMLElement
        Object.defineProperty(header, 'offsetHeight', { get: () => 28 })
        Object.defineProperties(diagram, {
            clientWidth: { get: () => 400 },
            clientHeight: { get: () => Math.max(200, parseFloat(canvas.style.minHeight) || 0) + header.offsetHeight },
        })
        vi.spyOn(diagram, 'getBoundingClientRect').mockImplementation(
            () => ({ height: diagram.clientHeight + 2 }) as DOMRect,
        )
        const initial = diagram.clientHeight
        for (let index = 0; index < 5; index++) {
            await (wrapper.vm as unknown as { refresh: () => Promise<void> }).refresh()
            await flushPromises()
            expect(diagram.clientHeight).toBe(initial)
        }
        expect(canvas.style.minHeight).toBe('')
        wrapper.unmount()
    })
    it('emits activation instead of opening a legacy preview when handled', async () => {
        const activate = vi.fn()
        const wrapper = mount(DependencyGraph, { props: { onActivate: activate } })
        await flushPromises()
        const link = wrapper.get('.dependency-task-list a[href="/task/DEV-8"]')
        link.element.addEventListener('click', (event) => {
            if ((event as MouseEvent).metaKey) {
                expect(event.defaultPrevented).toBe(false)
                event.preventDefault() // Suppress jsdom's unimplemented native document navigation.
            }
        })
        await link.trigger('click', { metaKey: true })
        expect(activate).not.toHaveBeenCalled()
        await link.trigger('click')
        expect(activate).toHaveBeenCalledExactlyOnceWith('DEV-8')
        expect(mocks.openTaskPanel).not.toHaveBeenCalled()
        wrapper.unmount()
    })
    it('bounds responsive card grids, wraps compact metadata, and allows a tall viewport', () => {
        expect(graphSource).toMatch(/max-height:\s*min\(82dvh, 1000px\)/)
        expect(graphSource).not.toContain('max-height: 500px')
        expect(graphSource).toContain('grid-template-columns: repeat(auto-fill, minmax(min(100%, 17rem), 1fr))')
        expect(graphSource).toMatch(
            /\.dependency-task-list > li\s*\{[^}]*width: 100%;\s*max-width: 20rem;\s*box-sizing: border-box/,
        )
        expect(graphSource).toMatch(/\.dependency-task-list > li\s*\{[^}]*display: grid/)
        expect(graphSource).toMatch(
            /\.dependency-edge-list\s*\{[^}]*grid-template-columns: repeat\(auto-fill, minmax\(min\(100%, 18rem\), 1fr\)\)/,
        )
        expect(graphSource).toMatch(/\.dependency-edge-list\s*\{[^}]*width: 100%;\s*max-width: 100%/)
        expect(graphSource).toMatch(/\.dependency-edge-list > li\s*\{[^}]*flex-wrap: wrap/)
        expect(graphSource).toMatch(
            /\.dependency-edge-list > li\s*\{[^}]*width: fit-content;\s*max-width: 100%;\s*box-sizing: border-box/,
        )
        expect(graphSource).toMatch(
            /\.dependency-edge-list a[^}]*min-width: 0;\s*max-width: 100%;[^}]*overflow-wrap: anywhere/,
        )
        expect(graphSource).toMatch(
            /\.dependency-relationship-chip\s*\{[^}]*flex-wrap: wrap;\s*max-width: 100%;\s*box-sizing: border-box/,
        )
        expect(cardSource).toContain('BoardCardMeta')
        expect(cardSource).not.toContain('TaskHoverCardBody')
        expect(cardSource).toMatch(/:deep\(\.row\)\s*\{[^}]*flex-wrap: wrap/)
    })
    it.each(['DEV-8', 'DEV-0008', 'DEV8', 'DEV0008', '8', '0008'])(
        'highlights a typed exact ID without navigation: %s',
        async (q) => {
            vi.stubGlobal(
                'matchMedia',
                vi.fn(() => ({ matches: true })),
            )
            const push = vi.spyOn(window.history, 'pushState')
            const replace = vi.spyOn(window.history, 'replaceState')
            const wrapper = mount(DependencyGraph, { props: { filters: { q } } })
            await flushPromises()
            expect(wrapper.get('.dependency-task-list [aria-current="true"]').attributes('data-task-id')).toBe('DEV-8')
            expect(wrapper.get('.dependency-node[aria-current="true"]').attributes('data-task-id')).toBe('DEV-8')
            expect(mocks.openTaskPanel).not.toHaveBeenCalled()
            expect(push).not.toHaveBeenCalled()
            expect(replace).not.toHaveBeenCalled()
            await wrapper.setProps({ filters: { q: 'DEV-8x' } })
            await flushPromises()
            expect(wrapper.findAll('[aria-current="true"]')).toHaveLength(0)
            wrapper.unmount()
            push.mockRestore()
            replace.mockRestore()
        },
    )
    it('does not guess numeric homonyms, honors project identity, and preserves manual selection on refresh', async () => {
        mocks.graph.mockResolvedValue(
            graph({ nodes: [node('DEV-8'), { ...node('OTHER-8'), project: 'OTHER' }], edges: [] }),
        )
        const wrapper = mount(DependencyGraph, { props: { showIsolated: true, filters: { q: '8' } } })
        await flushPromises()
        expect(wrapper.findAll('[aria-current="true"]')).toHaveLength(0)
        await wrapper.setProps({ project: 'DEV' })
        await flushPromises()
        expect(wrapper.get('[aria-current="true"]').attributes('data-task-id')).toBe('DEV-8')
        await wrapper
            .findAll('button')
            .find((button) => button.text() === 'Highlight OTHER-8')!
            .trigger('click')
        await wrapper.vm.refresh()
        await flushPromises()
        expect(wrapper.get('[aria-current="true"]').attributes('data-task-id')).toBe('OTHER-8')
        await wrapper.setProps({ filters: { q: 'dev8' } })
        await flushPromises()
        expect(wrapper.findAll('[aria-current="true"]')).toHaveLength(0)
        await wrapper.setProps({ filters: { q: 'DEV8' } })
        await flushPromises()
        expect(wrapper.get('[aria-current="true"]').attributes('data-task-id')).toBe('DEV-8')
        wrapper.unmount()
    })
    it('keeps independent matches but excludes context reached only by hidden kinds during broad search', async () => {
        mocks.graph.mockResolvedValue(
            graph({
                nodes: [
                    node('DEV-8'),
                    node('INDEPENDENT-1'),
                    { ...node('BLOCKER-1'), matches_filter: false },
                    { ...node('RELATED-1'), matches_filter: false },
                ],
                edges: [
                    { source: 'BLOCKER-1', target: 'DEV-8', kind: 'dependency', provenance: [] },
                    { source: 'DEV-8', target: 'RELATED-1', kind: 'related', provenance: [] },
                ],
            }),
        )
        const wrapper = mount(DependencyGraph, {
            props: { showIsolated: true, filters: { q: 'search' }, relationshipKinds: ['dependency'] },
        })
        await flushPromises()
        expect(
            wrapper
                .findAll('[aria-label="Dependency tasks"] > li')
                .map((entry) => entry.attributes('data-task-id'))
                .sort(),
        ).toEqual(['BLOCKER-1', 'DEV-8', 'INDEPENDENT-1'])
        expect(wrapper.get('[aria-label="Relationship legend"]').text()).toContain('blocks')
        expect(wrapper.get('[aria-label="Relationship legend"]').text()).not.toContain('related')
        await wrapper.setProps({ relationshipKinds: [] })
        await flushPromises()
        expect(
            wrapper
                .findAll('[aria-label="Dependency tasks"] > li')
                .map((entry) => entry.attributes('data-task-id'))
                .sort(),
        ).toEqual(['DEV-8', 'INDEPENDENT-1'])
        expect(wrapper.find('[aria-label="Relationship legend"]').exists()).toBe(false)
        wrapper.unmount()
    })
    it('refetches explicit independent relationship kinds, including none, before the server cap', async () => {
        mocks.graph.mockResolvedValue(dev54Graph())
        const wrapper = mount(DependencyGraph, { props: { root: 'DEV54', relationshipKinds: ['related'] } })
        await flushPromises()
        expect(visibleIds(wrapper)).toEqual(['DEV-23', 'DEV-28', 'DEV-54'])
        expect(wrapper.findAll('input')).toHaveLength(0)
        await wrapper.setProps({ relationshipKinds: ['hierarchy'] })
        await flushPromises()
        expect(visibleIds(wrapper)).toEqual(['DEV-53', 'DEV-54'])
        await wrapper.setProps({ relationshipKinds: [] })
        await flushPromises()
        expect(visibleIds(wrapper)).toEqual(['DEV-54'])
        expect(wrapper.findAll('.dependency-edge-list li')).toHaveLength(0)
        expect(mocks.graph).toHaveBeenCalledTimes(3)
        expect(mocks.graph).toHaveBeenLastCalledWith(
            expect.objectContaining({ relationship_kinds: [], include_isolated: false }),
        )
        wrapper.unmount()
    })
    it('keeps all-off cards accessible without silently restoring fields', async () => {
        const fields = Object.fromEntries(
            [
                'id',
                'title',
                'status',
                'priority',
                'task_type',
                'assignee',
                'reporter',
                'effort',
                'tags',
                'sprints',
                'due_date',
                'modified',
            ].map((key) => [key, false]),
        )
        const wrapper = mount(DependencyGraph, { props: { fields, showIsolated: true } })
        await flushPromises()
        expect(wrapper.get('.dependency-task-list a[href="/task/DEV-8"]').text()).toBe('DEV-8')
        expect(wrapper.get('.dependency-task-list a[href="/task/DEV-8"]').attributes('aria-label')).toContain('DEV-8')
        expect(wrapper.find('.task-hover-card__header').exists()).toBe(false)
        expect(wrapper.get('.dependency-task-list').text()).not.toContain('Todo')
        wrapper.unmount()
    })
    it('toggles every compact summary field on visible cards with no duplicate headers', async () => {
        const fields = Object.fromEntries(
            [
                'id',
                'title',
                'status',
                'priority',
                'task_type',
                'assignee',
                'reporter',
                'effort',
                'tags',
                'sprints',
                'due_date',
                'modified',
                'custom:area',
            ].map((key) => [key, false]),
        )
        mocks.graph.mockResolvedValue(
            graph({
                nodes: [
                    {
                        ...node('DEV-8'),
                        task: {
                            id: 'DEV-8',
                            title: 'Inline title',
                            status: 'Todo',
                            priority: 'High',
                            task_type: 'Bug',
                            assignee: 'Alice',
                            reporter: 'Bob',
                            effort: '3h',
                            tags: ['backend'],
                            sprints: [7],
                            due_date: '2026-10-22',
                            created: '',
                            modified: '2026-10-22',
                            custom_fields: { area: 'Storage' },
                        },
                    },
                ],
                edges: [],
            }),
        )
        const wrapper = mount(DependencyGraph, { props: { fields, showIsolated: true } })
        await flushPromises()
        const cases = {
            id: 'DEV-8',
            title: 'Inline title',
            status: 'Todo',
            priority: 'High',
            assignee: 'Alice',
            reporter: 'Bob',
            effort: '3h',
            tags: 'backend',
            sprints: '#7',
            due_date: 'Due',
            modified: 'Updated',
            'custom:area': 'Storage',
        }
        for (const [key, expected] of Object.entries(cases)) {
            await wrapper.setProps({ fields: { ...fields, [key]: true } })
            expect(wrapper.get('.dependency-card-details').text()).toContain(expected)
            expect(wrapper.findAll('.task-header').length).toBeLessThanOrEqual(1)
            expect(wrapper.find('dl').exists()).toBe(false)
            expect(wrapper.find('[role="tooltip"]').exists()).toBe(false)
        }
        expect(mocks.graph).toHaveBeenCalledOnce()
        expect(mocks.getTask).not.toHaveBeenCalled()
        wrapper.unmount()
    })
    it('labels all canonical edge directions and separates parallel and reverse paths', async () => {
        vi.stubGlobal(
            'matchMedia',
            vi.fn(() => ({ matches: true })),
        )
        mocks.graph.mockResolvedValue(
            graph({
                edges: ['dependency', 'hierarchy', 'related', 'fixes', 'duplicate']
                    .map((kind) => edge('DEV-8', 'OTHER-2', kind as Parameters<typeof edge>[2]))
                    .concat(edge('OTHER-2', 'DEV-8')),
            }),
        )
        const wrapper = mount(DependencyGraph, { props: { type: 'all' } })
        await flushPromises()
        const paths = wrapper.findAll('g.dependency-edge > path')
        expect(new Set(paths.map((path) => path.attributes('d'))).size).toBe(6)
        expect(wrapper.findAll('g.dependency-edge > text')).toHaveLength(0)
        expect(wrapper.findAll('g.dependency-edge > title').map((text) => text.text())).toContain(
            'DEV-8 parent of OTHER-2',
        )
        expect(wrapper.text()).not.toContain('Arrowheads show direction; related links have no arrow.')
        for (const label of ['blocks', 'parent of', 'related', 'fixes', 'duplicate of'])
            expect(wrapper.get('[aria-label="Relationship legend"]').text()).toContain(label)
        expect(wrapper.get('.dependency-edge-list').text()).toContain('DEV-8 parent of OTHER-2')
        expect(wrapper.get('.dependency-edge-list').text()).toContain('OTHER-2 blocks DEV-8')
        expect(wrapper.get('g.dependency-edge--related > path').attributes('marker-end')).toBeUndefined()
        expect(new Set(paths.map((path) => path.attributes('marker-end'))).size).toBe(5)
        wrapper.unmount()
    })
    it('refreshes publicly without remounting, losing scroll, highlight, or diagram choice', async () => {
        const wrapper = mount(DependencyGraph, { props: { root: 'DEV-8' } })
        await flushPromises()
        await wrapper
            .findAll('button')
            .find((button) => button.text() === 'Show diagram')!
            .trigger('click')
        await wrapper.get('.dependency-diagram a[href="/task/OTHER-2"]').trigger('click')
        const diagram = wrapper.get('.dependency-diagram').element
        diagram.scrollTop = 37
        diagram.scrollLeft = 23
        await wrapper.setProps({ fields: { priority: false }, relationshipKinds: ['dependency'] })
        await wrapper.vm.refresh()
        await flushPromises()
        expect(wrapper.get('.dependency-diagram').element).toBe(diagram)
        expect(diagram.scrollTop).toBe(37)
        expect(diagram.scrollLeft).toBe(23)
        expect(wrapper.get('.dependency-task-list [aria-current="true"]').attributes('data-task-id')).toBe('OTHER-2')
        expect(mocks.graph).toHaveBeenCalledTimes(3)
        expect(mocks.close).not.toHaveBeenCalled()
        await wrapper.setProps({ sortBy: 'title', filters: { status: 'Todo' } })
        await flushPromises()
        expect(wrapper.get('.dependency-diagram').element.scrollTop).toBe(37)
        expect(wrapper.get('.dependency-diagram').element.scrollLeft).toBe(23)
        expect(wrapper.get('.dependency-task-list [aria-current="true"]').attributes('data-task-id')).toBe('OTHER-2')
        expect(mocks.close).not.toHaveBeenCalled()
        wrapper.unmount()
    })
    it('remeasures growing and shrinking cards and anchors edges at measured centers', async () => {
        let resize!: ResizeObserverCallback
        vi.stubGlobal(
            'ResizeObserver',
            class {
                constructor(callback: ResizeObserverCallback) {
                    resize = callback
                }
                observe() {}
                unobserve() {}
                disconnect() {}
            },
        )
        vi.stubGlobal(
            'matchMedia',
            vi.fn(() => ({ matches: true })),
        )
        mocks.graph.mockResolvedValue(
            graph({ nodes: ['A', 'B', 'C'].map((id) => node(id)), edges: [edge('A', 'B'), edge('A', 'C')] }),
        )
        const wrapper = mount(DependencyGraph)
        await flushPromises()
        const b = wrapper.get('.dependency-node[data-task-id="B"]').element
        const measure = async (height: number) => {
            vi.spyOn(b, 'getBoundingClientRect').mockReturnValue({ height } as DOMRect)
            resize([{ target: b } as ResizeObserverEntry], {} as ResizeObserver)
            await flushPromises()
        }
        await measure(760)
        const top = (id: string) =>
            Number.parseFloat((wrapper.get(`.dependency-node[data-task-id="${id}"]`).element as HTMLElement).style.top)
        expect(top('C')).toBeGreaterThan(top('B') + 760)
        expect(wrapper.get('g.dependency-edge > path').attributes('d')).toContain(`${top('B') + 380}`)
        await wrapper.setProps({ fields: { title: false } })
        await measure(84)
        expect(top('C')).toBe(top('B') + 84 + 32)
        wrapper.unmount()
    })
    it('projects the DEV54 inventory through enabled relationships and initially highlights only its focal task', async () => {
        const response = dev54Graph()
        mocks.graph.mockResolvedValue(response)
        const wrapper = mount(DependencyGraph, { props: { root: 'DEV54' } })
        await flushPromises()
        expect(visibleIds(wrapper)).toEqual(['DEV-54', 'DEV-55', 'DEV-59', 'DEV-60', 'DEV-64', 'DEV-82'])
        expect(wrapper.get('.dependency-count').text()).toBe('1 focal task, 5 connected tasks, 5 links')
        expect(
            wrapper.findAll('.dependency-task-list [aria-current="true"]').map((row) => row.attributes('data-task-id')),
        ).toEqual(['DEV-54'])
        expect(wrapper.get('.dependency-task-list [data-focused="true"]').text()).toContain('Focal task')
        expect(mocks.openTaskPanel).not.toHaveBeenCalled()
        await wrapper.findAll('input')[0]!.setValue(true)
        await flushPromises()
        expect(visibleIds(wrapper)).toEqual([
            'DEV-23',
            'DEV-28',
            'DEV-54',
            'DEV-55',
            'DEV-59',
            'DEV-60',
            'DEV-64',
            'DEV-82',
        ])
        await wrapper.findAll('input')[1]!.setValue(true)
        await flushPromises()
        expect(visibleIds(wrapper)).toHaveLength(9)
        await wrapper.findAll('input')[0]!.setValue(false)
        await flushPromises()
        expect(visibleIds(wrapper)).not.toContain('DEV-23')
        expect(visibleIds(wrapper)).toContain('DEV-53')
        expect(response.nodes).toHaveLength(9)
        expect(wrapper.emitted('loaded')?.[0]).toEqual([response])
        wrapper.unmount()
    })
    it('bounds enabled-path reachability, ignoring disabled shortcuts and preserving project isolates', async () => {
        mocks.graph.mockResolvedValue(
            graph({
                nodes: ['P-1', 'P-2', 'P-3', 'P-4', 'P-5'].map((id) => node(id)),
                edges: [edge('P-1', 'P-2'), edge('P-2', 'P-3'), edge('P-1', 'P-3', 'related'), edge('P-3', 'P-4')],
            }),
        )
        const wrapper = mount(DependencyGraph, { props: { root: 'P-1', depth: 1 } })
        await flushPromises()
        expect(visibleIds(wrapper)).toEqual(['P-1', 'P-2'])
        await wrapper.setProps({ depth: 2 })
        await flushPromises()
        expect(visibleIds(wrapper)).toEqual(['P-1', 'P-2', 'P-3'])
        expect(wrapper.findAll('.dependency-edge-list li')).toHaveLength(2)
        await wrapper.findAll('input')[0]!.setValue(true)
        await flushPromises()
        expect(visibleIds(wrapper)).toEqual(['P-1', 'P-2', 'P-3', 'P-4'])
        await wrapper.setProps({ depth: 0 })
        await flushPromises()
        expect(visibleIds(wrapper)).toEqual(['P-1'])
        expect(wrapper.findAll('.dependency-edge-list li')).toHaveLength(0)
        await wrapper.setProps({ root: '', project: 'P', showIsolated: true })
        await flushPromises()
        expect(visibleIds(wrapper)).toEqual(['P-1', 'P-2', 'P-3', 'P-4', 'P-5'])
        expect(wrapper.findAll('[aria-current="true"]')).toHaveLength(0)
        wrapper.unmount()
    })
    it.each(['DEV54', 'DEV0054', 'DEV-0054', 'DEV-54'])(
        'resolves exact-case canonical and padded aliases: %s',
        async (root) => {
            mocks.graph.mockResolvedValue(dev54Graph())
            const wrapper = mount(DependencyGraph, { props: { root } })
            await flushPromises()
            expect(wrapper.get('.dependency-task-list [data-focused="true"]').attributes('data-task-id')).toBe('DEV-54')
            expect(mocks.graph).toHaveBeenCalledWith(expect.objectContaining({ task_id: root }))
            wrapper.unmount()
        },
    )
    it.each(['dev54', 'DEV-999', ' DEV54', 'DEV54junk'])(
        'does not guess a missing or wrong-case focus: %s',
        async (root) => {
            mocks.graph.mockResolvedValue(dev54Graph())
            const wrapper = mount(DependencyGraph, { props: { root } })
            await flushPromises()
            expect(visibleIds(wrapper)).toEqual([])
            expect(wrapper.findAll('[aria-current="true"]')).toHaveLength(0)
            expect(wrapper.text()).toContain('could not be uniquely identified')
            wrapper.unmount()
        },
    )
    it.each(['missing', 'unreadable', 'ambiguous'] as const)(
        'keeps an explicit %s focal placeholder without opening it',
        async (state) => {
            mocks.graph.mockResolvedValue(graph({ nodes: [node('DEV-54', state)], edges: [] }))
            const wrapper = mount(DependencyGraph, { props: { root: 'DEV54' } })
            await flushPromises()
            expect(visibleIds(wrapper)).toEqual(['DEV-54'])
            expect(wrapper.get('.dependency-task-list [aria-current="true"]').attributes('data-task-id')).toBe('DEV-54')
            expect(wrapper.findAll('.dependency-task-list a')).toHaveLength(0)
            expect(mocks.openTaskPanel).not.toHaveBeenCalled()
            wrapper.unmount()
        },
    )
    it('rejects multiple alias candidates rather than selecting multiple tasks', async () => {
        mocks.graph.mockResolvedValue(graph({ nodes: [node('DEV-54'), node('DEV-0054')], edges: [] }))
        const wrapper = mount(DependencyGraph, { props: { root: 'DEV54' } })
        await flushPromises()
        expect(visibleIds(wrapper)).toEqual([])
        expect(wrapper.findAll('[aria-current="true"]')).toHaveLength(0)
        wrapper.unmount()
    })
    it('keeps one highlight across previews, hidden neighbors, SSE and scope changes', async () => {
        vi.useFakeTimers()
        mocks.graph.mockResolvedValue(dev54Graph())
        const wrapper = mount(DependencyGraph, { props: { root: 'DEV54' } })
        await flushPromises()
        await wrapper.findAll('input')[0]!.setValue(true)
        await flushPromises()
        await wrapper
            .findAll('button')
            .find((button) => button.text() === 'Highlight DEV-23')!
            .trigger('click')
        expect(
            wrapper.findAll('.dependency-task-list [aria-current="true"]').map((row) => row.attributes('data-task-id')),
        ).toEqual(['DEV-23'])
        await wrapper.findAll('input')[0]!.setValue(false)
        await flushPromises()
        expect(wrapper.get('.dependency-task-list [aria-current="true"]').attributes('data-task-id')).toBe('DEV-54')
        await wrapper.get('a[href="/task/DEV-55"]').trigger('click')
        expect(mocks.openTaskPanel).toHaveBeenCalledExactlyOnceWith({ taskId: 'DEV-55' })
        expect(wrapper.get('.dependency-task-list [aria-current="true"]').attributes('data-task-id')).toBe('DEV-55')
        mocks.on.mock.calls.find(([name]) => name === 'task_updated')![1]({ data: '{}' })
        await vi.advanceTimersByTimeAsync(120)
        await flushPromises()
        expect(wrapper.get('.dependency-task-list [data-focused="true"]').attributes('data-task-id')).toBe('DEV-54')
        expect(wrapper.findAll('.dependency-task-list [aria-current="true"]')).toHaveLength(1)
        await wrapper.setProps({ root: 'DEV55' })
        await flushPromises()
        expect(wrapper.get('.dependency-task-list [aria-current="true"]').attributes('data-task-id')).toBe('DEV-55')
        expect(mocks.openTaskPanel).toHaveBeenCalledTimes(1)
        wrapper.unmount()
    })
    it('defaults isolated desktop tasks to the list and preserves an explicit diagram choice on SSE', async () => {
        vi.useFakeTimers()
        vi.stubGlobal(
            'matchMedia',
            vi.fn(() => ({ matches: true })),
        )
        mocks.graph.mockResolvedValue(graph({ edges: [] }))
        const wrapper = mount(DependencyGraph, { props: { showIsolated: true } })
        await flushPromises()
        expect(wrapper.find('.dependency-diagram').exists()).toBe(false)
        expect(wrapper.text()).toContain('No connections in these layers')
        expect((wrapper.get('.dependency-lists').element as HTMLDetailsElement).open).toBe(true)
        await wrapper
            .findAll('button')
            .find((button) => button.text() === 'Show diagram')!
            .trigger('click')
        const callback = mocks.on.mock.calls.find(([name]) => name === 'task_updated')![1]
        callback({ data: '{}' })
        await vi.advanceTimersByTimeAsync(120)
        await flushPromises()
        expect(wrapper.find('.dependency-diagram').exists()).toBe(true)
        wrapper.unmount()
    })
    it('defaults desktop connected graphs to diagrams including related-only connections', async () => {
        vi.stubGlobal(
            'matchMedia',
            vi.fn(() => ({ matches: true })),
        )
        mocks.graph.mockResolvedValue(
            graph({ edges: [{ source: 'DEV-8', target: 'OTHER-2', kind: 'related', provenance: [] }] }),
        )
        const wrapper = mount(DependencyGraph, { props: { type: 'related' } })
        await flushPromises()
        expect(wrapper.find('.dependency-diagram').exists()).toBe(true)
        await wrapper.setProps({ type: 'dependency' })
        await flushPromises()
        expect(wrapper.find('.dependency-diagram').exists()).toBe(false)
        expect(wrapper.text()).toContain('No tasks in this dependency scope')
        wrapper.unmount()
    })
    it('preserves native modified and middle link actions in list and diagram', async () => {
        const wrapper = mount(DependencyGraph)
        await flushPromises()
        await wrapper
            .findAll('button')
            .find((button) => button.text() === 'Show diagram')!
            .trigger('click')
        for (const selector of ['[aria-label="Dependency tasks"] a', '.dependency-diagram a']) {
            const link = wrapper.get(selector)
            for (const options of [
                { ctrlKey: true },
                { metaKey: true },
                { shiftKey: true },
                { altKey: true },
                { button: 1 },
            ]) {
                const event = new MouseEvent('click', { bubbles: true, cancelable: true, ...options })
                let intercepted = true
                link.element.addEventListener(
                    'click',
                    (event) => {
                        intercepted = event.defaultPrevented
                        // Observe the component before suppressing jsdom's unsupported navigation.
                        event.preventDefault()
                    },
                    { once: true },
                )
                link.element.dispatchEvent(event)
                expect(intercepted).toBe(false)
            }
            expect(link.attributes('href')).toMatch(/^\/task\//)
        }
        expect(mocks.openTaskPanel).not.toHaveBeenCalled()
        wrapper.unmount()
    })
    it('forwards authoritative filters, emits returned metadata and keeps nonmatching blockers visible', async () => {
        const response = graph({ nodes: [node('DEV-8'), { ...node('OTHER-2'), matches_filter: false }] })
        mocks.graph.mockResolvedValueOnce(response)
        const wrapper = mount(DependencyGraph, { props: { filters: { status: 'Todo' } } })
        await flushPromises()
        expect(mocks.graph).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ filters: { status: 'Todo' } }))
        expect(wrapper.emitted('loaded')?.[0]).toEqual([response])
        expect(wrapper.get('[aria-label="Dependency tasks"]').text()).toContain('Context: outside filters')
        expect(wrapper.text()).toContain('OTHER-2')
        expect(wrapper.findAll('button').some((button) => /refresh|reload|check/i.test(button.text()))).toBe(false)
        wrapper.unmount()
    })
    it('refetches with sorting before the server node cap and forwards invalid explicit ordering for rejection', async () => {
        const wrapper = mount(DependencyGraph, {
            props: { filters: { status: 'Todo' }, sortBy: 'title', order: 'asc', maxNodes: 100 },
        })
        await flushPromises()
        expect(mocks.graph).toHaveBeenLastCalledWith(
            expect.objectContaining({ max_nodes: 100, filters: { status: 'Todo', sort_by: 'title', order: 'asc' } }),
        )
        await wrapper.setProps({ sortBy: 'due-date', order: 'desc' })
        await flushPromises()
        expect(mocks.graph).toHaveBeenLastCalledWith(
            expect.objectContaining({ filters: { status: 'Todo', sort_by: 'due', order: 'desc' } }),
        )
        mocks.graph.mockRejectedValueOnce(new Error('Invalid sort order'))
        await wrapper.setProps({ order: 'sideways' })
        await flushPromises()
        expect(mocks.graph).toHaveBeenLastCalledWith(
            expect.objectContaining({ filters: { status: 'Todo', sort_by: 'due', order: 'sideways' } }),
        )
        expect(wrapper.get('[role="alert"]').text()).toContain('Invalid sort order')
        wrapper.unmount()
    })
    it('renders compact graph metadata and configured fields inline without tooltips or task fetches', async () => {
        mocks.graph.mockResolvedValueOnce(
            graph({
                nodes: [
                    {
                        ...node('DEV-8'),
                        task: {
                            id: 'DEV-8',
                            title: 'Metadata title',
                            status: 'Todo',
                            priority: 'High',
                            task_type: 'Bug',
                            created: '2026-10-01',
                            modified: '2026-10-22',
                            tags: ['backend'],
                            sprints: [7],
                            custom_fields: { area: 'Storage' },
                        },
                    },
                ],
            }),
        )
        const wrapper = mount(DependencyGraph, {
            props: { showIsolated: true, fields: { priority: false, 'custom:area': true } },
        })
        await flushPromises()
        expect(wrapper.get('.dependency-task-list').text()).toContain('Storage')
        expect(wrapper.get('.dependency-task-list').text()).toContain('Metadata title')
        expect(wrapper.get('.dependency-card-details').text()).not.toContain('High')
        expect(wrapper.find('[role="tooltip"]').exists()).toBe(false)
        await wrapper
            .findAll('button')
            .find((button) => button.text() === 'Show diagram')!
            .trigger('click')
        expect(wrapper.get('.dependency-node').text()).toContain('Storage')
        expect(wrapper.get('.dependency-node').findAll('.task-header .title')).toHaveLength(1)
        expect(mocks.graph).toHaveBeenCalledOnce()
        expect(mocks.getTask).not.toHaveBeenCalled()
        wrapper.unmount()
    })
    it('sorts metadata within dependency ranks without reversing prerequisites', async () => {
        const summary = (id: string, title: string) => ({
            ...node(id),
            task: {
                id,
                title,
                status: 'Todo',
                priority: 'High',
                task_type: 'Bug',
                created: '',
                modified: '',
                tags: [],
                sprints: [],
                custom_fields: {},
            },
        })
        mocks.graph.mockResolvedValueOnce(
            graph({
                nodes: [summary('A', 'First'), summary('B', 'Bravo'), summary('C', 'Charlie')],
                edges: [
                    { source: 'A', target: 'B', kind: 'dependency', provenance: [] },
                    { source: 'A', target: 'C', kind: 'dependency', provenance: [] },
                ],
            }),
        )
        const wrapper = mount(DependencyGraph, { props: { sortBy: 'title', order: 'desc' } })
        await flushPromises()
        await wrapper
            .findAll('button')
            .find((button) => button.text() === 'Show diagram')!
            .trigger('click')
        expect(wrapper.findAll('.dependency-node').map((node) => node.get('.id').text())).toEqual(['A', 'C', 'B'])
        expect(wrapper.findAll('.dependency-node').map((node) => node.attributes('data-rank'))).toEqual(['0', '1', '1'])
        wrapper.unmount()
    })
    it('sorts independent tasks in the default list instead of retaining component ID order', async () => {
        const summary = (id: string, title: string) => ({
            ...node(id),
            title,
            task: {
                id,
                title,
                status: 'Todo',
                priority: 'High',
                task_type: 'Bug',
                created: '',
                modified: '',
                tags: [],
                sprints: [],
                custom_fields: {},
            },
        })
        mocks.graph.mockResolvedValue(
            graph({ nodes: [summary('A', 'Zulu'), node('X', 'missing'), summary('B', 'Alpha')], edges: [] }),
        )
        const wrapper = mount(DependencyGraph, { props: { showIsolated: true, sortBy: 'title', order: 'asc' } })
        await flushPromises()
        expect(wrapper.find('.dependency-diagram').exists()).toBe(false)
        expect(wrapper.findAll('[aria-label="Dependency tasks"] li a').map((link) => link.get('.id').text())).toEqual([
            'B',
            'A',
        ])
        expect(wrapper.findAll('[aria-label="Dependency tasks"] > li').slice(-1)[0]!.get('.id').text()).toBe('X')
        await wrapper.setProps({ order: 'desc' })
        await flushPromises()
        expect(wrapper.findAll('[aria-label="Dependency tasks"] li a').map((link) => link.get('.id').text())).toEqual([
            'A',
            'B',
        ])
        expect(wrapper.findAll('[aria-label="Dependency tasks"] > li').slice(-1)[0]!.get('.id').text()).toBe('X')
        wrapper.unmount()
    })
    it('keeps task-panel local layer controls enabled by default', async () => {
        const wrapper = mount(DependencyGraph, { props: { root: 'DEV-8' } })
        await flushPromises()
        expect(wrapper.findAll('input[type="checkbox"]')).toHaveLength(4)
        wrapper.unmount()
    })
    it('labels external workspace nodes read-only and permits list and SVG navigation without exposing paths', async () => {
        mocks.graph.mockResolvedValueOnce(graph({ nodes: [node('DEV-8'), node('OTHER-2', 'external')] }))
        const wrapper = mount(DependencyGraph, { props: { root: 'DEV-8' } })
        await flushPromises()
        expect(wrapper.get('[aria-label="Dependency tasks"]').text()).toContain('External workspace (read-only)')
        await wrapper.get('[aria-label="Dependency tasks"] a[href="/task/OTHER-2"]').trigger('click')
        await wrapper
            .findAll('button')
            .find((button) => button.text() === 'Show diagram')!
            .trigger('click')
        expect(wrapper.get('.dependency-diagram a[href="/task/OTHER-2"]').attributes('aria-label')).toContain(
            'External workspace (read-only)',
        )
        await wrapper.get('.dependency-diagram a[href="/task/OTHER-2"]').trigger('keydown', { key: 'Enter' })
        expect(mocks.openTaskPanel).toHaveBeenCalledTimes(2)
        expect(mocks.openTaskPanel).toHaveBeenLastCalledWith({ taskId: 'OTHER-2' })
        expect(wrapper.props('root')).toBe('DEV-8')
        wrapper.unmount()
    })
    it.each([
        { depth: 0, maxNodes: 1 },
        { depth: 8, maxNodes: 500 },
    ])('accepts supported graph bounds %o without clamping', async (bounds) => {
        const wrapper = mount(DependencyGraph, { props: { root: 'DEV-8', ...bounds } })
        await flushPromises()
        expect(mocks.graph).toHaveBeenCalledExactlyOnceWith({
            task_id: 'DEV-8',
            project: undefined,
            depth: bounds.depth,
            max_nodes: bounds.maxNodes,
            relationship_kinds: ['dependency'],
            include_isolated: false,
        })
        wrapper.unmount()
    })
    it.each([{ depth: -1 }, { depth: 9 }, { depth: 1.5 }, { depth: Number.NaN }, { maxNodes: 0 }, { maxNodes: 501 }])(
        'rejects invalid graph bounds %o without fetching a different scope',
        async (bounds) => {
            const wrapper = mount(DependencyGraph, { props: { root: 'DEV-8', ...bounds } })
            await flushPromises()
            expect(mocks.graph).not.toHaveBeenCalled()
            expect(wrapper.get('[role="alert"]').text()).toContain('must be an integer between')
            wrapper.unmount()
        },
    )
    it('uses returned graph membership including cross-project tasks, and prerequisite direction', async () => {
        const wrapper = mount(DependencyGraph, { props: { root: 'DEV-8', project: 'DEV' } })
        await flushPromises()
        expect(mocks.graph).toHaveBeenCalledExactlyOnceWith({
            task_id: 'DEV-8',
            project: 'DEV',
            depth: 1,
            max_nodes: 100,
            relationship_kinds: ['dependency'],
            include_isolated: false,
        })
        expect(wrapper.get('[aria-label="Dependency tasks"]').text()).toContain('OTHER-2')
        expect(wrapper.get('[aria-label="Dependency relationships"]').text()).toBe('OTHER-2 blocks DEV-8')
        expect(wrapper.find('.dependency-diagram svg').exists()).toBe(false)
        expect((wrapper.get('.dependency-lists').element as HTMLDetailsElement).open).toBe(true)
        await wrapper
            .findAll('button')
            .find((button) => button.text() === 'Show diagram')!
            .trigger('click')
        expect(wrapper.find('.dependency-diagram svg').exists()).toBe(true)
        expect((wrapper.get('.dependency-lists').element as HTMLDetailsElement).open).toBe(false)
        wrapper.unmount()
    })
    it('distinguishes missing, deleted, ambiguous and unreadable states, issues and partial bounds', async () => {
        mocks.graph.mockResolvedValueOnce(
            graph({
                nodes: [
                    node('DEV-8'),
                    node('DEV-2', 'deleted'),
                    node('DEV-3', 'missing'),
                    node('DEV-4', 'ambiguous'),
                    node('DEV-5', 'unreadable'),
                ],
                edges: [2, 3, 4, 5].map((id) => edge('DEV-8', `DEV-${id}`)),
                complete: false,
                truncated: true,
                issues: [
                    {
                        code: 'dependency_cycle',
                        severity: 'error',
                        message: 'Dependency cycle requires manual review',
                        tasks: ['DEV-8', 'DEV-2'],
                        repairable: false,
                    },
                ],
            }),
        )
        const wrapper = mount(DependencyGraph, { props: { root: 'DEV-8' } })
        await flushPromises()
        for (const text of [
            'Deleted',
            'Missing target',
            'Ambiguous ID',
            'Unreadable task',
            'Partial dependency view',
            'Circular dependency',
        ])
            expect(wrapper.text()).toContain(text)
        const list = wrapper.get('[aria-label="Dependency tasks"]')
        expect(list.findAll('a').filter((link) => link.text().startsWith('DEV-'))).toHaveLength(2)
        wrapper.unmount()
    })
    it('keeps optional layers synchronized between the list and SVG and retains graph context on task navigation', async () => {
        mocks.graph.mockResolvedValue(
            graph({
                edges: [...graph().edges, { source: 'DEV-8', target: 'OTHER-2', kind: 'related', provenance: [] }],
            }),
        )
        const wrapper = mount(DependencyGraph, { props: { root: 'DEV-8' } })
        await flushPromises()
        expect(wrapper.get('[aria-label="Dependency relationships"]').text()).not.toContain('related')
        await wrapper.findAll('input[type="checkbox"]')[0]!.setValue(true)
        await flushPromises()
        expect(wrapper.get('[aria-label="Dependency relationships"]').text()).toContain('related')
        await wrapper.get('[aria-label="Dependency tasks"] a[href="/task/DEV-8"]').trigger('click')
        expect(mocks.openTaskPanel).toHaveBeenCalledWith({ taskId: 'DEV-8' })
        expect(mocks.graph).toHaveBeenCalledTimes(2)
        expect(wrapper.props('root')).toBe('DEV-8')
        wrapper.unmount()
    })
    it('opens SVG task links by click and keyboard without replacing the graph root', async () => {
        const wrapper = mount(DependencyGraph, { props: { root: 'DEV-8', project: 'DEV', depth: 2 } })
        await flushPromises()
        await wrapper
            .findAll('button')
            .find((button) => button.text() === 'Show diagram')!
            .trigger('click')
        const link = wrapper.get('.dependency-diagram a[href="/task/OTHER-2"]')
        expect(link.attributes('aria-label')).toContain('OTHER-2')
        await link.trigger('click')
        await link.trigger('keydown', { key: 'Enter' })
        await link.trigger('keydown', { key: ' ' })
        expect(mocks.openTaskPanel).toHaveBeenCalledTimes(3)
        expect(mocks.openTaskPanel).toHaveBeenLastCalledWith({ taskId: 'OTHER-2' })
        expect(wrapper.props()).toMatchObject({ root: 'DEV-8', project: 'DEV', depth: 2 })
        expect(mocks.graph).toHaveBeenCalledOnce()
        wrapper.unmount()
    })
    it('labels cycle nodes and edges in the SVG and synchronized list without relying on color', async () => {
        mocks.graph.mockResolvedValueOnce(
            graph({
                issues: [
                    {
                        code: 'cycle',
                        severity: 'error',
                        message: 'Conflicting dependency',
                        tasks: ['DEV-8', 'OTHER-2'],
                        repairable: false,
                    },
                ],
            }),
        )
        const wrapper = mount(DependencyGraph, { props: { root: 'DEV-8' } })
        await flushPromises()
        await wrapper
            .findAll('button')
            .find((button) => button.text() === 'Show diagram')!
            .trigger('click')
        expect(wrapper.findAll('.dependency-node--cycle')).toHaveLength(2)
        expect(wrapper.get('.dependency-edge--cycle title').text()).toBe('OTHER-2 blocks DEV-8 / Cycle issue')
        expect(wrapper.get('.dependency-diagram a[href="/task/DEV-8"]').attributes('aria-label')).toContain(
            'Cycle issue',
        )
        expect(wrapper.get('[aria-label="Dependency tasks"]').text()).toContain('Cycle issue')
        expect(wrapper.get('[aria-label="Dependency relationships"]').text()).toContain('Cycle issue')
        wrapper.unmount()
    })
    it('includes incoming fixes and duplicate edges in the all-layers view and exposes separate toggles', async () => {
        mocks.graph.mockResolvedValue(
            graph({
                edges: [
                    ...graph().edges,
                    { source: 'OTHER-2', target: 'DEV-8', kind: 'fixes', provenance: [] },
                    { source: 'OTHER-2', target: 'DEV-8', kind: 'duplicate', provenance: [] },
                ],
            }),
        )
        const wrapper = mount(DependencyGraph, { props: { root: 'DEV-8', type: 'all' } })
        await flushPromises()
        expect(wrapper.get('[aria-label="Dependency relationships"]').text()).toContain('OTHER-2 fixes DEV-8')
        expect(wrapper.get('[aria-label="Dependency relationships"]').text()).toContain('OTHER-2 duplicate of DEV-8')
        const labels = wrapper.findAll('.dependency-toolbar label')
        await labels
            .find((label) => label.text() === 'Fixes layer')!
            .get('input')
            .setValue(false)
        await flushPromises()
        expect(wrapper.get('[aria-label="Dependency relationships"]').text()).not.toContain('fixes')
        expect(wrapper.get('[aria-label="Dependency relationships"]').text()).toContain('duplicate of')
        wrapper.unmount()
    })
    it('rejects late requests across A-B-A root changes', async () => {
        let resolve!: (value: RelationshipGraphResponse) => void
        mocks.graph.mockImplementationOnce(
            () =>
                new Promise((done) => {
                    resolve = done
                }),
        )
        const wrapper = mount(DependencyGraph, { props: { root: 'DEV-8' } })
        await wrapper.setProps({ root: 'DEV-9' })
        await flushPromises()
        await wrapper.setProps({ root: 'DEV-8' })
        await flushPromises()
        resolve(graph({ nodes: [node('STALE-1')] }))
        await flushPromises()
        expect(wrapper.text()).not.toContain('STALE-1')
        expect(
            wrapper
                .emitted('loaded')
                ?.flat()
                .some((value) => (value as RelationshipGraphResponse).nodes.some((node) => node.id === 'STALE-1')),
        ).toBe(false)
        expect(wrapper.text()).toContain('OTHER-2')
        wrapper.unmount()
    })
    it('refreshes on external tasks and reconnect, debounces events and closes only its subscription', async () => {
        vi.useFakeTimers()
        const wrapper = mount(DependencyGraph, { props: { root: 'DEV-8' } })
        await flushPromises()
        const callback = mocks.on.mock.calls.find(([name]) => name === 'task_updated')?.[1]
        expect(callback).toBeTypeOf('function')
        callback({ data: JSON.stringify({ id: 'OUTSIDE-1' }) })
        callback({ data: JSON.stringify({ id: 'OUTSIDE-2' }) })
        await vi.advanceTimersByTimeAsync(120)
        await flushPromises()
        expect(mocks.graph).toHaveBeenCalledTimes(2)
        mocks.reconnect?.()
        await vi.advanceTimersByTimeAsync(120)
        await flushPromises()
        expect(mocks.graph).toHaveBeenCalledTimes(3)
        wrapper.unmount()
        expect(mocks.close).toHaveBeenCalledOnce()
    })
    it('shows an API failure while retaining only an explicitly inactive previous view', async () => {
        const wrapper = mount(DependencyGraph, { props: { root: 'DEV-8' } })
        await flushPromises()
        mocks.graph.mockRejectedValueOnce(new Error('Unreadable scope'))
        await wrapper.setProps({ root: 'DEV-9' })
        await flushPromises()
        expect(wrapper.get('[role="alert"]').text()).toContain('Unreadable scope')
        expect(wrapper.get('.dependency-render').attributes('inert')).toBeDefined()
        expect(wrapper.find('.dependency-count').exists()).toBe(false)
        expect(wrapper.text()).toContain('Previous dependency view (inactive)')
        wrapper.unmount()
    })
})

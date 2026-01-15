import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ref } from 'vue';

const routeState: { query: Record<string, any>; hash: string } = { query: {}, hash: '' }

vi.mock('vue-router', () => ({
    useRoute: () => routeState,
}))

vi.mock('../api/client', () => ({
    api: {
        listTasks: vi.fn(),
    },
}))

vi.mock('../components/toast', () => ({
    showToast: vi.fn(),
}))

vi.mock('../components/IconGlyph.vue', () => ({
    default: { template: '<span class="icon" />' },
}))

vi.mock('../components/UiButton.vue', () => ({
    default: {
        emits: ['click'],
        template: '<button type="button" class="btn" @click="$emit(\'click\', $event)"><slot /></button>',
    },
}))

vi.mock('../components/UiCard.vue', () => ({
    default: { template: '<div class="card"><slot /></div>' },
}))

vi.mock('../components/UiLoader.vue', () => ({
    default: { template: '<div class="loader" />' },
}))

vi.mock('../components/UiEmptyState.vue', () => ({
    default: { template: '<div class="empty" />' },
}))

vi.mock('../components/UiSelect.vue', () => ({
    default: {
        props: ['modelValue'],
        emits: ['update:modelValue'],
        template: '<select class="select" @change="$emit(\'update:modelValue\', $event.target && $event.target.value)"><slot /></select>',
    },
}))

vi.mock('../components/ReloadButton.vue', () => ({
    default: { template: '<button type="button" class="reload"><slot /></button>' },
}))

vi.mock('../components/SmartListChips.vue', () => ({
    default: { template: '<div class="chips" />' },
}))

vi.mock('../components/FilterBar.vue', () => ({
    default: {
        emits: ['update:value'],
        template: '<div class="filter-bar" />',
    },
}))

vi.mock('../components/analytics/SprintAnalyticsDialog.vue', () => ({
    default: { template: '<div class="analytics" />' },
}))

vi.mock('../composables/useSprints', () => ({
    useSprints: () => ({
        sprints: ref<any[]>([]),
        loading: ref(false),
        refresh: vi.fn(async () => { }),
        missingSprints: ref<any[]>([]),
        hasMissing: ref(false),
    }),
}))

vi.mock('../composables/useTaskPanelController', () => ({
    useTaskPanelController: () => ({
        openTaskPanel: vi.fn(),
    }),
}))

vi.mock('../composables/useConfig', () => ({
    useConfig: () => ({
        sprintDefaults: ref<any>({}),
        statuses: ref<string[]>(['open', 'done']),
        priorities: ref<string[]>(['low', 'med', 'high']),
        types: ref<string[]>(['task']),
        customFields: ref<string[]>([]),
        refresh: vi.fn(async () => { }),
    }),
}))

vi.mock('../composables/useSprintAnalytics', () => ({
    DEFAULT_VELOCITY_PARAMS: { limit: 4, metric: 'tasks' },
    useSprintAnalytics: () => ({
        getSummary: vi.fn(() => undefined),
        getBurndown: vi.fn(() => undefined),
        getVelocity: vi.fn(() => undefined),
        getSummaryError: vi.fn(() => null),
        getBurndownError: vi.fn(() => null),
        getVelocityError: vi.fn(() => null),
        isSummaryLoading: vi.fn(() => false),
        isBurndownLoading: vi.fn(() => false),
        isVelocityLoading: vi.fn(() => false),
        loadSummary: vi.fn(async () => { }),
        loadBurndown: vi.fn(async () => { }),
        loadVelocity: vi.fn(async () => { }),
    }),
}))

vi.mock('../composables/useCopyModifier', () => ({
    useCopyModifier: () => ({
        copyModifierActive: ref(false),
        resolveCopyModifier: vi.fn(() => false),
        resetCopyModifier: vi.fn(),
        bindCopyModifierListeners: vi.fn(),
        unbindCopyModifierListeners: vi.fn(),
    }),
}))

import SprintsList from '../pages/SprintsList.vue';

function makeTask(idNum: number) {
    const padded = String(idNum).padStart(3, '0')
    return {
        id: `PRJ-${padded}`,
        title: `Task ${padded}`,
        status: 'open',
        priority: 'med',
        task_type: 'task',
        reporter: null,
        assignee: null,
        effort: null,
        due_date: null,
        created: '2026-01-01T00:00:00Z',
        modified: '2026-01-01T00:00:00Z',
        tags: [],
        relationships: {},
        comments: [],
        custom_fields: {},
        sprints: [],
        references: [],
        history: [],
    }
}

describe('SprintsList backlog pagination', () => {
    beforeEach(async () => {
        routeState.query = {}
        routeState.hash = ''

        localStorage.clear()
        localStorage.setItem('lotar.sprints.sort', JSON.stringify({ key: 'id', dir: 'asc' }))

        const { api } = await import('../api/client')
            ; (api.listTasks as any).mockResolvedValue({
                status: 'ok',
                count: 120,
                total: 120,
                limit: 200,
                offset: 0,
                tasks: Array.from({ length: 120 }, (_, idx) => makeTask(idx + 1)),
            })
    })

    it('shows the first page of backlog tasks using the tasks page-size preference', async () => {
        const wrapper = mount(SprintsList)
        await flushPromises()

        const rows = wrapper.findAll('#backlog tbody tr.task-row')
        expect(rows).toHaveLength(50)
        expect(rows[0].text()).toContain('1')
        wrapper.unmount()
    })

    it('pages forward through backlog tasks', async () => {
        const wrapper = mount(SprintsList)
        await flushPromises()

        const next = wrapper.find('button[aria-label="Next backlog page"]')
        expect(next.exists()).toBe(true)

        await next.trigger('click')
        await flushPromises()

        const rows = wrapper.findAll('#backlog tbody tr.task-row')
        expect(rows).toHaveLength(50)
        expect(rows[0].text()).toContain('51')

        wrapper.unmount()
    })
})

import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import type {
    SprintBurndownResponse,
    SprintListItem,
    SprintSummaryReportResponse,
    SprintVelocityEntryPayload,
    SprintVelocityResponse,
} from '../api/types'
import SprintAnalyticsDialog from '../components/analytics/SprintAnalyticsDialog.vue'

const sprint: SprintListItem = {
    id: 3,
    display_name: 'Sprint Three',
    state: 'active',
}

function summary(overrides: Partial<SprintSummaryReportResponse['lifecycle']> = {}): SprintSummaryReportResponse {
    return {
        status: 'ok',
        sprint: {
            status_warnings: [{ code: 'overdue', message: 'Sprint ran past its end date' }],
        },
        lifecycle: {
            planned_start: '2026-01-05T09:00:00Z',
            actual_start: '2026-01-06T09:00:00Z',
            planned_end: '2026-01-19T17:00:00Z',
            ...overrides,
        },
        metrics: { tasks: {}, points: null, hours: null },
        timeline: {},
        blocked_tasks: [],
    } as unknown as SprintSummaryReportResponse
}

function burndown(withPoints: boolean): SprintBurndownResponse {
    return {
        status: 'ok',
        totals: { tasks: 4, points: withPoints ? 10 : null, hours: null },
        series: [{ date: '2026-01-06', remaining_tasks: 4, ideal_tasks: 4 }],
    } as unknown as SprintBurndownResponse
}

function velocityEntry(id: number, completed: number, ratio: number): SprintVelocityEntryPayload {
    return { summary: { id }, completed, completion_ratio: ratio } as unknown as SprintVelocityEntryPayload
}

const velocity = {
    status: 'ok',
    metric: 'tasks',
    count: 3,
    truncated: false,
    average_velocity: 99,
    entries: [velocityEntry(1, 4, 0.5), velocityEntry(2, 8, 1), velocityEntry(3, 6, 0.75)],
} as unknown as SprintVelocityResponse

type Props = InstanceType<typeof SprintAnalyticsDialog>['$props']

function mountDialog(props: Partial<Props> = {}) {
    return mount(SprintAnalyticsDialog, {
        props: {
            open: true,
            sprints: [sprint],
            selectedSprintId: 3,
            activeTab: 'health',
            burndownMetric: 'tasks',
            velocityMetric: 'tasks',
            ...props,
        } as Props,
        global: {
            stubs: {
                teleport: true,
                SprintHealthWidget: true,
                BurndownChart: {
                    name: 'BurndownChart',
                    props: ['series', 'metric'],
                    template: '<div class="burndown-stub" />',
                },
                VelocityTrend: {
                    name: 'VelocityTrend',
                    props: ['response', 'metric', 'currentSprintId', 'windowSize'],
                    template: '<div class="velocity-stub" />',
                },
            },
        },
    })
}

function button(wrapper: ReturnType<typeof mountDialog>, text: string) {
    return wrapper.findAll('button').find((candidate) => candidate.text().trim() === text)!
}

describe('SprintAnalyticsDialog', () => {
    it('renders nothing while closed', () => {
        const wrapper = mountDialog({ open: false })
        expect(wrapper.find('[role="dialog"]').exists()).toBe(false)
    })

    it('titles the dialog with the selected sprint and closes three ways', async () => {
        const wrapper = mountDialog()
        expect(wrapper.find('h2').text()).toContain('Sprint Three')

        await wrapper.find('[aria-label="Close dialog"]').trigger('click')
        await wrapper.find('.sprint-analytics__overlay').trigger('click')
        window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
        expect(wrapper.emitted('close')).toHaveLength(3)
        wrapper.unmount()
    })

    it('emits tab changes only for a different tab', async () => {
        const wrapper = mountDialog()
        await button(wrapper, 'Health').trigger('click')
        await button(wrapper, 'Velocity').trigger('click')
        expect(wrapper.emitted('update:activeTab')).toEqual([['velocity']])
        wrapper.unmount()
    })

    it('refreshes only when a sprint is selected', async () => {
        const selected = mountDialog()
        await selected.find('[title="Refresh analytics"]').trigger('click')
        expect(selected.emitted('refresh')).toHaveLength(1)
        selected.unmount()

        const unselected = mountDialog({ selectedSprintId: null })
        await unselected.find('[title="Refresh analytics"]').trigger('click')
        expect(unselected.emitted('refresh')).toBeUndefined()
        unselected.unmount()
    })

    describe('burndown tab', () => {
        it('only offers metrics the burndown has data for', async () => {
            const wrapper = mountDialog({ activeTab: 'burndown', summary: summary(), burndown: burndown(true) })
            expect(button(wrapper, 'Hours').attributes('disabled')).toBeDefined()

            await button(wrapper, 'Hours').trigger('click')
            await button(wrapper, 'Story points').trigger('click')
            expect(wrapper.emitted('update:burndownMetric')).toEqual([['points']])
            wrapper.unmount()
        })

        it('charts the series for the chosen metric', () => {
            const wrapper = mountDialog({ activeTab: 'burndown', summary: summary(), burndown: burndown(false) })
            const chart = wrapper.findComponent({ name: 'BurndownChart' })
            expect(chart.props('metric')).toBe('tasks')
            expect(chart.props('series')).toHaveLength(1)
            wrapper.unmount()
        })

        it('explains a missing chart before the sprint starts', () => {
            const wrapper = mountDialog({
                activeTab: 'burndown',
                burndownMetric: 'points',
                summary: summary({ actual_start: null }),
                burndown: burndown(false),
            })
            expect(wrapper.find('.sprint-analytics__chart-placeholder').text()).toContain(
                'Burndown will render once Sprint Three has an actual start date.',
            )
            wrapper.unmount()
        })

        it('asks for a sprint when none is selected', () => {
            const wrapper = mountDialog({ activeTab: 'burndown', selectedSprintId: null })
            expect(wrapper.find('.sprint-analytics__empty').text()).toContain('Open analytics from a sprint')
            wrapper.unmount()
        })
    })

    describe('velocity tab', () => {
        it('falls back to tasks when the chosen metric has no data', () => {
            const wrapper = mountDialog({ activeTab: 'velocity', velocityMetric: 'hours', velocity })
            expect(wrapper.emitted('update:velocityMetric')).toEqual([['tasks']])
            wrapper.unmount()
        })

        it('narrows the trend to focused sprints and recomputes averages', () => {
            const wrapper = mountDialog({ activeTab: 'velocity', velocity, velocityFocusSprintIds: [1, 3] })
            const trend = wrapper.findComponent({ name: 'VelocityTrend' })
            const response = trend.props('response') as SprintVelocityResponse
            expect(response.entries.map((entry) => entry.summary.id)).toEqual([1, 3])
            expect(response.count).toBe(2)
            expect(response.average_velocity).toBe(5)
            expect(response.average_completion_ratio).toBeCloseTo(0.625)
            expect(trend.props('currentSprintId')).toBe(3)
            wrapper.unmount()
        })

        it('shows loading and error states instead of the chart', () => {
            const loading = mountDialog({ activeTab: 'velocity', velocity, velocityLoading: true })
            expect(loading.findComponent({ name: 'VelocityTrend' }).exists()).toBe(false)
            loading.unmount()

            const failed = mountDialog({ activeTab: 'velocity', velocityError: 'Velocity unavailable' })
            expect(failed.find('.sprint-analytics__error').text()).toBe('Velocity unavailable')
            failed.unmount()
        })
    })

    describe('history tab', () => {
        it('orders lifecycle milestones by date and lists warnings last', () => {
            const wrapper = mountDialog({ activeTab: 'history', summary: summary() })
            const labels = wrapper.findAll('.timeline-item__label').map((item) => item.text().split('·')[0]!.trim())
            expect(labels).toEqual(['Planned start', 'Started', 'Planned end', 'overdue'])
            const descriptions = wrapper.findAll('.timeline-item__description')
            expect(descriptions[descriptions.length - 1]!.text()).toBe('Sprint ran past its end date')
            wrapper.unmount()
        })

        it('explains an empty timeline', () => {
            const wrapper = mountDialog({ activeTab: 'history', selectedSprintId: null })
            expect(wrapper.find('.sprint-analytics__empty').text()).toContain('lifecycle milestones')
            wrapper.unmount()
        })
    })
})

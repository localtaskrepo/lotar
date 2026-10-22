import { flushPromises, mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import BoardColumnsDialog from '../components/BoardColumnsDialog.vue'
import type { BoardLayout } from '../composables/useBoardLayout'

const statuses = ['Todo', 'Doing', 'Review', 'Done'].map((label) => ({ key: label.toLowerCase(), label }))
const plain: BoardLayout = { columns: statuses.map((s) => ({ statuses: [s.key] })), hidden: [] }

function mountDialog(layout: BoardLayout = plain) {
    return mount(BoardColumnsDialog, { props: { open: true, statuses, layout }, global: { stubs: { teleport: true } } })
}
type Wrapper = ReturnType<typeof mountDialog>
const row = (wrapper: Wrapper, label: string) => wrapper.find(`[data-layout-status="${label}"]`)
const column = (wrapper: Wrapper, index: number) => wrapper.findAll('.board-columns-dialog__column')[index]!
const titles = (wrapper: Wrapper) => wrapper.findAll('.board-columns-dialog__title').map((t) => t.text())
async function menuMove(wrapper: Wrapper, label: string, item: string) {
    const button = row(wrapper, label).findAll('.board-columns-dialog__menu-item').find((b) => b.text() === item)
    expect(button, `${item} for ${label}`).toBeTruthy()
    await button!.trigger('click')
}
async function applied(wrapper: Wrapper) {
    await wrapper.find('[data-testid="board-columns-apply"]').trigger('click')
    const events = wrapper.emitted('apply')!
    return events[events.length - 1]![0] as BoardLayout
}
const stacks = (layout: BoardLayout) => layout.columns.map((c) => c.statuses.join('+'))

describe('BoardColumnsDialog (DEV-130)', () => {
    it('shows columns side by side with plain titles until renamed', async () => {
        const wrapper = mountDialog({ columns: [{ statuses: ['todo'] }, { statuses: ['doing', 'review'], name: 'Active' }, { statuses: ['done'] }], hidden: [] })
        await flushPromises()
        expect(titles(wrapper)).toEqual(['Todo', 'Active', 'Done'])
        expect(wrapper.find('input.board-columns-dialog__rename').exists()).toBe(false)

        await wrapper.find('[aria-label="Rename column Done"]').trigger('click')
        const input = wrapper.find('input.board-columns-dialog__rename')
        await input.setValue('Closed')
        await input.trigger('keydown', { key: 'Enter' })
        expect(titles(wrapper)).toEqual(['Todo', 'Active', 'Closed'])

        // Escape restores the previous name.
        await wrapper.find('[aria-label="Rename column Active"]').trigger('click')
        await wrapper.find('input.board-columns-dialog__rename').setValue('Oops')
        await wrapper.find('input.board-columns-dialog__rename').trigger('keydown', { key: 'Escape' })
        expect(titles(wrapper)).toEqual(['Todo', 'Active', 'Closed'])
        expect((await applied(wrapper)).columns.map((c) => c.name)).toEqual([undefined, 'Active', 'Closed'])
    })

    it('toggles visibility and emits the edited layout on apply', async () => {
        const wrapper = mountDialog()
        await flushPromises()
        await row(wrapper, 'Doing').find('[aria-label="Hide Doing"]').trigger('click')
        expect(row(wrapper, 'Doing').find('[aria-label="Show Doing"]').exists()).toBe(true)
        expect((await applied(wrapper)).hidden).toEqual(['doing'])
    })

    it('moves statuses into any column from the status menu and back into their own column', async () => {
        const wrapper = mountDialog()
        await flushPromises()
        await menuMove(wrapper, 'Done', 'Move to Todo')
        expect(stacks(await applied(wrapper))).toEqual(['todo+done', 'doing', 'review'])
        await menuMove(wrapper, 'Done', 'Own column')
        expect(stacks(await applied(wrapper))).toEqual(['todo', 'done', 'doing', 'review'])
    })

    it('reorders columns by dragging their header or with the arrow keys on the grip', async () => {
        const wrapper = mountDialog()
        await flushPromises()
        await column(wrapper, 3).find('.board-columns-dialog__column-head').trigger('dragstart')
        await column(wrapper, 1).trigger('dragover', { clientX: 0 })
        await column(wrapper, 1).trigger('drop')
        expect(titles(wrapper)).toEqual(['Todo', 'Done', 'Doing', 'Review'])

        await column(wrapper, 0).find('.board-columns-dialog__grip').trigger('keydown', { key: 'ArrowRight' })
        expect(titles(wrapper)).toEqual(['Done', 'Todo', 'Doing', 'Review'])
        expect(stacks(await applied(wrapper))).toEqual(['done', 'todo', 'doing', 'review'])
    })

    it('stacks by dragging a status onto another column, status, or the new-column target', async () => {
        const wrapper = mountDialog({ columns: [{ statuses: ['todo', 'doing'] }, { statuses: ['review'] }, { statuses: ['done'] }], hidden: [] })
        await flushPromises()
        await row(wrapper, 'Done').trigger('dragstart')
        await row(wrapper, 'Doing').trigger('drop')
        expect(stacks(await applied(wrapper))).toEqual(['todo+done+doing', 'review'])
        await row(wrapper, 'Review').trigger('dragstart')
        await column(wrapper, 0).trigger('drop')
        expect(stacks(await applied(wrapper))).toEqual(['todo+done+doing+review'])
        await row(wrapper, 'Todo').trigger('dragstart')
        await wrapper.find('.board-columns-dialog__new-column').trigger('drop')
        expect(stacks(await applied(wrapper))).toEqual(['done+doing+review', 'todo'])
    })

    it('keeps per-column settings with the column and resets to one column per status', async () => {
        const wrapper = mountDialog({ columns: [{ statuses: ['todo'] }, { statuses: ['doing', 'review'], name: 'Active', wipLimit: 4 }, { statuses: ['done'] }], hidden: ['done'] })
        await flushPromises()
        await column(wrapper, 1).find('.board-columns-dialog__grip').trigger('keydown', { key: 'ArrowLeft' })
        await column(wrapper, 0).findAll('.board-columns-dialog__settings input')[2]!.setValue('5')
        const result = await applied(wrapper)
        expect(result.columns[0]).toEqual({ statuses: ['doing', 'review'], name: 'Active', wipLimit: 4, maxAgeDays: undefined, maxVisible: 5 })
        expect(result.hidden).toEqual(['done'])
        await wrapper.findAll('button').find((b) => b.text() === 'Reset to defaults')!.trigger('click')
        const reset = await applied(wrapper)
        expect(stacks(reset)).toEqual(['todo', 'doing', 'review', 'done'])
        expect(reset.hidden).toEqual([])
    })
})

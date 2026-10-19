import { ref, type Ref } from 'vue'
import type { ColKey } from './useColumns'

export interface UseColumnDragReorderOptions {
    /** MIME type tagging this table's column drags; drops read it first, then fall back to text/plain. */
    mime: string
    /** The caller's useColumns() columnOrder ref; a reorder assigns the next full order array to it. */
    columnOrder: Ref<ColKey[]>
    /** Sort callback fired by header activation once the post-drag click guard allows it. */
    onSort: (col: ColKey) => void
}

/** Clicks right after a finished drag are swallowed so ending a drag never toggles sorting. */
const DRAG_SORT_GUARD_MS = 250

/**
 * Shared column drag-reorder behavior for task tables (DEV-112).
 * Owns drag indicator state, the 250ms post-drag click guard, MIME-tagged
 * drag payloads, before/after drop positioning, and reorder semantics over
 * the caller's useColumns() columnOrder (hidden columns keep their slots).
 */
export function useColumnDragReorder(options: UseColumnDragReorderOptions) {
    const { mime, columnOrder, onSort } = options

    const draggingCol = ref<ColKey | null>(null)
    const dragOverCol = ref<ColKey | null>(null)
    const dragOverPos = ref<'before' | 'after'>('before')
    const lastDragAt = ref(0)

    function onHeaderActivate(col: ColKey) {
        if (Date.now() - lastDragAt.value < DRAG_SORT_GUARD_MS) return
        onSort(col)
    }

    function moveColumn(from: ColKey, to: ColKey, pos: 'before' | 'after') {
        if (from === to) return
        const next = columnOrder.value.filter((c) => c !== from)
        const idx = next.indexOf(to)
        if (idx < 0) return
        const insertAt = pos === 'after' ? idx + 1 : idx
        next.splice(insertAt, 0, from)
        columnOrder.value = next
    }

    function onColDragStart(col: ColKey, event: DragEvent) {
        draggingCol.value = col
        dragOverCol.value = null
        dragOverPos.value = 'before'
        lastDragAt.value = Date.now()
        if (event.dataTransfer) {
            event.dataTransfer.effectAllowed = 'move'
            try {
                event.dataTransfer.setData(mime, col)
                event.dataTransfer.setData('text/plain', col)
            } catch {
                // ignore
            }
        }
    }

    function onColDragOver(col: ColKey, event: DragEvent) {
        dragOverCol.value = col
        const target = event.currentTarget as HTMLElement | null
        if (!target) {
            dragOverPos.value = 'before'
            return
        }
        const rect = target.getBoundingClientRect?.()
        if (!rect || !rect.width) {
            dragOverPos.value = 'before'
            return
        }
        const mid = rect.left + rect.width / 2
        dragOverPos.value = event.clientX <= mid ? 'before' : 'after'
    }

    function onColDrop(target: ColKey, event: DragEvent) {
        const source = (event.dataTransfer?.getData(mime) || event.dataTransfer?.getData('text/plain') || '') as ColKey
        if (source) moveColumn(source, target, dragOverPos.value)
        draggingCol.value = null
        dragOverCol.value = null
        dragOverPos.value = 'before'
        lastDragAt.value = Date.now()
    }

    function onColDragEnd() {
        draggingCol.value = null
        dragOverCol.value = null
        dragOverPos.value = 'before'
        lastDragAt.value = Date.now()
    }

    return {
        draggingCol,
        dragOverCol,
        dragOverPos,
        lastDragAt,
        onHeaderActivate,
        moveColumn,
        onColDragStart,
        onColDragOver,
        onColDrop,
        onColDragEnd,
    }
}

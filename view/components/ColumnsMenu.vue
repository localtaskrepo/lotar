<script setup lang="ts">
import { nextTick, onUnmounted, ref, watch } from 'vue'
import UiButton from './UiButton.vue'
import type { FieldOption } from '../composables/useColumns'

const props = defineProps<{
    open: boolean
    options: FieldOption[]
    isVisible: (key: string) => boolean
    setVisible: (key: string, event: Event) => void
    label?: string
}>()

const emit = defineEmits<{
    'update:open': [value: boolean]
    reset: []
}>()

const rootRef = ref<HTMLElement | null>(null)
const popoverRef = ref<HTMLElement | null>(null)
const popoverStyle = ref<Record<string, string>>({})

function close() {
    emit('update:open', false)
}

function clampToViewport() {
    const root = rootRef.value
    const pop = popoverRef.value
    if (!root || !pop) return
    const margin = 8
    const rootRect = root.getBoundingClientRect()
    const popRect = pop.getBoundingClientRect()
    // CSS anchors the popover to the wrapper's right edge; keep it inside the
    // viewport by shifting it (via inline left) when that anchor overflows.
    const desiredLeft = rootRect.right - popRect.width
    const maxLeft = Math.max(margin, window.innerWidth - margin - popRect.width)
    const clampedLeft = Math.min(Math.max(margin, desiredLeft), maxLeft)
    const dx = Math.round(clampedLeft - rootRect.left)
    popoverStyle.value = dx === 0 ? {} : { left: `${dx}px`, right: 'auto' }
}

function onWindowClick(event: MouseEvent) {
    const target = event.target
    if (target instanceof Node && rootRef.value?.contains(target)) return
    close()
}

function onWindowKey(event: KeyboardEvent) {
    if (event.key === 'Escape') close()
}

function onWindowReposition() {
    if (props.open) clampToViewport()
}

watch(
    () => props.open,
    async (open) => {
        if (typeof window === 'undefined') return
        if (open) {
            window.addEventListener('click', onWindowClick)
            window.addEventListener('keydown', onWindowKey)
            window.addEventListener('resize', onWindowReposition)
            window.addEventListener('scroll', onWindowReposition, true)
            await nextTick()
            clampToViewport()
        } else {
            window.removeEventListener('click', onWindowClick)
            window.removeEventListener('keydown', onWindowKey)
            window.removeEventListener('resize', onWindowReposition)
            window.removeEventListener('scroll', onWindowReposition, true)
        }
    },
    { immediate: true },
)

onUnmounted(() => {
    if (typeof window === 'undefined') return
    window.removeEventListener('click', onWindowClick)
    window.removeEventListener('keydown', onWindowKey)
    window.removeEventListener('resize', onWindowReposition)
    window.removeEventListener('scroll', onWindowReposition, true)
})
</script>

<template>
    <div ref="rootRef" class="columns-menu-wrapper" data-columns-menu>
        <slot name="trigger" :open="open" :toggle="() => emit('update:open', !open)" />
        <div v-if="open" ref="popoverRef" class="columns-popover card" :style="popoverStyle" data-columns-menu-popover role="dialog" :aria-label="label ?? 'Columns'">
            <div class="columns-popover__title" v-if="label">{{ label }}</div>
            <div class="col" :style="{ gap: '6px' }">
                <label
                    v-for="opt in options"
                    :key="opt.key"
                    class="row column-option"
                    :style="{ gap: '6px', alignItems: 'center' }"
                >
                    <input
                        type="checkbox"
                        :checked="isVisible(opt.key)"
                        @change="setVisible(opt.key, $event)"
                    />
                    <span>{{ opt.label }}</span>
                </label>
                <div class="row" :style="{ gap: '6px', marginTop: '6px' }">
                    <UiButton type="button" variant="ghost" @click="emit('reset')">Reset</UiButton>
                    <UiButton type="button" @click="close">Close</UiButton>
                </div>
            </div>
        </div>
    </div>
</template>

<style scoped>
.columns-menu-wrapper {
    position: relative;
    display: inline-block;
}

.columns-popover {
    position: absolute;
    top: calc(100% + var(--space-1, 0.25rem));
    right: 0;
    z-index: var(--z-popover);
    margin-top: 0;
    padding: var(--space-3, 0.75rem);
    border: 1px solid var(--color-border, var(--border));
    border-radius: var(--radius-lg, 0.75rem);
    background: var(--color-bg, var(--bg));
    box-shadow: var(--shadow-popover);
    min-width: 220px;
    max-width: min(320px, calc(100vw - 16px));
}

.columns-popover__title {
    font-size: var(--text-sm, 0.875rem);
    font-weight: 600;
    color: var(--color-muted, var(--muted));
    margin-bottom: 6px;
}

.column-option {
    padding: 4px 6px;
    border-radius: var(--radius-sm, 0.25rem);
}

.column-option:hover {
    background: var(--color-hover, rgba(0, 0, 0, 0.04));
}
</style>

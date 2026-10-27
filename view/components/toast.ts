import { ref } from 'vue'

export type ToastAction = { label: string; run: () => void }
export type Toast = { id: number; title?: string; message: string; action?: ToastAction }
export const toasts = ref<Toast[]>([])
let id = 1

export function dismissToast(toastId: number) {
    toasts.value = toasts.value.filter((x) => x.id !== toastId)
}

/** Show a toast; returns its id so callers can dismiss it early. */
export function showToast(message: string, title?: string, duration = 3000, action?: ToastAction): number {
    const t: Toast = { id: id++, title, message, ...(action ? { action } : {}) }
    toasts.value.push(t)
    setTimeout(() => dismissToast(t.id), duration)
    return t.id
}

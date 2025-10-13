import { ref } from 'vue';

export type Toast = { id: number; title?: string; message: string }
export const toasts = ref<Toast[]>([])
let id = 1
export function showToast(message: string, title?: string) {
  const t = { id: id++, title, message }
  toasts.value.push(t)
  setTimeout(() => { toasts.value = toasts.value.filter(x => x.id !== t.id) }, 3000)
}

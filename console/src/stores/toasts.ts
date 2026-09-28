/** Toasts: short notices in a corner, announced to screen readers. */
import { reactive } from 'vue'

export type ToastKind = 'info' | 'success' | 'error'

export interface Toast {
  id: number
  kind: ToastKind
  title: string
  message?: string
}

export interface ToastInput {
  kind?: ToastKind
  title: string
  message?: string
  /** Milliseconds before it goes; 0 keeps it until dismissed. */
  timeout?: number
}

let next = 1
const timers = new Map<number, ReturnType<typeof setTimeout>>()

export const toasts = reactive<Toast[]>([])

export function dismissToast(id: number): void {
  const index = toasts.findIndex((t) => t.id === id)
  if (index >= 0) toasts.splice(index, 1)
  const timer = timers.get(id)
  if (timer) clearTimeout(timer)
  timers.delete(id)
}

export function toast(input: ToastInput): number {
  const kind = input.kind ?? 'info'
  // The same message twice in a row is one toast.
  const same = toasts.find(
    (t) => t.kind === kind && t.title === input.title && t.message === input.message,
  )
  if (same) dismissToast(same.id)
  const id = next++
  toasts.push({ id, kind, title: input.title, message: input.message })
  while (toasts.length > 4) dismissToast(toasts[0]!.id)
  const timeout = input.timeout ?? (kind === 'error' ? 9000 : 4500)
  if (timeout > 0)
    timers.set(
      id,
      setTimeout(() => dismissToast(id), timeout),
    )
  return id
}

export function clearToasts(): void {
  for (const t of [...toasts]) dismissToast(t.id)
}

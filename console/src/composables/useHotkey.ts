/** Keyboard shortcuts, with ⌘ on a Mac and Ctrl elsewhere. */
import { onScopeDispose } from 'vue'

export const isMac =
  typeof navigator !== 'undefined' &&
  /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent)

/** The label of the palette's shortcut: ⌘K on a Mac, Ctrl K elsewhere. */
export const modKey = isMac ? '⌘' : 'Ctrl'

export interface Hotkey {
  key: string
  mod?: boolean
  /** Also when focus is in a text field. */
  inInputs?: boolean
}

function typing(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false
  return target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)
}

export function matches(event: KeyboardEvent, hotkey: Hotkey): boolean {
  if (event.key.toLowerCase() !== hotkey.key.toLowerCase()) return false
  const mod = event.metaKey || event.ctrlKey
  if (hotkey.mod ? !mod : mod || event.altKey) return false
  if (!hotkey.inInputs && typing(event.target)) return false
  return true
}

export function useHotkey(hotkey: Hotkey, handler: (event: KeyboardEvent) => void): void {
  const listener = (event: KeyboardEvent) => {
    if (matches(event, hotkey)) handler(event)
  }
  window.addEventListener('keydown', listener)
  onScopeDispose(() => window.removeEventListener('keydown', listener))
}

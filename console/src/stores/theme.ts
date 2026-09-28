/**
 * Light and dark: the system's by default, or the one chosen here, kept in
 * localStorage and applied as <html data-theme>, which brand/tokens.css reads.
 * public/theme-init.js applies it before the first paint.
 */
import { computed, ref } from 'vue'

export type ThemeChoice = 'system' | 'light' | 'dark'

export const THEME_KEY = 'stretto-console:theme'

function stored(): ThemeChoice {
  try {
    const value = localStorage.getItem(THEME_KEY)
    return value === 'light' || value === 'dark' ? value : 'system'
  } catch {
    return 'system'
  }
}

const media = typeof matchMedia === 'function' ? matchMedia('(prefers-color-scheme: dark)') : null
const systemDark = ref(media?.matches ?? false)
media?.addEventListener?.('change', (e) => (systemDark.value = e.matches))

export const theme = ref<ThemeChoice>(stored())

/** The theme on screen now. */
export const resolvedTheme = computed<'light' | 'dark'>(() =>
  theme.value === 'system' ? (systemDark.value ? 'dark' : 'light') : theme.value,
)

export function applyTheme(choice: ThemeChoice): void {
  const root = document.documentElement
  if (choice === 'system') root.removeAttribute('data-theme')
  else root.setAttribute('data-theme', choice)
}

export function setTheme(choice: ThemeChoice): void {
  theme.value = choice
  try {
    if (choice === 'system') localStorage.removeItem(THEME_KEY)
    else localStorage.setItem(THEME_KEY, choice)
  } catch {
    // Not stored: it still applies until the page is closed.
  }
  applyTheme(choice)
}

/** Light, then dark, then the system's. */
export function cycleTheme(): ThemeChoice {
  const order: ThemeChoice[] = ['system', 'light', 'dark']
  const nextChoice = order[(order.indexOf(theme.value) + 1) % order.length]!
  setTheme(nextChoice)
  return nextChoice
}

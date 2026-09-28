/**
 * The domain filter in the top bar: one domain (a server's name) or all of
 * them. Lists and the overview show only that domain's sessions, flows and
 * servers. Kept per browser.
 */
import { ref, watch } from 'vue'

const KEY = 'stretto-console:domain'

function stored(): string | null {
  try {
    return localStorage.getItem(KEY) || null
  } catch {
    return null
  }
}

export const domainFilter = ref<string | null>(stored())

watch(domainFilter, (value) => {
  try {
    if (value) localStorage.setItem(KEY, value)
    else localStorage.removeItem(KEY)
  } catch {
    // Not kept past this page.
  }
})

/** Whether an item of `domain` passes the filter. */
export function inDomain(domain: string | null | undefined): boolean {
  return !domainFilter.value || domain === domainFilter.value
}

/** The domains the filter offers: every domain with sessions, and every registered server. */
export const domainNames = ref<string[]>([])

export function setDomainNames(names: string[]): void {
  domainNames.value = [...new Set(names.filter(Boolean))].sort((a, b) => a.localeCompare(b))
}

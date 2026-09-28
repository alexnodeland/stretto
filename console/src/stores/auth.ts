/**
 * Who the console is talking to: /api/meta (version, data dir, read-only,
 * auth), and whether the browser holds the token. A 401 from any request
 * shows the sign-in screen.
 */
import { computed, ref } from 'vue'
import { api, ApiError } from '@/api/client'
import type { Meta } from '@/api/types'

export type AuthState = 'unknown' | 'signed-in' | 'signed-out' | 'unreachable'

export const meta = ref<Meta | null>(null)
export const authState = ref<AuthState>('unknown')
export const metaError = ref<string | null>(null)

/** Writes and actions are refused: --read-only. */
export const readOnly = computed(() => meta.value?.read_only ?? false)

export function markSignedOut(): void {
  authState.value = 'signed-out'
}

export async function loadMeta(): Promise<void> {
  try {
    meta.value = await api.meta({ quiet: true })
    authState.value = 'signed-in'
    metaError.value = null
  } catch (e) {
    if (e instanceof ApiError && e.status === 401) authState.value = 'signed-out'
    else {
      authState.value = 'unreachable'
      metaError.value = e instanceof Error ? e.message : String(e)
    }
  }
}

/**
 * Check a token, then hand it to the server as `?token=`: the server sets its
 * HttpOnly cookie and redirects to the same page without the token.
 * Resolves to an error message when the token is refused.
 */
export async function signIn(
  token: string,
  here: Location = window.location,
): Promise<string | null> {
  const value = token.trim()
  if (!value) return 'Paste the token stretto-console printed when it started.'
  let response: Response
  try {
    response = await fetch('/api/meta', {
      headers: { Accept: 'application/json', Authorization: `Bearer ${value}` },
      credentials: 'include',
    })
  } catch {
    return 'The console server did not answer. Is stretto-console running?'
  }
  if (response.status === 401)
    return 'That token was not accepted. Check it against the one stretto-console printed.'
  if (!response.ok) return `The console answered ${response.status}.`
  const params = new URLSearchParams(here.search)
  params.set('token', value)
  here.assign(`${here.pathname}?${params.toString()}${here.hash}`)
  return null
}

/**
 * Sign out: the cookie is HttpOnly, so the page cannot clear it itself; it
 * asks the server to replace it with an empty token (`?token=`), which the
 * server's cookie rule sets like any other, and every request is then a 401.
 */
export function signOut(here: Location = window.location): void {
  here.assign('/?token=')
}

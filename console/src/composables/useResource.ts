/**
 * A resource loaded from the API: its data, the first load's error (shown as
 * the page's error state), and refreshes when the server says what it holds
 * changed. A refresh keeps the data on screen; one that fails keeps it too and
 * says so in a toast.
 */
import {
  onScopeDispose,
  ref,
  shallowRef,
  watch,
  type Ref,
  type ShallowRef,
  type WatchSource,
} from 'vue'
import { ApiError, type RequestOptions } from '@/api/client'
import { onChanged } from '@/api/events'
import type { Changed, ChangedWhat } from '@/api/types'
import { toast } from '@/stores/toasts'

export interface ResourceOptions {
  /** Load again when these change (a route param, a filter). */
  watch?: WatchSource[]
  /** Load again on these `changed` events. */
  events?: ChangedWhat[]
  /** Only for events whose keys pass (e.g. the session on screen). */
  filter?: (event: Changed) => boolean
  /** Wait for the first load until this is true. */
  enabled?: () => boolean
}

export interface Resource<T> {
  data: ShallowRef<T | null>
  error: Ref<ApiError | null>
  /** The first load, or a load after the inputs changed: show skeletons. */
  loading: Ref<boolean>
  /** A load with data on screen: hold the frame. */
  refreshing: Ref<boolean>
  refresh: () => Promise<void>
}

export function useResource<T>(
  fetcher: (options: RequestOptions) => Promise<T>,
  options: ResourceOptions = {},
): Resource<T> {
  const data = shallowRef<T | null>(null)
  const error = ref<ApiError | null>(null)
  const loading = ref(true)
  const refreshing = ref(false)
  let controller: AbortController | null = null
  let generation = 0

  async function load(kind: 'reset' | 'refresh'): Promise<void> {
    if (options.enabled && !options.enabled()) return
    controller?.abort()
    const mine = new AbortController()
    controller = mine
    const run = ++generation
    const background = kind === 'refresh' && data.value !== null
    if (background) refreshing.value = true
    else loading.value = true
    try {
      const value = await fetcher({ signal: mine.signal, quiet: true })
      if (run !== generation) return
      data.value = value
      error.value = null
    } catch (e) {
      if (run !== generation || (e instanceof DOMException && e.name === 'AbortError')) return
      const err =
        e instanceof ApiError ? e : new ApiError(0, e instanceof Error ? e.message : String(e))
      if (err.status === 401) return
      if (background) {
        toast({ kind: 'error', title: 'Could not refresh', message: err.message })
      } else {
        error.value = err
        data.value = null
      }
    } finally {
      if (run === generation) {
        loading.value = false
        refreshing.value = false
      }
    }
  }

  let pending: ReturnType<typeof setTimeout> | null = null
  function schedule(): void {
    // Changes come in bursts (a session being written): refresh once.
    if (pending) return
    pending = setTimeout(() => {
      pending = null
      void load('refresh')
    }, 250)
  }

  const stops: (() => void)[] = []
  for (const what of options.events ?? []) {
    stops.push(
      onChanged(what, (event) => {
        if (!options.filter || options.filter(event)) schedule()
      }),
    )
  }
  if (options.watch?.length) {
    stops.push(watch(options.watch, () => void load('reset')))
  }
  onScopeDispose(() => {
    stops.forEach((stop) => stop())
    if (pending) clearTimeout(pending)
    controller?.abort()
  })

  void load('reset')

  return { data, error, loading, refreshing, refresh: () => load('refresh') }
}

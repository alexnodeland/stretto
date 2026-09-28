/**
 * Live updates from `GET /api/events` (Server-Sent Events): `changed` when
 * sessions, flows, servers or jobs change on disk, and `job` with a job's
 * progress. One EventSource for the whole app; pages subscribe by topic.
 */
import { ref, type Ref } from 'vue'
import type { Changed, ChangedWhat, Job } from './types'

export type LiveStatus = 'connecting' | 'live' | 'reconnecting' | 'off'

type ChangedListener = (event: Changed) => void
type JobListener = (job: Job) => void

const changedListeners = new Map<ChangedWhat, Set<ChangedListener>>()
const jobListeners = new Set<JobListener>()

export const liveStatus: Ref<LiveStatus> = ref('off')

let source: EventSource | null = null
let retry: ReturnType<typeof setTimeout> | null = null
let attempts = 0
let onLost: (() => void) | null = null

/** Subscribe to `changed` events about one kind of thing. Returns the unsubscribe. */
export function onChanged(what: ChangedWhat, listener: ChangedListener): () => void {
  let set = changedListeners.get(what)
  if (!set) {
    set = new Set()
    changedListeners.set(what, set)
  }
  set.add(listener)
  return () => set.delete(listener)
}

/** Subscribe to `job` events (a job's progress). Returns the unsubscribe. */
export function onJob(listener: JobListener): () => void {
  jobListeners.add(listener)
  return () => jobListeners.delete(listener)
}

/** Deliver an event as if it came from the server (the mock and the tests use it). */
export function emitChanged(event: Changed): void {
  changedListeners.get(event.what)?.forEach((listener) => listener(event))
}

export function emitJob(job: Job): void {
  jobListeners.forEach((listener) => listener(job))
}

function parse<T>(data: string): T | null {
  try {
    return JSON.parse(data) as T
  } catch {
    return null
  }
}

/**
 * Open the stream. `lost` runs when the browser gives up on it (a closed
 * connection, as after a 401), so the app can check whether it is signed out.
 */
export function connectEvents(lost?: () => void): void {
  onLost = lost ?? null
  if (source || typeof EventSource === 'undefined') return
  liveStatus.value = attempts === 0 ? 'connecting' : 'reconnecting'
  const es = new EventSource('/api/events', { withCredentials: true })
  source = es
  es.onopen = () => {
    attempts = 0
    liveStatus.value = 'live'
  }
  es.addEventListener('changed', (e) => {
    const event = parse<Changed>((e as MessageEvent<string>).data)
    if (event && typeof event.what === 'string')
      emitChanged({ what: event.what, keys: event.keys ?? [] })
  })
  es.addEventListener('job', (e) => {
    const job = parse<Job>((e as MessageEvent<string>).data)
    if (job && typeof job.id === 'string') emitJob(job)
  })
  es.onerror = () => {
    if (es.readyState === EventSource.CLOSED) {
      // The browser will not retry: back off and open it again.
      source = null
      liveStatus.value = 'reconnecting'
      attempts += 1
      onLost?.()
      const delay = Math.min(30_000, 1000 * 2 ** Math.min(attempts, 5))
      if (retry) clearTimeout(retry)
      retry = setTimeout(() => {
        retry = null
        connectEvents(onLost ?? undefined)
      }, delay)
    } else {
      liveStatus.value = 'reconnecting'
    }
  }
}

export function disconnectEvents(): void {
  if (retry) clearTimeout(retry)
  retry = null
  source?.close()
  source = null
  attempts = 0
  liveStatus.value = 'off'
}

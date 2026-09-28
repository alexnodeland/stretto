import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { effectScope, nextTick } from 'vue'
import { clearToasts, dismissToast, toast, toasts } from '@/stores/toasts'
import { useResource } from '@/composables/useResource'
import { emitChanged } from '@/api/events'
import { ApiError } from '@/api/client'
import { cycleTheme, setTheme, theme } from '@/stores/theme'
import { noteJob, noteJobs, runningJobs } from '@/stores/jobs'
import type { Job } from '@/api/types'

describe('toasts', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    clearToasts()
  })
  afterEach(() => vi.useRealTimers())

  it('come and go on their own, errors staying longer', () => {
    toast({ title: 'Copied', kind: 'success' })
    toast({ title: 'Failed', kind: 'error', message: 'no' })
    expect(toasts.map((t) => t.title)).toEqual(['Copied', 'Failed'])
    vi.advanceTimersByTime(5000)
    expect(toasts.map((t) => t.title)).toEqual(['Failed'])
    vi.advanceTimersByTime(5000)
    expect(toasts).toHaveLength(0)
  })

  it('keep one copy of the same message, and at most four', () => {
    toast({ title: 'Same' })
    toast({ title: 'Same' })
    expect(toasts).toHaveLength(1)
    for (let i = 0; i < 6; i++) toast({ title: `t${i}` })
    expect(toasts).toHaveLength(4)
    dismissToast(toasts[0]!.id)
    expect(toasts).toHaveLength(3)
  })
})

describe('useResource', () => {
  it('loads, then refreshes on the events it follows, keeping the data on screen', async () => {
    vi.useFakeTimers()
    let n = 0
    const fetcher = vi.fn(async () => ++n)
    const scope = effectScope()
    const r = scope.run(() => useResource(fetcher, { events: ['sessions'] }))!
    expect(r.loading.value).toBe(true)
    await vi.runAllTimersAsync()
    expect(r.data.value).toBe(1)
    expect(r.loading.value).toBe(false)
    emitChanged({ what: 'flows', keys: [] })
    emitChanged({ what: 'sessions', keys: ['a'] })
    emitChanged({ what: 'sessions', keys: ['b'] })
    await vi.advanceTimersByTimeAsync(300)
    expect(fetcher).toHaveBeenCalledTimes(2)
    expect(r.data.value).toBe(2)
    scope.stop()
    vi.useRealTimers()
  })

  it('shows the first load’s error, and keeps data when a refresh fails', async () => {
    const scope = effectScope()
    let fail = true
    const r = scope.run(() =>
      useResource(async () => {
        if (fail) throw new ApiError(500, 'the data dir is not readable')
        return 'ok'
      }),
    )!
    await nextTick()
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(r.error.value?.message).toBe('the data dir is not readable')
    fail = false
    await r.refresh()
    expect(r.data.value).toBe('ok')
    expect(r.error.value).toBeNull()
    scope.stop()
  })
})

describe('the theme', () => {
  it('cycles system, light, dark and sets data-theme', () => {
    setTheme('system')
    expect(document.documentElement.hasAttribute('data-theme')).toBe(false)
    expect(cycleTheme()).toBe('light')
    expect(document.documentElement.getAttribute('data-theme')).toBe('light')
    expect(cycleTheme()).toBe('dark')
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark')
    expect(localStorage.getItem('stretto-console:theme')).toBe('dark')
    expect(cycleTheme()).toBe('system')
    expect(theme.value).toBe('system')
    expect(localStorage.getItem('stretto-console:theme')).toBeNull()
  })
})

describe('running jobs', () => {
  it('counts the queued and running ones as events come in', () => {
    const job = (id: string, status: Job['status']) => ({ id, status }) as Job
    noteJobs([job('a', 'succeeded'), job('b', 'running')])
    expect(runningJobs.value).toBe(1)
    expect(noteJob(job('c', 'queued'))).toBeUndefined()
    expect(runningJobs.value).toBe(2)
    expect(noteJob(job('b', 'failed'))).toBe('running')
    expect(runningJobs.value).toBe(1)
  })
})

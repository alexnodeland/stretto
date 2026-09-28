import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import ProbBar from '@/components/ui/ProbBar.vue'
import ModeBadge from '@/components/ModeBadge.vue'
import KindBadge from '@/components/KindBadge.vue'
import JobStatus from '@/components/JobStatus.vue'
import UiPagination from '@/components/ui/UiPagination.vue'
import RichText from '@/components/RichText.vue'

describe('ProbBar', () => {
  it('shows a lookup’s value against the threshold, petrol when it acts', () => {
    const w = mount(ProbBar, { props: { value: 0.8685, threshold: 0.3, acts: true } })
    expect(w.text()).toContain('0.87')
    expect(w.text()).toContain('≥ 0.30')
    expect(w.find('.pb-fill').classes()).toContain('acts')
    expect(w.find('.pb-fill').attributes('style')).toContain('width: 86.85%')
    expect(w.find('.pb-tick').attributes('style')).toContain('left: 30%')
    expect(w.find('[role="meter"]').attributes('aria-label')).toBe(
      '0.87 against a threshold of 0.30',
    )
  })

  it('says “below” in words, not only in color', () => {
    const w = mount(ProbBar, { props: { value: 0.0528, threshold: 0.3, acts: false } })
    expect(w.text()).toContain('0.05')
    expect(w.text()).toContain('< 0.30')
    expect(w.find('.pb-fill').classes()).not.toContain('acts')
  })
})

describe('badges', () => {
  it('names each mode as the lists do', () => {
    expect(mount(ModeBadge, { props: { mode: 'served' } }).text()).toBe('Served')
    expect(mount(ModeBadge, { props: { mode: 'shadow' } }).text()).toBe('Shadow')
    expect(mount(ModeBadge, { props: { mode: 'recorded' } }).text()).toBe('Recorded')
    expect(mount(ModeBadge, { props: { mode: 'serve' } }).text()).toBe('Serving')
    expect(mount(ModeBadge, { props: { mode: 'record' } }).text()).toBe('Recording')
    expect(
      mount(ModeBadge, { props: { mode: 'shadow' } })
        .find('.badge')
        .classes(),
    ).toContain('badge-dashed')
  })

  it('flags write tools, and says a tool with no hint is neither', () => {
    const write = mount(KindBadge, { props: { kind: 'write' } })
    expect(write.text()).toBe('write')
    expect(write.find('svg').exists()).toBe(true)
    expect(write.attributes('title')).toMatch(/never calls it/)
    expect(mount(KindBadge, { props: { kind: 'generic' } }).text()).toBe('neither')
    expect(mount(KindBadge, { props: { kind: 'read' } }).attributes('title')).toMatch(
      /may look it up/,
    )
  })

  it('says a job’s status in words', () => {
    expect(mount(JobStatus, { props: { status: 'failed' } }).text()).toBe('Failed')
    expect(mount(JobStatus, { props: { status: 'running' } }).text()).toBe('Running')
    expect(mount(JobStatus, { props: { status: 'queued' } }).text()).toBe('Queued')
    expect(mount(JobStatus, { props: { status: 'cancelled' } }).text()).toBe('Cancelled')
    // stretto drift's alarm: it succeeded, and found the agent changed.
    expect(mount(JobStatus, { props: { status: 'succeeded', alarm: true } }).text()).toBe('Alarm')
    expect(mount(JobStatus, { props: { status: 'succeeded', alarm: false } }).text()).toBe(
      'Succeeded',
    )
  })
})

describe('UiPagination', () => {
  it('counts the range and pages by the limit', async () => {
    const w = mount(UiPagination, {
      props: { total: 312, limit: 50, offset: 50, noun: 'sessions' },
    })
    expect(w.text()).toContain('51–100 of 312 sessions')
    const [newer, older] = w.findAll('button')
    await newer!.trigger('click')
    await older!.trigger('click')
    expect(w.emitted('update:offset')).toEqual([[0], [100]])
  })

  it('stops at the ends', () => {
    const w = mount(UiPagination, { props: { total: 60, limit: 50, offset: 50 } })
    const [newer, older] = w.findAll('button')
    expect(newer!.attributes('disabled')).toBeUndefined()
    expect(older!.attributes('disabled')).toBeDefined()
  })
})

describe('RichText', () => {
  it('sets backticked words as code', () => {
    const w = mount(RichText, {
      props: { text: 'Run it with `--scope user` or `--scope project`.' },
    })
    expect(w.findAll('code').map((c) => c.text())).toEqual(['--scope user', '--scope project'])
    expect(w.text()).toBe('Run it with --scope user or --scope project.')
  })
})

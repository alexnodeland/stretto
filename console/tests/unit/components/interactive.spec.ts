import { afterEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import UiTabs from '@/components/ui/UiTabs.vue'
import ThresholdSlider from '@/components/flow/ThresholdSlider.vue'
import SignInScreen from '@/components/layout/SignInScreen.vue'
import * as auth from '@/stores/auth'

describe('UiTabs', () => {
  const tabs = [
    { id: 'timeline', label: 'Timeline' },
    { id: 'decisions', label: 'Decisions', count: 5 },
    { id: 'raw', label: 'Raw log' },
  ]

  it('marks the selected tab and wires its panel', () => {
    const w = mount(UiTabs, {
      props: { tabs, modelValue: 'decisions', idPrefix: 's', label: 'Views' },
    })
    const selected = w.find('[aria-selected="true"]')
    expect(selected.text()).toContain('Decisions')
    expect(selected.text()).toContain('5')
    expect(selected.attributes('aria-controls')).toBe('s-panel-decisions')
    expect(selected.attributes('tabindex')).toBe('0')
    expect(w.findAll('[tabindex="-1"]')).toHaveLength(2)
  })

  it('moves with the arrow keys, Home and End, wrapping around', async () => {
    const w = mount(UiTabs, {
      props: { tabs, modelValue: 'timeline', idPrefix: 's', label: 'Views' },
      attachTo: document.body,
    })
    const first = w.findAll('button')[0]!
    await first.trigger('keydown', { key: 'ArrowRight' })
    await first.trigger('keydown', { key: 'ArrowLeft' })
    await first.trigger('keydown', { key: 'End' })
    await first.trigger('keydown', { key: 'Home' })
    expect(w.emitted('update:modelValue')).toEqual([['decisions'], ['raw'], ['raw'], ['timeline']])
    w.unmount()
  })
})

describe('ThresholdSlider', () => {
  it('rounds to two places and keeps within 0 and 1', async () => {
    const w = mount(ThresholdSlider, { props: { modelValue: 0.3 } })
    const range = w.find('input[type="range"]')
    ;(range.element as HTMLInputElement).value = '0.456'
    await range.trigger('input')
    const number = w.find('input[type="number"]')
    ;(number.element as HTMLInputElement).value = '7'
    await number.trigger('change')
    expect(w.emitted('update:modelValue')).toEqual([[0.46], [1]])
  })

  it('goes back to the default, and marks it', async () => {
    const w = mount(ThresholdSlider, {
      props: { modelValue: 0.55, marks: [{ value: 0.5, label: '0.5 promoted at' }] },
    })
    expect(w.text()).toContain('0.3 default')
    expect(w.text()).toContain('0.5 promoted at')
    await w.find('button').trigger('click')
    expect(w.emitted('update:modelValue')).toEqual([[0.3]])
    const atDefault = mount(ThresholdSlider, { props: { modelValue: 0.3 } })
    expect(atDefault.find('button').attributes('disabled')).toBeDefined()
  })
})

describe('the sign-in screen', () => {
  afterEach(() => vi.unstubAllGlobals())

  function fakeLocation(path = '/sessions', search = '?mode=served') {
    return { pathname: path, search, hash: '', assign: vi.fn() } as unknown as Location
  }

  it('checks the token, then hands it to the server as ?token= to set the cookie', async () => {
    const fetch = vi.fn(async () => new Response('{}', { status: 200 }))
    vi.stubGlobal('fetch', fetch)
    const here = fakeLocation()
    expect(await auth.signIn(' abc123 ', here)).toBeNull()
    expect(fetch).toHaveBeenCalledWith(
      '/api/meta',
      expect.objectContaining({
        headers: expect.objectContaining({ Authorization: 'Bearer abc123' }),
      }),
    )
    expect(here.assign).toHaveBeenCalledWith('/sessions?mode=served&token=abc123')
  })

  it('says when the token is refused, and when there is none', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response('{}', { status: 401 })),
    )
    const here = fakeLocation()
    expect(await auth.signIn('wrong', here)).toMatch(/not accepted/)
    expect(await auth.signIn('   ', here)).toMatch(/Paste the token/)
    expect(here.assign).not.toHaveBeenCalled()
  })

  it('shows the refusal under the field', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response('{}', { status: 401 })),
    )
    const w = mount(SignInScreen)
    await w.find('input').setValue('wrong')
    await w.find('form').trigger('submit')
    await new Promise((r) => setTimeout(r, 0))
    expect(w.find('[role="alert"]').text()).toMatch(/not accepted/)
    expect(w.find('input').attributes('aria-invalid')).toBe('true')
  })

  it('signs out by replacing the cookie with an empty token', () => {
    const here = fakeLocation()
    auth.signOut(here)
    expect(here.assign).toHaveBeenCalledWith('/?token=')
  })
})

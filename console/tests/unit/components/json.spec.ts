import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import UiJson from '@/components/ui/UiJson.vue'

const value = {
  email: 'c41@example.com',
  orders: ['#W41a', '#W41b'],
  payment_methods: { credit_card_41: { source: 'credit_card' } },
  n: 3,
  ok: true,
  none: null,
}

describe('the JSON tree', () => {
  it('shows keys and values, strings quoted', () => {
    const w = mount(UiJson, { props: { value, label: 'Result' } })
    expect(w.text()).toContain('Result')
    expect(w.text()).toContain('email')
    expect(w.text()).toContain('"c41@example.com"')
    expect(w.text()).toContain('"#W41a"')
    expect(w.find('.jn-null').text()).toBe('null')
    expect(w.find('.jn-boolean').text()).toBe('true')
  })

  it('opens two levels, and folds a branch with its size', async () => {
    const w = mount(UiJson, { props: { value, depth: 2 } })
    expect(w.text()).not.toContain('"credit_card"')
    const nested = w.findAll('button.jn-toggle').find((b) => b.text().includes('credit_card_41'))!
    expect(nested.attributes('aria-expanded')).toBe('false')
    expect(nested.text()).toContain('1 key')
    await nested.trigger('click')
    expect(w.text()).toContain('"credit_card"')
  })

  it('expands and collapses everything', async () => {
    const w = mount(UiJson, { props: { value, depth: 1 } })
    expect(w.text()).not.toContain('"#W41a"')
    const all = w.find('button.json-tool')
    expect(all.text()).toBe('Expand all')
    await all.trigger('click')
    expect(w.text()).toContain('"#W41a"')
    expect(w.text()).toContain('"credit_card"')
    expect(all.text()).toBe('Collapse all')
    await all.trigger('click')
    expect(w.text()).not.toContain('"#W41a"')
  })

  it('shows a scalar on its own', () => {
    expect(
      mount(UiJson, { props: { value: 'user_41' } })
        .find('.jn-string')
        .text(),
    ).toBe('"user_41"')
  })
})

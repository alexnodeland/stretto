import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import SessionTimeline from '@/components/session/SessionTimeline.vue'
import DecisionsTable from '@/components/session/DecisionsTable.vue'
import ActivityChart from '@/components/ActivityChart.vue'
import SitesTable from '@/components/flow/SitesTable.vue'
import BindingsList from '@/components/flow/BindingsList.vue'
import FlowToolsTable from '@/components/flow/FlowToolsTable.vue'
import { previewSites } from '@/lib/flow'
import { servedShopSession, shopSpec } from '../../../mock/fixtures/world.ts'
import { buildFlows, detailAt } from '../../../mock/fixtures/flows.ts'
import type { DayActivity } from '@/api/types'

const mid = (a: number, b: number) => (a + b) / 2
const rec = servedShopSession(
  shopSpec(1_790_000_000_000, 4242, 'served', 'logs/shop'),
  41,
  'cancel',
  mid,
)
const detail = rec.detail(rec.spec.session)
const flows = buildFlows({
  shopCompiled: 1,
  promotedCompiled: 2,
  retailCopied: 3,
  scratchModified: 4,
})

describe('the session timeline', () => {
  it('shows the customer, the agent’s calls and what stretto read ahead under them', () => {
    const w = mount(SessionTimeline, { props: { detail, threshold: 0.3 } })
    const text = w.text()
    expect(text).toContain('Hi, I’m c41@example.com'.replace('’', "'"))
    expect(text).toContain('Turn 1')
    expect(text).toContain('find_user_id_by_email')
    expect(text).toContain('Read ahead by stretto: 3 lookups')
    expect(w.findAll('[data-testid="decision-made"]')).toHaveLength(3)
    expect(w.findAll('[data-testid="decision-back"]')).toHaveLength(2)
    expect(text).toContain('get_order_details at 0.05, below 0.3')
    expect(text).toContain('the site is not promoted')
    expect(text).toContain('0.87')
    expect(text).toContain('≥ 0.30')
  })

  it('opens a call to show its arguments and result', async () => {
    const w = mount(SessionTimeline, { props: { detail, threshold: 0.3 } })
    const toggle = w.find('.call-toggle')
    expect(toggle.attributes('aria-expanded')).toBe('false')
    await toggle.trigger('click')
    expect(toggle.attributes('aria-expanded')).toBe('true')
    expect(w.text()).toContain('Arguments')
    expect(w.text()).toContain('"c41@example.com"')
  })

  it('expands every call and lookup at once', async () => {
    const w = mount(SessionTimeline, { props: { detail, threshold: 0.3 } })
    await w.find('.tl-expand').trigger('click')
    expect(w.findAll('.call-toggle[aria-expanded="true"]')).toHaveLength(2)
    expect(w.findAll('.dr-toggle[aria-expanded="true"]')).toHaveLength(3)
    expect(w.text()).toContain('"#W41b"')
  })

  it('lists every decision in the table', () => {
    const w = mount(DecisionsTable, { props: { detail, threshold: 0.3 } })
    const rows = w.findAll('tr.decision')
    expect(rows).toHaveLength(5)
    expect(rows[0]!.text()).toContain('looks up')
    expect(rows[0]!.text()).toContain('get_user_details')
    expect(rows[3]!.text()).toContain('hands back')
    expect(rows[3]!.text()).toContain('below 0.3')
    // One group per call the flow decided after, headed by that call.
    const groups = w.findAll('tr.group')
    expect(groups).toHaveLength(2)
    expect(groups[0]!.text()).toContain('find_user_id_by_email')
    expect(groups[1]!.text()).toContain('cancel_pending_order')
  })
})

describe('the activity chart', () => {
  const days: DayActivity[] = Array.from({ length: 14 }, (_, i) => ({
    day: `2026-09-${String(15 + i).padStart(2, '0')}`,
    sessions: i % 3,
    tool_calls: 5 + i,
    flow_lookups: i > 6 ? 3 : 0,
  }))

  it('draws the agent’s calls and the reads stretto made for each day', () => {
    const w = mount(ActivityChart, { props: { days } })
    expect(w.findAll('.bar-agent')).toHaveLength(14)
    expect(w.findAll('.bar-lookups')).toHaveLength(7)
    expect(w.text()).toContain('Agent’s calls')
    expect(w.text()).toContain('Read ahead by stretto')
    expect(w.find('[role="img"]').attributes('aria-label')).toContain(
      '161 by the agent and 21 read ahead by stretto',
    )
  })

  it('reads out a day from the keyboard', async () => {
    const w = mount(ActivityChart, { props: { days } })
    await w.find('.plot-focus').trigger('keydown', { key: 'End' })
    expect(w.find('.tip').text()).toContain('Mon 28 Sep')
    expect(w.find('.tip').text()).toContain('18 agent’s calls')
    await w.find('.plot-focus').trigger('keydown', { key: 'Escape' })
    expect(w.find('.tip').exists()).toBe(false)
  })

  it('has a table with every value', async () => {
    const w = mount(ActivityChart, { props: { days } })
    await w.find('.view-toggle').trigger('click')
    const rows = w.findAll('tbody tr')
    expect(rows).toHaveLength(14)
    expect(rows[0]!.text()).toContain('Mon 28 Sep')
  })
})

describe('the flow’s tables', () => {
  it('says what the flow does after each call at the threshold', () => {
    const d = detailAt(
      flows.find((f) => f.summary.key === 'retail')!,
      0.3,
    )
    const w = mount(SitesTable, { props: { previews: previewSites(d, 0.3), promoted: false } })
    const product = w.find('[data-testid="site-get_product_details"]')
    expect(product.text()).toContain('hands back: 0.13 is below 0.30')
    expect(product.text()).toContain('75%')
    const orders = w.find('[data-testid="site-get_order_details"]')
    expect(orders.text()).toContain('looks up get_order_details')
    expect(orders.text()).toContain('78%')
    expect(orders.text()).toContain('9 steps in training')
  })

  it('shows each binding as argument ← tool path (count)', () => {
    const d = detailAt(
      flows.find((f) => f.summary.key === 'shop')!,
      0.3,
    )
    const w = mount(BindingsList, { props: { bindings: d.bindings } })
    const orders = w.find('[data-testid="binding-get_order_details"]')
    expect(orders.text()).toContain('order_id')
    expect(orders.text()).toContain('get_user_details')
    expect(orders.text()).toContain('$.orders[*]')
    expect(orders.text()).toContain('(12 · 100%)')
    expect(w.find('[data-testid="binding-find_user_id_by_email"]').text()).toContain('no source')
  })

  it('lists reads first and flags writes as never called', () => {
    const d = detailAt(
      flows.find((f) => f.summary.key === 'shop')!,
      0.3,
    )
    const w = mount(FlowToolsTable, { props: { tools: d.tools } })
    const rows = w.findAll('tbody tr')
    expect(rows.map((r) => r.find('.tool').text())).toEqual([
      'find_user_id_by_email',
      'get_order_details',
      'get_user_details',
      'cancel_pending_order',
    ])
    expect(rows[3]!.text()).toContain('never calls it')
    expect(rows[0]!.text()).toContain('may call it')
  })
})

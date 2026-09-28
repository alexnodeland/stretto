import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import SessionTimeline from '@/components/session/SessionTimeline.vue'
import DecisionsTable from '@/components/session/DecisionsTable.vue'
import ActivityChart from '@/components/ActivityChart.vue'
import SitesTable from '@/components/flow/SitesTable.vue'
import BindingsList from '@/components/flow/BindingsList.vue'
import FlowToolsTable from '@/components/flow/FlowToolsTable.vue'
import StageComparison from '@/components/flow/StageComparison.vue'
import FlowVersions from '@/components/flow/FlowVersions.vue'
import DiffLists from '@/components/flow/DiffLists.vue'
import { comparison, counts, shopStage } from '../../../mock/fixtures/stage.ts'
import { diffFlows } from '../../../mock/fixtures/flows.ts'
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

describe('a staged flow', () => {
  const last = shopStage(1_790_000_000_000).last!

  it('compares the two flows site by site, with the share’s interval and the difference', () => {
    const w = mount(StageComparison, { props: { last } })
    const orders = w.find('[data-testid="stage-site-get_order_details"]')
    expect(orders.text()).toContain('3 lookups')
    expect(orders.text()).toContain('1 lookup')
    const total = w.find('[data-testid="stage-total"]')
    expect(total.text()).toContain('Every site')
    expect(total.text()).toContain('12 lookups')
    expect(total.text()).toContain('9 used (75%')
    expect(w.find('[data-testid="stage-delta"]').text()).toBe('+1 used, −2 detours')
    expect(w.text()).toContain('as stretto promote counts it')
    expect(w.text()).not.toContain('The proxy had already made')
  })

  it('says what the proxy had served, and what was left out', () => {
    const site = { site: 'a', committed: counts(3, 3, 1, 2), staged: counts(3, 3, 1, 1) }
    const w = mount(StageComparison, {
      props: { last: { ...comparison(1, 4, 4, 4, [site]), unanswered: [1, 0] } },
    })
    expect(w.text()).toContain('already made 2 of the committed flow’s lookups and 1 of the staged')
    expect(w.text()).toContain('1 decision of the committed flow’s')
    expect(w.find('[data-testid="stage-delta"]').text()).toBe('+1 detour')
  })

  it('leaves the committed flow out when there was none', () => {
    const w = mount(StageComparison, { props: { last: { ...last, committed: false } } })
    expect(w.findAll('thead th')).toHaveLength(2)
    expect(w.find('[data-testid="stage-delta"]').exists()).toBe(false)
  })

  it('lists the versions, the committed one first, and offers to roll back to the rest', async () => {
    const version = (n: number, kind: 'found' | 'commit' | 'rollback', current: boolean) => ({
      version: n,
      kind,
      unix_ms: 1_790_000_000_000 + n,
      restored: kind === 'rollback' ? 1 : null,
      note: kind === 'commit' ? 'reads the order first' : null,
      changes: kind === 'commit' ? ['the habit learned from 6 → 11 successful sessions'] : [],
      evidence: kind === 'commit' ? last : null,
      current,
    })
    const versions = [
      version(3, 'rollback', true),
      version(2, 'commit', false),
      version(1, 'found', false),
    ]
    const w = mount(FlowVersions, { props: { versions, readOnly: false } })
    expect(w.find('[data-testid="version-3"]').text()).toContain('rolled back to version 1')
    expect(w.find('[data-testid="version-3"]').text()).toContain('committed now')
    expect(w.find('[data-testid="rollback-3"]').exists()).toBe(false)
    const commit = w.find('[data-testid="version-2"]')
    expect(commit.text()).toContain('reads the order first')
    expect(commit.text()).toContain(
      'On the last 5 sessions before it, the staged flow’s lookups: 11, 10 used, 1 detour; the committed flow’s: 12, 9 used, 3 detours.',
    )
    expect(commit.text()).toContain('1 change from the version before')
    expect(w.find('[data-testid="version-1"]').text()).toContain('found in place')
    await w.find('[data-testid="rollback-1"]').trigger('click')
    expect(w.emitted('rollback')![0]).toEqual([versions[2]])
    const empty = mount(FlowVersions, { props: { versions: [], readOnly: true } })
    expect(empty.text()).toContain('No version has been committed')
  })

  it('lists what committing it would change', () => {
    const [shop, staged] = ['shop', 'shop.staged'].map((k) =>
      flows.find((f) => f.summary.key === k)!,
    )
    const diff = { from: 'shop', to: 'shop.staged', ...diffFlows(shop!, staged!) }
    const w = mount(DiffLists, { props: { diff } })
    expect(w.text()).toContain('Nothing needs review')
    expect(w.text()).toContain('Every change: 3')
    expect(w.text()).toContain('What the agent did next in training')
    const back = { from: 'shop.staged', to: 'shop', ...diffFlows(staged!, shop!) }
    const r = mount(DiffLists, { props: { diff: back } })
    expect(r.find('[data-testid="diff-review"]').text()).toContain('now acts after')
  })
})

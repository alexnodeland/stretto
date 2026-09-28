import { describe, expect, it } from 'vitest'
import { buildTimeline, decisionValue } from '@/lib/timeline'
import { Recorder } from '../../mock/fixtures/recorder.ts'
import { servedShopSession, shopSpec } from '../../mock/fixtures/world.ts'
import { SHOP_TOOLS, shop } from '../../mock/fixtures/tools.ts'

const mid = (a: number, b: number) => (a + b) / 2

function served(kind: 'cancel' | 'status' | 'typo') {
  const rec = servedShopSession(
    shopSpec(1_790_000_000_000, 4242, 'served', 'logs/shop'),
    41,
    kind,
    mid,
  )
  return rec.detail(rec.spec.session)
}

describe('a session, turn by turn', () => {
  it('puts the conversation where it fell, and each turn’s calls in order', () => {
    const detail = served('cancel')
    const entries = buildTimeline(detail)
    expect(
      entries.map((e) => (e.type === 'message' ? e.message.role : `turn ${e.turn.index + 1}`)),
    ).toEqual(['user', 'turn 1', 'assistant', 'user', 'turn 2', 'assistant'])
    const first = entries[1]!
    if (first.type !== 'turn') throw new Error('expected a turn')
    expect(first.calls.map((c) => c.call.tool)).toEqual(['find_user_id_by_email'])
  })

  it('nests the flow’s lookups under the call they followed, each with its decision', () => {
    const detail = served('status')
    const turn = buildTimeline(detail).find((e) => e.type === 'turn')!
    if (turn.type !== 'turn') throw new Error('expected a turn')
    const call = turn.calls[0]!
    expect(call.decisions.map((d) => d.decision.action)).toEqual([
      'lookup',
      'lookup',
      'lookup',
      'hand_back',
    ])
    expect(call.decisions.slice(0, 3).map((d) => d.lookup?.tool)).toEqual([
      'get_user_details',
      'get_order_details',
      'get_order_details',
    ])
    expect(call.decisions.slice(0, 3).every((d) => d.lookup?.after === call.call.id)).toBe(true)
    expect(call.decisions[3]!.decision.reason).toBe('get_order_details at 0.05, below 0.3')
    expect(call.run?.max_lookups).toBe(8)
  })

  it('weighs a decision as the lookup’s probability times its binding’s chance', () => {
    const detail = served('status')
    const values = detail.decisions.map((d) => decisionValue(d))
    expect(values[0]).toBeCloseTo(0.9925520080633771 * 0.875)
    expect(values[3]).toBeCloseTo(0.052769325969608226)
    expect(decisionValue({ prob: null, binding: null })).toBeNull()
  })

  it('keeps a failed call and the flow’s hand-back after it', () => {
    const detail = served('typo')
    const turns = buildTimeline(detail).filter((e) => e.type === 'turn')
    const first = turns[0]!
    if (first.type !== 'turn') throw new Error('expected a turn')
    expect(first.calls[0]!.call.ok).toBe(false)
    expect(first.calls[0]!.decisions[0]!.decision.site).toBe('find_user_id_by_email (error)')
  })

  it('gives a call nobody answered a turn of its own', () => {
    const rec = new Recorder({
      session: 's',
      started: 0,
      domain: 'shop',
      agentModel: null,
      client: { name: 'test', version: '1' },
      upstream: { kind: 'stdio', command: ['stretto-mcp-demo'] },
      server: { name: 'stretto-mcp-demo', version: '0.1.0', instructions: null },
      tools: SHOP_TOOLS,
      flow: 'none',
      dir: 'logs/shop',
    })
    rec.call('find_user_id_by_email', { email: 'c1@example.com' }, shop.findUser(1))
    rec.unanswered('get_user_details', { user_id: 'user_1' })
    const detail = rec.detail('s')
    detail.turns = detail.turns.slice(0, 1)
    const entries = buildTimeline(detail)
    expect(entries).toHaveLength(2)
    const last = entries[1]!
    if (last.type !== 'turn') throw new Error('expected a turn')
    expect(last.calls[0]!.call.ok).toBeNull()
  })
})

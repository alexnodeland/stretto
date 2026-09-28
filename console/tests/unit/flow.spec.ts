import { describe, expect, it } from 'vitest'
import { buildFlowGraph, edgeWidth, likeliest, nodeWidth, previewSites, siteGate } from '@/lib/flow'
import { buildFlows, detailAt } from '../../mock/fixtures/flows.ts'

const flows = buildFlows({
  shopCompiled: 1,
  promotedCompiled: 2,
  retailCopied: 3,
  scratchModified: 4,
})
const byKey = (key: string) => flows.find((f) => f.summary.key === key)!

describe('what a flow does at a threshold', () => {
  it('agrees with the server at the threshold it computed', () => {
    const detail = detailAt(byKey('retail'), 0.3)
    const previews = previewSites(detail, 0.3)
    expect(previews.map((p) => p.acts)).toEqual(detail.sites.map((s) => s.choice?.acts))
    expect(previews.filter((p) => p.acts)).toHaveLength(4)
  })

  it('previews another threshold with review::show’s rule', () => {
    const detail = detailAt(byKey('retail'), 0.3)
    // get_order_details after get_order_details weighs 0.78 × 0.92 = 0.71; the rest 0.75 and 0.92; get_product_details 0.13.
    expect(
      previewSites(detail, 0.72)
        .filter((p) => p.acts)
        .map((p) => p.site.tool),
    ).toEqual(['find_user_id_by_email', 'find_user_id_by_name_zip', 'get_user_details'])
    expect(previewSites(detail, 0.1).filter((p) => p.acts)).toHaveLength(5)
    expect(previewSites(detail, 0.95).filter((p) => p.acts)).toHaveLength(0)
    const below = previewSites(detail, 0.72).find((p) => p.site.tool === 'get_order_details')!
    expect(below.reason).toBe('0.71 is below 0.72')
  })

  it('takes the lookup with the largest share at a site', () => {
    const site = detailAt(byKey('retail'), 0.3).sites.find((s) => s.tool === 'get_order_details')!
    expect(likeliest(site)?.tool).toBe('get_order_details')
  })

  it('hands back where a promoted flow’s site was not promoted, or a site is switched off', () => {
    const detail = detailAt(byKey('shop-promoted'), 0.3)
    const site = {
      ...detail.sites[0]!,
      promoted: { ...detail.sites[0]!.promoted!, promoted: false },
    }
    expect(siteGate(detail, site, 0.3)).toMatchObject({ allowed: false, reason: 'not promoted' })
    expect(siteGate(detail, { ...detail.sites[0]!, threshold: 1.5 }, 0.3)).toMatchObject({
      allowed: false,
      reason: 'switched off',
    })
    expect(siteGate(detail, detail.sites[0]!, 0.3)).toMatchObject({ allowed: true, threshold: 0.3 })
  })
})

describe('the flow’s graph', () => {
  it('draws call → lookup edges, a loop for a repeated lookup, and marks what acts', () => {
    const detail = detailAt(byKey('shop'), 0.3)
    const graph = buildFlowGraph(previewSites(detail, 0.3), detail.tools)
    expect(graph.nodes.map((n) => n.id).sort()).toEqual([
      'find_user_id_by_email',
      'get_order_details',
      'get_user_details',
    ])
    const loop = graph.edges.find((e) => e.loop)!
    expect(loop.source).toBe('get_order_details')
    expect(loop.acts).toBe(true)
    expect(graph.edges.every((e) => e.acts)).toBe(true)
    const node = graph.nodes.find((n) => n.id === 'get_user_details')!
    expect(node).toMatchObject({ kind: 'read', lookup: true, site: true, acts: true })
    const x = (id: string) => graph.nodes.find((n) => n.id === id)!.x
    expect(x('find_user_id_by_email')).toBeLessThan(x('get_user_details'))
    expect(x('get_user_details')).toBeLessThan(x('get_order_details'))
  })

  it('runs top to bottom when asked, for a narrow screen', () => {
    const detail = detailAt(byKey('shop'), 0.3)
    const graph = buildFlowGraph(previewSites(detail, 0.3), detail.tools, 'TB')
    expect(graph.direction).toBe('TB')
    const at = (id: string) => graph.nodes.find((n) => n.id === id)!
    expect(at('find_user_id_by_email').y).toBeLessThan(at('get_user_details').y)
    expect(at('get_user_details').y).toBeLessThan(at('get_order_details').y)
    // Narrower nodes, and room on the right for the loop on get_order_details.
    expect(at('get_user_details').width).toBeLessThan(nodeWidth('get_user_details'))
    expect(graph.width).toBeGreaterThan(Math.max(...graph.nodes.map((n) => n.x + n.width)) + 24)
  })

  it('greys out what hands back at a higher threshold', () => {
    const detail = detailAt(byKey('shop'), 0.3)
    const graph = buildFlowGraph(previewSites(detail, 0.5), detail.tools)
    expect(graph.edges.find((e) => e.loop)!.acts).toBe(false)
    expect(graph.edges.filter((e) => e.acts)).toHaveLength(2)
  })

  it('sizes nodes by their names and edges by their shares', () => {
    expect(nodeWidth('x')).toBe(168)
    expect(nodeWidth('exchange_delivered_order_items')).toBeGreaterThan(260)
    expect(edgeWidth(0)).toBe(1.5)
    expect(edgeWidth(1)).toBe(7.5)
    expect(edgeWidth(2)).toBe(7.5)
  })
})

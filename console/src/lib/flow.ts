/**
 * What a flow does at a threshold, and its graph: after each call (a site),
 * the lookups it may make next. The server computes `acts` for the threshold
 * it was asked about; while the slider moves, the page previews it here with
 * review::show's rule, so the two agree: at each site the flow takes the
 * lookup with the largest share, and acts when that share times the chance
 * its bound arguments are the agent's reaches the threshold (the site's own,
 * if a search set one), where the site is active: not left out by a
 * promotion, nor switched off.
 */
import dagre from '@dagrejs/dagre'
import type { FlowDetail, FlowTool, LookupView, SiteView, ToolKind } from '@/api/types'

/**
 * Whether the flow may act after this site at all (the server's `active`),
 * why not, and the threshold it weighs lookups against there.
 */
export function siteGate(site: SiteView, threshold: number) {
  const t = site.threshold ?? threshold
  if (site.active) return { allowed: true, reason: null, threshold: t }
  return { allowed: false, reason: t > 1 ? 'switched off' : 'not promoted', threshold: t }
}

/** The lookup the flow weighs at a site: the largest weighed share, the first of equals. */
export function likeliest(site: SiteView): LookupView | null {
  let best: LookupView | null = null
  for (const lookup of site.lookups) {
    if (!best || lookup.weighed_share > best.weighed_share) best = lookup
  }
  return best
}

export interface SitePreview {
  site: SiteView
  choice: LookupView | null
  prob: number
  acts: boolean
  threshold: number
  /** Why the flow hands back here, in words. */
  reason: string | null
}

/** What the flow does after each site at `threshold`. */
export function previewSites(detail: FlowDetail, threshold: number): SitePreview[] {
  const exact = Math.abs(detail.threshold - threshold) < 1e-9
  return detail.sites.map((site) => {
    const gate = siteGate(site, threshold)
    const choice =
      (site.choice?.tool ? site.lookups.find((l) => l.tool === site.choice!.tool) : null) ??
      likeliest(site)
    const prob = site.choice?.tool ? site.choice.prob : (choice?.prob ?? 0)
    let acts: boolean
    if (exact && site.choice) acts = site.choice.acts
    else acts = gate.allowed && !!choice && prob >= gate.threshold
    let reason: string | null = null
    if (!acts) {
      if (!gate.allowed) reason = gate.reason
      else if (!choice) reason = 'no lookup offered'
      else if (!choice.bindable) reason = `cannot bind ${choice.tool}’s arguments`
      else reason = `${prob.toFixed(2)} is below ${gate.threshold.toFixed(2)}`
    }
    return { site, choice, prob, acts, threshold: gate.threshold, reason }
  })
}

/** Whether a lookup acts after a site, from the preview. */
export function lookupActs(preview: SitePreview, lookup: LookupView): boolean {
  return preview.acts && preview.choice?.tool === lookup.tool
}

export interface GraphNode {
  id: string
  tool: string
  failed: boolean
  kind: ToolKind | null
  /** A call the flow decides after. */
  site: boolean
  /** A lookup the flow may make. */
  lookup: boolean
  /** The target of a lookup that acts at this threshold. */
  acts: boolean
  x: number
  y: number
  width: number
  height: number
}

export interface GraphEdge {
  id: string
  source: string
  target: string
  count: number
  share: number
  weighedShare: number
  prob: number
  acts: boolean
  /** A lookup of the tool just called (after get_order_details, get_order_details again). */
  loop: boolean
  /** Stroke width: 1.5 px, plus up to 6 px by share. */
  width: number
}

/** Left to right on a wide screen; top to bottom on a narrow one, where a chain of calls fits. */
export type GraphDirection = 'LR' | 'TB'

export interface FlowGraph {
  nodes: GraphNode[]
  edges: GraphEdge[]
  width: number
  height: number
  direction: GraphDirection
}

export const NODE_HEIGHT = 58

/** A node wide enough for its tool name in the mono face. */
export function nodeWidth(label: string, direction: GraphDirection = 'LR'): number {
  return direction === 'LR'
    ? Math.round(Math.min(300, Math.max(168, 44 + label.length * 7.9)))
    : Math.round(Math.min(260, Math.max(120, 32 + label.length * 7.9)))
}

export function edgeWidth(share: number): number {
  return Math.round((1.5 + 6 * Math.max(0, Math.min(1, share))) * 10) / 10
}

function nodeId(tool: string, failed: boolean): string {
  return failed ? `${tool} (error)` : tool
}

/** The graph of call → lookup edges, laid out with dagre, left to right unless told otherwise. */
export function buildFlowGraph(
  previews: SitePreview[],
  tools: FlowTool[],
  direction: GraphDirection = 'LR',
): FlowGraph {
  const kinds = new Map(tools.map((t) => [t.name, t.kind]))
  const nodes = new Map<string, GraphNode>()
  const ensure = (tool: string, failed: boolean): GraphNode => {
    const id = nodeId(tool, failed)
    let node = nodes.get(id)
    if (!node) {
      node = {
        id,
        tool,
        failed,
        kind: kinds.get(tool) ?? null,
        site: false,
        lookup: false,
        acts: false,
        x: 0,
        y: 0,
        width: nodeWidth(id, direction),
        height: NODE_HEIGHT,
      }
      nodes.set(id, node)
    }
    return node
  }
  const edges: GraphEdge[] = []
  for (const preview of previews) {
    const { site } = preview
    const from = ensure(site.tool, site.failed)
    from.site = true
    for (const lookup of site.lookups) {
      const to = ensure(lookup.tool, false)
      to.lookup = true
      const acts = lookupActs(preview, lookup)
      if (acts) to.acts = true
      edges.push({
        id: `${from.id}→${to.id}`,
        source: from.id,
        target: to.id,
        count: lookup.count,
        share: lookup.share,
        weighedShare: lookup.weighed_share,
        prob: lookup.prob,
        acts,
        loop: from.id === to.id,
        width: edgeWidth(lookup.share),
      })
    }
  }

  const g = new dagre.graphlib.Graph()
  g.setGraph(
    direction === 'LR'
      ? { rankdir: 'LR', nodesep: 36, ranksep: 120, marginx: 24, marginy: 40 }
      : // Room on the right for a loop, which goes round a node's right side.
        { rankdir: 'TB', nodesep: 28, ranksep: 72, marginx: 24, marginy: 24 },
  )
  g.setDefaultEdgeLabel(() => ({}))
  for (const node of nodes.values()) g.setNode(node.id, { width: node.width, height: node.height })
  for (const edge of edges) {
    if (!edge.loop)
      g.setEdge(edge.source, edge.target, { weight: 1 + Math.round(edge.share * 4), minlen: 1 })
  }
  dagre.layout(g)
  let width = 0
  let height = 0
  for (const node of nodes.values()) {
    const placed = g.node(node.id) as { x: number; y: number } | undefined
    if (placed) {
      node.x = placed.x - node.width / 2
      node.y = placed.y - node.height / 2
    }
    width = Math.max(width, node.x + node.width)
    height = Math.max(height, node.y + node.height)
  }
  const loops = direction === 'TB' && edges.some((e) => e.loop) ? 88 : 0
  return {
    nodes: [...nodes.values()],
    edges,
    width: width + 24 + loops,
    height: height + (direction === 'LR' ? 40 : 24),
    direction,
  }
}

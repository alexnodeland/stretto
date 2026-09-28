/**
 * The mock's flows: the shop flow the quickstart learns (from six sessions,
 * with no key), the flow `stretto stage` learned again beside it from those
 * and five more, the same flow promoted on three shadow sessions, the live
 * cold start's retail flow from docs/results (with an arbiter), and a file
 * that does not load. Each FlowDetail is computed at the threshold asked
 * for, with review::show's rule.
 */
import type {
  BindingView,
  FlowDetail,
  FlowSummary,
  FlowTool,
  PromotionView,
  SiteRecord,
  SiteView,
} from '../../src/api/types.ts'
import texts from './data/texts.json' with { type: 'json' }

interface LookupSpec {
  tool: string
  count: number
  share: number
  weighed_share: number
  binding_chance: number | null
}

interface SiteSpec {
  tool: string
  failed?: boolean
  steps: number
  lookups: LookupSpec[]
  hand_back_share: number
  promoted?: SiteRecord | null
  threshold?: number | null
}

export interface FlowRecord {
  summary: FlowSummary
  tools: FlowTool[]
  sites: SiteSpec[]
  bindings: BindingView[]
  promotion: PromotionView | null
  weighedBy: 'reach' | 'habit'
  review: (threshold: number) => string
  raw: string
  warnings: string[]
}

const SHOP_CONTRACTS: Record<string, string> = {
  cancel_pending_order: 'order_id:string!, reason:string!',
  find_user_id_by_email: 'email:string!',
  get_order_details: 'order_id:string!',
  get_user_details: 'user_id:string!',
}

const SHOP_TOOLS: FlowTool[] = [
  {
    name: 'cancel_pending_order',
    kind: 'write',
    summary: "Cancel a pending order. The reason is 'no longer needed' or 'ordered by mistake'.",
    args: {},
    contract: SHOP_CONTRACTS.cancel_pending_order!,
  },
  {
    name: 'find_user_id_by_email',
    kind: 'read',
    summary: 'Find a user id by email.',
    args: {},
    contract: SHOP_CONTRACTS.find_user_id_by_email!,
  },
  {
    name: 'get_order_details',
    kind: 'read',
    summary: 'Get the status and details of an order.',
    args: {},
    contract: SHOP_CONTRACTS.get_order_details!,
  },
  {
    name: 'get_user_details',
    kind: 'read',
    summary: "Get a user's details, including their orders.",
    args: {},
    contract: SHOP_CONTRACTS.get_user_details!,
  },
]

const chance = (agreed: number, tried: number) => (agreed + 1) / (tried + 2)

/**
 * A binding's chances from `[agreed, tried]`, where the customer had not
 * mentioned the values the flow picked and where they had: each smoothed,
 * and over both.
 */
function scored(unmentioned: [number, number], mentioned: [number, number]) {
  const both: [number, number] = [unmentioned[0] + mentioned[0], unmentioned[1] + mentioned[1]]
  return {
    chance: [chance(...unmentioned), chance(...mentioned), chance(...both)] as [
      number,
      number,
      number,
    ],
    agreed: [unmentioned, mentioned] as [[number, number], [number, number]],
  }
}

/** A lookup whose arguments the flow cannot bind: only the customer knows them. */
const UNBOUND = {
  chance: null,
  agreed: [
    [0, 0],
    [0, 0],
  ] as [[number, number], [number, number]],
}

const SHOP_BINDINGS: BindingView[] = [
  {
    tool: 'find_user_id_by_email',
    calls: 6,
    ...UNBOUND,
    args: [{ name: 'email', required: true, calls: 6, constant: null, sources: [] }],
  },
  {
    tool: 'get_order_details',
    calls: 12,
    ...scored([12, 12], [0, 0]),
    args: [
      {
        name: 'order_id',
        required: true,
        calls: 12,
        constant: null,
        sources: [{ tool: 'get_user_details', path: '$.orders[*]', count: 12, share: 1 }],
      },
    ],
  },
  {
    tool: 'get_user_details',
    calls: 6,
    ...scored([6, 6], [0, 0]),
    args: [
      {
        name: 'user_id',
        required: true,
        calls: 6,
        constant: null,
        sources: [{ tool: 'find_user_id_by_email', path: '$', count: 6, share: 1 }],
      },
    ],
  },
]

function shopSites(promoted: boolean): SiteSpec[] {
  const p = (decisions: number, lookups: number): SiteRecord | null =>
    promoted
      ? { decisions, lookups, used: lookups, served: 0, tasks: 3, lower: 0.53, promoted: true }
      : null
  return [
    {
      tool: 'find_user_id_by_email',
      steps: 6,
      lookups: [
        {
          tool: 'get_user_details',
          count: 6,
          share: 1,
          weighed_share: 1,
          binding_chance: chance(6, 6),
        },
      ],
      hand_back_share: 0,
      promoted: p(3, 3),
    },
    {
      tool: 'get_order_details',
      steps: 12,
      lookups: [
        {
          tool: 'get_order_details',
          count: 6,
          share: 0.5,
          weighed_share: 0.5,
          binding_chance: chance(12, 12),
        },
      ],
      hand_back_share: 0.5,
      promoted: p(6, 3),
    },
    {
      tool: 'get_user_details',
      steps: 6,
      lookups: [
        {
          tool: 'get_order_details',
          count: 6,
          share: 1,
          weighed_share: 1,
          binding_chance: chance(12, 12),
        },
      ],
      hand_back_share: 0,
      promoted: p(3, 3),
    },
  ]
}

const RETAIL_BINDINGS: BindingView[] = [
  {
    tool: 'find_user_id_by_email',
    calls: 1,
    ...UNBOUND,
    args: [{ name: 'email', required: true, calls: 1, constant: null, sources: [] }],
  },
  {
    tool: 'find_user_id_by_name_zip',
    calls: 2,
    ...UNBOUND,
    args: ['first_name', 'last_name', 'zip'].map((name) => ({
      name,
      required: true,
      calls: 2,
      constant: null,
      sources: [],
    })),
  },
  {
    tool: 'get_order_details',
    calls: 10,
    ...scored([10, 10], [0, 0]),
    args: [
      {
        name: 'order_id',
        required: true,
        calls: 10,
        constant: null,
        sources: [{ tool: 'get_user_details', path: '$.orders[*]', count: 10, share: 1 }],
      },
    ],
  },
  {
    tool: 'get_product_details',
    calls: 4,
    ...scored([0, 0], [2, 4]),
    args: [
      {
        name: 'product_id',
        required: true,
        calls: 4,
        constant: null,
        sources: [{ tool: 'get_order_details', path: '$.items[*].product_id', count: 4, share: 1 }],
      },
    ],
  },
  {
    tool: 'get_user_details',
    calls: 3,
    ...scored([2, 2], [0, 0]),
    args: [
      {
        name: 'user_id',
        required: true,
        calls: 3,
        constant: null,
        sources: [{ tool: 'find_user_id_by_name_zip', path: '$', count: 2, share: 2 / 3 }],
      },
    ],
  },
]

const RETAIL_SITES: SiteSpec[] = [
  {
    tool: 'find_user_id_by_email',
    steps: 1,
    lookups: [
      { tool: 'get_user_details', count: 1, share: 1, weighed_share: 1, binding_chance: 0.75 },
    ],
    hand_back_share: 0,
  },
  {
    tool: 'find_user_id_by_name_zip',
    steps: 2,
    lookups: [
      { tool: 'get_user_details', count: 2, share: 1, weighed_share: 1, binding_chance: 0.75 },
    ],
    hand_back_share: 0,
  },
  {
    tool: 'get_order_details',
    steps: 9,
    lookups: [
      {
        tool: 'get_order_details',
        count: 7,
        share: 7 / 9,
        weighed_share: 7 / 9,
        binding_chance: chance(10, 10),
      },
      {
        tool: 'get_product_details',
        count: 1,
        share: 1 / 9,
        weighed_share: 1 / 9,
        binding_chance: chance(2, 4),
      },
    ],
    hand_back_share: 1 / 9,
  },
  {
    tool: 'get_product_details',
    steps: 4,
    lookups: [
      {
        tool: 'get_product_details',
        count: 1,
        share: 0.25,
        weighed_share: 0.25,
        binding_chance: chance(2, 4),
      },
    ],
    hand_back_share: 0.75,
  },
  {
    tool: 'get_user_details',
    steps: 3,
    lookups: [
      {
        tool: 'get_order_details',
        count: 3,
        share: 1,
        weighed_share: 1,
        binding_chance: chance(10, 10),
      },
    ],
    hand_back_share: 0,
  },
]

function siteName(site: SiteSpec): string {
  return site.failed ? `${site.tool} (error)` : site.tool
}

/** A flow's sites at `threshold`: the likeliest lookup acts when share × chance reaches it. */
export function sitesAt(flow: FlowRecord, threshold: number): SiteView[] {
  const promoted = flow.promotion !== null
  return flow.sites.map((site) => {
    const t = site.threshold ?? threshold
    const allowed = t <= 1 && (!promoted || !!site.promoted?.promoted)
    let best: LookupSpec | null = null
    for (const l of site.lookups) if (!best || l.weighed_share > best.weighed_share) best = l
    const prob = (l: LookupSpec) => l.weighed_share * (l.binding_chance ?? 0)
    const acts = allowed && !!best && prob(best) >= t
    // As `stretto flow-show` words it.
    const describe = (l: LookupSpec) =>
      `\`${l.tool}\` ${l.weighed_share.toFixed(2)} × ${(l.binding_chance ?? 0).toFixed(2)} = ${prob(l).toFixed(2)}`
    const verdict = !allowed
      ? t > 1
        ? 'hands back: switched off'
        : 'hands back: not promoted'
      : !best
        ? 'hands back: no lookup offered'
        : `${prob(best) >= t ? 'looks up' : 'hands back:'} ${describe(best)}`
    const next = [
      ...site.lookups.map((l) => ({ action: l.tool, share: l.share })),
      ...(site.hand_back_share > 0 ? [{ action: 'respond', share: site.hand_back_share }] : []),
    ].sort((a, b) => b.share - a.share)
    return {
      name: siteName(site),
      tool: site.tool,
      failed: !!site.failed,
      steps: site.steps,
      weighed_by: flow.weighedBy,
      lookups: site.lookups.map((l) => ({
        tool: l.tool,
        count: l.count,
        share: l.share,
        weighed_share: l.weighed_share,
        binding_chance: l.binding_chance,
        bindable: l.binding_chance !== null,
        prob: prob(l),
        acts: acts && l === best,
      })),
      hand_back_share: site.hand_back_share,
      choice: best ? { tool: best.tool, prob: prob(best), acts } : null,
      promoted: site.promoted ?? null,
      threshold: site.threshold ?? null,
      next,
      active: allowed,
      verdict,
    }
  })
}

/** The review's Sites table and threshold, rewritten for another threshold. */
function reviewAt(markdown: string, flow: () => FlowRecord, threshold: number): string {
  if (Math.abs(threshold - 0.3) < 1e-9) return markdown
  const sites = sitesAt(flow(), threshold)
  const t = String(Number(threshold.toFixed(2)))
  return markdown
    .split('\n')
    .map((line) => {
      line = line.replace('at a threshold of 0.3,', `at a threshold of ${t},`)
      const m = /^\| `([a-z_]+)`( \(failed\))? \|/.exec(line)
      if (!m || (!line.includes('looks up') && !line.includes('hands back'))) return line
      const site = sites.find((s) => s.tool === m[1] && s.failed === !!m[2])
      if (!site || !site.choice?.tool) return line
      const l = site.lookups.find((x) => x.tool === site.choice!.tool)!
      const verdict = `${site.choice.acts ? 'looks up' : 'hands back:'} \`${l.tool}\` ${l.weighed_share.toFixed(2)} × ${(l.binding_chance ?? 0).toFixed(2)} = ${l.prob.toFixed(2)}`
      const cells = line.split(' | ')
      cells[cells.length - 1] = `${verdict} |`
      return cells.join(' | ')
    })
    .join('\n')
}

export function detailAt(flow: FlowRecord, threshold: number): FlowDetail {
  return {
    summary: flow.summary,
    threshold,
    tools: flow.tools,
    sites: sitesAt(flow, threshold),
    bindings: flow.bindings,
    provenance: {
      stretto: flow.summary.stretto_version,
      sources: flow.summary.sources,
      habit_episodes: flow.summary.habit_episodes,
      arbiter_cases: flow.summary.arbiter_cases,
      compiled_unix_ms: flow.summary.compiled_unix_ms,
    },
    promotion: flow.promotion,
    review_markdown: flow.review(threshold),
    warnings: flow.warnings,
    drift: null,
  }
}

function summary(
  partial: Partial<FlowSummary> & Pick<FlowSummary, 'key' | 'path' | 'name' | 'domain'>,
): FlowSummary {
  return {
    format_version: 1,
    stretto_version: '0.1.0',
    sources: [],
    habit_episodes: 0,
    arbiter_cases: 0,
    compiled_unix_ms: 0,
    modified_unix_ms: 0,
    size_bytes: 0,
    decider: 'reach',
    has_arbiter: false,
    has_reach: true,
    tools: { read: 0, write: 0, generic: 0 },
    sites: 0,
    lookups: 0,
    lookup_tools: [],
    promoted: null,
    surprise: null,
    served_by: [],
    stage: null,
    error: null,
    ...partial,
  }
}

function counts(tools: FlowTool[]) {
  return {
    read: tools.filter((t) => t.kind === 'read').length,
    write: tools.filter((t) => t.kind === 'write').length,
    generic: tools.filter((t) => t.kind === 'generic').length,
  }
}

function distinctLookups(sites: SiteSpec[]): number {
  return new Set(sites.flatMap((s) => s.lookups.map((l) => l.tool))).size
}

export interface FlowTimes {
  shopCompiled: number
  promotedCompiled: number
  retailCopied: number
  scratchModified: number
  /** When `stretto stage` last learned the shop's staged flow; a day after the promotion by default. */
  stagedCompiled?: number
}

/** The shop's sites as `stretto stage` learned them again from eleven sessions. */
function stagedShopSites(): SiteSpec[] {
  const [find, order, user] = shopSites(false)
  return [
    {
      ...find!,
      steps: 11,
      lookups: [{ ...find!.lookups[0]!, count: 11, binding_chance: chance(11, 11) }],
    },
    {
      ...order!,
      steps: 22,
      lookups: [
        {
          ...order!.lookups[0]!,
          count: 6,
          share: 0.27,
          weighed_share: 0.27,
          binding_chance: chance(22, 22),
        },
      ],
      hand_back_share: 0.73,
    },
    {
      ...user!,
      steps: 11,
      lookups: [{ ...user!.lookups[0]!, count: 11, binding_chance: chance(22, 22) }],
    },
  ]
}

export function buildFlows(times: FlowTimes): FlowRecord[] {
  const flows: FlowRecord[] = []

  const shop: FlowRecord = {
    summary: summary({
      key: 'shop',
      path: 'shop.flow.json',
      name: 'shop',
      domain: 'shop',
      sources: ['claude-sonnet-5'],
      habit_episodes: 6,
      compiled_unix_ms: times.shopCompiled,
      modified_unix_ms: times.shopCompiled,
      size_bytes: 4925,
      tools: counts(SHOP_TOOLS),
      sites: 3,
      lookups: 2,
    }),
    tools: SHOP_TOOLS,
    sites: shopSites(false),
    bindings: SHOP_BINDINGS,
    promotion: null,
    weighedBy: 'reach',
    review: (t) => reviewAt(texts.shop_review, () => shop, t),
    raw: '',
    warnings: [],
  }
  flows.push(shop)

  const stagedCompiled = times.stagedCompiled ?? times.promotedCompiled + 86_400_000
  const staged: FlowRecord = {
    summary: summary({
      key: 'shop.staged',
      path: 'shop.staged.flow.json',
      name: 'shop.staged',
      domain: 'shop',
      sources: ['claude-sonnet-5'],
      habit_episodes: 11,
      compiled_unix_ms: stagedCompiled,
      modified_unix_ms: stagedCompiled,
      size_bytes: 5012,
      tools: counts(SHOP_TOOLS),
      sites: 3,
      lookups: 2,
    }),
    tools: SHOP_TOOLS,
    sites: stagedShopSites(),
    bindings: SHOP_BINDINGS,
    promotion: null,
    weighedBy: 'reach',
    review: (t) => reviewAt(texts.shop_review, () => staged, t),
    raw: '',
    warnings: [],
  }
  flows.push(staged)

  const promotion: PromotionView = {
    bar: { threshold: 0.3, min_used: 0.7, min_lower: 0.5, min_tasks: 3 },
    sites_promoted: 3,
    sites_scored: 4,
  }
  const promoted: FlowRecord = {
    summary: summary({
      key: 'shop-promoted',
      path: 'shop-promoted.flow.json',
      name: 'shop',
      domain: 'shop',
      sources: ['claude-sonnet-5'],
      habit_episodes: 6,
      compiled_unix_ms: times.promotedCompiled,
      modified_unix_ms: times.promotedCompiled,
      size_bytes: 5611,
      tools: counts(SHOP_TOOLS),
      sites: 3,
      lookups: 2,
      promoted: { sites_promoted: 3, sites_scored: 4 },
      served_by: ['shop'],
    }),
    tools: SHOP_TOOLS,
    sites: shopSites(true),
    bindings: SHOP_BINDINGS,
    promotion,
    weighedBy: 'reach',
    review: (t) => reviewAt(texts.shop_promoted_review, () => promoted, t),
    raw: '',
    warnings: [],
  }
  flows.push(promoted)

  const retailTools = texts.retail_tools as FlowTool[]
  const retail: FlowRecord = {
    summary: summary({
      key: 'retail',
      path: 'flows/retail.flow.json',
      name: 'retail',
      domain: 'retail',
      stretto_version: texts.retail_provenance.stretto,
      sources: texts.retail_provenance.sources,
      habit_episodes: texts.retail_provenance.habit_episodes,
      arbiter_cases: texts.retail_provenance.arbiter_cases,
      compiled_unix_ms: texts.retail_provenance.compiled_unix_ms,
      modified_unix_ms: times.retailCopied,
      size_bytes: texts.retail_size,
      decider: 'arbiter',
      has_arbiter: true,
      has_reach: false,
      tools: counts(retailTools),
      sites: RETAIL_SITES.length,
      lookups: distinctLookups(RETAIL_SITES),
    }),
    tools: retailTools,
    sites: RETAIL_SITES,
    bindings: RETAIL_BINDINGS,
    promotion: null,
    weighedBy: 'habit',
    review: (t) => reviewAt(texts.retail_review, () => retail, t),
    raw: '',
    warnings: [],
  }
  flows.push(retail)

  flows.push({
    summary: summary({
      key: 'orders',
      path: 'scratch/orders.flow.json',
      name: 'orders',
      domain: '',
      decider: 'habit',
      has_reach: false,
      modified_unix_ms: times.scratchModified,
      size_bytes: 2210,
      error: 'stretto_flow 3 is not a version this build reads (1 or 2)',
    }),
    tools: [],
    sites: [],
    bindings: [],
    promotion: null,
    weighedBy: 'habit',
    review: () => '',
    raw: '{"stretto_flow":3,"provenance":{"stretto":"0.2.0"}}',
    warnings: [],
  })

  for (const f of flows) {
    if (!f.raw) f.raw = rawFlow(f)
  }
  // What each flow may look up, from its sites, as the console lists it.
  for (const f of flows)
    f.summary.lookup_tools = [
      ...new Set(f.sites.flatMap((s) => s.lookups.map((l) => l.tool))),
    ].sort()
  return flows
}

/** A flow file's JSON, enough of it to read and download (the IR's fields, abridged). */
function rawFlow(f: FlowRecord): string {
  const s = f.summary
  return JSON.stringify({
    stretto_flow: s.format_version,
    provenance: {
      stretto: s.stretto_version,
      sources: s.sources,
      habit_episodes: s.habit_episodes,
      arbiter_cases: s.arbiter_cases,
      compiled_unix_ms: s.compiled_unix_ms,
    },
    manifest: {
      domain: s.domain,
      tools: Object.fromEntries(f.tools.map((t) => [t.name, t.kind])),
      docs: Object.fromEntries(f.tools.map((t) => [t.name, { summary: t.summary, args: t.args }])),
    },
    map: { fields: {}, ids: [] },
    group: 1,
    sites: {
      reads: f.tools.filter((t) => t.kind === 'read').map((t) => t.name),
      next: f.sites.map((site) => [
        [site.tool, !!site.failed],
        Object.fromEntries(site.lookups.map((l) => [l.tool, l.count])),
      ]),
      feeds: {},
    },
    predicates: [],
    weighed: [],
    folds: [],
    bindings: {
      args: Object.fromEntries(
        f.bindings.map((b) => [
          b.tool,
          [b.calls, Object.fromEntries(b.args.map((a) => [a.name, a.calls]))],
        ]),
      ),
      sources: f.bindings.flatMap((b) =>
        b.args.map((a) => [
          [b.tool, a.name],
          { values: a.calls, found: a.sources.map((src) => [[src.tool, src.path], src.count]) },
        ]),
      ),
    },
    model: s.has_arbiter ? 'jev-latest' : '',
    ...(f.promotion
      ? {
          promoted: {
            bar: f.promotion.bar,
            sites: Object.fromEntries(
              f.sites.filter((x) => x.promoted).map((x) => [siteName(x), x.promoted]),
            ),
          },
        }
      : {}),
    contracts: Object.fromEntries(
      f.tools.filter((t) => t.contract).map((t) => [t.name, t.contract]),
    ),
  })
}

/** A change list's `## ` headings and the `- ` items under each. */
function sectionsOf(markdown: string): { title: string; changes: string[] }[] {
  const sections: { title: string; changes: string[] }[] = []
  for (const line of markdown.split('\n')) {
    if (line.startsWith('## ')) sections.push({ title: line.slice(3).trim(), changes: [] })
    else if (line.startsWith('- ') && sections.length)
      sections[sections.length - 1]!.changes.push(line.slice(2))
  }
  return sections.filter((x) => x.changes.length)
}

/** What flow-diff says between two flows, as the console shows it. */
export function diffFlows(
  from: FlowRecord,
  to: FlowRecord,
): {
  review: string[]
  changes: string[]
  sections: { title: string; changes: string[] }[]
  markdown: string
} {
  const d = diffLists(from, to)
  return { ...d, sections: sectionsOf(d.markdown) }
}

function diffLists(
  from: FlowRecord,
  to: FlowRecord,
): { review: string[]; changes: string[]; markdown: string } {
  const pair = `${from.summary.key}→${to.summary.key}`
  if (pair === 'shop→shop-promoted') {
    return {
      review: [],
      changes: [
        'Promotion: none: it may act after every call → 3 of 4 sites scored (at least 70% used, a lower bound of 0.50, 3 tasks, at 0.3)',
      ],
      markdown: texts.diff_shop_promoted,
    }
  }
  if (pair === 'shop→retail') {
    const md = texts.diff_shop_retail
    const review = md
      .split('## ')[0]!
      .split('\n')
      .filter((l) => l.startsWith('- '))
      .map((l) => l.slice(2).replace(/`/g, ''))
    const changes = md
      .split('\n')
      .filter((l) => l.startsWith('- '))
      .map((l) => l.slice(2).replace(/`/g, ''))
      .filter((l) => !review.includes(l))
    return { review, changes, markdown: md }
  }
  if (pair === 'shop→shop.staged' || pair === 'shop.staged→shop') {
    const [a, b] = pair === 'shop→shop.staged' ? ['6', '11'] : ['11', '6']
    const [x, y] = pair === 'shop→shop.staged' ? ['50%', '27%'] : ['27%', '50%']
    const acts = pair === 'shop→shop.staged'
    const changes = [
      acts
        ? 'after `get_order_details`: hands back (was: looks up `get_order_details`)'
        : 'after `get_order_details`: looks up `get_order_details` (was: hands back)',
      `after \`get_order_details\`: \`get_order_details\` ${x} → ${y}`,
      `the habit learned from ${a} → ${b} successful sessions or episodes`,
    ]
    const review = acts ? [] : ['the flow now acts after `get_order_details`, where it handed back']
    const markdown = [
      '# Flow diff: shop',
      '',
      review.length
        ? '**Needs review:**\n\n' + review.map((r) => `- ${r}`).join('\n')
        : 'Nothing needs review: the flow runs the same program, and calls no tool, makes no lookup, binds no argument and asks no model or question it did not before.',
      '',
      '## With reach, at 0.3',
      '',
      `- ${changes[0]}`,
      '',
      '## What the agent did next in training',
      '',
      `- ${changes[1]}`,
      '',
      '## Provenance',
      '',
      `- ${changes[2]}`,
      '',
    ].join('\n')
    return { review, changes, markdown }
  }
  if (from.summary.key === to.summary.key) {
    return {
      review: [],
      changes: [],
      markdown: `# Flow diff: ${to.summary.domain}\n\nNo changes.\n`,
    }
  }
  const before = new Set(from.tools.filter((t) => t.kind === 'read').map((t) => t.name))
  const after = to.tools.filter((t) => t.kind === 'read').map((t) => t.name)
  const review = after
    .filter((t) => !before.has(t))
    .map((t) => `${t} is now marked read-only, so the flow may call it`)
  const lookupsBefore = new Set(
    from.sites.flatMap((s) => s.lookups.map((l) => `${l.tool} after ${s.tool}`)),
  )
  for (const s of to.sites)
    for (const l of s.lookups)
      if (!lookupsBefore.has(`${l.tool} after ${s.tool}`))
        review.push(`a new lookup: ${l.tool} after ${s.tool}`)
  const changes = [
    `provenance: ${from.summary.sources.join(', ') || 'no source'} → ${to.summary.sources.join(', ') || 'no source'}`,
  ]
  const markdown = [
    `# Flow diff: ${to.summary.domain}`,
    '',
    review.length
      ? '**Needs review:**\n\n' + review.map((r) => `- ${r}`).join('\n')
      : 'Nothing needs review.',
    '',
    '## Provenance',
    '',
    ...changes.map((c) => `- ${c}`),
    '',
  ].join('\n')
  return { review, changes, markdown }
}

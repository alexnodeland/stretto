/**
 * The mock console's data dir, as `~/.stretto` looks two weeks after the
 * quickstart's loop was run on a real deployment: six sessions recorded on
 * the demo shop, a flow learned from them with no key, three shadow sessions,
 * the flow promoted on them and served since; a notes server recorded
 * through the official filesystem server; a ticket desk over HTTP that was
 * never registered; and the jobs that did it.
 */
import type { Job, ServerEntry, SessionSummary, SessionDetail } from '../../src/api/types.ts'
import { Recorder, type SessionSpec, type ToolDef } from './recorder.ts'
import { FS_TOOLS, SHOP_TOOLS, TICKET_TOOLS, shop } from './tools.ts'
import { buildFlows, type FlowRecord } from './flows.ts'
import { shopStage, type StageRecord } from './stage.ts'
import texts from './data/texts.json' with { type: 'json' }

export interface SessionRecord {
  key: string
  summary: SessionSummary
  detail: SessionDetail
  raw: string
  flowLog: string
}

export interface World {
  now: number
  dataDir: string
  sessions: SessionRecord[]
  flows: FlowRecord[]
  /** Each deployment's staged learning (`stretto stage`), by its committed flow. */
  stages: StageRecord[]
  servers: ServerEntry[]
  jobs: Job[]
  /** What the jobs' reports hold, by path. */
  reports: Record<string, string>
}

/**
 * A report command's output split as `--out` splits it: the line it prints,
 * and the Markdown it writes to the report.
 */
export function splitReport(text: string): { line: string; report: string } {
  const [line = '', ...rest] = text.trimEnd().split('\n')
  return { line: `${line}\n`, report: `${rest.join('\n').trim()}\n` }
}

const DAY = 86_400_000

/** A small deterministic generator, so every run of the mock shows the same world. */
function rng(seed: number) {
  let s = seed >>> 0
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0
    return s / 2 ** 32
  }
}

/** The session id the proxy writes: its start in UTC and its process id. */
export function sessionId(started: number, pid: number): string {
  const d = new Date(started)
  const p = (n: number, w = 2) => String(n).padStart(w, '0')
  return `${d.getUTCFullYear()}${p(d.getUTCMonth() + 1)}${p(d.getUTCDate())}T${p(d.getUTCHours())}${p(d.getUTCMinutes())}${p(d.getUTCSeconds())}.${p(d.getUTCMilliseconds(), 3)}Z-${pid}`
}

function localDay(now: number, daysAgo: number, hour: number, minute: number): number {
  const d = new Date(now)
  d.setHours(0, 0, 0, 0)
  d.setDate(d.getDate() - daysAgo)
  return d.getTime() + hour * 3_600_000 + minute * 60_000
}

const lookUser = (n: number) => ({
  tool: 'get_user_details',
  arguments: { user_id: `user_${n}` },
  prob: 0.9925520080633771,
  binding: 0.875,
  result: shop.user(n),
})
const lookOrder = (n: number, which: 'a' | 'b') => ({
  tool: 'get_order_details',
  arguments: { order_id: `#W${n}${which}` },
  prob: which === 'a' ? 0.9959143854769734 : 0.9486398991730227,
  binding: 0.9285714285714286,
  result: shop.order(n, which),
})
const below = {
  reason: 'get_order_details at 0.05, below 0.3',
  prob: 0.052769325969608226,
  tool: 'get_order_details',
}

const opening = (n: number, kind: 'cancel' | 'status') =>
  kind === 'cancel'
    ? `Hi, I'm c${n}@example.com. I want to cancel an order I no longer need.`
    : `Hello, c${n}@example.com here. Where are my orders?`

/** The demo shop's server, as a session's spec names it. */
export function shopSpec(
  started: number,
  pid: number,
  flow: SessionSpec['flow'],
  dir: string,
): SessionSpec {
  return {
    session: sessionId(started, pid),
    started,
    domain: 'shop',
    agentModel: 'claude-sonnet-5',
    client: { name: 'claude-code', version: '2.1.0' },
    upstream: { kind: 'stdio', command: ['stretto-mcp-demo', '--world', 'retail'] },
    server: {
      name: 'stretto-mcp-demo',
      version: '0.1.0',
      instructions: 'A demo server for stretto-proxy: two tools that echo their arguments.',
    },
    tools: SHOP_TOOLS,
    flow,
    dir,
  }
}

/**
 * A customer served by the promoted flow: after the agent finds them, the
 * flow reads their details and both orders into the same result, and hands
 * back at 0.05, below 0.3. A typo in the email fails the first call first.
 */
export function servedShopSession(
  spec: SessionSpec,
  n: number,
  kind: 'cancel' | 'status' | 'typo',
  between: (a: number, b: number) => number,
): Recorder {
  const rec = new Recorder(spec)
  rec.say('user', opening(n, kind === 'typo' ? 'status' : kind), between(900, 1600))
  if (kind === 'typo') {
    rec.call(
      'find_user_id_by_email',
      { email: `c${n}@example.org` },
      shop.noUser(`c${n}@example.org`),
      {
        think: between(2600, 3600),
        handBack: { reason: 'no lookups followed here in training', prob: null },
      },
    )
    rec.say(
      'assistant',
      `I couldn't find an account for c${n}@example.org. Could you check the email address?`,
      between(2200, 3000),
    )
    rec.say('user', `Sorry, it's c${n}@example.com.`, between(5000, 9000))
  }
  rec.call('find_user_id_by_email', { email: `c${n}@example.com` }, shop.findUser(n), {
    think: between(2400, 3800),
    lookups: [lookUser(n), lookOrder(n, 'a'), lookOrder(n, 'b')],
    handBack: below,
  })
  if (kind === 'cancel') {
    rec.say(
      'assistant',
      `You have two pending orders, #W${n}a and #W${n}b. Which one should I cancel?`,
      between(2600, 3800),
    )
    rec.say('user', 'The first one, please.', between(6000, 12000))
    rec.call(
      'cancel_pending_order',
      { order_id: `#W${n}a`, reason: 'no longer needed' },
      shop.cancelled(n, 'a'),
      {
        think: between(2200, 3200),
        handBack: { reason: 'the site is not promoted', prob: null },
      },
    )
    rec.say('assistant', `Order #W${n}a is cancelled.`, between(1800, 2600))
  } else {
    rec.say('assistant', `Your orders #W${n}a and #W${n}b are both pending.`, between(2400, 3400))
  }
  rec.end()
  return rec
}

export function buildWorld(
  now: number = Date.now(),
  options: { empty?: boolean; dataDir?: string } = {},
): World {
  const dataDir = options.dataDir ?? '/home/me/.stretto'
  if (options.empty)
    return { now, dataDir, sessions: [], flows: [], stages: [], servers: [], jobs: [], reports: {} }

  const rand = rng(20260928)
  const between = (a: number, b: number) => a + rand() * (b - a)
  let pid = 4100
  const at = (daysAgo: number, hour: number, minute: number) => {
    let t = localDay(now, daysAgo, hour, minute) + Math.floor(rand() * 59_000)
    if (t > now - 20 * 60_000) t = now - (20 + Math.floor(rand() * 90)) * 60_000
    return t
  }

  const sessions: SessionRecord[] = []
  const add = (rec: Recorder) => {
    const key = rec.spec.session
    sessions.push({
      key,
      summary: rec.summary(key),
      detail: rec.detail(key),
      raw: rec.raw,
      flowLog: rec.flowLog,
    })
  }

  const shop_ = (started: number, flow: SessionSpec['flow'], dir: string) =>
    shopSpec(started, (pid += 7 + Math.floor(rand() * 40)), flow, dir)

  // 1. Six customers recorded without a flow: what `stretto learn` learned from.
  const training: [number, 'cancel' | 'status'][] = [
    [1, 'cancel'],
    [2, 'status'],
    [3, 'cancel'],
    [4, 'status'],
    [5, 'cancel'],
    [6, 'cancel'],
  ]
  training.forEach(([n, kind], i) => {
    const rec = new Recorder(
      shop_(at(13 - Math.floor(i / 2), 10 + (i % 2) * 4, 12 + i * 7), 'none', 'logs/shop'),
    )
    rec.say('user', opening(n, kind), between(900, 1600))
    rec.call('find_user_id_by_email', { email: `c${n}@example.com` }, shop.findUser(n), {
      think: between(2600, 4200),
    })
    rec.call('get_user_details', { user_id: `user_${n}` }, shop.user(n), {
      think: between(1800, 3000),
    })
    rec.call('get_order_details', { order_id: `#W${n}a` }, shop.order(n, 'a'), {
      think: between(1800, 3200),
    })
    rec.call('get_order_details', { order_id: `#W${n}b` }, shop.order(n, 'b'), {
      think: between(1700, 2900),
    })
    if (kind === 'cancel') {
      rec.say(
        'assistant',
        `You have two pending orders, #W${n}a and #W${n}b. Which one should I cancel?`,
        between(2500, 3800),
      )
      rec.say('user', 'The first one, please.', between(6000, 14000))
      rec.call(
        'cancel_pending_order',
        { order_id: `#W${n}a`, reason: 'no longer needed' },
        shop.cancelled(n, 'a'),
        { think: between(2200, 3400) },
      )
      rec.say('assistant', `Order #W${n}a is cancelled.`, between(1800, 2600))
    } else {
      rec.say('assistant', `Your orders #W${n}a and #W${n}b are both pending.`, between(2400, 3600))
    }
    rec.end()
    add(rec)
  })

  // 2. Three customers with the flow in shadow: it decides and logs, and looks nothing up.
  const shadow: [number, 'cancel' | 'status'][] = [
    [51, 'cancel'],
    [52, 'status'],
    [53, 'cancel'],
  ]
  shadow.forEach(([n, kind], i) => {
    const rec = new Recorder(shop_(at(10 - i, 11 + i * 2, 5 + i * 11), 'shadow', 'shadow/shop'))
    rec.say('user', opening(n, kind), between(900, 1600))
    rec.call('find_user_id_by_email', { email: `c${n}@example.com` }, shop.findUser(n), {
      think: between(2600, 4000),
      lookups: [lookUser(n)],
    })
    rec.call('get_user_details', { user_id: `user_${n}` }, shop.user(n), {
      think: between(1800, 3000),
      lookups: [lookOrder(n, 'a')],
    })
    rec.call('get_order_details', { order_id: `#W${n}a` }, shop.order(n, 'a'), {
      think: between(1800, 3000),
      lookups: [lookOrder(n, 'b')],
    })
    rec.call('get_order_details', { order_id: `#W${n}b` }, shop.order(n, 'b'), {
      think: between(1700, 2800),
      handBack: below,
    })
    if (kind === 'cancel') {
      rec.say(
        'assistant',
        `You have two pending orders, #W${n}a and #W${n}b. Which one should I cancel?`,
        between(2500, 3600),
      )
      rec.say('user', 'The first one, please.', between(6000, 12000))
      rec.call(
        'cancel_pending_order',
        { order_id: `#W${n}a`, reason: 'no longer needed' },
        shop.cancelled(n, 'a'),
        { think: between(2200, 3200) },
      )
      rec.say('assistant', `Order #W${n}a is cancelled.`, between(1800, 2600))
    } else {
      rec.say('assistant', `Your orders #W${n}a and #W${n}b are both pending.`, between(2400, 3400))
    }
    rec.end()
    add(rec)
  })

  // 3. Served since the promotion: after the agent finds the customer, the
  // flow reads their details and both orders into the same result.
  const served: [number, number, number, 'cancel' | 'status' | 'typo'][] = [
    [7, 9, 41, 'cancel'],
    [7, 15, 42, 'status'],
    [6, 10, 43, 'status'],
    [6, 16, 44, 'cancel'],
    [5, 11, 45, 'typo'],
    [5, 14, 46, 'cancel'],
    [4, 9, 47, 'status'],
    [4, 13, 48, 'cancel'],
    [4, 17, 49, 'status'],
    [3, 10, 54, 'cancel'],
    [3, 15, 55, 'cancel'],
    [2, 9, 56, 'status'],
    [2, 12, 57, 'typo'],
    [2, 16, 58, 'cancel'],
    [1, 10, 59, 'status'],
    [1, 14, 60, 'cancel'],
    [0, 9, 61, 'cancel'],
    [0, 11, 62, 'status'],
  ]
  for (const [daysAgo, hour, n, kind] of served) {
    add(
      servedShopSession(
        shop_(at(daysAgo, hour, Math.floor(rand() * 50)), 'served', 'logs/shop'),
        n,
        kind,
        between,
      ),
    )
  }

  // 4. Notes, through the official filesystem server, recorded from Cursor.
  const notesSpec = (started: number): SessionSpec => ({
    session: sessionId(started, (pid += 11 + Math.floor(rand() * 60))),
    started,
    domain: 'notes',
    agentModel: null,
    client: { name: 'cursor', version: '1.7.2' },
    upstream: {
      kind: 'stdio',
      command: ['npx', '-y', '@modelcontextprotocol/server-filesystem', '/home/me/notes'],
    },
    server: { name: 'secure-filesystem-server', version: '0.6.3', instructions: null },
    tools: FS_TOOLS,
    flow: 'none',
    dir: 'logs/notes',
  })
  const standups = ['09-15', '09-22']
  const notesDays: [number, number][] = [
    [9, 16],
    [8, 10],
    [7, 11],
    [6, 17],
    [5, 9],
    [4, 11],
    [3, 16],
    [3, 18],
    [2, 10],
    [1, 11],
    [1, 17],
    [0, 10],
  ]
  notesDays.forEach(([daysAgo, hour], i) => {
    const rec = new Recorder(notesSpec(at(daysAgo, hour, Math.floor(rand() * 55))))
    const week = standups[i % 2]!
    rec.say(
      'user',
      i % 3 === 2
        ? 'Add "follow up with Priya about the Q4 plan" to my inbox.'
        : 'What did we decide at the last standup?',
      between(800, 1400),
    )
    rec.call(
      'list_allowed_directories',
      {},
      { text: 'Allowed directories:\n/home/me/notes' },
      { think: between(2000, 3000) },
    )
    if (i % 3 === 2) {
      rec.call(
        'read_text_file',
        { path: '/home/me/notes/inbox.md' },
        { text: '# Inbox\n\n- Renew the domain\n- Book the offsite venue\n' },
        { think: between(2000, 3200) },
      )
      rec.call(
        'edit_file',
        {
          path: '/home/me/notes/inbox.md',
          edits: [
            {
              oldText: '- Book the offsite venue\n',
              newText: '- Book the offsite venue\n- Follow up with Priya about the Q4 plan\n',
            },
          ],
        },
        {
          text: '```diff\n@@ -3,2 +3,3 @@\n - Renew the domain\n - Book the offsite venue\n+- Follow up with Priya about the Q4 plan\n```',
        },
        { think: between(2600, 4200) },
      )
      rec.say('assistant', 'Added it to the end of your inbox.', between(1600, 2400))
    } else {
      rec.call(
        'search_files',
        { path: '/home/me/notes', pattern: 'standup' },
        { text: standups.map((s) => `/home/me/notes/2026/${s}-standup.md`).join('\n') },
        { think: between(2200, 3400) },
      )
      if (i === 5) {
        rec.call(
          'read_text_file',
          { path: `/home/me/notes/2026/${week}-standups.md` },
          {
            text: `Error: ENOENT: no such file or directory, open '/home/me/notes/2026/${week}-standups.md'`,
            isError: true,
          },
          { think: between(2000, 3000) },
        )
      }
      rec.call(
        'read_text_file',
        { path: `/home/me/notes/2026/${week}-standup.md` },
        {
          text: `# Standup ${week}\n\n- Ship the console behind a flag\n- Priya owns the Q4 plan\n- Next review on Friday\n`,
        },
        { think: between(2000, 3200) },
      )
      rec.call(
        'get_file_info',
        { path: `/home/me/notes/2026/${week}-standup.md` },
        {
          text: `size: 1204\ncreated: 2026-${week}T09:12:44.000Z\nmodified: 2026-${week}T09:40:02.000Z\nisDirectory: false\nisFile: true\npermissions: 644`,
        },
        { think: between(1600, 2600) },
      )
      rec.say(
        'assistant',
        'You decided to ship the console behind a flag; Priya owns the Q4 plan, and the next review is on Friday.',
        between(2400, 3600),
      )
    }
    rec.end()
    add(rec)
  })

  // 5. A ticket desk over Streamable HTTP, never added to the registry.
  const ticketSpec = (started: number): SessionSpec => ({
    session: sessionId(started, (pid += 9 + Math.floor(rand() * 30))),
    started,
    domain: 'tickets',
    agentModel: 'claude-haiku-4.5',
    client: { name: 'claude-code', version: '2.1.0' },
    upstream: { kind: 'http', url: 'https://desk.example.com/mcp' },
    server: {
      name: 'desk-mcp',
      version: '2.4.1',
      instructions: 'Tickets, requesters and comments for the support desk.',
    },
    tools: TICKET_TOOLS as ToolDef[],
    flow: 'none',
    dir: 'logs/tickets',
  })
  ;[
    [3, 14, 'T-4821'],
    [2, 15, 'T-4830'],
    [1, 16, 'T-4838'],
  ].forEach(([daysAgo, hour, ticket]) => {
    const rec = new Recorder(
      ticketSpec(at(daysAgo as number, hour as number, Math.floor(rand() * 50))),
    )
    rec.say(
      'user',
      `Can you look at ${ticket} and tell the requester we're on it?`,
      between(800, 1500),
    )
    rec.call(
      'get_ticket',
      { ticket_id: ticket },
      {
        text: JSON.stringify({
          id: ticket,
          subject: 'Export fails on large workspaces',
          status: 'open',
          requester_id: 'r-118',
          comments: 2,
        }),
      },
      { think: between(2600, 3800) },
    )
    rec.call(
      'get_requester',
      { requester_id: 'r-118' },
      {
        text: JSON.stringify({
          id: 'r-118',
          name: 'Dana Whitfield',
          plan: 'team',
          open_tickets: 3,
        }),
      },
      { think: between(2000, 3000) },
    )
    rec.call(
      'add_comment',
      {
        ticket_id: ticket,
        body: "Thanks Dana, we're looking into the export failure and will update you today.",
        internal: false,
      },
      { text: JSON.stringify({ ok: true, comment_id: 'c-9912' }) },
      { think: between(3000, 4600) },
    )
    rec.say(
      'assistant',
      `I've replied on ${ticket} to let Dana know we're on it.`,
      between(1800, 2600),
    )
    rec.end()
    add(rec)
  })

  sessions.sort((a, b) => b.summary.started_unix_ms - a.summary.started_unix_ms)

  const trainingEnd = Math.max(
    ...sessions
      .filter((s) => s.summary.path.startsWith('logs/shop') && s.summary.mode === 'recorded')
      .map((s) => s.summary.started_unix_ms + s.summary.duration_ms),
  )
  const shadowEnd = Math.max(
    ...sessions
      .filter((s) => s.summary.mode === 'shadow')
      .map((s) => s.summary.started_unix_ms + s.summary.duration_ms),
  )
  const shopCompiled = trainingEnd + 26 * 60_000
  const promotedCompiled = shadowEnd + 3 * 3_600_000
  const flows = buildFlows({
    shopCompiled,
    promotedCompiled,
    retailCopied: now - 5 * DAY - 3_600_000 * 4,
    scratchModified: now - 12 * DAY,
  })

  const servers: ServerEntry[] = [
    {
      name: 'shop',
      description: 'The demo shop, stretto-mcp-demo --world retail',
      upstream: { kind: 'stdio', command: ['stretto-mcp-demo', '--world', 'retail'], env: [] },
      mode: 'serve',
      flow: '~/.stretto/shop-promoted.flow.json',
      record_dir: null,
      decider: null,
      threshold: null,
      created_unix_ms: localDay(now, 13, 9, 40),
      updated_unix_ms: promotedCompiled + 12 * 60_000,
    },
    {
      name: 'notes',
      description: 'Meeting notes, through the official filesystem server',
      upstream: {
        kind: 'stdio',
        command: ['npx', '-y', '@modelcontextprotocol/server-filesystem', '/home/me/notes'],
        env: [],
      },
      mode: 'record',
      flow: null,
      record_dir: null,
      decider: null,
      threshold: null,
      created_unix_ms: localDay(now, 9, 15, 20),
      updated_unix_ms: localDay(now, 9, 15, 20),
    },
    {
      name: 'orders-api',
      description: 'The orders service, over Streamable HTTP',
      upstream: {
        kind: 'http',
        url: 'https://orders.example.com/mcp',
        headers: [{ name: 'Authorization', env: 'ORDERS_AUTH' }],
      },
      mode: 'shadow',
      flow: '~/.stretto/orders.flow.json',
      record_dir: null,
      decider: null,
      threshold: null,
      created_unix_ms: localDay(now, 2, 17, 5),
      updated_unix_ms: localDay(now, 2, 17, 5),
    },
  ]

  const job = (
    partial: Partial<Job> & Pick<Job, 'id' | 'kind' | 'title' | 'created_unix_ms'>,
  ): Job => ({
    params: {},
    status: 'succeeded',
    started_unix_ms: partial.created_unix_ms + 180,
    finished_unix_ms: partial.created_unix_ms + 1400,
    exit_code: 0,
    output: '',
    artifacts: [],
    ...partial,
  })
  const learnedAt = shopCompiled - 900
  const audit = splitReport(texts.audit_output)
  const promote = splitReport(texts.promote_output)
  const reports: Record<string, string> = {
    'console/jobs/j-0005.audit.md': audit.report,
    'console/jobs/j-0005.audit.json': `${JSON.stringify({ flow: 'shop', episodes: 8, decisions: 32, agreement: 1 }, null, 2)}\n`,
    'console/jobs/j-0003.promote.md': promote.report,
  }
  const jobs: Job[] = [
    job({
      id: 'j-0006',
      kind: 'doctor',
      title: 'Check the installation',
      created_unix_ms: now - 2 * 3_600_000 - 17 * 60_000,
      finished_unix_ms: now - 2 * 3_600_000 - 17 * 60_000 + 900,
      output: '',
    }),
    job({
      id: 'j-0005',
      kind: 'audit',
      title: 'Audit shop-promoted on logs/shop',
      params: { kind: 'audit', flow: 'shop-promoted', sessions: 'logs/shop', decider: null },
      created_unix_ms: now - DAY - 3 * 3_600_000,
      finished_unix_ms: now - DAY - 3 * 3_600_000 + 2100,
      output: audit.line,
      artifacts: [
        { kind: 'report', path: 'console/jobs/j-0005.audit.json', key: null },
        { kind: 'report', path: 'console/jobs/j-0005.audit.md', key: null },
      ],
    }),
    job({
      id: 'j-0004',
      kind: 'learn',
      title: 'Learn notes from logs/notes/archive',
      params: {
        kind: 'learn',
        domain: 'notes',
        sessions: 'logs/notes/archive',
        out: null,
        overwrite: false,
        habit_only: true,
        constants: false,
      },
      status: 'failed',
      exit_code: 1,
      created_unix_ms: now - 3 * DAY - 5 * 3_600_000,
      finished_unix_ms: now - 3 * DAY - 5 * 3_600_000 + 700,
      output: 'Error: no successful training episodes to learn a habit from (set rewards to 1)\n',
    }),
    job({
      id: 'j-0003',
      kind: 'promote',
      title: 'Promote shop on shadow/shop',
      params: {
        kind: 'promote',
        flow: 'shop',
        sessions: 'shadow/shop',
        oracle_cache: null,
        threshold: 0.3,
        min_used: 0.7,
        min_lower: 0.5,
        min_tasks: 3,
        out: 'shop-promoted.flow.json',
        overwrite: false,
      },
      created_unix_ms: promotedCompiled - 1500,
      finished_unix_ms: promotedCompiled + 200,
      output: promote.line,
      artifacts: [
        { kind: 'flow', path: 'shop-promoted.flow.json', key: 'shop-promoted' },
        { kind: 'report', path: 'console/jobs/j-0003.promote.md', key: null },
      ],
    }),
    job({
      id: 'j-0002',
      kind: 'learn',
      title: 'Learn shop from logs/shop',
      params: {
        kind: 'learn',
        domain: 'shop',
        sessions: 'logs/shop',
        out: null,
        overwrite: false,
        habit_only: true,
        constants: false,
      },
      created_unix_ms: learnedAt,
      finished_unix_ms: learnedAt + 1100,
      output: `stretto: learned the shop flow from 6 sessions (4 tools) and wrote ${dataDir}/shop.flow.json\n`,
      artifacts: [{ kind: 'flow', path: 'shop.flow.json', key: 'shop' }],
    }),
    job({
      id: 'j-0001',
      kind: 'doctor',
      title: 'Check the installation',
      created_unix_ms: localDay(now, 13, 9, 31),
      finished_unix_ms: localDay(now, 13, 9, 31) + 800,
      output: '',
    }),
  ]

  const staged = flows.find((f) => f.summary.key === 'shop.staged')!
  const stages = [shopStage(staged.summary.compiled_unix_ms)]
  return { now, dataDir, sessions, flows, stages, servers, jobs, reports }
}

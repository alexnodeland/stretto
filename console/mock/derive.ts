/**
 * What the console server derives from the data dir: the overview, server
 * views and their host configuration (as `stretto init` prints it), probes,
 * settings and doctor's output. The mock computes them from its world.
 */
import type {
  DiscoveredUpstream,
  FlowSummary,
  HealthItem,
  HostConfig,
  HostName,
  Overview,
  ProbeResult,
  ServerEntry,
  ServerView,
  Settings,
  ToolInfo,
} from '../src/api/types.ts'
import type { World } from './fixtures/world.ts'
import { FS_TOOLS, SHOP_TOOLS } from './fixtures/tools.ts'
import type { ToolDef } from './fixtures/recorder.ts'

export interface MockOptions {
  auth: boolean
  readOnly: boolean
  keySet: boolean
  redactSalt: boolean
}

const DAY = 86_400_000

function dayKey(ms: number): string {
  const d = new Date(ms)
  const p = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`
}

/** `~/.stretto/x` (or a path relative to the data dir) as the data dir's relative path. */
export function relative(world: World, path: string): string {
  if (path.startsWith('~/.stretto/')) return path.slice('~/.stretto/'.length)
  if (path.startsWith(world.dataDir + '/')) return path.slice(world.dataDir.length + 1)
  return path
}

export function flowByPath(world: World, path: string | null): FlowSummary | null {
  if (!path) return null
  const rel = relative(world, path)
  return world.flows.find((f) => f.summary.path === rel)?.summary ?? null
}

export function flowSummaries(world: World): FlowSummary[] {
  return world.flows
    .map((f) => ({
      ...f.summary,
      served_by: world.servers
        .filter((s) => s.flow && relative(world, s.flow) === f.summary.path)
        .map((s) => s.name),
    }))
    .sort((a, b) => b.modified_unix_ms - a.modified_unix_ms)
}

export function defaultRecord(name: string, mode: ServerEntry['mode']): string {
  return `~/.stretto/${mode === 'shadow' ? 'shadow' : 'logs'}/${name}`
}

/** stretto_report::init::Setup::args, with the console's extras (threshold, HTTP). */
export function proxyArgs(world: World, entry: ServerEntry): string[] {
  const args = [
    '--record',
    entry.record_dir ?? defaultRecord(entry.name, entry.mode),
    '--domain',
    entry.name,
  ]
  if (entry.flow && entry.mode !== 'record') {
    args.push('--flow', entry.flow)
    const flow = flowByPath(world, entry.flow)
    const decider =
      entry.decider ?? (flow?.has_arbiter ? null : flow?.has_reach === false ? 'habit' : 'reach')
    if (decider) args.push('--flow-decider', decider)
    if (entry.threshold !== null) args.push('--flow-threshold', String(entry.threshold))
    if (entry.mode === 'shadow') args.push('--flow-shadow')
  }
  if (entry.upstream.kind === 'http') {
    args.push('--upstream', entry.upstream.url)
    for (const h of entry.upstream.headers) args.push('--upstream-header', `${h.name}=${h.env}`)
  } else {
    args.push('--', ...entry.upstream.command)
  }
  return args
}

export function serverIssues(world: World, entry: ServerEntry): string[] {
  const issues: string[] = []
  if (entry.mode !== 'record' && !entry.flow) issues.push(`mode is ${entry.mode} but no flow`)
  if (entry.flow) {
    const flow = flowByPath(world, entry.flow)
    if (!flow) issues.push(`flow not found: ${entry.flow}`)
    else if (flow.error) issues.push(`the flow does not load: ${flow.error}`)
    else if (flow.domain !== entry.name)
      issues.push(`the flow's domain is ${flow.domain}, not ${entry.name}`)
    if (entry.mode === 'record') issues.push('a flow is set, but the mode is record: it is not run')
  }
  return issues
}

export function serverView(world: World, entry: ServerEntry): ServerView {
  const sessions = world.sessions.filter((s) => s.summary.domain === entry.name)
  const flow = flowByPath(world, entry.flow)
  return {
    ...entry,
    proxy_args: proxyArgs(world, entry),
    sessions: sessions.length,
    last_session_unix_ms: sessions.length
      ? Math.max(...sessions.map((s) => s.summary.started_unix_ms))
      : null,
    flow_summary: flow ? (flowSummaries(world).find((f) => f.key === flow.key) ?? flow) : null,
    issues: serverIssues(world, entry),
  }
}

export function discovered(world: World): DiscoveredUpstream[] {
  const byDomain = new Map<string, DiscoveredUpstream>()
  for (const s of world.sessions) {
    const domain = s.summary.domain
    const upstream = s.summary.upstream
    if (!domain || !upstream) continue
    const d = byDomain.get(domain)
    if (!d) {
      byDomain.set(domain, {
        domain,
        upstream,
        sessions: 1,
        last_seen_unix_ms: s.summary.started_unix_ms,
        registered: world.servers.some((x) => x.name === domain),
      })
    } else {
      d.sessions += 1
      if (s.summary.started_unix_ms > d.last_seen_unix_ms) {
        d.last_seen_unix_ms = s.summary.started_unix_ms
        d.upstream = upstream
      }
    }
  }
  return [...byDomain.values()].sort((a, b) => b.last_seen_unix_ms - a.last_seen_unix_ms)
}

function shellQuote(word: string): string {
  if (word !== '' && /^[A-Za-z0-9\-_./:=@%+,~]+$/.test(word)) return word
  return `'${word.replace(/'/g, `'\\''`)}'`
}

const PLACEMENT: Record<HostName, string> = {
  'claude-code':
    'Run the command in the project where you use Claude Code. With `--scope user` it applies to every project; `--scope project` writes it to .mcp.json, to share.',
  'claude-desktop':
    'Merge it into claude_desktop_config.json (on macOS in ~/Library/Application Support/Claude/, on Windows in %APPDATA%\\Claude\\), then restart Claude Desktop.',
  cursor: 'Merge it into ~/.cursor/mcp.json (every project) or .cursor/mcp.json (this project).',
  vscode:
    'Merge it into .vscode/mcp.json in the workspace, or into your user mcp.json (the command MCP: Open User Configuration).',
}

/** What `stretto init` prints for a host: the configuration, where it goes, and the next steps. */
export function hostConfig(world: World, entry: ServerEntry, host: HostName): HostConfig {
  const args = proxyArgs(world, entry)
  const proxy = 'stretto-proxy'
  let snippet: string
  if (host === 'claude-code') {
    snippet =
      ['claude', 'mcp', 'add', entry.name, '--', proxy, ...args].map(shellQuote).join(' ') + '\n'
  } else {
    const key = host === 'vscode' ? 'servers' : 'mcpServers'
    const kind = host === 'vscode' ? '      "type": "stdio",\n' : ''
    snippet = `{\n  "${key}": {\n    ${JSON.stringify(entry.name)}: {\n${kind}      "command": ${JSON.stringify(proxy)},\n      "args": [${args.map((a) => JSON.stringify(a)).join(', ')}]\n    }\n  }\n}\n`
  }
  return {
    host,
    language: host === 'claude-code' ? 'shell' : 'json',
    snippet,
    placement: PLACEMENT[host],
    next_steps: nextSteps(world, entry, host),
  }
}

/** stretto_report::init::next_steps, from the step the server is at. */
function nextSteps(world: World, entry: ServerEntry, host: HostName): string {
  const d = entry.name
  const init = `stretto init --host ${host} --domain ${d}`
  const server =
    entry.upstream.kind === 'stdio'
      ? entry.upstream.command.map(shellQuote).join(' ')
      : `--upstream ${entry.upstream.url}`
  const record = entry.record_dir ?? defaultRecord(d, entry.mode)
  const flow = `~/.stretto/${d}.flow.json`
  const promoted = `~/.stretto/${d}-promoted.flow.json`
  const steps: string[] = []
  const serve = `Serve it: after each of the agent's calls, the flow's lookups ride in the same result. Replace the configuration with what this prints:\n${init} --flow ${promoted} -- ${server}`
  if (entry.mode === 'record' || !entry.flow) {
    steps.push(`Use the agent as usual. The proxy records each session in ${record}.`)
    steps.push(
      `Learn a flow from the sessions, with no key:\nstretto learn --sessions ${record} --domain ${d} --habit-only --out ${flow}`,
    )
    steps.push(
      `Review what it may do: the tools it may call, the lookups it may make and where their arguments come from:\nstretto flow-show ${flow}`,
    )
    steps.push(
      `Run it in shadow: it decides and logs, but looks nothing up. Replace the configuration above with what this prints:\n${init} --flow ${flow} --shadow -- ${server}`,
    )
    steps.push(
      `Keep the flow to the calls where its lookups were the agent's own:\nstretto promote --flow ${flow} --sessions ~/.stretto/shadow/${d} --decider reach --out ${promoted}`,
    )
    steps.push(serve)
  } else if (entry.mode === 'shadow') {
    const summary = flowByPath(world, entry.flow)
    steps.push(
      `Use the agent as usual. The proxy records each session in ${record}, and next to it what the flow would have looked up (<session>.flow.jsonl).`,
    )
    steps.push(
      `Keep the flow to the calls where its lookups were the agent's own:\nstretto promote --flow ${entry.flow} --sessions ${record}${summary?.has_arbiter ? ' --oracle-cache ~/.stretto/oracle-cache' : ' --decider reach'} --out ${promoted}`,
    )
    steps.push(serve)
  } else {
    steps.push(
      `Use the agent as usual. After each of its calls, the flow's lookups ride in the same result. The proxy records each session in ${record}, and next to it each of the flow's decisions (<session>.flow.jsonl).`,
    )
    steps.push(
      `When the agent, its prompts or the server change, learn again and review what changed (flow-diff exits with 1 when a change needs review):\nstretto learn --sessions ${record} --domain ${d} --habit-only --out ~/.stretto/${d}-new.flow.json\nstretto flow-diff ${entry.flow} ~/.stretto/${d}-new.flow.json`,
    )
  }
  let out = 'Next steps (docs/walkthrough.md runs them on a real server):\n'
  steps.forEach((step, i) => {
    const [first, ...rest] = step.split('\n')
    out += `\n${i + 1}. ${first}\n`
    for (const line of rest) out += `     ${line}\n`
  })
  return out
}

function infos(tools: ToolDef[]): ToolInfo[] {
  return tools.map((t) => ({
    name: t.name,
    kind: t.readOnly === null ? 'generic' : t.readOnly ? 'read' : 'write',
    description: t.description,
    read_only_hint: t.readOnly,
    destructive_hint: t.destructive ?? null,
    contract: Object.keys(t.args)
      .map((a) => `${a}:string${t.required.includes(a) ? '!' : ''}`)
      .join(', '),
  }))
}

/** Connect to the server as the proxy would, and list its tools. */
export function probe(world: World, entry: ServerEntry): ProbeResult {
  const fail = (ms: number, error: string): ProbeResult => ({
    ok: false,
    ms,
    error,
    server_name: null,
    server_version: null,
    protocol_version: null,
    instructions: null,
    tools: [],
    flow_warnings: [],
  })
  if (entry.upstream.kind === 'http') {
    const header = entry.upstream.headers[0]
    return fail(
      214,
      header
        ? `POST ${entry.upstream.url}: 401 Unauthorized. ${header.name} is sent from ${header.env}, which is not set in the console's environment.`
        : `POST ${entry.upstream.url}: could not connect (connection refused)`,
    )
  }
  const program = entry.upstream.command[0] ?? ''
  let tools: ToolDef[] | null = null
  let server = { name: '', version: '', instructions: null as string | null }
  if (program.endsWith('stretto-mcp-demo')) {
    tools = SHOP_TOOLS
    server = {
      name: 'stretto-mcp-demo',
      version: '0.1.0',
      instructions: 'A demo server for stretto-proxy: two tools that echo their arguments.',
    }
  } else if (entry.upstream.command.some((w) => w.includes('server-filesystem'))) {
    tools = FS_TOOLS
    server = { name: 'secure-filesystem-server', version: '0.6.3', instructions: null }
  }
  if (!tools)
    return fail(
      12,
      `could not start ${program || 'the command'}: No such file or directory (os error 2)`,
    )
  const result: ProbeResult = {
    ok: true,
    ms: program.endsWith('stretto-mcp-demo') ? 41 : 1840,
    error: null,
    server_name: server.name,
    server_version: server.version,
    protocol_version: '2025-06-18',
    instructions: server.instructions,
    tools: infos(tools),
    flow_warnings: [],
  }
  const flow = world.flows.find((f) => f.summary.path === relative(world, entry.flow ?? ''))
  if (flow && entry.mode !== 'record') {
    const listed = new Map(result.tools.map((t) => [t.name, t]))
    for (const t of flow.tools.filter((x) => x.kind === 'read')) {
      const now = listed.get(t.name)
      if (!now)
        result.flow_warnings.push(
          `${t.name}: the flow looks it up, and the server no longer lists it`,
        )
      else if (now.read_only_hint === false)
        result.flow_warnings.push(
          `${t.name}: the flow reads it, and the server now marks it readOnlyHint: false`,
        )
    }
  }
  return result
}

export function health(world: World, options: MockOptions): HealthItem[] {
  const items: HealthItem[] = [
    { level: 'ok', message: `${world.dataDir} exists and is writable` },
    { level: 'ok', message: 'stretto 0.1.0 and stretto-proxy 0.1.0 found in /home/me/.cargo/bin' },
  ]
  items.push(
    options.keySet
      ? { level: 'ok', message: 'TYPESAFE_API_KEY is set (its value is not shown)' }
      : {
          level: 'note',
          message:
            'TYPESAFE_API_KEY is not set: flows served with reach or the habit need none; a flow with an arbiter asks Jev and needs it',
        },
  )
  for (const f of world.flows)
    if (f.summary.error)
      items.push({ level: 'error', message: `${f.summary.path} does not load: ${f.summary.error}` })
  for (const s of world.servers)
    for (const issue of serverIssues(world, s))
      items.push({ level: 'warn', message: `Server ${s.name}: ${issue}` })
  for (const d of discovered(world)) {
    if (!d.registered)
      items.push({
        level: 'note',
        message: `${d.sessions} sessions of ${d.domain} came through a server that is not registered`,
      })
  }
  return items
}

export function overview(world: World, options: MockOptions, jobs: World['jobs']): Overview {
  const sessions = world.sessions.map((s) => s.summary)
  const days: Overview['activity'] = []
  for (let i = 13; i >= 0; i--) {
    const day = dayKey(world.now - i * DAY)
    const of = sessions.filter((s) => dayKey(s.started_unix_ms) === day)
    days.push({
      day,
      sessions: of.length,
      tool_calls: of.reduce((n, s) => n + s.tool_calls, 0),
      flow_lookups: of.reduce((n, s) => n + s.flow_lookups, 0),
    })
  }
  const domains = new Map<string, Overview['domains'][number]>()
  for (const s of sessions) {
    if (!s.domain) continue
    const d = domains.get(s.domain) ?? {
      name: s.domain,
      sessions: 0,
      last_session_unix_ms: null,
      modes: { recorded: 0, shadow: 0, served: 0 },
      flows: [],
      servers: [],
    }
    d.sessions += 1
    d.modes[s.mode] += 1
    d.last_session_unix_ms = Math.max(d.last_session_unix_ms ?? 0, s.started_unix_ms)
    domains.set(s.domain, d)
  }
  for (const d of domains.values()) {
    d.flows = flowSummaries(world)
      .filter((f) => f.domain === d.name && !f.error)
      .map((f) => f.key)
    d.servers = world.servers.filter((s) => s.name === d.name).map((s) => s.name)
  }
  const weekAgo = world.now - 7 * DAY
  return {
    totals: {
      sessions: sessions.length,
      sessions_7d: sessions.filter((s) => s.started_unix_ms >= weekAgo).length,
      domains: domains.size,
      flows: world.flows.length,
      servers: world.servers.length,
      tool_calls: sessions.reduce((n, s) => n + s.tool_calls, 0),
      flow_lookups: sessions.reduce((n, s) => n + s.flow_lookups, 0),
      hand_backs: sessions.reduce((n, s) => n + s.hand_backs, 0),
      shadow_decisions: world.sessions.reduce(
        (n, s) => n + s.detail.decisions.filter((d) => d.shadow).length,
        0,
      ),
      errors: sessions.reduce((n, s) => n + s.errors, 0),
    },
    domains: [...domains.values()].sort(
      (a, b) => (b.last_session_unix_ms ?? 0) - (a.last_session_unix_ms ?? 0),
    ),
    activity: days,
    recent_sessions: sessions.slice(0, 8),
    health: health(world, options),
    jobs: jobs.slice(0, 5).map((j) => ({ ...j, output: j.output.slice(-65536) })),
  }
}

export function settings(world: World, options: MockOptions): Settings {
  const logs = world.sessions.reduce((n, s) => n + s.summary.size_bytes + s.flowLog.length, 0)
  const flows = world.flows.reduce((n, f) => n + f.summary.size_bytes, 0)
  const other = 18_432
  return {
    data_dir: world.dataDir,
    disk: {
      logs_bytes: logs,
      flows_bytes: flows,
      cache_bytes: 0,
      other_bytes: other,
      total_bytes: logs + flows + other,
    },
    sessions: world.sessions.length,
    flows: world.flows.length,
    key_set: options.keySet,
    retention_note:
      'The console deletes nothing: Delete moves a session or a flow to console/trash. stretto-proxy --retain-days N deletes logs and cached answers older than N days when it starts.',
    binaries: [
      { name: 'stretto', path: '/home/me/.cargo/bin/stretto', version: 'stretto 0.1.0' },
      {
        name: 'stretto-proxy',
        path: '/home/me/.cargo/bin/stretto-proxy',
        version: 'stretto-proxy 0.1.0',
      },
    ],
    version: '0.1.0',
    read_only: options.readOnly,
    auth: options.auth,
  }
}

/** `stretto doctor`'s output for the mock's data dir. */
export function doctorOutput(world: World, options: MockOptions): string {
  const lines = [
    'stretto 0.1.0 (/home/me/.cargo/bin/stretto)',
    '',
    'ok       stretto-proxy 0.1.0 (/home/me/.cargo/bin/stretto-proxy)',
    'ok       stretto-procedure 0.1.0 (/home/me/.cargo/bin/stretto-procedure)',
    'ok       stretto-mcp-demo 0.1.0 (/home/me/.cargo/bin/stretto-mcp-demo)',
    `ok       ${world.dataDir} exists and is writable`,
    options.keySet
      ? 'ok       TYPESAFE_API_KEY is set (its value is not shown); `stretto doctor --network` asks Jev one question with it'
      : 'note     TYPESAFE_API_KEY is not set: flows served with `reach` or the habit need none',
  ]
  const good = world.flows.filter((f) => !f.summary.error)
  lines.push(`ok       ${good.length} flows in ~/.stretto:`)
  for (const f of good) {
    const s = f.summary
    const decides = s.has_arbiter
      ? `decides with its arbiter, which asks ${'`jev-latest`'}`
      : 'decides by `reach`'
    lines.push(
      `           ~/.stretto/${s.path}: domain ${s.domain}, from ${s.habit_episodes} sessions, ${decides}${s.promoted ? ', promoted' : ''}`,
    )
  }
  for (const f of world.flows.filter((x) => x.summary.error))
    lines.push(`error    ~/.stretto/${f.summary.path}: ${f.summary.error}`)
  lines.push('ok       recorded sessions in ~/.stretto:')
  const dirs = new Map<string, { n: number; domain: string }>()
  for (const s of world.sessions) {
    const dir = s.summary.path.split('/').slice(0, -1).join('/')
    const d = dirs.get(dir) ?? { n: 0, domain: s.summary.domain ?? '' }
    d.n += 1
    dirs.set(dir, d)
  }
  for (const [dir, d] of [...dirs.entries()].sort())
    lines.push(`           ~/.stretto/${dir}: ${d.n} sessions, domain ${d.domain}`)
  return lines.join('\n') + '\n'
}

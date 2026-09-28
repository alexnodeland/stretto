/**
 * A session, recorded as stretto-proxy records it: the header, every line on
 * the wire (client, server, proxy, context), the flow's decisions and runs,
 * and the console's view of it (calls, turns, events), built from the same
 * simulation so the raw log, the timeline and the counts agree.
 */
import type {
  CallView,
  ContextMessage,
  FlowDecision,
  FlowRun,
  SessionDetail,
  SessionMode,
  SessionSummary,
  TimelineEvent,
  ToolInfo,
  ToolKind,
  Turn,
  Upstream,
} from '../../src/api/types.ts'

export interface ToolDef {
  name: string
  description: string
  readOnly: boolean | null
  destructive?: boolean
  args: Record<string, string>
  required: string[]
}

export interface Result {
  text: string
  isError?: boolean
}

export interface Look {
  tool: string
  arguments: Record<string, unknown>
  prob: number
  binding: number
  probs?: Record<string, number>
  result: Result
}

export interface HandBack {
  reason: string
  prob: number | null
  tool?: string
}

type Line = Record<string, unknown>

function kindOf(tool: ToolDef | undefined): ToolKind {
  if (!tool || tool.readOnly === null) return 'generic'
  return tool.readOnly ? 'read' : 'write'
}

function tryJson(text: string): unknown | null {
  const t = text.trim()
  if (!t.startsWith('{') && !t.startsWith('[')) return null
  try {
    return JSON.parse(t)
  } catch {
    return null
  }
}

function compact(value: unknown, max = 96): string {
  const text = typeof value === 'string' ? value : JSON.stringify(value)
  return text.length > max ? `${text.slice(0, max - 1)}…` : text
}

export interface SessionSpec {
  session: string
  started: number
  domain: string
  agentModel: string | null
  client: { name: string; version: string }
  upstream: Upstream
  server: { name: string; version: string; instructions: string | null }
  tools: ToolDef[]
  /** served: the flow's lookups ride in the results; shadow: decided and logged only. */
  flow: 'none' | 'shadow' | 'served'
  dir: string
}

export class Recorder {
  readonly spec: SessionSpec
  readonly lines: Line[] = []
  readonly calls: CallView[] = []
  readonly context: ContextMessage[] = []
  readonly decisions: FlowDecision[] = []
  readonly runs: FlowRun[] = []
  readonly events: TimelineEvent[] = []
  readonly turns: Turn[] = []
  private t = 0
  private id = 1
  private lookupId = 1
  private readonly toolsByName: Map<string, ToolDef>

  constructor(spec: SessionSpec) {
    this.spec = spec
    this.toolsByName = new Map(spec.tools.map((t) => [t.name, t]))
    this.open()
  }

  private line(from: string, message: unknown, event: Omit<TimelineEvent, 't_ms' | 'from'>) {
    this.lines.push({ t_ms: this.t, from, message })
    this.events.push({ t_ms: this.t, from: from as TimelineEvent['from'], ...event })
  }

  private wait(ms: number) {
    this.t += Math.max(0, Math.round(ms))
  }

  private open() {
    const init = this.id++
    this.line(
      'client',
      {
        jsonrpc: '2.0',
        id: init,
        method: 'initialize',
        params: { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: this.spec.client },
      },
      {
        kind: 'request',
        method: 'initialize',
        id: String(init),
        summary: `initialize from ${this.spec.client.name} ${this.spec.client.version}`,
      },
    )
    this.wait(this.spec.upstream.kind === 'http' ? 180 : 38)
    this.line(
      'server',
      {
        id: init,
        jsonrpc: '2.0',
        result: {
          capabilities: { tools: { listChanged: false } },
          ...(this.spec.server.instructions ? { instructions: this.spec.server.instructions } : {}),
          protocolVersion: '2025-06-18',
          serverInfo: { name: this.spec.server.name, version: this.spec.server.version },
        },
      },
      {
        kind: 'response',
        method: null,
        id: String(init),
        summary: `${this.spec.server.name} ${this.spec.server.version}, protocol 2025-06-18`,
      },
    )
    this.wait(3)
    this.line(
      'client',
      { jsonrpc: '2.0', method: 'notifications/initialized' },
      {
        kind: 'notification',
        method: 'notifications/initialized',
        id: null,
        summary: 'notifications/initialized',
      },
    )
    const list = this.id++
    this.line(
      'client',
      { jsonrpc: '2.0', id: list, method: 'tools/list', params: {} },
      { kind: 'request', method: 'tools/list', id: String(list), summary: 'tools/list' },
    )
    this.wait(this.spec.upstream.kind === 'http' ? 120 : 4)
    this.line(
      'server',
      {
        id: list,
        jsonrpc: '2.0',
        result: {
          tools: this.spec.tools.map((t) => ({
            ...(t.readOnly === null
              ? {}
              : {
                  annotations: {
                    readOnlyHint: t.readOnly,
                    ...(t.destructive ? { destructiveHint: true } : {}),
                  },
                }),
            description: t.description,
            inputSchema: {
              properties: Object.fromEntries(
                Object.keys(t.args).map((a) => [a, { type: 'string', description: t.args[a] }]),
              ),
              required: t.required,
              type: 'object',
            },
            name: t.name,
          })),
        },
      },
      {
        kind: 'response',
        method: null,
        id: String(list),
        summary: `${this.spec.tools.length} tools: ${this.spec.tools.map((t) => t.name).join(', ')}`,
      },
    )
  }

  /** A line of the conversation, as the host appended it to the context file. */
  say(role: 'user' | 'assistant', content: string, after = 0) {
    this.wait(after)
    this.context.push({ t_ms: this.t, role, content })
    this.line(
      'context',
      { role, content },
      {
        kind: 'context',
        method: null,
        id: null,
        summary: `${role === 'user' ? 'customer' : 'agent'}: ${compact(content, 110)}`,
      },
    )
  }

  /**
   * One of the agent's calls, in a turn of its own. With a served flow, the
   * proxy holds the server's result, makes the flow's lookups, and answers
   * with the result and theirs; with a flow in shadow it only logs.
   */
  call(
    tool: string,
    args: Record<string, unknown>,
    result: Result,
    options: {
      think?: number
      latency?: number
      lookups?: Look[]
      handBack?: HandBack | null
    } = {},
  ): string {
    this.wait(options.think ?? 2600)
    const id = this.id++
    const callId = String(id)
    const start = this.t
    this.line(
      'client',
      { jsonrpc: '2.0', id, method: 'tools/call', params: { name: tool, arguments: args } },
      {
        kind: 'request',
        method: 'tools/call',
        id: callId,
        summary: `tools/call ${tool} ${compact(args, 80)}`,
      },
    )
    this.wait(options.latency ?? (this.spec.upstream.kind === 'http' ? 140 : 6))
    const response = {
      id,
      jsonrpc: '2.0',
      result: { content: [{ text: result.text, type: 'text' }], isError: !!result.isError },
    }
    const flowActs = this.spec.flow !== 'none'
    if (this.spec.flow !== 'served') {
      this.line('server', response, {
        kind: result.isError ? 'error' : 'response',
        method: null,
        id: callId,
        summary: `${result.isError ? 'error' : 'result'}: ${compact(result.text, 100)}`,
      })
    } else {
      // The proxy reads the server's answer and holds it while the flow decides.
      this.lines.push({ t_ms: this.t, from: 'server', message: response })
      this.events.push({
        t_ms: this.t,
        from: 'server',
        kind: result.isError ? 'error' : 'response',
        method: null,
        id: callId,
        summary: `${result.isError ? 'error' : 'result'}: ${compact(result.text, 100)} (held for the flow)`,
      })
    }
    const resultT = this.t
    const view: CallView = {
      id: callId,
      by: 'agent',
      tool,
      kind: kindOf(this.toolsByName.get(tool)),
      arguments: args,
      t_ms: start,
      result_t_ms: resultT,
      latency_ms: resultT - start,
      ok: !result.isError,
      result_text: result.text,
      result_json: tryJson(result.text),
      result_truncated: false,
      turn: this.turns.length,
      after: null,
      decision: null,
    }
    this.calls.push(view)

    let end = resultT
    if (flowActs) {
      const site = result.isError ? `${tool} (error)` : tool
      const shadow = this.spec.flow === 'shadow'
      const made: Look[] = options.lookups ?? []
      const runSites: [string, unknown, number][] = []
      const appended: string[] = []
      let prevSite = site
      made.forEach((look, i) => {
        this.wait(1)
        const index = this.decisions.length
        this.decisions.push({
          after: callId,
          address: `decide#${shadow ? 0 : i}`,
          site: prevSite,
          action: 'lookup',
          tool: look.tool,
          arguments: look.arguments,
          prob: look.prob,
          probs: look.probs ?? { [look.tool]: look.prob, respond: 0 },
          binding: look.binding,
          reason: null,
          shadow,
          ms: 0,
        })
        if (!shadow) {
          const lid = `stretto-${this.lookupId++}`
          const t0 = this.t
          this.line(
            'proxy',
            {
              id: lid,
              jsonrpc: '2.0',
              method: 'tools/call',
              params: { arguments: look.arguments, name: look.tool },
            },
            {
              kind: 'request',
              method: 'tools/call',
              id: lid,
              summary: `tools/call ${look.tool} ${compact(look.arguments, 80)} (the flow's lookup)`,
            },
          )
          this.wait(this.spec.upstream.kind === 'http' ? 110 : 1)
          this.line(
            'server',
            {
              id: lid,
              jsonrpc: '2.0',
              result: {
                content: [{ text: look.result.text, type: 'text' }],
                isError: !!look.result.isError,
              },
            },
            {
              kind: look.result.isError ? 'error' : 'response',
              method: null,
              id: lid,
              summary: `result: ${compact(look.result.text, 100)}`,
            },
          )
          this.calls.push({
            id: lid,
            by: 'flow',
            tool: look.tool,
            kind: kindOf(this.toolsByName.get(look.tool)),
            arguments: look.arguments,
            t_ms: t0,
            result_t_ms: this.t,
            latency_ms: this.t - t0,
            ok: !look.result.isError,
            result_text: look.result.text,
            result_json: tryJson(look.result.text),
            result_truncated: false,
            turn: null,
            after: callId,
            decision: index,
          })
          appended.push(`${look.tool} ${JSON.stringify(look.arguments)}:\n${look.result.text}`)
          runSites.push([`decide#${i}`, i + 1, 0], [`outcome#${i}`, !look.result.isError, -0.074])
        } else {
          runSites.push([`decide#0`, 0, 0])
        }
        prevSite = look.tool
      })
      const hb = options.handBack
      // In shadow the flow decides once after each call: a lookup it would make, or handing back.
      if (hb !== null && !(shadow && made.length)) {
        const reason = hb?.reason ?? 'no lookups followed here in training'
        this.decisions.push({
          after: callId,
          address: `decide#${shadow ? 0 : made.length}`,
          site: made.length && !shadow ? prevSite : site,
          action: 'hand_back',
          tool: null,
          arguments: null,
          prob: hb?.prob ?? null,
          probs: hb?.tool && hb.prob !== null ? { [hb.tool]: hb.prob, respond: 0 } : {},
          binding: null,
          reason,
          shadow,
          ms: 0,
        })
        runSites.push([`decide#${shadow ? 0 : made.length}`, 0, 0])
      }
      this.runs.push({
        after: callId,
        call: tool,
        failed: !!result.isError,
        max_lookups: 8,
        sites: shadow ? [['decide#0', 0, 0]] : runSites,
        surprise: made.length && !shadow ? 0.2817473369319663 : -0,
      })
      if (!shadow) {
        this.wait(1)
        const content = [{ text: result.text, type: 'text' }]
        if (appended.length) {
          content.push({
            text: `--- Also looked up automatically (current results; no need to repeat these calls) ---\n\n${appended.join('\n\n')}`,
            type: 'text',
          })
        }
        this.line(
          'proxy',
          { id, jsonrpc: '2.0', result: { content, isError: !!result.isError } },
          {
            kind: 'response',
            method: null,
            id: callId,
            summary: appended.length
              ? `result, with ${appended.length} ${appended.length === 1 ? 'lookup' : 'lookups'} appended`
              : 'result, as the server gave it',
          },
        )
        end = this.t
      }
    }
    this.turns.push({ index: this.turns.length, start_ms: start, end_ms: end, calls: [callId] })
    return callId
  }

  /** A call the server never answered: it stays open. */
  unanswered(tool: string, args: Record<string, unknown>, think = 2400) {
    this.wait(think)
    const id = this.id++
    this.line(
      'client',
      { jsonrpc: '2.0', id, method: 'tools/call', params: { name: tool, arguments: args } },
      {
        kind: 'request',
        method: 'tools/call',
        id: String(id),
        summary: `tools/call ${tool} ${compact(args, 80)}`,
      },
    )
    this.calls.push({
      id: String(id),
      by: 'agent',
      tool,
      kind: kindOf(this.toolsByName.get(tool)),
      arguments: args,
      t_ms: this.t,
      result_t_ms: null,
      latency_ms: null,
      ok: null,
      result_text: null,
      result_json: null,
      result_truncated: false,
      turn: this.turns.length,
      after: null,
      decision: null,
    })
    this.turns.push({
      index: this.turns.length,
      start_ms: this.t,
      end_ms: this.t,
      calls: [String(id)],
    })
  }

  end(after = 800) {
    this.wait(after)
  }

  get header() {
    const s = this.spec
    return {
      stretto_mcp_log: 2,
      session: s.session,
      started_unix_ms: s.started,
      ...(s.upstream.kind === 'stdio'
        ? { server_command: s.upstream.command }
        : { upstream: s.upstream.url }),
      domain: s.domain,
      agent_model: s.agentModel,
    }
  }

  get raw(): string {
    return [this.header, ...this.lines].map((l) => JSON.stringify(l)).join('\n') + '\n'
  }

  get flowLog(): string {
    const out: string[] = []
    for (const run of this.runs) {
      for (const d of this.decisions.filter((x) => x.after === run.after))
        out.push(JSON.stringify(d))
      out.push(
        JSON.stringify({
          after: Number(run.after),
          run: {
            call: run.call,
            failed: run.failed,
            max_lookups: run.max_lookups,
            sites: run.sites,
            surprise: run.surprise,
          },
        }),
      )
    }
    return out.join('\n')
  }

  toolInfos(): ToolInfo[] {
    return this.spec.tools.map((t) => ({
      name: t.name,
      kind: kindOf(t),
      description: t.description,
      read_only_hint: t.readOnly,
      destructive_hint: t.destructive ?? null,
    }))
  }

  summary(key: string): SessionSummary {
    const agent = this.calls.filter((c) => c.by === 'agent')
    const flowCalls = this.calls.filter((c) => c.by === 'flow')
    const mode: SessionMode =
      this.spec.flow === 'served'
        ? 'served'
        : this.spec.flow === 'shadow' && this.decisions.length
          ? 'shadow'
          : 'recorded'
    const raw = this.raw
    return {
      key,
      path: `${this.spec.dir}/${this.spec.session}.jsonl`,
      session_id: this.spec.session,
      domain: this.spec.domain,
      agent: this.spec.agentModel ?? this.spec.client.name,
      started_unix_ms: this.spec.started,
      duration_ms: this.t,
      mode,
      tool_calls: agent.length,
      llm_turns: this.turns.length,
      errors: this.calls.filter((c) => c.ok === false).length,
      flow_lookups: flowCalls.length,
      hand_backs: this.decisions.filter((d) => d.action === 'hand_back').length,
      shadow_lookups: this.decisions.filter((d) => d.action === 'lookup' && d.shadow).length,
      writes: agent.filter((c) => c.kind === 'write').length,
      upstream: this.spec.upstream,
      size_bytes: new TextEncoder().encode(raw).length,
      has_flow_log: this.decisions.length > 0,
      has_confirm_log: false,
    }
  }

  detail(key: string): SessionDetail {
    return {
      summary: this.summary(key),
      header: this.header,
      tools: this.toolInfos(),
      turns: this.turns,
      calls: this.calls,
      context: this.context,
      decisions: this.decisions,
      runs: this.runs,
      confirmations: [],
      events: this.events,
      truncated: false,
    }
  }
}

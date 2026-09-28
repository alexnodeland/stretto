/**
 * The console API's types, field for field as the spec names them (snake_case).
 *
 * The server's DTOs will be exported by ts-rs into `./generated/`; until they
 * land, this file mirrors them by hand, so switching over is a rename of the
 * import path, not a redesign. Timestamps are unix milliseconds (`*_unix_ms`),
 * sizes are bytes, and probabilities are 0..1.
 */

/** Every error the API returns: `{"error": "message"}` with a 4xx or 5xx status. */
export interface ApiErrorBody {
  error: string
}

/** `{ok: true}`, the answer to a delete. */
export interface Ok {
  ok: boolean
}

// ---------------------------------------------------------------------------
// Meta and health

/** `GET /api/health` (no auth). */
export interface Health {
  ok: boolean
  version: string
}

/** A binary the console found beside itself, else on PATH. */
export interface Binary {
  path: string
  version: string
}

/** `GET /api/meta`. */
export interface Meta {
  version: string
  data_dir: string
  read_only: boolean
  /** Whether the console asks for its token (false with `--no-auth`). */
  auth: boolean
  /** TYPESAFE_API_KEY or TYPESAFE_API_KEY_FILE is present (never the value). */
  key_set: boolean
  stretto: Binary | null
  proxy: Binary | null
}

// ---------------------------------------------------------------------------
// Overview

export interface Totals {
  sessions: number
  sessions_7d: number
  domains: number
  flows: number
  servers: number
  tool_calls: number
  flow_lookups: number
  hand_backs: number
  shadow_decisions: number
  errors: number
}

export type SessionMode = 'recorded' | 'shadow' | 'served'

export interface ModeCounts {
  recorded: number
  shadow: number
  served: number
}

export interface DomainSummary {
  name: string
  sessions: number
  last_session_unix_ms: number | null
  modes: ModeCounts
  /** Flow keys. */
  flows: string[]
  /** Registry server names. */
  servers: string[]
}

/** One of the last 14 days, oldest first. */
export interface DayActivity {
  /** `YYYY-MM-DD`. */
  day: string
  sessions: number
  tool_calls: number
  flow_lookups: number
}

export type HealthLevel = 'ok' | 'note' | 'warn' | 'error'

export interface HealthItem {
  level: HealthLevel
  message: string
}

/** `GET /api/overview`. */
export interface Overview {
  totals: Totals
  domains: DomainSummary[]
  activity: DayActivity[]
  /** The 8 newest. */
  recent_sessions: SessionSummary[]
  health: HealthItem[]
  /** The 5 newest. */
  jobs: Job[]
}

// ---------------------------------------------------------------------------
// Sessions (traces)

/** Where a session's server was: a command, or a Streamable HTTP endpoint. */
export type Upstream = { kind: 'stdio'; command: string[] } | { kind: 'http'; url: string }

export interface SessionSummary {
  key: string
  /** Relative to the data dir. */
  path: string
  session_id: string
  domain: string | null
  /** The header's agent_model, else the host's clientInfo.name. */
  agent: string | null
  started_unix_ms: number
  duration_ms: number
  mode: SessionMode
  /** The agent's own tools/call. */
  tool_calls: number
  /** Inferred as stretto_trace does: parallel calls share a turn. */
  llm_turns: number
  /** Failed calls. */
  errors: number
  /** Lookups the proxy made for the flow. */
  flow_lookups: number
  hand_backs: number
  /** "lookup" decisions with shadow: true. */
  shadow_lookups: number
  /** Calls to tools the session's tools/list marks readOnlyHint: false. */
  writes: number
  upstream: Upstream | null
  size_bytes: number
  has_flow_log: boolean
  has_confirm_log: boolean
}

/** `GET /api/sessions`: a page, newest first. */
export interface SessionPage {
  total: number
  items: SessionSummary[]
}

export interface SessionQuery {
  domain?: string | null
  mode?: SessionMode | null
  q?: string | null
  limit?: number
  offset?: number
}

export type ToolKind = 'read' | 'write' | 'generic'

/** A tool as a server's tools/list gives it. */
export interface ToolInfo {
  name: string
  kind: ToolKind
  description: string | null
  read_only_hint: boolean | null
  destructive_hint: boolean | null
}

export interface Turn {
  index: number
  start_ms: number
  end_ms: number
  /** Call ids. */
  calls: string[]
}

/** Every tools/call in a session, the agent's and the flow's, in order. */
export interface CallView {
  id: string
  by: 'agent' | 'flow'
  tool: string
  kind: ToolKind
  arguments: unknown
  t_ms: number
  result_t_ms: number | null
  latency_ms: number | null
  /** null: no response. */
  ok: boolean | null
  result_text: string | null
  /** The result text parsed, when it is JSON. */
  result_json: unknown | null
  result_truncated: boolean
  /** Index into turns, for the agent's calls. */
  turn: number | null
  /** For the flow's lookups: the agent call they followed. */
  after: string | null
  /** For the flow's lookups: index into decisions. */
  decision: number | null
}

export interface ContextMessage {
  t_ms: number
  role: 'user' | 'assistant'
  content: string
}

/** One line of `<session>.flow.jsonl`. */
export interface FlowDecision {
  /** The agent call the decision followed. */
  after: string
  /** The site in the flow's run, as `decide#0`. */
  address: string
  /** The tool whose call just returned (with ` (error)` after a failed one). */
  site: string
  action: 'lookup' | 'hand_back'
  tool: string | null
  arguments: unknown | null
  /** The decider's probability of the likeliest lookup. */
  prob: number | null
  probs: Record<string, number>
  /** The chance that the bound arguments are the agent's own. */
  binding: number | null
  reason: string | null
  shadow: boolean
  ms: number | null
}

/** A run line of `<session>.flow.jsonl`: the flow's run after one call. */
export interface FlowRun {
  after: string
  call: string
  failed: boolean
  max_lookups: number
  /** Each site as `[address, value, logp]`. */
  sites: [string, unknown, number][]
  /** In nats. */
  surprise: number | null
}

export interface TimelineEvent {
  t_ms: number
  from: 'client' | 'server' | 'proxy' | 'context'
  kind: 'request' | 'response' | 'notification' | 'error' | 'context' | 'raw'
  method: string | null
  id: string | null
  summary: string
}

/** `GET /api/sessions/:key`. */
export interface SessionDetail {
  summary: SessionSummary
  /** The raw header line. */
  header: unknown
  tools: ToolInfo[]
  turns: Turn[]
  calls: CallView[]
  context: ContextMessage[]
  decisions: FlowDecision[]
  runs: FlowRun[]
  /** Raw lines of `<session>.confirm.jsonl`. */
  confirmations: unknown[]
  events: TimelineEvent[]
  /** Results over 64 KiB are cut (result_truncated on the call). */
  truncated: boolean
}

// ---------------------------------------------------------------------------
// Flows

export type Decider = 'arbiter' | 'habit' | 'reach'

export interface ToolCounts {
  read: number
  write: number
  generic: number
}

export interface PromotedCounts {
  sites_promoted: number
  sites_scored: number
}

export interface FlowSummary {
  key: string
  path: string
  name: string
  domain: string
  format_version: number
  stretto_version: string
  sources: string[]
  habit_episodes: number
  arbiter_cases: number
  compiled_unix_ms: number
  modified_unix_ms: number
  size_bytes: number
  /** Flow::served_decider. */
  decider: Decider
  has_arbiter: boolean
  has_reach: boolean
  tools: ToolCounts
  /** Sites with next lookups. */
  sites: number
  /** Distinct lookup tools. */
  lookups: number
  promoted: PromotedCounts | null
  /** Registry servers whose flow is this file. */
  served_by: string[]
  /** Set (and the rest defaulted) when the file doesn't load. */
  error: string | null
}

/** `GET /api/flows`, sorted by modified desc. */
export interface FlowList {
  items: FlowSummary[]
}

export interface FlowTool {
  name: string
  kind: ToolKind
  summary: string | null
  args: Record<string, string>
  contract: string | null
}

export interface SiteLookup {
  tool: string
  /** Times seen in training. */
  count: number
  share: number
  weighed_share: number
  binding_chance: number | null
  bindable: boolean
  prob: number
  acts: boolean
}

export interface SiteChoice {
  tool: string | null
  prob: number
  acts: boolean
}

export interface SitePromotion {
  decisions: number
  lookups: number
  used: number
  tasks: number
  lower: number
  promoted: boolean
}

export interface SiteView {
  /** "tool", or "tool (error)" after a failed call. */
  name: string
  tool: string
  failed: boolean
  /** Training steps after it. */
  steps: number
  weighed_by: 'reach' | 'habit'
  lookups: SiteLookup[]
  hand_back_share: number
  choice: SiteChoice | null
  promoted: SitePromotion | null
  /** A per-site override of the threshold. */
  threshold: number | null
}

export interface BindingSource {
  tool: string
  path: string
  count: number
  share: number
}

export interface BindingArg {
  name: string
  required: boolean
  calls: number
  constant: unknown | null
  sources: BindingSource[]
}

export interface BindingView {
  tool: string
  calls: number
  /** When the customer had not mentioned the values, when they had, and over both. */
  chance: [number, number, number] | null
  args: BindingArg[]
}

export interface Provenance {
  stretto: string
  sources: string[]
  habit_episodes: number
  arbiter_cases: number
  compiled_unix_ms: number
}

export interface PromotionBar {
  threshold: number
  min_used: number
  min_lower: number
  min_tasks: number
}

export interface Promotion {
  bar: PromotionBar
  sites_promoted: number
  sites_scored: number
}

/** `GET /api/flows/:key?threshold=`. */
export interface FlowDetail {
  summary: FlowSummary
  threshold: number
  tools: FlowTool[]
  sites: SiteView[]
  bindings: BindingView[]
  provenance: Provenance
  promotion: Promotion | null
  /** review::show(flow, threshold), the "as text" view. */
  review_markdown: string
  /** Such as a tool the flow reads whose recorded tools/list says readOnlyHint: false. */
  warnings: string[]
}

/** `GET /api/flows/diff?from=&to=&threshold=`. */
export interface FlowDiffView {
  from: string
  to: string
  /** Changes a reviewer must look at. */
  review: string[]
  /** The rest. */
  changes: string[]
  markdown: string
}

// ---------------------------------------------------------------------------
// Servers (upstream MCP config)

export type ServerMode = 'record' | 'shadow' | 'serve'

/** `--upstream-header NAME=VAR`: the header's name and the variable holding its value. */
export interface HeaderVar {
  name: string
  env: string
}

export type ServerUpstream =
  | {
      kind: 'stdio'
      command: string[]
      /** Variable names the server needs (documentation; values are never stored). */
      env: string[]
    }
  | { kind: 'http'; url: string; headers: HeaderVar[] }

export interface ServerEntry {
  /** The host's name for the server, which is the domain (a-z0-9_-). */
  name: string
  description: string | null
  upstream: ServerUpstream
  mode: ServerMode
  /** A flow file path (may start with ~/). */
  flow: string | null
  /** Default ~/.stretto/logs/<name> (shadow: ~/.stretto/shadow/<name>). */
  record_dir: string | null
  decider: Decider | null
  threshold: number | null
  created_unix_ms: number
  updated_unix_ms: number
}

/** The body of `POST /api/servers` and `PUT /api/servers/:name`. */
export type ServerInput = Omit<ServerEntry, 'created_unix_ms' | 'updated_unix_ms'>

export interface ServerView extends ServerEntry {
  /** The stretto-proxy command line. */
  proxy_args: string[]
  /** Sessions whose domain is the server's name. */
  sessions: number
  last_session_unix_ms: number | null
  flow_summary: FlowSummary | null
  /** Such as "mode is serve but no flow". */
  issues: string[]
}

/** An upstream seen in session headers. */
export interface DiscoveredUpstream {
  domain: string
  upstream: Upstream | null
  sessions: number
  last_seen_unix_ms: number
  registered: boolean
}

/** `GET /api/servers`. */
export interface ServerList {
  items: ServerView[]
  discovered: DiscoveredUpstream[]
}

export type HostName = 'claude-code' | 'claude-desktop' | 'cursor' | 'vscode'

/** `GET /api/servers/:name/config?host=`: what `stretto init` prints. */
export interface HostConfig {
  host: HostName
  language: 'shell' | 'json'
  snippet: string
  placement: string
  next_steps: string
}

/** `POST /api/servers/:name/probe`. */
export interface ProbeResult {
  ok: boolean
  ms: number
  error: string | null
  server_name: string | null
  server_version: string | null
  protocol_version: string | null
  instructions: string | null
  tools: ToolInfo[]
  /** Tools the flow reads that the server now marks as writes, or no longer lists. */
  flow_warnings: string[]
}

// ---------------------------------------------------------------------------
// Jobs (the stretto CLI, as subprocesses)

export type JobKind = 'learn' | 'promote' | 'audit' | 'redact' | 'doctor'

export type JobStatus = 'queued' | 'running' | 'succeeded' | 'failed'

export interface JobArtifact {
  kind: 'flow' | 'report' | 'dir'
  path: string
  key: string | null
}

export interface Job {
  id: string
  kind: JobKind
  title: string
  params: unknown
  status: JobStatus
  created_unix_ms: number
  started_unix_ms: number | null
  finished_unix_ms: number | null
  exit_code: number | null
  /** The tail (last 64 KiB) in lists; in full from GET /api/jobs/:id. */
  output: string
  artifacts: JobArtifact[]
}

export interface JobList {
  items: Job[]
}

export interface LearnJob {
  kind: 'learn'
  domain: string
  /** A directory, relative to the data dir or ~/…. */
  sessions: string
  /** Default <data>/<domain>.flow.json. */
  out: string | null
  /** Replace an existing flow; without it the server never overwrites one. */
  overwrite: boolean
  habit_only: boolean
  constants: boolean
}

export interface PromoteJob {
  kind: 'promote'
  /** A flow key. */
  flow: string
  sessions: string
  oracle_cache: string | null
  threshold: number
  min_used: number
  min_lower: number
  min_tasks: number
  /** Default <flow name>.promoted.flow.json. */
  out: string | null
}

export interface AuditJob {
  kind: 'audit'
  /** A flow key. */
  flow: string
  sessions: string
  decider: Decider | null
}

export interface RedactJob {
  kind: 'redact'
  sessions: string
  out: string
  keep_shared: number
  hash_fields: string[]
}

export interface DoctorJob {
  kind: 'doctor'
}

/** The body of `POST /api/jobs`. */
export type NewJob = LearnJob | PromoteJob | AuditJob | RedactJob | DoctorJob

// ---------------------------------------------------------------------------
// Settings

export interface DiskUsage {
  logs_bytes: number
  flows_bytes: number
  cache_bytes: number
  other_bytes: number
  total_bytes: number
}

export interface BinaryInfo {
  name: string
  path: string | null
  version: string | null
}

/** `GET /api/settings`. */
export interface Settings {
  data_dir: string
  disk: DiskUsage
  sessions: number
  flows: number
  key_set: boolean
  retention_note: string
  binaries: BinaryInfo[]
  version: string
  read_only: boolean
  auth: boolean
}

// ---------------------------------------------------------------------------
// Live updates: GET /api/events (Server-Sent Events)

export type ChangedWhat = 'sessions' | 'flows' | 'servers' | 'jobs'

/** `event: changed`. */
export interface ChangedEvent {
  what: ChangedWhat
  keys: string[]
}

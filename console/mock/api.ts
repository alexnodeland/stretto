/**
 * The mock console server: every endpoint of the spec over the mock world,
 * with the real server's rules (the token cookie, the X-Stretto-Console
 * header on writes, read-only, 404/409/400 with `{"error"}`), live updates
 * over SSE, and jobs that run and write their output line by line.
 *
 * Test hooks under /__mock: GET /__mock/state, POST /__mock/state with any of
 * {"auth", "read_only", "empty", "data_dir", "key_set", "redact_salt",
 * "latency", "job_step_ms", "reset"}. "empty" and "data_dir" build the world
 * anew; "job_step_ms" is how long a new job waits in the queue, and between
 * the lines it prints.
 */
import type { IncomingMessage, ServerResponse } from 'node:http'
import type {
  Changed,
  HostName,
  Job,
  JobKind,
  JobRequest,
  ServerEntry,
  ServerInput,
  SessionMode,
} from '../src/api/types.ts'
import {
  buildWorld,
  servedShopSession,
  shopSpec,
  splitReport,
  type World,
} from './fixtures/world.ts'
import { detailAt, diffFlows, type FlowRecord } from './fixtures/flows.ts'
import { comparison, counts, stageView, type StageRecord } from './fixtures/stage.ts'
import {
  discovered,
  doctorOutput,
  flowSummaries,
  hostConfig,
  overview,
  probe,
  serverView,
  settings,
  type MockOptions,
} from './derive.ts'
import texts from './fixtures/data/texts.json' with { type: 'json' }

export const MOCK_TOKEN = 'stretto-mock-token'

/** When the mock started: the world is built around it, so a reset gives the same sessions and ids. */
const ANCHOR = Date.now()

interface State {
  world: World
  options: MockOptions
  latency: number
  trash: string[]
  /** Set by a test: how long a new job queues, and between its lines. */
  jobStepMs?: number
}

function initialState(): State {
  const env = typeof process !== 'undefined' ? process.env : {}
  const world = buildWorld(ANCHOR, { empty: env.MOCK_EMPTY === '1' })
  const options: MockOptions = {
    auth: env.MOCK_AUTH === '1',
    readOnly: env.MOCK_READ_ONLY === '1',
    keySet: env.MOCK_KEY_SET === '1',
    redactSalt: env.MOCK_REDACT_SALT === '1',
  }
  fillDoctor(world, options)
  return { world, options, latency: Number(env.MOCK_LATENCY ?? 140), trash: [] }
}

function fillDoctor(world: World, options: MockOptions) {
  for (const job of world.jobs)
    if (job.kind === 'doctor' && !job.output) job.output = doctorOutput(world, options)
}

let state = initialState()

// ---------------------------------------------------------------- SSE

const clients = new Set<ServerResponse>()

function send(event: string, data: unknown) {
  const payload = `event: ${event}\ndata: ${JSON.stringify(data)}\n\n`
  for (const res of clients) res.write(payload)
}

function changed(what: Changed['what'], keys: string[] = []) {
  send('changed', { what, keys })
}

setInterval(() => {
  for (const res of clients) res.write(': ping\n\n')
}, 15_000).unref?.()

// ---------------------------------------------------------------- live traffic

let customer = 63
function traffic() {
  if (state.world.sessions.length === 0) return
  const now = Date.now()
  const kinds = ['cancel', 'status', 'cancel'] as const
  const kind = kinds[customer % kinds.length]!
  const spec = shopSpec(now - 30_000, 9000 + customer, 'served', 'logs/shop')
  const rec = servedShopSession(spec, customer++, kind, (a, b) => (a + b) / 2)
  const key = rec.spec.session
  state.world.sessions.unshift({
    key,
    summary: rec.summary(key),
    detail: rec.detail(key),
    raw: rec.raw,
    flowLog: rec.flowLog,
  })
  state.world.now = now
  changed('sessions', [key])
}
if (typeof process !== 'undefined' && process.env.MOCK_TRAFFIC !== '0') {
  setInterval(traffic, Number(process.env.MOCK_TRAFFIC_MS ?? 45_000)).unref?.()
}

// ---------------------------------------------------------------- helpers

class HttpError extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message)
  }
}

function json(res: ServerResponse, status: number, body: unknown) {
  res.statusCode = status
  res.setHeader('Content-Type', 'application/json')
  res.setHeader('Cache-Control', 'no-store')
  res.end(JSON.stringify(body))
}

async function readBody(req: IncomingMessage): Promise<unknown> {
  const chunks: Buffer[] = []
  for await (const chunk of req) chunks.push(chunk as Buffer)
  const text = Buffer.concat(chunks).toString('utf8')
  if (!text) return undefined
  try {
    return JSON.parse(text)
  } catch {
    throw new HttpError(400, 'the body is not JSON')
  }
}

function cookie(req: IncomingMessage, name: string): string | null {
  const header = req.headers.cookie ?? ''
  for (const part of header.split(';')) {
    const [k, ...v] = part.trim().split('=')
    if (k === name) return decodeURIComponent(v.join('='))
  }
  return null
}

function authorized(req: IncomingMessage): boolean {
  if (!state.options.auth) return true
  const bearer = /^Bearer (.+)$/.exec(req.headers.authorization ?? '')?.[1]
  return cookie(req, 'stretto_console') === MOCK_TOKEN || bearer === MOCK_TOKEN
}

function sessionRecord(key: string) {
  const s = state.world.sessions.find((x) => x.key === key)
  if (!s) throw new HttpError(404, `no session ${key}`)
  return s
}

function flowRecord(key: string): FlowRecord {
  const f = state.world.flows.find((x) => x.summary.key === key)
  if (!f) throw new HttpError(404, `no flow ${key}`)
  return f
}

/** The deployment the flow `key` belongs to: its committed flow's path and its staged flow's. */
function stagePaths(key: string): { committed: string; staged: string; name: string } {
  const f = flowRecord(key)
  const path = f.summary.path
  const staged = /^(.*)\.staged\.flow\.json$/.exec(path)
  const base = staged ? staged[1]! : path.replace(/\.flow\.json$/, '')
  return {
    committed: `${base}.flow.json`,
    staged: `${base}.staged.flow.json`,
    name: base.split('/').pop()!,
  }
}

/** The stage record of the deployment the flow `key` belongs to, if it has one. */
function stageOf(key: string): StageRecord | null {
  const { committed, staged } = stagePaths(key)
  const keyAt = (path: string) =>
    state.world.flows.find((f) => f.summary.path === path)?.summary.key
  return (
    state.world.stages.find(
      (s) =>
        s.committed === keyAt(committed) ||
        s.staged === keyAt(staged) ||
        s.committed === key ||
        s.staged === key,
    ) ?? null
  )
}

/** `GET /api/flows/:key/stage` */
function stageFor(key: string) {
  const paths = stagePaths(key)
  const byPath = (path: string) => state.world.flows.find((f) => f.summary.path === path) ?? null
  const committed = byPath(paths.committed)
  const staged = byPath(paths.staged)
  const summaries = flowSummaries(state.world)
  const diff =
    committed && staged
      ? { from: committed.summary.key, to: staged.summary.key, ...diffFlows(committed, staged) }
      : null
  return stageView(
    stageOf(key),
    committed?.summary.key ?? null,
    { ...paths, dataDir: state.world.dataDir, domain: flowRecord(key).summary.domain },
    staged ? (summaries.find((f) => f.key === staged.summary.key) ?? null) : null,
    diff,
  )
}

/** A flow as another file: the committed flow's key and path, the other flow's content. */
function asFlow(from: FlowRecord, key: string, path: string, name: string): FlowRecord {
  const now = Date.now()
  return {
    ...from,
    summary: { ...from.summary, key, path, name, modified_unix_ms: now },
  }
}

function note(text: unknown): string | null {
  return typeof text === 'string' && text.trim() ? text.trim() : null
}

/** `POST /api/flows/:key/commit`, as `stretto flow-commit` does it. */
function commitFlow(key: string, body: unknown) {
  const paths = stagePaths(key)
  const world = state.world
  const staged = world.flows.find((f) => f.summary.path === paths.staged)
  if (!staged)
    throw new HttpError(
      409,
      `there is no staged flow at ${world.dataDir}/${paths.staged}: stretto stage learns it`,
    )
  let record = stageOf(key)
  if (!record) {
    record = { committed: '', staged: staged.summary.key, pending: true, last: null, versions: [] }
    world.stages.push(record)
  }
  if (!record.pending)
    throw new HttpError(409, 'the staged flow is the committed one: nothing to commit')
  const now = Date.now()
  const before = world.flows.find((f) => f.summary.path === paths.committed)
  const version = (n: number, kind: 'found' | 'commit', flow: FlowRecord, extra = {}) => ({
    view: {
      version: n,
      kind,
      unix_ms: now,
      restored: null,
      note: null,
      changes: [] as string[],
      evidence: null,
      current: false,
      ...extra,
    },
    flow,
  })
  if (before && record.versions.length === 0) record.versions.push(version(1, 'found', before))
  const changes = before ? diffFlows(before, staged).changes : []
  const committed = asFlow(staged, before?.summary.key ?? paths.name, paths.committed, paths.name)
  record.versions.push(
    version(record.versions.length + 1, 'commit', committed, {
      note: note((body as { note?: unknown } | null)?.note),
      changes,
      evidence: record.last,
    }),
  )
  for (const v of record.versions) v.view.current = false
  record.versions[record.versions.length - 1]!.view.current = true
  record.committed = committed.summary.key
  record.pending = false
  world.flows = [committed, ...world.flows.filter((f) => f.summary.path !== paths.committed)]
  changed('flows', [committed.summary.key, staged.summary.key])
  return record.versions[record.versions.length - 1]!.view
}

/** `POST /api/flows/:key/rollback`, as `stretto flow-rollback` does it. */
function rollbackFlow(key: string, body: unknown) {
  const paths = stagePaths(key)
  const world = state.world
  const record = stageOf(key)
  const versions = record?.versions ?? []
  const latest = versions.length
  const asked = (body as { to?: unknown } | null)?.to
  const to = typeof asked === 'number' ? asked : latest - 1
  if (!record || latest < 1 || (typeof asked !== 'number' && latest < 2))
    throw new HttpError(
      409,
      `${world.dataDir}/${paths.committed} has no earlier version to roll back to`,
    )
  const target = versions.find((v) => v.view.version === to)
  if (!target)
    throw new HttpError(
      409,
      `there is no version ${to}: ${world.dataDir}/${paths.committed} has versions 1 to ${latest}`,
    )
  if (to === latest) throw new HttpError(409, `version ${to} is the committed flow already`)
  const current = world.flows.find((f) => f.summary.path === paths.committed)!
  const restored = asFlow(target.flow, current.summary.key, paths.committed, paths.name)
  versions.push({
    view: {
      version: latest + 1,
      kind: 'rollback',
      unix_ms: Date.now(),
      restored: to,
      note: note((body as { note?: unknown } | null)?.note),
      changes: diffFlows(current, target.flow).changes,
      evidence: null,
      current: true,
    },
    flow: restored,
  })
  for (const v of versions.slice(0, -1)) v.view.current = false
  record.pending = true
  world.flows = [restored, ...world.flows.filter((f) => f.summary.path !== paths.committed)]
  changed('flows', [restored.summary.key])
  return versions[versions.length - 1]!.view
}

function serverEntry(name: string): ServerEntry {
  const s = state.world.servers.find((x) => x.name === name)
  if (!s) throw new HttpError(404, `no server named ${name}`)
  return s
}

function threshold(value: string | null): number {
  if (value === null || value === '') return 0.3
  const t = Number(value)
  if (!Number.isFinite(t) || t < 0)
    throw new HttpError(400, `threshold ${value}: give a number from 0 to 1`)
  return t
}

/** A request's server, every optional field present. */
function entryOf(input: ServerInput): Omit<ServerEntry, 'created_unix_ms' | 'updated_unix_ms'> {
  return {
    name: input.name,
    description: input.description ?? null,
    upstream: input.upstream,
    mode: input.mode,
    flow: input.flow ?? null,
    record_dir: input.record_dir ?? null,
    decider: input.decider ?? null,
    threshold: input.threshold ?? null,
  }
}

function checkServer(input: unknown, name?: string): ServerInput {
  if (!input || typeof input !== 'object') throw new HttpError(400, 'the body must be a server')
  const s = input as ServerInput
  if (typeof s.name !== 'string' || !/^[a-z0-9_-]+$/.test(s.name) || s.name.startsWith('-')) {
    throw new HttpError(
      400,
      `name ${JSON.stringify(s.name)}: use lowercase letters, digits, - and _`,
    )
  }
  if (name && s.name !== name) throw new HttpError(400, 'a server cannot be renamed: add a new one')
  if (!s.upstream || (s.upstream.kind !== 'stdio' && s.upstream.kind !== 'http'))
    throw new HttpError(400, 'upstream: give a command or a URL')
  if (s.upstream.kind === 'stdio' && (!Array.isArray(s.upstream.command) || !s.upstream.command[0]))
    throw new HttpError(400, 'upstream.command: give the command that starts the server')
  if (s.upstream.kind === 'http') {
    try {
      const u = new URL(s.upstream.url)
      if (!/^https?:$/.test(u.protocol)) throw new Error()
    } catch {
      throw new HttpError(
        400,
        `upstream.url ${JSON.stringify(s.upstream.url)} is not an http(s) URL`,
      )
    }
  }
  if (!['record', 'shadow', 'serve'].includes(s.mode))
    throw new HttpError(400, `mode ${JSON.stringify(s.mode)}: record, shadow or serve`)
  if (s.mode !== 'record' && !s.flow)
    throw new HttpError(400, `mode ${s.mode} runs a flow: give its path`)
  if (
    s.threshold !== null &&
    (typeof s.threshold !== 'number' || s.threshold < 0 || s.threshold > 1)
  )
    throw new HttpError(400, 'threshold: a number from 0 to 1')
  return {
    name: s.name,
    description: s.description || null,
    upstream:
      s.upstream.kind === 'stdio'
        ? { kind: 'stdio', command: s.upstream.command, env: s.upstream.env ?? [] }
        : { kind: 'http', url: s.upstream.url, headers: s.upstream.headers ?? [] },
    mode: s.mode,
    flow: s.flow || null,
    record_dir: s.record_dir || null,
    decider: s.decider ?? null,
    threshold: s.threshold ?? null,
  }
}

function checkPath(path: unknown, what: string): string {
  if (typeof path !== 'string' || !path.trim())
    throw new HttpError(400, `${what}: give a directory`)
  if (path.split('/').includes('..'))
    throw new HttpError(400, `${what} ${path}: a path must stay inside the data dir (no ..)`)
  if (path.startsWith('/') && !path.startsWith(state.world.dataDir))
    throw new HttpError(400, `${what} ${path}: a path must be inside the data dir or start with ~/`)
  return path
    .replace(/^~\/\.stretto\//, '')
    .replace(new RegExp(`^${state.world.dataDir}/`), '')
    .replace(/\/$/, '')
}

// ---------------------------------------------------------------- jobs

let jobCounter = 7
/** The timers that move each unfinished job on, so that it can be cancelled. */
const jobTimers = new Map<
  string,
  { start?: ReturnType<typeof setTimeout>; tick?: ReturnType<typeof setInterval> }
>()

function jobTitle(job: JobRequest): string {
  switch (job.kind) {
    case 'learn':
      return `Learn ${job.domain} from ${job.sessions}`
    case 'promote':
      return `Promote ${flowRecord(job.flow).summary.name} on ${job.sessions}`
    case 'audit':
      return `Audit ${flowRecord(job.flow).summary.name} on ${job.sessions}`
    case 'stage':
      return `Stage ${stagePaths(job.flow).name} from ${job.sessions}`
    case 'redact':
      return `Redact ${job.sessions}`
    default:
      return 'Check the installation'
  }
}

/** The request with its defaults filled in, as the server keeps it in `params`. */
function withDefaults(req: JobRequest): JobRequest {
  switch (req.kind) {
    case 'learn':
      return {
        ...req,
        out: req.out ?? `${req.domain}.flow.json`,
        overwrite: req.overwrite ?? false,
        habit_only: req.habit_only ?? true,
        constants: req.constants ?? false,
      }
    case 'promote':
      return {
        ...req,
        oracle_cache: req.oracle_cache ?? 'oracle-cache',
        threshold: req.threshold ?? 0.3,
        min_used: req.min_used ?? 0.7,
        min_lower: req.min_lower ?? 0.5,
        min_tasks: req.min_tasks ?? 3,
        out: req.out ?? `${flowRecord(req.flow).summary.name}.promoted.flow.json`,
        overwrite: req.overwrite ?? false,
      }
    case 'audit':
      return { ...req, decider: req.decider ?? null }
    case 'stage':
      return {
        ...req,
        window: req.window ?? 50,
        decider: req.decider ?? null,
        half_life: req.half_life ?? null,
        constants: req.constants ?? false,
      }
    case 'redact':
      return { ...req, keep_shared: req.keep_shared ?? 3, hash_fields: req.hash_fields ?? [] }
    default:
      return req
  }
}

function startJob(input: unknown): Job {
  if (!input || typeof input !== 'object') throw new HttpError(400, 'the body must be a job')
  const kinds: JobKind[] = ['learn', 'promote', 'audit', 'stage', 'redact', 'doctor']
  if (!kinds.includes((input as JobRequest).kind))
    throw new HttpError(
      400,
      `the job is not as expected: kind ${JSON.stringify((input as { kind: unknown }).kind)}`,
    )
  const req = withDefaults(input as JobRequest)
  const id = `j-${String(jobCounter).padStart(4, '0')}`
  let lines: string[] = []
  let fail = false
  /** What it writes, as planned: after it ends, only what exists. */
  const artifacts: Job['artifacts'] = []
  const reports: Record<string, string> = {}
  let newFlow: FlowRecord | null = null
  /** For a stage job: what it learns, and the run it records. */
  let staged: { flow: FlowRecord; last: StageRecord['last'] } | null = null
  const world = state.world
  if (req.kind === 'learn') {
    if (!/^[a-z0-9_-]+$/.test(req.domain ?? ''))
      throw new HttpError(
        400,
        `domain ${JSON.stringify(req.domain)}: use lowercase letters, digits, - and _`,
      )
    const dir = checkPath(req.sessions, 'sessions')
    const out = req.out ? checkPath(req.out, 'out') : `${req.domain}.flow.json`
    if (world.flows.some((f) => f.summary.path === out) && !req.overwrite)
      throw new HttpError(409, `${out} exists: choose another path, or overwrite it`)
    const sessions = world.sessions.filter((s) => s.summary.path.startsWith(dir + '/'))
    if (!sessions.length) {
      fail = true
      lines = ['Error: no successful training episodes to learn a habit from (set rewards to 1)']
    } else {
      const tools = sessions[0]!.detail.tools.length
      lines = [
        `stretto: learned the ${req.domain} flow from ${sessions.length} sessions (${tools} tools) and wrote ${world.dataDir}/${out}`,
      ]
      const base = world.flows.find((f) => f.summary.key === 'shop')!
      const key = out.replace(/^.*\//, '').replace(/\.flow\.json$/, '')
      newFlow = {
        ...base,
        summary: {
          ...base.summary,
          key,
          path: out,
          name: req.domain,
          domain: req.domain,
          habit_episodes: sessions.length,
          compiled_unix_ms: Date.now(),
          modified_unix_ms: Date.now(),
          served_by: [],
        },
      }
      artifacts.push({ kind: 'flow', path: out, key })
    }
  } else if (req.kind === 'promote' || req.kind === 'audit') {
    flowRecord(req.flow)
    const dir = checkPath(req.sessions, 'sessions')
    if (req.kind === 'promote') {
      const out = checkPath(req.out!, 'out')
      if (world.flows.some((f) => f.summary.path === out) && !req.overwrite)
        throw new HttpError(409, `${out} exists: choose another path, or overwrite it`)
      const { line, report } = splitReport(texts.promote_output)
      lines = [line.trimEnd()]
      const key = out.replace(/^.*\//, '').replace(/\.flow\.json$/, '')
      const base = world.flows.find((f) => f.summary.key === 'shop-promoted')!
      newFlow = {
        ...base,
        summary: {
          ...base.summary,
          key,
          path: out,
          compiled_unix_ms: Date.now(),
          modified_unix_ms: Date.now(),
          served_by: [],
        },
      }
      artifacts.push({ kind: 'flow', path: out, key })
      artifacts.push({ kind: 'report', path: `console/jobs/${id}.promote.md`, key: null })
      reports[`console/jobs/${id}.promote.md`] = report
    } else {
      const { line, report } = splitReport(texts.audit_output)
      lines = [line.trimEnd()]
      artifacts.push({ kind: 'report', path: `console/jobs/${id}.audit.json`, key: null })
      artifacts.push({ kind: 'report', path: `console/jobs/${id}.audit.md`, key: null })
      reports[`console/jobs/${id}.audit.json`] =
        `${JSON.stringify({ flow: req.flow, episodes: 8, decisions: 32, agreement: 1 }, null, 2)}\n`
      reports[`console/jobs/${id}.audit.md`] = report
    }
    void dir
  } else if (req.kind === 'stage') {
    const paths = stagePaths(req.flow)
    const dir = checkPath(req.sessions, 'sessions')
    if (!req.window || req.window < 1) throw new HttpError(400, 'window: at least 1 session')
    if (req.half_life !== null && req.half_life !== undefined && !(req.half_life > 0))
      throw new HttpError(400, `half_life ${req.half_life}: a number of sessions, more than 0`)
    const sessions = world.sessions.filter((s) => s.summary.path.startsWith(dir + '/'))
    const record = stageOf(req.flow)
    const before = record?.last?.sessions ?? 0
    const fresh = Math.max(0, sessions.length - before)
    lines = [
      `stretto: staged ${world.dataDir}/${paths.staged} from ${sessions.length} sessions, ${fresh} of them new`,
    ]
    const base = world.flows.find((f) => f.summary.path === paths.staged) ?? flowRecord(req.flow)
    const now = Date.now()
    const key = paths.staged.replace(/^.*\//, '').replace(/\.flow\.json$/, '')
    const flow: FlowRecord = {
      ...base,
      summary: {
        ...base.summary,
        key,
        path: paths.staged,
        name: key,
        habit_episodes: sessions.length,
        compiled_unix_ms: now,
        modified_unix_ms: now,
        served_by: [],
      },
    }
    const last = record?.last
      ? { ...record.last, learned_unix_ms: now, sessions: sessions.length, new: fresh }
      : comparison(now, sessions.length, fresh, 0, [])
    if (!record?.last)
      last.total = { site: '', committed: counts(0, 0, 0), staged: counts(0, 0, 0) }
    staged = { flow, last }
    artifacts.push({ kind: 'flow', path: paths.staged, key })
    artifacts.push({ kind: 'report', path: `console/jobs/${id}.stage.md`, key: null })
    artifacts.push({ kind: 'report', path: `console/jobs/${id}.stage.json`, key: null })
    reports[`console/jobs/${id}.stage.md`] = `# Staged flow: ${world.dataDir}/${paths.committed}\n`
    reports[`console/jobs/${id}.stage.json`] = `${JSON.stringify(last, null, 2)}\n`
  } else if (req.kind === 'redact') {
    if (!state.options.redactSalt)
      throw new HttpError(
        400,
        'set STRETTO_REDACT_SALT in the console’s environment to redact: without a salt, anyone could hash guesses and match them',
      )
    const dir = checkPath(req.sessions, 'sessions')
    const out = checkPath(req.out, 'out')
    const n = world.sessions.filter((s) => s.summary.path.startsWith(dir + '/')).length
    lines = [
      `stretto: redacted ${n} sessions into ${world.dataDir}/${out}; values fewer than ${req.keep_shared} of them share are hashed`,
    ]
    artifacts.push({ kind: 'dir', path: out, key: null })
  } else {
    lines = doctorOutput(world, state.options).trimEnd().split('\n')
  }

  jobCounter++
  const job: Job = {
    id,
    kind: req.kind,
    title: jobTitle(req),
    params: req,
    status: 'queued',
    created_unix_ms: Date.now(),
    started_unix_ms: null,
    finished_unix_ms: null,
    exit_code: null,
    output: '',
    artifacts: [...artifacts],
  }
  world.jobs.unshift(job)
  const step = state.jobStepMs ?? Number(process.env.MOCK_JOB_STEP_MS ?? 280)
  const timers: { start?: ReturnType<typeof setTimeout>; tick?: ReturnType<typeof setInterval> } =
    {}
  jobTimers.set(id, timers)
  timers.start = setTimeout(() => {
    job.status = 'running'
    job.started_unix_ms = Date.now()
    job.output = `$ stretto ${commandLine(req)}\n`
    send('job', job)
    changed('jobs', [id])
    let i = 0
    const tick = setInterval(() => {
      if (i < lines.length) {
        job.output += lines[i++] + '\n'
        send('job', job)
        return
      }
      clearInterval(tick)
      jobTimers.delete(id)
      job.status = fail ? 'failed' : 'succeeded'
      job.exit_code = fail ? 1 : 0
      job.finished_unix_ms = Date.now()
      if (fail) job.artifacts = []
      else Object.assign(world.reports, reports)
      if (newFlow && !fail) {
        world.flows = world.flows.filter((f) => f.summary.path !== newFlow!.summary.path)
        world.flows.unshift(newFlow)
        changed('flows', [newFlow.summary.key])
      }
      if (staged && req.kind === 'stage') {
        const { flow, last } = staged
        world.flows = [flow, ...world.flows.filter((f) => f.summary.path !== flow.summary.path)]
        let record = stageOf(req.flow)
        if (!record) {
          const committed = world.flows.find(
            (f) => f.summary.path === stagePaths(req.flow).committed,
          )
          record = {
            committed: committed?.summary.key ?? '',
            staged: flow.summary.key,
            pending: true,
            last: null,
            versions: [],
          }
          world.stages.push(record)
        }
        record.last = last
        record.pending = true
        changed('flows', [flow.summary.key, record.committed].filter(Boolean))
      }
      send('job', job)
      changed('jobs', [id])
    }, step)
    timers.tick = tick
  }, step)
  return job
}

/**
 * `POST /api/jobs/:id/cancel`, as the console does it: a queued job is
 * cancelled at once; a running one's `stretto` is killed, and it ends a
 * moment later, with the exit code of a SIGKILL.
 */
function cancelJob(id: string): Job {
  const job = state.world.jobs.find((j) => j.id === id)
  if (!job) throw new HttpError(404, `no job ${JSON.stringify(id)}`)
  if (job.status !== 'queued' && job.status !== 'running')
    throw new HttpError(409, `job ${id} has ended (${job.status}): there is nothing to cancel`)
  const timers = jobTimers.get(id)
  clearTimeout(timers?.start)
  clearInterval(timers?.tick)
  jobTimers.delete(id)
  const end = (note: string, code: number | null) => {
    job.output += note
    job.status = 'cancelled'
    job.exit_code = code
    job.finished_unix_ms = Date.now()
    job.artifacts = []
    send('job', job)
    changed('jobs', [id])
  }
  if (job.status === 'queued') {
    end('stretto-console: cancelled before it started\n', null)
    return job
  }
  const now = { ...job }
  setTimeout(() => end('stretto-console: cancelled\n', 137), 300)
  return now
}

function commandLine(req: JobRequest): string {
  const d = state.world.dataDir
  switch (req.kind) {
    case 'learn':
      return `learn --sessions ${d}/${req.sessions} --domain ${req.domain}${req.habit_only ? ' --habit-only' : ''}${req.constants ? ' --constants' : ''} --out ${d}/${req.out ?? `${req.domain}.flow.json`}`
    case 'promote':
      return `promote --flow ${d}/${flowRecord(req.flow).summary.path} --sessions ${d}/${req.sessions} --threshold ${req.threshold} --min-used ${req.min_used} --min-lower ${req.min_lower} --min-tasks ${req.min_tasks} --out ${d}/${req.out ?? `${flowRecord(req.flow).summary.name}.promoted.flow.json`}`
    case 'audit':
      return `audit --flow ${d}/${flowRecord(req.flow).summary.path} --sessions ${d}/${req.sessions}${req.decider ? ` --decider ${req.decider}` : ''} --json ${d}/console/jobs/report.json`
    case 'stage':
      return `stage --flow ${d}/${stagePaths(req.flow).committed} --sessions ${d}/${req.sessions} --window ${req.window}${req.decider ? ` --decider ${req.decider}` : ''}${req.half_life ? ` --half-life ${req.half_life}` : ''}${req.constants ? ' --constants' : ''}`
    case 'redact':
      return `redact --sessions ${d}/${req.sessions} --out ${d}/${req.out} --keep-shared ${req.keep_shared}${req.hash_fields?.length ? ` --hash-field ${req.hash_fields.join(',')}` : ''}`
    default:
      return 'doctor'
  }
}

// ---------------------------------------------------------------- routing

type Handler = (ctx: {
  req: IncomingMessage
  res: ServerResponse
  url: URL
  params: string[]
  body: unknown
}) => unknown

const routes: [string, RegExp, Handler][] = [
  ['GET', /^\/api\/health$/, () => ({ ok: true, version: '0.1.0' })],
  [
    'GET',
    /^\/api\/meta$/,
    () => ({
      version: '0.1.0',
      data_dir: state.world.dataDir,
      read_only: state.options.readOnly,
      auth: state.options.auth,
      key_set: state.options.keySet,
      stretto: { path: '/home/me/.cargo/bin/stretto', version: 'stretto 0.1.0' },
      proxy: { path: '/home/me/.cargo/bin/stretto-proxy', version: 'stretto-proxy 0.1.0' },
    }),
  ],
  ['GET', /^\/api\/overview$/, () => overview(state.world, state.options, state.world.jobs)],
  [
    'GET',
    /^\/api\/sessions$/,
    ({ url }) => {
      const domain = url.searchParams.get('domain')
      const mode = url.searchParams.get('mode') as SessionMode | null
      const q = (url.searchParams.get('q') ?? '').trim().toLowerCase()
      const limit = Math.min(200, Math.max(1, Number(url.searchParams.get('limit') ?? 50)))
      const offset = Math.max(0, Number(url.searchParams.get('offset') ?? 0))
      const items = state.world.sessions
        .filter((s) => !domain || s.summary.domain === domain)
        .filter((s) => !mode || s.summary.mode === mode)
        .filter((s) => {
          if (!q) return true
          const hay = [
            s.summary.session_id,
            s.summary.domain,
            s.summary.agent,
            ...s.detail.tools.map((t) => t.name),
          ]
            .join(' ')
            .toLowerCase()
          return hay.includes(q)
        })
        .map((s) => s.summary)
      return { total: items.length, items: items.slice(offset, offset + limit) }
    },
  ],
  ['GET', /^\/api\/sessions\/([^/]+)$/, ({ params }) => sessionRecord(params[0]!).detail],
  [
    'GET',
    /^\/api\/sessions\/([^/]+)\/raw$/,
    ({ res, params }) => {
      const s = sessionRecord(params[0]!)
      res.statusCode = 200
      res.setHeader('Content-Type', 'application/x-ndjson')
      res.setHeader('Content-Disposition', `attachment; filename="${s.summary.session_id}.jsonl"`)
      res.end(s.raw)
      return undefined
    },
  ],
  [
    'DELETE',
    /^\/api\/sessions\/([^/]+)$/,
    ({ params }) => {
      const s = sessionRecord(params[0]!)
      state.world.sessions = state.world.sessions.filter((x) => x !== s)
      state.trash.push(s.summary.path)
      changed('sessions', [s.key])
      return { ok: true }
    },
  ],
  ['GET', /^\/api\/flows$/, () => ({ items: flowSummaries(state.world) })],
  [
    'GET',
    /^\/api\/flows\/diff$/,
    ({ url }) => {
      const from = flowRecord(url.searchParams.get('from') ?? '')
      const to = flowRecord(url.searchParams.get('to') ?? '')
      if (from.summary.error || to.summary.error)
        throw new HttpError(
          400,
          `${(from.summary.error ? from : to).summary.path} does not load, so it cannot be compared`,
        )
      threshold(url.searchParams.get('threshold'))
      return { from: from.summary.key, to: to.summary.key, ...diffFlows(from, to) }
    },
  ],
  [
    'GET',
    /^\/api\/flows\/([^/]+)$/,
    ({ url, params }) => {
      const f = flowRecord(params[0]!)
      if (f.summary.error) throw new HttpError(400, f.summary.error)
      const detail = detailAt(f, threshold(url.searchParams.get('threshold')))
      detail.summary = flowSummaries(state.world).find((s) => s.key === f.summary.key) ?? f.summary
      return detail
    },
  ],
  ['GET', /^\/api\/flows\/([^/]+)\/stage$/, ({ params }) => stageFor(params[0]!)],
  ['POST', /^\/api\/flows\/([^/]+)\/commit$/, ({ params, body }) => commitFlow(params[0]!, body)],
  [
    'POST',
    /^\/api\/flows\/([^/]+)\/rollback$/,
    ({ params, body }) => rollbackFlow(params[0]!, body),
  ],
  [
    'GET',
    /^\/api\/flows\/([^/]+)\/raw$/,
    ({ res, params }) => {
      const f = flowRecord(params[0]!)
      res.statusCode = 200
      res.setHeader('Content-Type', 'application/json')
      res.setHeader(
        'Content-Disposition',
        `attachment; filename="${f.summary.path.split('/').pop()}"`,
      )
      res.end(f.raw)
      return undefined
    },
  ],
  [
    'DELETE',
    /^\/api\/flows\/([^/]+)$/,
    ({ params }) => {
      const f = flowRecord(params[0]!)
      state.world.flows = state.world.flows.filter((x) => x !== f)
      state.trash.push(f.summary.path)
      changed('flows', [f.summary.key])
      return { ok: true }
    },
  ],
  [
    'GET',
    /^\/api\/servers$/,
    () => ({
      items: state.world.servers.map((s) => serverView(state.world, s)),
      discovered: discovered(state.world),
    }),
  ],
  [
    'POST',
    /^\/api\/servers$/,
    ({ body }) => {
      const input = checkServer(body)
      if (state.world.servers.some((s) => s.name === input.name))
        throw new HttpError(409, `a server named ${input.name} exists`)
      const now = Date.now()
      const entry: ServerEntry = { ...entryOf(input), created_unix_ms: now, updated_unix_ms: now }
      state.world.servers.push(entry)
      state.world.servers.sort((a, b) => a.name.localeCompare(b.name))
      changed('servers', [entry.name])
      return serverView(state.world, entry)
    },
  ],
  [
    'PUT',
    /^\/api\/servers\/([^/]+)$/,
    ({ params, body }) => {
      const current = serverEntry(params[0]!)
      const input = checkServer(body, current.name)
      const entry: ServerEntry = {
        ...entryOf(input),
        created_unix_ms: current.created_unix_ms,
        updated_unix_ms: Date.now(),
      }
      state.world.servers = state.world.servers.map((s) => (s === current ? entry : s))
      changed('servers', [entry.name])
      return serverView(state.world, entry)
    },
  ],
  [
    'DELETE',
    /^\/api\/servers\/([^/]+)$/,
    ({ params }) => {
      const current = serverEntry(params[0]!)
      state.world.servers = state.world.servers.filter((s) => s !== current)
      changed('servers', [current.name])
      return { ok: true }
    },
  ],
  [
    'GET',
    /^\/api\/servers\/([^/]+)\/config$/,
    ({ url, params }) => {
      const host = (url.searchParams.get('host') ?? 'claude-code') as HostName
      if (!['claude-code', 'claude-desktop', 'cursor', 'vscode'].includes(host))
        throw new HttpError(400, `host ${host}: claude-code, claude-desktop, cursor or vscode`)
      return hostConfig(state.world, serverEntry(params[0]!), host)
    },
  ],
  [
    'POST',
    /^\/api\/servers\/([^/]+)\/probe$/,
    ({ params }) => probe(state.world, serverEntry(params[0]!)),
  ],
  [
    'GET',
    /^\/api\/jobs$/,
    () => ({ items: state.world.jobs.map((j) => ({ ...j, output: j.output.slice(-65536) })) }),
  ],
  [
    'GET',
    /^\/api\/jobs\/([^/]+)$/,
    ({ params }) => {
      const job = state.world.jobs.find((j) => j.id === params[0])
      if (!job) throw new HttpError(404, `no job ${params[0]}`)
      return job
    },
  ],
  [
    'POST',
    /^\/api\/jobs$/,
    ({ res, body }) => {
      const job = startJob(body)
      changed('jobs', [job.id])
      res.statusCode = 202
      return job
    },
  ],
  ['POST', /^\/api\/jobs\/([^/]+)\/cancel$/, ({ params }) => cancelJob(params[0]!)],
  [
    'GET',
    /^\/api\/jobs\/([^/]+)\/artifacts\/(\d+)$/,
    ({ res, params }) => {
      const job = state.world.jobs.find((j) => j.id === params[0])
      if (!job) throw new HttpError(404, `no job ${params[0]}`)
      const artifact = job.artifacts[Number(params[1])]
      if (!artifact) throw new HttpError(404, `job ${job.id} has no artifact ${params[1]}`)
      if (artifact.kind === 'dir') throw new HttpError(400, `${artifact.path} is a directory`)
      const text =
        artifact.kind === 'flow'
          ? (state.world.flows.find((f) => f.summary.path === artifact.path)?.raw ?? null)
          : (state.world.reports[artifact.path] ?? null)
      if (text === null) throw new HttpError(404, `${artifact.path}: not found`)
      res.statusCode = 200
      res.setHeader(
        'Content-Type',
        artifact.path.endsWith('.json')
          ? 'application/json'
          : artifact.path.endsWith('.md')
            ? 'text/markdown; charset=utf-8'
            : 'text/plain; charset=utf-8',
      )
      res.end(text)
      return undefined
    },
  ],
  [
    'POST',
    /^\/api\/logout$/,
    ({ res }) => {
      res.setHeader('Set-Cookie', 'stretto_console=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0')
      return { ok: true }
    },
  ],
  ['GET', /^\/api\/settings$/, () => settings(state.world, state.options)],
]

function events(req: IncomingMessage, res: ServerResponse) {
  res.writeHead(200, {
    'Content-Type': 'text/event-stream',
    'Cache-Control': 'no-cache',
    Connection: 'keep-alive',
  })
  res.write(': connected\n\n')
  clients.add(res)
  req.on('close', () => clients.delete(res))
}

function mockControl(req: IncomingMessage, res: ServerResponse, body: unknown) {
  if (req.method === 'POST' && body && typeof body === 'object') {
    const b = body as Record<string, unknown>
    if (b.reset) state = initialState()
    if (typeof b.empty === 'boolean' || typeof b.data_dir === 'string') {
      state.world = buildWorld(ANCHOR, {
        empty: b.empty === true,
        dataDir: typeof b.data_dir === 'string' ? b.data_dir : undefined,
      })
      fillDoctor(state.world, state.options)
    }
    if (typeof b.auth === 'boolean') state.options.auth = b.auth
    if (typeof b.read_only === 'boolean') state.options.readOnly = b.read_only
    if (typeof b.key_set === 'boolean') state.options.keySet = b.key_set
    if (typeof b.redact_salt === 'boolean') state.options.redactSalt = b.redact_salt
    if (typeof b.latency === 'number') state.latency = b.latency
    if (typeof b.job_step_ms === 'number') state.jobStepMs = b.job_step_ms
    for (const what of ['sessions', 'flows', 'servers', 'jobs'] as const) changed(what)
  }
  json(res, 200, {
    ...state.options,
    latency: state.latency,
    sessions: state.world.sessions.length,
    token: MOCK_TOKEN,
  })
}

/** The middleware: /api/*, /__mock/*, and `?token=` on any page. Anything else goes on to Vite. */
export async function handle(
  req: IncomingMessage,
  res: ServerResponse,
  next: () => void,
): Promise<void> {
  const url = new URL(req.url ?? '/', 'http://localhost')
  const path = url.pathname
  if (!path.startsWith('/api/') && !path.startsWith('/__mock/')) {
    if (
      req.method === 'GET' &&
      url.searchParams.has('token') &&
      !path.startsWith('/@') &&
      !path.includes('.')
    ) {
      const token = url.searchParams.get('token') ?? ''
      url.searchParams.delete('token')
      res.statusCode = 302
      res.setHeader(
        'Set-Cookie',
        `stretto_console=${encodeURIComponent(token)}; HttpOnly; SameSite=Strict; Path=/`,
      )
      res.setHeader('Location', `${url.pathname}${url.search}${url.hash}`)
      res.end()
      return
    }
    next()
    return
  }
  try {
    const body = req.method === 'POST' || req.method === 'PUT' ? await readBody(req) : undefined
    if (path === '/__mock/state') {
      mockControl(req, res, body)
      return
    }
    if (path !== '/api/health' && !authorized(req))
      throw new HttpError(401, 'sign in: open the link stretto-console printed, or paste its token')
    if (req.method !== 'GET') {
      if (req.headers['x-stretto-console'] !== '1')
        throw new HttpError(403, 'a write needs the X-Stretto-Console: 1 header')
      if (state.options.readOnly && path !== '/api/logout')
        throw new HttpError(
          403,
          'the console is read-only (--read-only): nothing can be changed from here',
        )
    }
    if (path === '/api/events' && req.method === 'GET') {
      events(req, res)
      return
    }
    if (state.latency > 0)
      await new Promise((r) => setTimeout(r, state.latency * (0.6 + Math.random() * 0.8)))
    for (const [method, pattern, handler] of routes) {
      if (method !== req.method) continue
      const m = pattern.exec(path)
      if (!m) continue
      const params = m.slice(1).map((p) => decodeURIComponent(p))
      const result = handler({ req, res, url, params, body })
      if (result !== undefined)
        json(res, res.statusCode && res.statusCode !== 200 ? res.statusCode : 200, result)
      return
    }
    const known = routes.some(([, pattern]) => pattern.test(path))
    throw new HttpError(
      known ? 405 : 404,
      known ? `${req.method} is not allowed on ${path}` : `no endpoint ${path}`,
    )
  } catch (e) {
    if (e instanceof HttpError) json(res, e.status, { error: e.message })
    else json(res, 500, { error: e instanceof Error ? e.message : String(e) })
  }
}

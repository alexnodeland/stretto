/**
 * The console's API client: fetch with the session cookie, the
 * `X-Stretto-Console: 1` header on every write (the server refuses a write
 * without it, so a cross-site form cannot make one), typed results, a 401
 * handed to the sign-in screen, and other errors handed to the toasts.
 */
import type {
  FlowDetail,
  FlowDiffView,
  FlowList,
  Health,
  HostConfig,
  HostName,
  Job,
  JobList,
  Meta,
  JobRequest,
  Ok,
  Overview,
  ProbeResult,
  ServerInput,
  ServerList,
  ServerView,
  SessionDetail,
  SessionList,
  SessionQuery,
  Settings,
} from './types'

export type Method = 'GET' | 'POST' | 'PUT' | 'DELETE'

/** An error the API returned (`{"error": …}`), or 0 when the server did not answer. */
export class ApiError extends Error {
  readonly status: number

  constructor(status: number, message: string) {
    super(message)
    this.name = 'ApiError'
    this.status = status
  }
}

export interface RequestOptions {
  signal?: AbortSignal
  /** Do not toast the error: the caller shows it (a page's error state). */
  quiet?: boolean
  /** How to read the body: JSON (the default for JSON responses) or text. */
  as?: 'json' | 'text'
}

export interface ClientHooks {
  /** A 401: the token is missing or wrong. */
  onUnauthorized?: () => void
  /** Any other error, unless the request was quiet. */
  onError?: (error: ApiError, request: { method: Method; path: string }) => void
}

let hooks: ClientHooks = {}

/** Where 401s and errors go: the app wires these to the sign-in screen and the toasts. */
export function setClientHooks(next: ClientHooks): void {
  hooks = next
}

/** The header every POST, PUT and DELETE carries. */
export const WRITE_HEADER = 'X-Stretto-Console'

const JSON_TYPE = /^application\/([\w.-]+\+)?json\b/i

async function errorMessage(response: Response): Promise<string> {
  let text = ''
  try {
    text = await response.text()
  } catch {
    // The body is gone; fall back to the status.
  }
  if (text) {
    try {
      const body = JSON.parse(text) as unknown
      if (
        body &&
        typeof body === 'object' &&
        typeof (body as { error?: unknown }).error === 'string'
      ) {
        return (body as { error: string }).error
      }
    } catch {
      // Not JSON: a proxy's page or plain text.
    }
    const plain = text.trim()
    if (plain && plain.length <= 300 && !plain.startsWith('<')) return plain
  }
  return `${response.status} ${response.statusText || 'error'}`.trim()
}

function isAbort(error: unknown): boolean {
  return error instanceof DOMException && error.name === 'AbortError'
}

/** One request to the console's API. */
export async function request<T>(
  method: Method,
  path: string,
  body?: unknown,
  options: RequestOptions = {},
): Promise<T> {
  const headers: Record<string, string> = { Accept: 'application/json' }
  if (method !== 'GET') headers[WRITE_HEADER] = '1'
  if (body !== undefined) headers['Content-Type'] = 'application/json'
  let response: Response
  try {
    response = await fetch(path, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
      credentials: 'include',
      signal: options.signal,
    })
  } catch (e) {
    if (isAbort(e)) throw e
    const error = new ApiError(0, 'The console server did not answer. Is stretto-console running?')
    if (!options.quiet) hooks.onError?.(error, { method, path })
    throw error
  }
  if (!response.ok) {
    const error = new ApiError(response.status, await errorMessage(response))
    if (response.status === 401) hooks.onUnauthorized?.()
    else if (!options.quiet) hooks.onError?.(error, { method, path })
    throw error
  }
  if (response.status === 204) return undefined as T
  const type = response.headers.get('content-type') ?? ''
  const as = options.as ?? (JSON_TYPE.test(type) ? 'json' : 'text')
  if (as === 'json') return (await response.json()) as T
  return (await response.text()) as T
}

/** `?a=1&b=x`, leaving out empty values. */
export function query(
  params: Record<string, string | number | boolean | null | undefined>,
): string {
  const search = new URLSearchParams()
  for (const [key, value] of Object.entries(params)) {
    if (value === undefined || value === null || value === '') continue
    search.set(key, String(value))
  }
  const text = search.toString()
  return text ? `?${text}` : ''
}

const seg = encodeURIComponent

/** Every endpoint of the spec, typed. */
export const api = {
  health: (o?: RequestOptions) => request<Health>('GET', '/api/health', undefined, o),
  meta: (o?: RequestOptions) => request<Meta>('GET', '/api/meta', undefined, o),
  overview: (o?: RequestOptions) => request<Overview>('GET', '/api/overview', undefined, o),

  sessions: (q: SessionQuery = {}, o?: RequestOptions) =>
    request<SessionList>(
      'GET',
      `/api/sessions${query({ domain: q.domain, mode: q.mode, q: q.q, limit: q.limit, offset: q.offset })}`,
      undefined,
      o,
    ),
  session: (key: string, o?: RequestOptions) =>
    request<SessionDetail>('GET', `/api/sessions/${seg(key)}`, undefined, o),
  sessionRawUrl: (key: string) => `/api/sessions/${seg(key)}/raw`,
  sessionRaw: (key: string, o?: RequestOptions) =>
    request<string>('GET', `/api/sessions/${seg(key)}/raw`, undefined, { ...o, as: 'text' }),
  deleteSession: (key: string, o?: RequestOptions) =>
    request<Ok>('DELETE', `/api/sessions/${seg(key)}`, undefined, o),

  flows: (o?: RequestOptions) => request<FlowList>('GET', '/api/flows', undefined, o),
  flow: (key: string, threshold?: number | null, o?: RequestOptions) =>
    request<FlowDetail>('GET', `/api/flows/${seg(key)}${query({ threshold })}`, undefined, o),
  flowRawUrl: (key: string) => `/api/flows/${seg(key)}/raw`,
  flowRaw: (key: string, o?: RequestOptions) =>
    request<string>('GET', `/api/flows/${seg(key)}/raw`, undefined, { ...o, as: 'text' }),
  flowDiff: (from: string, to: string, threshold?: number | null, o?: RequestOptions) =>
    request<FlowDiffView>('GET', `/api/flows/diff${query({ from, to, threshold })}`, undefined, o),
  deleteFlow: (key: string, o?: RequestOptions) =>
    request<Ok>('DELETE', `/api/flows/${seg(key)}`, undefined, o),

  servers: (o?: RequestOptions) => request<ServerList>('GET', '/api/servers', undefined, o),
  createServer: (input: ServerInput, o?: RequestOptions) =>
    request<ServerView>('POST', '/api/servers', input, o),
  updateServer: (name: string, input: ServerInput, o?: RequestOptions) =>
    request<ServerView>('PUT', `/api/servers/${seg(name)}`, input, o),
  deleteServer: (name: string, o?: RequestOptions) =>
    request<Ok>('DELETE', `/api/servers/${seg(name)}`, undefined, o),
  serverConfig: (name: string, host: HostName, o?: RequestOptions) =>
    request<HostConfig>('GET', `/api/servers/${seg(name)}/config${query({ host })}`, undefined, o),
  probeServer: (name: string, o?: RequestOptions) =>
    request<ProbeResult>('POST', `/api/servers/${seg(name)}/probe`, undefined, o),

  jobs: (o?: RequestOptions) => request<JobList>('GET', '/api/jobs', undefined, o),
  job: (id: string, o?: RequestOptions) =>
    request<Job>('GET', `/api/jobs/${seg(id)}`, undefined, o),
  createJob: (job: JobRequest, o?: RequestOptions) => request<Job>('POST', '/api/jobs', job, o),
  /** Cancel a queued or running job: 409 once it has ended. */
  cancelJob: (id: string, o?: RequestOptions) =>
    request<Job>('POST', `/api/jobs/${seg(id)}/cancel`, undefined, o),
  /** A report or flow a job wrote, by its index in `artifacts`. */
  jobArtifactUrl: (id: string, index: number) => `/api/jobs/${seg(id)}/artifacts/${index}`,
  jobArtifact: (id: string, index: number, o?: RequestOptions) =>
    request<string>('GET', `/api/jobs/${seg(id)}/artifacts/${index}`, undefined, {
      ...o,
      as: 'text',
    }),

  /** Clears the token's cookie. */
  logout: (o?: RequestOptions) => request<Ok>('POST', '/api/logout', undefined, o),

  settings: (o?: RequestOptions) => request<Settings>('GET', '/api/settings', undefined, o),
}

export type Api = typeof api

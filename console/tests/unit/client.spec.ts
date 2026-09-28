import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { api, ApiError, query, request, setClientHooks, WRITE_HEADER } from '@/api/client'

type Call = { url: string; init: RequestInit }

function respond(status: number, body: unknown, type = 'application/json') {
  const text = typeof body === 'string' ? body : JSON.stringify(body)
  return new Response(status === 204 ? null : text, { status, headers: { 'Content-Type': type } })
}

let calls: Call[] = []
let next: () => Response | Promise<Response>
const onError = vi.fn()
const onUnauthorized = vi.fn()

beforeEach(() => {
  calls = []
  next = () => respond(200, { ok: true })
  vi.stubGlobal(
    'fetch',
    vi.fn(async (url: string, init: RequestInit) => {
      calls.push({ url, init })
      return next()
    }),
  )
  onError.mockReset()
  onUnauthorized.mockReset()
  setClientHooks({ onError, onUnauthorized })
})

afterEach(() => {
  vi.unstubAllGlobals()
  setClientHooks({})
})

function headers(i = 0): Record<string, string> {
  return calls[i]!.init.headers as Record<string, string>
}

describe('the API client', () => {
  it('sends reads with the cookie and without the write header', async () => {
    next = () => respond(200, { ok: true, version: '0.1.0' })
    const health = await api.health()
    expect(health).toEqual({ ok: true, version: '0.1.0' })
    expect(calls[0]!.url).toBe('/api/health')
    expect(calls[0]!.init.method).toBe('GET')
    expect(calls[0]!.init.credentials).toBe('include')
    expect(headers()[WRITE_HEADER]).toBeUndefined()
    expect(calls[0]!.init.body).toBeUndefined()
  })

  it('marks every write with X-Stretto-Console: 1 and sends JSON', async () => {
    await api.createJob({ kind: 'doctor' })
    await api.deleteSession('20260928T014620.569Z-654')
    await api.updateServer('shop', {
      name: 'shop',
      description: null,
      upstream: { kind: 'stdio', command: ['stretto-mcp-demo', '--world', 'retail'], env: [] },
      mode: 'record',
      flow: null,
      record_dir: null,
      decider: null,
      threshold: null,
    })
    await api.probeServer('shop')
    await api.cancelJob('20260928T044952.337Z-4458')
    expect(calls.map((c) => c.init.method)).toEqual(['POST', 'DELETE', 'PUT', 'POST', 'POST'])
    for (let i = 0; i < calls.length; i++) expect(headers(i)[WRITE_HEADER]).toBe('1')
    expect(headers(0)['Content-Type']).toBe('application/json')
    expect(JSON.parse(String(calls[0]!.init.body))).toEqual({ kind: 'doctor' })
    expect(calls[1]!.url).toBe('/api/sessions/20260928T014620.569Z-654')
    expect(calls[3]!.url).toBe('/api/servers/shop/probe')
    expect(calls[4]!.url).toBe('/api/jobs/20260928T044952.337Z-4458/cancel')
    expect(calls[4]!.init.body).toBeUndefined()
  })

  it('builds queries without empty values and encodes path segments', async () => {
    expect(query({ a: 1, b: '', c: null, d: undefined, e: 'x y' })).toBe('?a=1&e=x+y')
    expect(query({})).toBe('')
    await api.sessions({ domain: 'shop', mode: null, q: '', limit: 50, offset: 0 })
    expect(calls[0]!.url).toBe('/api/sessions?domain=shop&limit=50&offset=0')
    await api.flow('shop~1a2b3c4d', 0.45)
    expect(calls[1]!.url).toBe('/api/flows/shop~1a2b3c4d?threshold=0.45')
    await api.flowDiff('shop', 'shop-promoted', 0.3)
    expect(calls[2]!.url).toBe('/api/flows/diff?from=shop&to=shop-promoted&threshold=0.3')
    await api.serverConfig('a/b', 'claude-code')
    expect(calls[3]!.url).toBe('/api/servers/a%2Fb/config?host=claude-code')
  })

  it('reads a flow’s stage, and commits and rolls it back with the write header', async () => {
    await api.flowStage('shop.staged')
    await api.commitFlow('shop', { note: 'reads the order first' })
    await api.rollbackFlow('shop', { to: 1, note: null })
    expect(calls.map((c) => [c.init.method, c.url])).toEqual([
      ['GET', '/api/flows/shop.staged/stage'],
      ['POST', '/api/flows/shop/commit'],
      ['POST', '/api/flows/shop/rollback'],
    ])
    expect(headers(1)[WRITE_HEADER]).toBe('1')
    expect(JSON.parse(String(calls[1]!.init.body))).toEqual({ note: 'reads the order first' })
    expect(JSON.parse(String(calls[2]!.init.body))).toEqual({ to: 1, note: null })
  })

  it('turns {"error"} into an ApiError with the server’s message, and toasts it', async () => {
    next = () => respond(409, { error: 'a server named shop exists' })
    const error = await api.createServer({} as never).catch((e: unknown) => e)
    expect(error).toBeInstanceOf(ApiError)
    expect((error as ApiError).status).toBe(409)
    expect((error as ApiError).message).toBe('a server named shop exists')
    expect(onError).toHaveBeenCalledTimes(1)
    expect(onUnauthorized).not.toHaveBeenCalled()
  })

  it('keeps quiet errors for the caller to show', async () => {
    next = () => respond(404, { error: 'no session x' })
    await expect(api.session('x', { quiet: true })).rejects.toThrow('no session x')
    expect(onError).not.toHaveBeenCalled()
  })

  it('hands a 401 to the sign-in screen, not to the toasts', async () => {
    next = () => respond(401, { error: 'sign in' })
    await expect(api.meta()).rejects.toMatchObject({ status: 401 })
    expect(onUnauthorized).toHaveBeenCalledTimes(1)
    expect(onError).not.toHaveBeenCalled()
  })

  it('says so when the server does not answer', async () => {
    next = () => {
      throw new TypeError('Failed to fetch')
    }
    await expect(api.overview()).rejects.toMatchObject({ status: 0 })
    expect(onError.mock.calls[0]![0].message).toMatch(/did not answer/)
  })

  it('falls back to the status for a body that is not JSON', async () => {
    next = () => respond(502, '<html>Bad gateway</html>', 'text/html')
    await expect(request('GET', '/api/meta', undefined, { quiet: true })).rejects.toThrow(/^502/)
    next = () => respond(500, 'the data dir is not readable', 'text/plain')
    await expect(request('GET', '/api/meta', undefined, { quiet: true })).rejects.toThrow(
      'the data dir is not readable',
    )
  })

  it('reads a raw log as text, not JSON', async () => {
    next = () => respond(200, '{"a":1}\n{"b":2}\n', 'application/x-ndjson')
    const raw = await api.sessionRaw('k')
    expect(raw).toBe('{"a":1}\n{"b":2}\n')
    expect(api.sessionRawUrl('k~1')).toBe('/api/sessions/k~1/raw')
    expect(api.flowRawUrl('shop')).toBe('/api/flows/shop/raw')
  })

  it('passes aborts through untouched', async () => {
    next = () => {
      throw new DOMException('aborted', 'AbortError')
    }
    await expect(api.jobs()).rejects.toMatchObject({ name: 'AbortError' })
    expect(onError).not.toHaveBeenCalled()
  })
})

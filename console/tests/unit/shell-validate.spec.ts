import { describe, expect, it } from 'vitest'
import { splitCommand } from '@/lib/shell'
import {
  checkEnvName,
  checkHeaderName,
  checkServer,
  checkServerName,
  checkThreshold,
  checkUrl,
} from '@/lib/validate'
import type { ServerInput } from '@/api/types'

describe('splitting a command line', () => {
  it('splits on spaces and keeps quoted words whole', () => {
    expect(splitCommand('npx -y @acme/shop-mcp').words).toEqual(['npx', '-y', '@acme/shop-mcp'])
    expect(splitCommand(`server --root '/home/me/My Notes' --name "a \\"b\\""`).words).toEqual([
      'server',
      '--root',
      '/home/me/My Notes',
      '--name',
      'a "b"',
    ])
    expect(splitCommand('  a   b  ').words).toEqual(['a', 'b'])
    expect(splitCommand("a''b").words).toEqual(['ab'])
    expect(splitCommand("''").words).toEqual([''])
    expect(splitCommand('a\\ b').words).toEqual(['a b'])
    expect(splitCommand('').words).toEqual([])
  })

  it('says when a quote is not closed', () => {
    expect(splitCommand(`echo 'oops`).error).toMatch(/single quote/)
    expect(splitCommand(`echo "oops`).error).toMatch(/double quote/)
    expect(splitCommand('fine').error).toBeNull()
  })
})

describe('the server form’s checks', () => {
  it('takes names that can name a directory and a flow', () => {
    expect(checkServerName('shop')).toBeNull()
    expect(checkServerName('orders_api-2')).toBeNull()
    expect(checkServerName('')).toMatch(/name/)
    expect(checkServerName('Shop')).toMatch(/lowercase/)
    expect(checkServerName('a.b')).toMatch(/lowercase/)
    expect(checkServerName('-x')).toMatch(/Start/)
    expect(checkServerName('x'.repeat(65))).toMatch(/64/)
  })

  it('checks variable and header names, URLs and thresholds', () => {
    expect(checkEnvName('ORDERS_AUTH')).toBeNull()
    expect(checkEnvName('1BAD')).toMatch(/not a variable name/)
    expect(checkHeaderName('Authorization')).toBeNull()
    expect(checkHeaderName('Bad Header')).toMatch(/not a header name/)
    expect(checkUrl('https://example.com/mcp')).toBeNull()
    expect(checkUrl('ftp://example.com')).toMatch(/http/)
    expect(checkUrl('not a url')).toMatch(/not a URL/)
    expect(checkUrl('https://me:secret@example.com/mcp')).toMatch(/credentials/)
    expect(checkThreshold(null)).toBeNull()
    expect(checkThreshold(0.3)).toBeNull()
    expect(checkThreshold(1.5)).toMatch(/between 0 and 1/)
  })

  const base: ServerInput = {
    name: 'orders',
    description: null,
    upstream: { kind: 'stdio', command: ['npx', '-y', '@acme/orders'], env: ['ORDERS_KEY'] },
    mode: 'record',
    flow: null,
    record_dir: null,
    decider: null,
    threshold: null,
    guards: false,
    judge: null,
    commit: false,
    retain_days: null,
  }

  it('checks the guards, the judge and retention as the console does', () => {
    const retail = { ...base, name: 'retail', guards: true, commit: true, retain_days: 7 }
    expect(checkServer(retail)).toEqual({})
    expect(checkServer({ ...base, guards: true }).guards).toMatch(
      /no policy guards for orders: retail and airline have them/,
    )
    expect(checkServer({ ...base, name: '', guards: true }).guards).toMatch(/this server/)
    expect(checkServer({ ...retail, judge: { mode: 'log', context: '  ' } }).context).toMatch(
      /conversation/,
    )
    for (const days of [0, 1.5, -3]) {
      expect(checkServer({ ...retail, retain_days: days }).retain_days).toMatch(/at least one/)
    }
  })

  it('checks a surprise threshold set by hand', () => {
    expect(checkServer({ ...base, surprise: { kind: 'off' } })).toEqual({})
    expect(checkServer({ ...base, surprise: { kind: 'threshold', nats: 2.5 } })).toEqual({})
    for (const nats of [0, -1, NaN]) {
      expect(checkServer({ ...base, surprise: { kind: 'threshold', nats } }).surprise).toMatch(
        /in nats, above 0/,
      )
    }
  })

  it('accepts a good server and names every problem by field', () => {
    expect(checkServer(base)).toEqual({})
    expect(checkServer(base, ['orders']).name).toMatch(/exists/)
    expect(
      checkServer({ ...base, upstream: { kind: 'stdio', command: [], env: ['bad name'] } }),
    ).toMatchObject({
      command: expect.stringMatching(/command/),
      env: expect.stringMatching(/not a variable name/),
    })
    expect(checkServer({ ...base, mode: 'serve' }).flow).toMatch(/Serving runs a flow/)
    expect(checkServer({ ...base, mode: 'shadow' }).flow).toMatch(/Shadow mode runs a flow/)
    expect(
      checkServer({
        ...base,
        upstream: {
          kind: 'http',
          url: 'https://x.example/mcp',
          headers: [{ name: 'Authorization', env: '9X' }],
        },
      }).headers,
    ).toMatch(/not a variable name/)
  })
})

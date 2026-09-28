import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import ServerForm, { type ServerModel } from '@/components/server/ServerForm.vue'
import type { FlowSummary, ServerInput } from '@/api/types'

const blank: ServerModel = {
  name: '',
  description: '',
  kind: 'stdio',
  command: '',
  env: [],
  url: '',
  headers: [],
  mode: 'record',
  flow: '',
  record_dir: '',
  decider: '',
  threshold: '',
  guards: false,
  judge: '',
  context: '',
  commit: false,
  retain_days: '',
  surprise: '',
  surprise_nats: '',
  flow_tools_only: false,
  flow_tools: [],
  listen: false,
  listen_addr: '127.0.0.1:8931',
  listen_token: '',
}

const flow = {
  key: 'orders',
  path: 'orders.flow.json',
  domain: 'orders',
  error: null,
} as FlowSummary

function form(initial: Partial<ServerModel> = {}, taken: string[] = []) {
  return mount(ServerForm, {
    props: {
      initial: { ...blank, ...initial },
      editing: false,
      taken,
      flows: [flow],
      dataDir: '/home/me/.stretto',
      saving: false,
      serverError: null,
    },
  })
}

describe('the server form', () => {
  it('says what is missing when submitted empty, and submits nothing', async () => {
    const w = form()
    await w.find('form').trigger('submit')
    expect(w.text()).toContain('Give the server a name.')
    expect(w.text()).toContain('Give the command that starts the server.')
    expect(w.emitted('submit')).toBeUndefined()
  })

  it('submits a command server with its words and variable names', async () => {
    const w = form({ name: 'orders', command: `npx -y @acme/orders --root '/srv/My Orders'` })
    const env = w.find('input[placeholder="ORDERS_API_KEY"]')
    await env.setValue('ORDERS_API_KEY')
    await env.trigger('keydown', { key: 'Enter' })
    expect(w.findAll('.chip').map((c) => c.text())).toContain('ORDERS_API_KEY')
    await w.find('form').trigger('submit')
    const [input] = w.emitted('submit')![0] as [ServerInput]
    expect(input).toEqual({
      name: 'orders',
      description: null,
      upstream: {
        kind: 'stdio',
        command: ['npx', '-y', '@acme/orders', '--root', '/srv/My Orders'],
        env: ['ORDERS_API_KEY'],
      },
      mode: 'record',
      flow: null,
      record_dir: null,
      decider: null,
      threshold: null,
      surprise: null,
      flow_tools: [],
      guards: false,
      judge: null,
      commit: false,
      retain_days: null,
      listen: null,
    })
  })

  it('sets the guards, the judge, stretto_commit and how long sessions are kept', async () => {
    // Only retail and airline have guards.
    const shop = form({ name: 'shop', command: 'shop-mcp' })
    expect(shop.find('[data-testid="server-guards"]').attributes('disabled')).toBeDefined()
    const w = form({ name: 'retail', command: 'retail-mcp' })
    expect(w.find('[data-testid="server-judge"]').exists()).toBe(false)
    await w.find('[data-testid="server-guards"]').setValue(true)
    await w.find('[data-testid="server-judge"]').setValue('enforce')
    await w.find('form').trigger('submit')
    expect(w.text()).toContain('Name the file the host appends the conversation to.')
    expect(w.emitted('submit')).toBeUndefined()
    await w.find('[data-testid="server-context"]').setValue(' ~/.stretto/context/retail.jsonl ')
    await w.find('[data-testid="server-commit"]').setValue(true)
    await w.find('[data-testid="server-retain"]').setValue('30')
    await w.find('form').trigger('submit')
    const [input] = w.emitted('submit')![0] as [ServerInput]
    expect(input).toMatchObject({
      guards: true,
      judge: { mode: 'enforce', context: '~/.stretto/context/retail.jsonl' },
      commit: true,
      retain_days: 30,
    })
    expect(w.text()).toContain(
      '--retain-days 30 --guards --confirm-judge enforce --context ~/.stretto/context/retail.jsonl --commit -- retail-mcp',
    )
    // The guards off take the judge with them.
    await w.find('[data-testid="server-guards"]').setValue(false)
    await w.find('form').trigger('submit')
    expect((w.emitted('submit')![1] as [ServerInput])[0]).toMatchObject({
      guards: false,
      judge: null,
    })
  })

  it('serves the flow with its surprise gate, without it, or at another threshold', async () => {
    const gated = {
      ...flow,
      key: 'gated',
      path: 'gated.flow.json',
      surprise: { window: 3, threshold: 2.5, quantile: 0.95 },
    } as FlowSummary
    const w = mount(ServerForm, {
      props: {
        initial: { ...blank, name: 'orders', command: 'orders-mcp', mode: 'serve' },
        editing: false,
        taken: [],
        flows: [flow, gated],
        dataDir: '/home/me/.stretto',
        saving: false,
        serverError: null,
      },
    })
    const select = w.find('select[data-testid="server-flow"]')
    await select.setValue('~/.stretto/orders.flow.json')
    expect(w.text()).toContain('The flow has no gate')
    await select.setValue('~/.stretto/gated.flow.json')
    expect(w.text()).toContain(
      'The flow hands back once 3 of the agent’s steps in a row average more than 2.50 nats of surprise (learned at the 0.95 quantile).',
    )
    await w.find('[data-testid="server-surprise"]').setValue('threshold')
    await w.find('form').trigger('submit')
    expect(w.text()).toContain('Give the threshold in nats, above 0.')
    expect(w.emitted('submit')).toBeUndefined()
    await w.find('[data-testid="server-surprise-nats"]').setValue('4')
    await w.find('form').trigger('submit')
    const [input] = w.emitted('submit')![0] as [ServerInput]
    expect(input.surprise).toEqual({ kind: 'threshold', nats: 4 })
    expect(w.text()).toContain('--flow ~/.stretto/gated.flow.json --flow-surprise 4 -- orders-mcp')
    await w.find('[data-testid="server-surprise"]').setValue('off')
    await w.find('form').trigger('submit')
    expect((w.emitted('submit')![1] as [ServerInput])[0].surprise).toEqual({ kind: 'off' })
    expect(w.text()).toContain('--flow-surprise off')
  })

  it('keeps the flow to some of its lookups, chosen from the flow or typed', async () => {
    const known = {
      ...flow,
      lookup_tools: ['get_order_details', 'get_user_details'],
    } as FlowSummary
    const w = mount(ServerForm, {
      props: {
        initial: { ...blank, name: 'orders', command: 'orders-mcp', mode: 'serve' },
        editing: false,
        taken: [],
        flows: [known],
        dataDir: '/home/me/.stretto',
        saving: false,
        serverError: null,
      },
    })
    await w.find('select[data-testid="server-flow"]').setValue('~/.stretto/orders.flow.json')
    expect(w.find('[data-testid="server-flow-tool-choices"]').exists()).toBe(false)
    await w.find('[data-testid="server-flow-tools-only"]').setValue(true)
    await w.find('form').trigger('submit')
    expect(w.text()).toContain(
      'Choose at least one tool, or let the flow call every tool it looks up.',
    )
    expect(w.emitted('submit')).toBeUndefined()
    await w.find('[data-testid="server-flow-tool-get_user_details"]').setValue(true)
    await w.find('form').trigger('submit')
    const [input] = w.emitted('submit')![0] as [ServerInput]
    expect(input.flow_tools).toEqual(['get_user_details'])
    expect(w.text()).toContain('--flow-tools get_user_details -- orders-mcp')
    // A flow the console does not have: its tools are typed.
    const typed = form({
      name: 'orders',
      command: 'orders-mcp',
      mode: 'serve',
      flow: '/srv/flows/orders.flow.json',
      flow_tools_only: true,
    })
    await typed.find('[data-testid="server-flow-tools-text"]').setValue('get_a, get_b')
    await typed.find('form').trigger('submit')
    expect((typed.emitted('submit')![0] as [ServerInput])[0].flow_tools).toEqual(['get_a', 'get_b'])
  })

  it('lets hosts connect by URL to one proxy, with a token beyond loopback', async () => {
    const w = form({ name: 'orders', command: 'orders-mcp' })
    expect(w.find('[data-testid="server-listen-addr"]').exists()).toBe(false)
    await w.find('[data-testid="server-listen"]').setValue(true)
    await w.find('[data-testid="server-listen-addr"]').setValue('0.0.0.0:8931')
    await w.find('form').trigger('submit')
    expect(w.text()).toContain(
      'Without a token file, the proxy listens only on a loopback address.',
    )
    expect(w.emitted('submit')).toBeUndefined()
    await w.find('[data-testid="server-listen-addr"]').setValue('localhost')
    expect(w.text()).toContain('An address and a port, such as 127.0.0.1:8931.')
    await w.find('[data-testid="server-listen-addr"]').setValue('0.0.0.0:8931')
    await w.find('[data-testid="server-listen-token"]').setValue('~/.stretto/proxy-token')
    await w.find('form').trigger('submit')
    const [input] = w.emitted('submit')![0] as [ServerInput]
    expect(input.listen).toEqual({ addr: '0.0.0.0:8931', token_file: '~/.stretto/proxy-token' })
    expect(w.text()).toContain(
      '--listen 0.0.0.0:8931 --listen-token-file ~/.stretto/proxy-token -- orders-mcp',
    )
  })

  it('refuses a taken name and an unclosed quote', async () => {
    const w = form({ name: 'shop', command: `npx 'oops` }, ['shop'])
    await w.find('form').trigger('submit')
    expect(w.text()).toContain('A server named shop exists.')
    expect(w.text()).toContain('A single quote is not closed.')
  })

  it('needs a flow to serve, offers the data dir’s, and passes the threshold', async () => {
    const w = form({ name: 'orders', command: 'orders-mcp', mode: 'serve' })
    await w.find('form').trigger('submit')
    expect(w.text()).toContain('Serving runs a flow: choose one.')
    const select = w.find('select[data-testid="server-flow"]')
    expect(select.findAll('option').map((o) => o.attributes('value'))).toContain(
      '~/.stretto/orders.flow.json',
    )
    await select.setValue('~/.stretto/orders.flow.json')
    await w.find('input[placeholder="0.3"]').setValue('0.45')
    await w.find('form').trigger('submit')
    const [input] = w.emitted('submit')![0] as [ServerInput]
    expect(input).toMatchObject({
      mode: 'serve',
      flow: '~/.stretto/orders.flow.json',
      threshold: 0.45,
    })
    expect(w.text()).toContain(
      '--flow ~/.stretto/orders.flow.json --flow-threshold 0.45 -- orders-mcp',
    )
  })

  it('submits an HTTP server with headers from variables', async () => {
    const w = form({
      name: 'desk',
      kind: 'http',
      url: 'https://desk.example.com/mcp',
      headers: [{ name: 'Authorization', env: 'DESK_AUTH' }],
    })
    await w.find('form').trigger('submit')
    const [input] = w.emitted('submit')![0] as [ServerInput]
    expect(input.upstream).toEqual({
      kind: 'http',
      url: 'https://desk.example.com/mcp',
      headers: [{ name: 'Authorization', env: 'DESK_AUTH' }],
    })
    expect(w.text()).toContain(
      '--upstream https://desk.example.com/mcp --upstream-header Authorization=DESK_AUTH',
    )
  })
})

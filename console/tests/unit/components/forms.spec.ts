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
    })
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

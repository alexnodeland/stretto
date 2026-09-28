/** The server form's checks, the same the server makes, so mistakes show before a save. */
import type { ServerInput } from '@/api/types'

/** The host's name for the server is the domain: it names a directory and a flow's file. */
export function checkServerName(name: string): string | null {
  if (!name) return 'Give the server a name.'
  if (name.length > 64) return 'Keep the name to 64 characters.'
  if (!/^[a-z0-9_-]+$/.test(name)) return 'Use lowercase letters, digits, - and _.'
  if (name.startsWith('-')) return 'Start with a letter, a digit or _.'
  return null
}

export function checkEnvName(name: string): string | null {
  if (!name) return 'Name the variable.'
  if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name))
    return `${name} is not a variable name: letters, digits and _, not starting with a digit.`
  return null
}

export function checkHeaderName(name: string): string | null {
  if (!name) return 'Name the header.'
  if (!/^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/.test(name)) return `${name} is not a header name.`
  return null
}

export function checkUrl(url: string): string | null {
  if (!url) return 'Give the server’s URL.'
  let parsed: URL
  try {
    parsed = new URL(url)
  } catch {
    return 'That is not a URL.'
  }
  if (parsed.protocol !== 'https:' && parsed.protocol !== 'http:')
    return 'Use an http:// or https:// URL.'
  if (parsed.username || parsed.password)
    return 'Leave credentials out of the URL: send them in a header from a variable.'
  return null
}

export function checkThreshold(value: number | null): string | null {
  if (value === null) return null
  if (Number.isNaN(value) || value < 0 || value > 1) return 'A threshold is between 0 and 1.'
  return null
}

/** The domains the proxy has policy guards for (`stretto_report::guards`). */
export const GUARDED_DOMAINS: readonly string[] = ['retail', 'airline']

export type ServerErrors = Partial<
  Record<
    | 'name'
    | 'command'
    | 'env'
    | 'url'
    | 'headers'
    | 'flow'
    | 'threshold'
    | 'surprise'
    | 'flow_tools'
    | 'guards'
    | 'context'
    | 'retain_days'
    | 'listen',
    string
  >
>

/** Every problem with a server, by field. */
export function checkServer(input: ServerInput, taken: readonly string[] = []): ServerErrors {
  const errors: ServerErrors = {}
  const name = checkServerName(input.name)
  if (name) errors.name = name
  else if (taken.includes(input.name)) errors.name = `A server named ${input.name} exists.`
  if (input.upstream.kind === 'stdio') {
    if (!input.upstream.command.length || !input.upstream.command[0])
      errors.command = 'Give the command that starts the server.'
    const env = input.upstream.env.map(checkEnvName).find(Boolean)
    if (env) errors.env = env
  } else {
    const url = checkUrl(input.upstream.url)
    if (url) errors.url = url
    for (const h of input.upstream.headers) {
      const problem = checkHeaderName(h.name) ?? checkEnvName(h.env)
      if (problem) {
        errors.headers = problem
        break
      }
    }
  }
  if (input.mode !== 'record' && !input.flow) {
    errors.flow =
      input.mode === 'shadow'
        ? 'Shadow mode runs a flow: choose one.'
        : 'Serving runs a flow: choose one.'
  }
  const threshold = checkThreshold(input.threshold ?? null)
  if (threshold) errors.threshold = threshold
  const surprise = input.surprise ?? null
  if (surprise?.kind === 'threshold' && !(Number.isFinite(surprise.nats) && surprise.nats > 0))
    errors.surprise = 'Give the threshold in nats, above 0.'
  // The proxy takes them comma-separated.
  const badTool = (input.flow_tools ?? []).find((t) => !t || /[\s,]/.test(t) || t.length > 128)
  if (badTool !== undefined) errors.flow_tools = `${JSON.stringify(badTool)} is not a tool’s name.`
  if (input.guards && !GUARDED_DOMAINS.includes(input.name))
    errors.guards = `There are no policy guards for ${input.name || 'this server'}: retail and airline have them.`
  if (input.judge && !input.judge.context.trim())
    errors.context = 'Name the file the host appends the conversation to.'
  const days = input.retain_days ?? null
  if (days !== null && (!Number.isInteger(days) || days < 1))
    errors.retain_days = 'Keep sessions for a whole number of days, at least one.'
  const listen = input.listen ?? null
  if (listen) {
    const at = /^(\d{1,3}(?:\.\d{1,3}){3}|\[[0-9a-fA-F:.]+\]):(\d{1,5})$/.exec(listen.addr.trim())
    const octets = at?.[1]?.startsWith('[') ? [] : (at?.[1]?.split('.') ?? [])
    if (!at || octets.some((n) => Number(n) > 255) || Number(at[2]) > 65535)
      errors.listen = 'An address and a port, such as 127.0.0.1:8931.'
    else if (Number(at[2]) === 0) errors.listen = 'Hosts need the port the proxy listens on, not 0.'
    else if (!listen.token_file?.trim() && !loopback(at[1] ?? ''))
      errors.listen = 'Without a token file, the proxy listens only on a loopback address.'
  }
  return errors
}

/** Whether an address's host, as `--listen` takes it, is this machine's loopback. */
export function loopback(host: string): boolean {
  return /^127\./.test(host) || host === '[::1]'
}

/** The URL of a proxy listening at `addr`: loopback for an unspecified address, as the proxy prints it. */
export function listenUrl(addr: string): string {
  const named = addr
    .trim()
    .replace(/^0\.0\.0\.0:/, '127.0.0.1:')
    .replace(/^\[::\]:/, '[::1]:')
  return `http://${named}/mcp`
}

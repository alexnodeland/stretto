import { describe, expect, it } from 'vitest'
import {
  basename,
  commandText,
  formatBytes,
  formatCompact,
  formatCount,
  formatDate,
  formatDateTime,
  formatDay,
  formatDuration,
  formatOffset,
  formatPercent,
  formatProb,
  formatRelative,
  formatTime,
  plural,
  serverModeLabel,
  sessionModeLabel,
  shellQuote,
  upstreamText,
} from '@/lib/format'

describe('numbers', () => {
  it('writes counts with thousands separators, and a dash for none', () => {
    expect(formatCount(0)).toBe('0')
    expect(formatCount(4513)).toBe('4,513')
    expect(formatCount(1234567)).toBe('1,234,567')
    expect(formatCount(null)).toBe('—')
    expect(formatCount(undefined)).toBe('—')
    expect(formatCount(Number.NaN)).toBe('—')
  })

  it('compacts only past ten thousand', () => {
    expect(formatCompact(9999)).toBe('9,999')
    expect(formatCompact(12_900)).toBe('12.9K')
    expect(formatCompact(20_000)).toBe('20K')
    expect(formatCompact(4_200_000)).toBe('4.2M')
    expect(formatCompact(123_456)).toBe('123K')
  })

  it('writes sizes in powers of 1000', () => {
    expect(formatBytes(0)).toBe('0 B')
    expect(formatBytes(940)).toBe('940 B')
    expect(formatBytes(4925)).toBe('4.9 KB')
    expect(formatBytes(12_300_000)).toBe('12.3 MB')
    expect(formatBytes(252_000)).toBe('252 KB')
    expect(formatBytes(null)).toBe('—')
  })

  it('writes probabilities to two places and shares as percentages', () => {
    expect(formatProb(0.9925520080633771)).toBe('0.99')
    expect(formatProb(0.052769325969608226)).toBe('0.05')
    expect(formatProb(null)).toBe('—')
    expect(formatPercent(7 / 9)).toBe('78%')
    expect(formatPercent(1)).toBe('100%')
    expect(formatPercent(0.1234, 1)).toBe('12.3%')
  })

  it('pluralizes', () => {
    expect(plural(1, 'session')).toBe('1 session')
    expect(plural(6, 'session')).toBe('6 sessions')
    expect(plural(1200, 'call')).toBe('1,200 calls')
    expect(plural(2, 'entry', 'entries')).toBe('2 entries')
  })
})

describe('durations', () => {
  it('picks the unit that reads best', () => {
    expect(formatDuration(0)).toBe('0 ms')
    expect(formatDuration(6)).toBe('6 ms')
    expect(formatDuration(1240)).toBe('1.24 s')
    expect(formatDuration(12_400)).toBe('12.4 s')
    expect(formatDuration(125_000)).toBe('2 min 5 s')
    expect(formatDuration(120_000)).toBe('2 min')
    expect(formatDuration(2 * 3_600_000 + 14 * 60_000)).toBe('2 h 14 min')
    expect(formatDuration(3 * 86_400_000 + 4 * 3_600_000)).toBe('3 d 4 h')
    expect(formatDuration(-5)).toBe('0 ms')
    expect(formatDuration(null)).toBe('—')
    expect(formatOffset(24)).toBe('+24 ms')
  })
})

describe('times', () => {
  const now = new Date(2026, 8, 28, 12, 0, 0).getTime()

  it('says how long ago', () => {
    expect(formatRelative(now - 20_000, now)).toBe('just now')
    expect(formatRelative(now - 4 * 60_000, now)).toBe('4 min ago')
    expect(formatRelative(now - 3 * 3_600_000, now)).toBe('3 h ago')
    expect(formatRelative(now - 26 * 3_600_000, now)).toBe('yesterday')
    expect(formatRelative(now - 5 * 86_400_000, now)).toBe('5 days ago')
    expect(formatRelative(new Date(2026, 8, 17, 10).getTime(), now)).toBe('17 Sep')
    expect(formatRelative(new Date(2025, 11, 3, 10).getTime(), now)).toBe('3 Dec 2025')
    expect(formatRelative(null, now)).toBe('never')
    expect(formatRelative(now + 30_000, now)).toBe('just now')
  })

  it('writes dates and times without ambiguity, in local time', () => {
    const t = new Date(2026, 8, 28, 1, 56, 53).getTime()
    expect(formatDateTime(t)).toBe('28 Sep 2026, 01:56')
    expect(formatDate(t)).toBe('28 Sep 2026')
    expect(formatTime(t)).toBe('01:56:53')
    expect(formatDateTime(null)).toBe('—')
  })

  it('names a day of the chart', () => {
    expect(formatDay('2026-09-28')).toBe('Mon 28 Sep')
    expect(formatDay('2026-09-28', false)).toBe('28 Sep')
    expect(formatDay('garbage')).toBe('garbage')
  })
})

describe('labels and commands', () => {
  it('names modes as the lists do', () => {
    expect(sessionModeLabel('served')).toBe('Served')
    expect(sessionModeLabel('recorded')).toBe('Recorded')
    expect(serverModeLabel('serve')).toBe('Serving')
    expect(serverModeLabel('record')).toBe('Recording')
    expect(serverModeLabel('shadow')).toBe('Shadow')
  })

  it('quotes words as stretto init does, and leaves placeholders alone', () => {
    expect(shellQuote('~/.stretto/logs/shop')).toBe('~/.stretto/logs/shop')
    expect(shellQuote('@acme/shop-mcp')).toBe('@acme/shop-mcp')
    expect(shellQuote('two words')).toBe("'two words'")
    expect(shellQuote("it's")).toBe(`'it'\\''s'`)
    expect(shellQuote('')).toBe("''")
    expect(shellQuote('<domain>.flow.json')).toBe('<domain>.flow.json')
    expect(
      commandText(['npx', '-y', '@modelcontextprotocol/server-filesystem', '/home/me/My Notes']),
    ).toBe("npx -y @modelcontextprotocol/server-filesystem '/home/me/My Notes'")
  })

  it('writes where a server is', () => {
    expect(
      upstreamText({ kind: 'stdio', command: ['stretto-mcp-demo', '--world', 'retail'] }),
    ).toBe('stretto-mcp-demo --world retail')
    expect(upstreamText({ kind: 'http', url: 'https://desk.example.com/mcp', headers: [] })).toBe(
      'https://desk.example.com/mcp',
    )
    expect(upstreamText(null)).toBe('unknown')
    expect(basename('logs/shop/20260928T014620.569Z-654.jsonl')).toBe(
      '20260928T014620.569Z-654.jsonl',
    )
  })
})

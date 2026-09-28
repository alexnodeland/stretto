/**
 * How the console writes numbers, sizes, durations, times and probabilities.
 * Every number shown on a page goes through here, so they read the same
 * everywhere: counts with thousands separators, probabilities to two places.
 */
import type { ServerMode, ServerUpstream, SessionMode, Upstream } from '@/api/types'

const counts = new Intl.NumberFormat('en-US')

/** 1,284. A dash for a missing value. */
export function formatCount(n: number | null | undefined): string {
  if (n === null || n === undefined || Number.isNaN(n)) return '—'
  return counts.format(n)
}

/** 1,284 below ten thousand, then 12.9K, 4.2M. */
export function formatCompact(n: number | null | undefined): string {
  if (n === null || n === undefined || Number.isNaN(n)) return '—'
  const abs = Math.abs(n)
  if (abs < 10_000) return counts.format(n)
  const units: [number, string][] = [
    [1e9, 'B'],
    [1e6, 'M'],
    [1e3, 'K'],
  ]
  for (const [size, unit] of units) {
    if (abs >= size) {
      const value = n / size
      const digits = Math.abs(value) >= 100 ? 0 : 1
      return `${value.toFixed(digits).replace(/\.0$/, '')}${unit}`
    }
  }
  return counts.format(n)
}

/** 940 B, 4.9 KB, 12.3 MB (powers of 1000). */
export function formatBytes(n: number | null | undefined): string {
  if (n === null || n === undefined || Number.isNaN(n)) return '—'
  if (n < 1000) return `${Math.round(n)} B`
  const units = ['KB', 'MB', 'GB', 'TB']
  let value = n
  let unit = 'B'
  for (const u of units) {
    if (value < 1000) break
    value /= 1000
    unit = u
  }
  return `${value >= 100 ? value.toFixed(0) : value.toFixed(1)} ${unit}`
}

/** 320 ms, 1.24 s, 12.4 s, 4 min 5 s, 2 h 14 min, 3 d 4 h. */
export function formatDuration(ms: number | null | undefined): string {
  if (ms === null || ms === undefined || Number.isNaN(ms)) return '—'
  if (ms < 0) ms = 0
  if (ms < 1000) return `${Math.round(ms)} ms`
  const s = ms / 1000
  if (s < 10) return `${s.toFixed(2)} s`
  if (s < 60) return `${s.toFixed(1)} s`
  const totalSeconds = Math.round(s)
  const minutes = Math.floor(totalSeconds / 60)
  if (minutes < 60) {
    const rest = totalSeconds % 60
    return rest ? `${minutes} min ${rest} s` : `${minutes} min`
  }
  const hours = Math.floor(minutes / 60)
  if (hours < 24) {
    const rest = minutes % 60
    return rest ? `${hours} h ${rest} min` : `${hours} h`
  }
  const days = Math.floor(hours / 24)
  const rest = hours % 24
  return rest ? `${days} d ${rest} h` : `${days} d`
}

/** A time into a session: +0 ms, +12 ms, +1.20 s, +2 min 3 s. */
export function formatOffset(ms: number): string {
  return `+${formatDuration(ms)}`
}

/** A probability to two places: 0.87. */
export function formatProb(p: number | null | undefined): string {
  if (p === null || p === undefined || Number.isNaN(p)) return '—'
  return p.toFixed(2)
}

/** A share as a percentage: 88%. */
export function formatPercent(x: number | null | undefined, digits = 0): string {
  if (x === null || x === undefined || Number.isNaN(x)) return '—'
  return `${(x * 100).toFixed(digits)}%`
}

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec']
const WEEKDAYS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat']
const two = (n: number) => String(n).padStart(2, '0')

function dayMonthText(d: Date): string {
  return `${d.getDate()} ${MONTHS[d.getMonth()]}`
}

function dateText(d: Date): string {
  return `${dayMonthText(d)} ${d.getFullYear()}`
}

/** 28 Sep 2026, 01:56 (local time). */
export function formatDateTime(ms: number | null | undefined): string {
  if (ms === null || ms === undefined) return '—'
  const d = new Date(ms)
  return `${dateText(d)}, ${two(d.getHours())}:${two(d.getMinutes())}`
}

/** 28 Sep 2026. */
export function formatDate(ms: number | null | undefined): string {
  if (ms === null || ms === undefined) return '—'
  return dateText(new Date(ms))
}

/** 01:56:53. */
export function formatTime(ms: number | null | undefined): string {
  if (ms === null || ms === undefined) return '—'
  const d = new Date(ms)
  return `${two(d.getHours())}:${two(d.getMinutes())}:${two(d.getSeconds())}`
}

/** just now, 4 min ago, 3 h ago, yesterday, 5 days ago, then the date. */
export function formatRelative(ms: number | null | undefined, now: number = Date.now()): string {
  if (ms === null || ms === undefined) return 'never'
  const diff = now - ms
  if (diff < 0) return diff > -60_000 ? 'just now' : formatDateTime(ms)
  const minutes = Math.floor(diff / 60_000)
  if (minutes < 1) return 'just now'
  if (minutes < 60) return `${minutes} min ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours} h ago`
  const days = Math.floor(hours / 24)
  if (days === 1) return 'yesterday'
  if (days < 7) return `${days} days ago`
  const date = new Date(ms)
  return date.getFullYear() === new Date(now).getFullYear() ? dayMonthText(date) : dateText(date)
}

/** A day of the activity chart (`YYYY-MM-DD`, a local calendar day): Mon 28 Sep. */
export function formatDay(day: string, withWeekday = true): string {
  const [y, m, d] = day.split('-').map(Number)
  if (!y || !m || !d) return day
  const date = new Date(y, m - 1, d)
  return withWeekday ? `${WEEKDAYS[date.getDay()]} ${dayMonthText(date)}` : dayMonthText(date)
}

/** "1 session", "2 sessions". */
export function plural(n: number, one: string, many = `${one}s`): string {
  return `${formatCount(n)} ${n === 1 ? one : many}`
}

/** A session's mode, as the sessions list names it. */
export function sessionModeLabel(mode: SessionMode): string {
  return { recorded: 'Recorded', shadow: 'Shadow', served: 'Served' }[mode]
}

/** A server's mode, as the servers list names it. */
export function serverModeLabel(mode: ServerMode): string {
  return { record: 'Recording', shadow: 'Shadow', serve: 'Serving' }[mode]
}

/** A word as a POSIX shell reads it back (as `stretto init` quotes it). A `<placeholder>` is left as it is. */
export function shellQuote(word: string): string {
  if (word !== '' && /^[A-Za-z0-9\-_./:=@%+,~]+$/.test(word)) return word
  if (/^[A-Za-z0-9\-_./:=@%+,~]*<[a-z -]+>[A-Za-z0-9\-_./:=@%+,~]*$/.test(word)) return word
  return `'${word.replace(/'/g, `'\\''`)}'`
}

/** A command line, quoted for a shell. */
export function commandText(words: readonly string[]): string {
  return words.map(shellQuote).join(' ')
}

/** Where a server is: its command, or its URL. */
export function upstreamText(upstream: Upstream | ServerUpstream | null | undefined): string {
  if (!upstream) return 'unknown'
  return upstream.kind === 'stdio' ? commandText(upstream.command) : upstream.url
}

/** The last part of a path: `logs/shop/x.jsonl` → `x.jsonl`. */
export function basename(path: string): string {
  const parts = path.split(/[\\/]/)
  return parts[parts.length - 1] || path
}

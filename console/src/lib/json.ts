/** JSON for people: pretty, or one short line. */

export function prettyJson(value: unknown): string {
  if (value === undefined) return ''
  try {
    return JSON.stringify(value, null, 2) ?? String(value)
  } catch {
    return String(value)
  }
}

/** One line, cut to `max` characters with an ellipsis. */
export function compactJson(value: unknown, max = 120): string {
  let text: string
  try {
    text = typeof value === 'string' ? value : (JSON.stringify(value) ?? String(value))
  } catch {
    text = String(value)
  }
  text = text.replace(/\s+/g, ' ')
  return text.length > max ? `${text.slice(0, max - 1)}…` : text
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
}

/** JSON Lines: one value per line, the lines that do not parse kept as text. */
export function parseJsonLines(
  text: string,
): { line: number; value: unknown; raw: string; ok: boolean }[] {
  const out: { line: number; value: unknown; raw: string; ok: boolean }[] = []
  text.split('\n').forEach((raw, i) => {
    if (!raw.trim()) return
    try {
      out.push({ line: i + 1, value: JSON.parse(raw), raw, ok: true })
    } catch {
      out.push({ line: i + 1, value: raw, raw, ok: false })
    }
  })
  return out
}

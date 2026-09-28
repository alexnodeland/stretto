/**
 * Splitting a command line typed into the server form into words, as a POSIX
 * shell would: spaces separate words, single quotes keep everything, double
 * quotes keep everything but a backslash before " \ $ or `.
 */

export interface Split {
  words: string[]
  error: string | null
}

export function splitCommand(text: string): Split {
  const words: string[] = []
  let word = ''
  let inWord = false
  let quote: "'" | '"' | null = null
  for (let i = 0; i < text.length; i++) {
    const c = text[i]!
    if (quote === "'") {
      if (c === "'") quote = null
      else word += c
      continue
    }
    if (quote === '"') {
      if (c === '"') quote = null
      else if (c === '\\' && i + 1 < text.length && '"\\$`'.includes(text[i + 1]!))
        word += text[++i]
      else word += c
      continue
    }
    if (c === "'" || c === '"') {
      quote = c
      inWord = true
    } else if (c === '\\') {
      if (i + 1 < text.length) word += text[++i]
      inWord = true
    } else if (/\s/.test(c)) {
      if (inWord) words.push(word)
      word = ''
      inWord = false
    } else {
      word += c
      inWord = true
    }
  }
  if (quote)
    return { words, error: `A ${quote === '"' ? 'double' : 'single'} quote is not closed.` }
  if (inWord) words.push(word)
  return { words, error: null }
}

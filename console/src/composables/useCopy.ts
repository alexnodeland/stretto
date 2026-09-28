/** Copy text to the clipboard, with a toast either way. */
import { toast } from '@/stores/toasts'

function fallbackCopy(text: string): boolean {
  const area = document.createElement('textarea')
  area.value = text
  area.setAttribute('readonly', '')
  area.style.position = 'fixed'
  area.style.opacity = '0'
  document.body.appendChild(area)
  area.select()
  try {
    return document.execCommand('copy')
  } catch {
    return false
  } finally {
    area.remove()
  }
}

async function write(text: string): Promise<boolean> {
  if (!navigator.clipboard?.writeText) return fallbackCopy(text)
  try {
    await navigator.clipboard.writeText(text)
    return true
  } catch {
    return fallbackCopy(text)
  }
}

export async function copyText(text: string, what = 'Text'): Promise<boolean> {
  const ok = await write(text)
  if (ok) toast({ kind: 'success', title: `${what} copied`, timeout: 2200 })
  else
    toast({
      kind: 'error',
      title: `Could not copy`,
      message: 'Select the text and copy it by hand.',
    })
  return ok
}

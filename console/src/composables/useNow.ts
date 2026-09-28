/** The time now, ticking every 30 s, for "5 min ago". */
import { onScopeDispose, ref } from 'vue'

const now = ref(Date.now())
let users = 0
let timer: ReturnType<typeof setInterval> | null = null

export function useNow() {
  users += 1
  if (!timer) timer = setInterval(() => (now.value = Date.now()), 30_000)
  onScopeDispose(() => {
    users -= 1
    if (users === 0 && timer) {
      clearInterval(timer)
      timer = null
    }
  })
  return now
}

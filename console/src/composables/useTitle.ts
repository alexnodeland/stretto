/** The document's title: "<page> · stretto console". */
import { watchEffect } from 'vue'

export function useTitle(title: () => string | null | undefined): void {
  watchEffect(() => {
    const t = title()
    document.title = t ? `${t} · stretto console` : 'stretto console'
  })
}

<script setup lang="ts">
/** A server's tools as its tools/list gave them, with their kinds from readOnlyHint. */
import KindBadge from './KindBadge.vue'
import type { ToolInfo } from '@/api/types'

defineProps<{ tools: ToolInfo[] }>()
function hints(t: ToolInfo): string {
  const parts: string[] = []
  parts.push(t.read_only_hint === null ? 'no readOnlyHint' : `readOnlyHint: ${t.read_only_hint}`)
  if (t.destructive_hint !== null) parts.push(`destructiveHint: ${t.destructive_hint}`)
  return parts.join(' · ')
}
</script>

<template>
  <div class="table-wrap">
    <table class="table stack tt">
      <thead>
        <tr>
          <th scope="col">Tool</th>
          <th scope="col">Kind</th>
          <th scope="col">Description</th>
          <th scope="col">Annotations</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="t in tools" :key="t.name">
          <td class="nowrap inline">
            <span class="tool">{{ t.name }}</span>
          </td>
          <td class="inline"><KindBadge :kind="t.kind" /></td>
          <td class="desc">{{ t.description ?? '—' }}</td>
          <td class="caption mono hints" :class="{ 'stack-hide': !hints(t) }">{{ hints(t) }}</td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<style scoped>
.desc {
  min-width: 240px;
  color: var(--stretto-text-muted);
}

.hints {
  font-size: 11.5px;
  white-space: nowrap;
}
</style>

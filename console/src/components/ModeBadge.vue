<script setup lang="ts">
/**
 * A session's or a server's mode, in the brand's terms: serving is petrol
 * (stretto acts), shadow a dashed petrol outline (it decides but looks
 * nothing up), recording neutral (it only records).
 */
import { computed } from 'vue'
import UiBadge from './ui/UiBadge.vue'
import type { ServerMode, SessionMode } from '@/api/types'
import { serverModeLabel, sessionModeLabel } from '@/lib/format'

const props = defineProps<{ mode: SessionMode | ServerMode }>()

const view = computed(() => {
  switch (props.mode) {
    case 'served':
      return {
        tone: 'accent' as const,
        label: sessionModeLabel('served'),
        title: 'The flow made lookups in this session',
      }
    case 'serve':
      return {
        tone: 'accent' as const,
        label: serverModeLabel('serve'),
        title: 'The proxy serves a flow: its lookups ride in the results',
      }
    case 'shadow':
      return {
        tone: 'dashed' as const,
        label: 'Shadow',
        title: 'The flow decides and logs, but looks nothing up',
      }
    case 'record':
      return {
        tone: 'outline' as const,
        label: serverModeLabel('record'),
        title: 'The proxy records sessions and runs no flow',
      }
    default:
      return {
        tone: 'outline' as const,
        label: sessionModeLabel('recorded'),
        title: 'Recorded without a flow',
      }
  }
})
</script>

<template>
  <UiBadge :tone="view.tone" :title="view.title" dot :class="`mode-${mode}`">{{
    view.label
  }}</UiBadge>
</template>

<style scoped>
.mode-served :deep(.badge-dot),
.mode-serve :deep(.badge-dot) {
  background: var(--stretto-accent-graphic);
}

.mode-shadow :deep(.badge-dot) {
  background: transparent;
  box-shadow: inset 0 0 0 1.5px var(--stretto-accent-graphic);
}

.mode-record :deep(.badge-dot),
.mode-recorded :deep(.badge-dot) {
  background: var(--stretto-border-strong);
}
</style>

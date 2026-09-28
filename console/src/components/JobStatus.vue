<script setup lang="ts">
/** A job's status, with an icon and its word. */
import { computed } from 'vue'
import { CircleCheck, CircleX, Clock, LoaderCircle } from '@lucide/vue'
import type { JobStatus } from '@/api/types'
import UiBadge from './ui/UiBadge.vue'

const props = defineProps<{ status: JobStatus }>()
const view = computed(() => {
  switch (props.status) {
    case 'running':
      return { tone: 'accent' as const, icon: LoaderCircle, label: 'Running', spin: true }
    case 'succeeded':
      return { tone: 'accent' as const, icon: CircleCheck, label: 'Succeeded', spin: false }
    case 'failed':
      return { tone: 'danger' as const, icon: CircleX, label: 'Failed', spin: false }
    default:
      return { tone: 'neutral' as const, icon: Clock, label: 'Queued', spin: false }
  }
})
</script>

<template>
  <UiBadge :tone="view.tone" :icon="view.icon" :class="{ spin: view.spin }">{{
    view.label
  }}</UiBadge>
</template>

<style scoped>
.spin :deep(svg) {
  animation: spin 0.9s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>

<script setup lang="ts">
/** A job's status, with an icon and its word. */
import { computed, type Component } from 'vue'
import { Ban, CircleCheck, CircleX, Clock, LoaderCircle, Siren } from '@lucide/vue'
import type { JobStatus } from '@/api/types'
import UiBadge from './ui/UiBadge.vue'

/** `alarm`: it succeeded, and found something to act on (`stretto drift`'s alarm). */
const props = defineProps<{ status: JobStatus; alarm?: boolean }>()

interface View {
  tone: 'accent' | 'danger' | 'neutral' | 'warn'
  icon: Component
  label: string
  spin: boolean
}
const views: Record<JobStatus, View> = {
  queued: { tone: 'neutral', icon: Clock, label: 'Queued', spin: false },
  running: { tone: 'accent', icon: LoaderCircle, label: 'Running', spin: true },
  succeeded: { tone: 'accent', icon: CircleCheck, label: 'Succeeded', spin: false },
  failed: { tone: 'danger', icon: CircleX, label: 'Failed', spin: false },
  cancelled: { tone: 'neutral', icon: Ban, label: 'Cancelled', spin: false },
}
const alarmView: View = { tone: 'warn', icon: Siren, label: 'Alarm', spin: false }
const view = computed(() =>
  props.status === 'succeeded' && props.alarm ? alarmView : views[props.status],
)
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

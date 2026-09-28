<script setup lang="ts">
/** A tool's kind, from the server's readOnlyHint: read, write, or neither (no hint). */
import { computed } from 'vue'
import { PenLine } from '@lucide/vue'
import UiBadge from './ui/UiBadge.vue'
import type { ToolKind } from '@/api/types'

const props = defineProps<{ kind: ToolKind }>()
const view = computed(() => {
  if (props.kind === 'write')
    return {
      tone: 'outline' as const,
      label: 'write',
      icon: PenLine,
      title: 'readOnlyHint: false. A flow never calls it.',
    }
  if (props.kind === 'read')
    return {
      tone: 'neutral' as const,
      label: 'read',
      icon: undefined,
      title: 'readOnlyHint: true. A flow may look it up.',
    }
  return {
    tone: 'neutral' as const,
    label: 'neither',
    icon: undefined,
    title: 'No readOnlyHint: neither read nor write. A flow never calls it.',
  }
})
</script>

<template>
  <UiBadge :tone="view.tone" :icon="view.icon" :title="view.title" mono>{{ view.label }}</UiBadge>
</template>

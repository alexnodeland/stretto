<script setup lang="ts">
/** "5 min ago", with the exact time on hover. */
import { computed } from 'vue'
import { useNow } from '@/composables/useNow'
import { formatDateTime, formatRelative } from '@/lib/format'

const props = defineProps<{ ms: number | null | undefined }>()
const now = useNow()
const iso = computed(() => (props.ms ? new Date(props.ms).toISOString() : undefined))
</script>

<template>
  <time :datetime="iso" :title="formatDateTime(ms)" class="nowrap">{{
    formatRelative(ms, now)
  }}</time>
</template>

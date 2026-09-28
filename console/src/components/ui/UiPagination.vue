<script setup lang="ts">
/** "1–50 of 312", and the pages before and after. */
import { computed } from 'vue'
import { ChevronLeft, ChevronRight } from '@lucide/vue'
import UiButton from './UiButton.vue'
import { formatCount } from '@/lib/format'

const props = defineProps<{ total: number; limit: number; offset: number; noun?: string }>()
const emit = defineEmits<{ 'update:offset': [offset: number] }>()
const first = computed(() => (props.total === 0 ? 0 : props.offset + 1))
const last = computed(() => Math.min(props.total, props.offset + props.limit))
</script>

<template>
  <nav class="pager" aria-label="Pages">
    <span class="pager-range num">
      {{ formatCount(first) }}–{{ formatCount(last) }} of {{ formatCount(total) }} {{ noun ?? '' }}
    </span>
    <span class="spacer" />
    <UiButton
      size="sm"
      :icon="ChevronLeft"
      :disabled="offset <= 0"
      @click="emit('update:offset', Math.max(0, offset - limit))"
    >
      Newer
    </UiButton>
    <UiButton
      size="sm"
      :icon-right="ChevronRight"
      :disabled="offset + limit >= total"
      @click="emit('update:offset', offset + limit)"
    >
      Older
    </UiButton>
  </nav>
</template>

<style scoped>
.pager {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px 20px;
  border-top: 1px solid var(--stretto-border);
}

.pager-range {
  font-size: 13px;
  color: var(--stretto-text-muted);
}

@media (max-width: 720px) {
  .pager {
    padding: 12px 16px;
  }
}
</style>

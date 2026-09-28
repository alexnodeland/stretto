<script setup lang="ts">
/** Doctor-like checks: each with an icon and its words. */
import { CircleCheck, CircleX, Info, TriangleAlert } from '@lucide/vue'
import type { HealthItem } from '@/api/types'
import RichText from './RichText.vue'

defineProps<{ items: HealthItem[] }>()

const icons = { ok: CircleCheck, note: Info, warn: TriangleAlert, error: CircleX }
const words = { ok: 'OK', note: 'Note', warn: 'Warning', error: 'Error' }
</script>

<template>
  <ul class="health">
    <li v-for="(item, i) in items" :key="i" class="health-item" :class="`lvl-${item.level}`">
      <component
        :is="icons[item.level]"
        class="health-icon"
        :size="16"
        :stroke-width="2"
        aria-hidden="true"
      />
      <span class="sr-only">{{ words[item.level] }}:</span>
      <span class="health-msg"><RichText :text="item.message" /></span>
    </li>
  </ul>
</template>

<style scoped>
.health {
  list-style: none;
  display: flex;
  flex-direction: column;
}

.health-item {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 10px 0;
  font-size: 13.5px;
  border-bottom: 1px solid var(--stretto-border);
}

.health-item:first-child {
  padding-top: 0;
}

.health-item:last-child {
  border-bottom: 0;
  padding-bottom: 0;
}

.health-icon {
  flex: none;
  margin-top: 2px;
}

.health-msg {
  min-width: 0;
  overflow-wrap: anywhere;
}

.lvl-ok .health-icon {
  color: var(--stretto-accent-graphic);
}

.lvl-note .health-icon {
  color: var(--stretto-text-subtle);
}

.lvl-warn .health-icon {
  color: var(--c-warn);
}

.lvl-error .health-icon {
  color: var(--c-danger);
}

.lvl-error .health-msg {
  color: var(--stretto-text);
}
</style>

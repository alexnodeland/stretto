<script setup lang="ts">
/** A KPI tile: what is counted, the count, its scope, and the last days as a sparkline. */
import { RouterLink, type RouteLocationRaw } from 'vue-router'
import Sparkline from './Sparkline.vue'
import { formatCompact, formatCount } from '@/lib/format'

defineProps<{
  label: string
  value: number
  scope: string
  trend?: number[]
  trendLabel?: string
  to?: RouteLocationRaw
  accent?: boolean
}>()
</script>

<template>
  <component :is="to ? RouterLink : 'div'" :to="to" class="stat" :class="{ link: to, accent }">
    <p class="stat-label">{{ label }}</p>
    <p class="stat-value" :title="formatCount(value)">{{ formatCompact(value) }}</p>
    <Sparkline
      v-if="trend && trend.length > 1 && trend.some((v) => v > 0)"
      class="stat-spark"
      :values="trend"
      :label="trendLabel ?? label"
    />
    <p class="stat-scope">{{ scope }}</p>
  </component>
</template>

<style scoped>
.stat {
  position: relative;
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  grid-template-areas:
    'label label'
    'value spark'
    'scope scope';
  align-items: end;
  align-content: start;
  column-gap: 12px;
  padding: 16px 18px 15px;
  border-radius: var(--c-radius-card);
  background: var(--stretto-surface);
  border: 1px solid var(--stretto-border);
  box-shadow: var(--c-shadow-sm);
  color: inherit;
  min-width: 0;
  transition:
    border-color 0.15s var(--c-ease),
    box-shadow 0.15s var(--c-ease);
}

.stat.link:hover {
  color: inherit;
  border-color: color-mix(in oklab, var(--stretto-border-strong) 45%, var(--stretto-border));
  box-shadow: var(--c-shadow-md);
}

.stat.accent::before {
  content: '';
  position: absolute;
  left: -1px;
  top: 16px;
  height: 20px;
  width: 3px;
  border-radius: 0 3px 3px 0;
  background: var(--stretto-accent-graphic);
}

.stat-label {
  grid-area: label;
  font-size: 13px;
  font-weight: 500;
  color: var(--stretto-text-muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.stat-value {
  grid-area: value;
  margin-top: 8px;
  font-size: 30px;
  font-weight: 600;
  letter-spacing: -0.025em;
  line-height: 1.05;
  color: var(--stretto-text);
}

.stat-spark {
  grid-area: spark;
  margin-bottom: 2px;
}

.stat-scope {
  grid-area: scope;
  margin-top: 8px;
  font-size: 12px;
  color: var(--stretto-text-subtle);
}
</style>

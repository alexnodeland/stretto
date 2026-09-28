<script setup lang="ts">
/** A small line of the last days, the latest point marked in the accent. */
import { computed } from 'vue'

const props = withDefaults(
  defineProps<{ values: number[]; width?: number; height?: number; label: string }>(),
  {
    width: 112,
    height: 32,
  },
)

const points = computed(() => {
  const n = props.values.length
  const max = Math.max(1, ...props.values)
  const pad = 4
  return props.values.map((v, i) => ({
    x: n > 1 ? pad + (i * (props.width - pad * 2)) / (n - 1) : props.width / 2,
    y: props.height - pad - (v / max) * (props.height - pad * 2),
  }))
})
const path = computed(() =>
  points.value.map((p, i) => `${i ? 'L' : 'M'}${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(' '),
)
const area = computed(() => {
  const p = points.value
  if (!p.length) return ''
  return `${path.value} L${p[p.length - 1]!.x.toFixed(1)},${props.height} L${p[0]!.x.toFixed(1)},${props.height} Z`
})
const last = computed(() => points.value[points.value.length - 1])
</script>

<template>
  <svg
    class="spark"
    :width="width"
    :height="height"
    :viewBox="`0 0 ${width} ${height}`"
    role="img"
    :aria-label="label"
  >
    <path :d="area" class="spark-area" />
    <path :d="path" class="spark-line" />
    <circle v-if="last" :cx="last.x" :cy="last.y" r="3.5" class="spark-dot" />
  </svg>
</template>

<style scoped>
.spark {
  overflow: visible;
}

.spark-area {
  fill: var(--stretto-chart-baseline);
  opacity: 0.08;
}

.spark-line {
  fill: none;
  stroke: var(--stretto-chart-baseline);
  stroke-width: 1.6;
  stroke-linejoin: round;
  stroke-linecap: round;
}

.spark-dot {
  fill: var(--stretto-chart-accent);
  stroke: var(--stretto-surface);
  stroke-width: 2;
}
</style>

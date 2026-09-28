<script setup lang="ts">
/**
 * A lookup of the tool just called (another order after an order), drawn as a
 * loop over its node, or beside it when the graph runs top to bottom.
 */
import { computed } from 'vue'
import { BaseEdge, EdgeLabelRenderer, Position, type EdgeProps } from '@vue-flow/core'
import type { GraphEdge } from '@/lib/flow'
import { formatPercent, formatProb } from '@/lib/format'

const props = defineProps<EdgeProps<GraphEdge>>()
const loop = computed(() => {
  const sx = props.sourceX
  const sy = props.sourceY
  if (props.sourcePosition === Position.Bottom) {
    // Top to bottom: out of the node's right side and back into it.
    const size = props.sourceNode?.dimensions
    const right = sx + (size?.width ?? 200) / 2
    const cy = sy - (size?.height ?? 58) / 2
    const reach = 64
    const d = `M${right},${cy + 12} C${right + reach},${cy + 44} ${right + reach},${cy - 44} ${right + 6},${cy - 12}`
    return { d, x: right + reach * 0.75, y: cy }
  }
  const tx = props.targetX - 6
  const ty = props.targetY
  const lift = 78
  const d = `M${sx},${sy} C${sx + 70},${sy - lift} ${tx - 70},${ty - lift} ${tx},${ty}`
  return { d, x: (sx + props.targetX) / 2, y: sy - lift * 0.75 }
})
const title = computed(
  () =>
    `After ${props.source}: ${props.target} again, ${formatPercent(props.data.share)} of what the agent did next in training (${props.data.count} times). ` +
    `Weighed with its binding: ${formatProb(props.data.prob)}. ${props.data.acts ? 'The flow looks it up.' : 'The flow hands back.'}`,
)
</script>

<template>
  <BaseEdge
    :id="id"
    :path="loop.d"
    :marker-end="`url(#${data.acts ? 'fg-arrow-acts' : 'fg-arrow'})`"
    :style="{
      stroke: data.acts ? 'var(--stretto-chart-accent)' : 'var(--stretto-border-strong)',
      strokeWidth: data.width,
      strokeLinecap: 'round',
      fill: 'none',
    }"
    :interaction-width="20"
  />
  <EdgeLabelRenderer>
    <div
      class="elabel nodrag nopan"
      :class="{ acts: data.acts }"
      :style="{ transform: `translate(-50%, -50%) translate(${loop.x}px, ${loop.y}px)` }"
      :title="title"
    >
      <strong class="num">{{ formatProb(data.prob) }}</strong>
      <span class="num">{{ formatPercent(data.share) }}</span>
    </div>
  </EdgeLabelRenderer>
</template>

<style scoped>
.elabel {
  position: absolute;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 2px 7px;
  border-radius: 999px;
  background: var(--stretto-surface);
  border: 1px solid var(--stretto-border);
  font-size: 11px;
  color: var(--stretto-text-subtle);
  pointer-events: all;
  white-space: nowrap;
}

.elabel strong {
  color: var(--stretto-text);
  font-weight: 600;
}

.elabel.acts {
  border-color: color-mix(in oklab, var(--stretto-accent-graphic) 55%, var(--stretto-border));
}

.elabel.acts strong {
  color: var(--stretto-accent);
}
</style>

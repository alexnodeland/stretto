<script setup lang="ts">
/** A lookup after a call: as wide as its share of what the agent did next, petrol when the flow makes it at this threshold. */
import { computed } from 'vue'
import {
  BaseEdge,
  EdgeLabelRenderer,
  Position,
  getBezierPath,
  type EdgeProps,
} from '@vue-flow/core'
import type { GraphEdge } from '@/lib/flow'
import { formatPercent, formatProb } from '@/lib/format'

const props = defineProps<EdgeProps<GraphEdge>>()
const path = computed(() =>
  getBezierPath({
    sourceX: props.sourceX,
    sourceY: props.sourceY,
    sourcePosition: props.sourcePosition,
    // Stop short of the node, for the arrowhead.
    targetX: props.targetPosition === Position.Left ? props.targetX - 6 : props.targetX,
    targetY: props.targetPosition === Position.Top ? props.targetY - 6 : props.targetY,
    targetPosition: props.targetPosition,
    curvature: 0.35,
  }),
)
const title = computed(
  () =>
    `After ${props.source}: ${props.target}, ${formatPercent(props.data.share)} of what the agent did next in training (${props.data.count} times). ` +
    `Weighed with its binding: ${formatProb(props.data.prob)}. ${props.data.acts ? 'The flow looks it up.' : 'The flow hands back.'}`,
)
</script>

<template>
  <BaseEdge
    :id="id"
    :path="path[0]"
    :marker-end="`url(#${data.acts ? 'fg-arrow-acts' : 'fg-arrow'})`"
    :style="{
      stroke: data.acts ? 'var(--stretto-chart-accent)' : 'var(--stretto-border-strong)',
      strokeWidth: data.width,
      strokeLinecap: 'round',
    }"
    :interaction-width="20"
  />
  <EdgeLabelRenderer>
    <div
      class="elabel nodrag nopan"
      :class="{ acts: data.acts }"
      :style="{ transform: `translate(-50%, -50%) translate(${path[1]}px, ${path[2]}px)` }"
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

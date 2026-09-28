<script setup lang="ts">
/**
 * The flow as a graph: after each call, the lookups it may make, left to
 * right, or top to bottom when the box is narrow. An edge is as wide as the
 * lookup's share of what the agent did next in training, and petrol when the
 * flow makes it at this threshold.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { VueFlow, useVueFlow, type Edge, type Node } from '@vue-flow/core'
import '@vue-flow/core/dist/style.css'
import { Maximize, Minus, Plus } from '@lucide/vue'
import FlowNode from './FlowNode.vue'
import ShareEdge from './ShareEdge.vue'
import LoopEdge from './LoopEdge.vue'
import type { FlowTool } from '@/api/types'
import {
  buildFlowGraph,
  type GraphDirection,
  type GraphEdge,
  type GraphNode,
  type SitePreview,
} from '@/lib/flow'

const props = defineProps<{ previews: SitePreview[]; tools: FlowTool[]; id: string }>()
const { setViewport, zoomIn, zoomOut } = useVueFlow(props.id)

const box = ref<HTMLElement | null>(null)
const boxWidth = ref(900)
/** The graph is drawn once the box's width is known, so it is laid out once, in the right direction. */
const measured = ref(false)
/** Below this width a left-to-right chain of calls would shrink past reading. */
const NARROW = 600
const direction = computed<GraphDirection>(() => (boxWidth.value < NARROW ? 'TB' : 'LR'))
const graph = computed(() => buildFlowGraph(props.previews, props.tools, direction.value))

const nodes = computed<Node<GraphNode>[]>(() =>
  graph.value.nodes.map((n) => ({
    id: n.id,
    type: 'tool',
    position: { x: n.x, y: n.y },
    data: n,
    draggable: false,
    connectable: false,
    selectable: false,
    focusable: false,
  })),
)
const edges = computed<Edge<GraphEdge>[]>(() =>
  graph.value.edges.map((e) => ({
    id: e.id,
    source: e.source,
    target: e.target,
    type: e.loop ? 'loop' : 'share',
    data: e,
    selectable: false,
    focusable: false,
  })),
)

const MAX_ZOOM = 1.15
const PAD = 12
/** The strip along the bottom kept for the zoom buttons, so they never cover a node. */
const CONTROLS = 40
/** How far the graph is scaled to fit the box's width. */
const scale = computed(() =>
  Math.min(MAX_ZOOM, (boxWidth.value - 2 * PAD) / Math.max(1, graph.value.width)),
)
/** As tall as the graph needs at that scale, within bounds, so it neither floats in space nor shrinks to nothing. */
const height = computed(() => {
  const max = direction.value === 'TB' ? 560 : 480
  return Math.round(
    Math.max(240, Math.min(max, graph.value.height * scale.value + 2 * PAD + CONTROLS)),
  )
})

/** Fits the whole graph, the room its loops and labels take included, in the box. */
function fit() {
  const w = box.value?.clientWidth || boxWidth.value
  const h = height.value
  const g = graph.value
  const room = h - 2 * PAD - CONTROLS
  const zoom = Math.max(
    0.3,
    Math.min(MAX_ZOOM, (w - 2 * PAD) / Math.max(1, g.width), room / Math.max(1, g.height)),
  )
  void setViewport({
    x: (w - g.width * zoom) / 2,
    y: PAD + (room - g.height * zoom) / 2,
    zoom,
  })
}

let observer: ResizeObserver | null = null
onMounted(() => {
  if (box.value) boxWidth.value = box.value.clientWidth || boxWidth.value
  measured.value = true
  if (box.value && typeof ResizeObserver !== 'undefined') {
    observer = new ResizeObserver((entries) => {
      const w = entries[0]?.contentRect.width
      if (w) boxWidth.value = w
      void nextTick(fit)
    })
    observer.observe(box.value)
  }
})
onBeforeUnmount(() => observer?.disconnect())
watch(
  () => `${direction.value}:${graph.value.nodes.map((n) => n.id).join()}`,
  () => void nextTick(fit),
)
</script>

<template>
  <div
    ref="box"
    class="fg"
    :style="{ height: `${height}px` }"
    :data-direction="direction"
    data-testid="flow-graph"
  >
    <svg class="fg-defs" aria-hidden="true" width="0" height="0">
      <defs>
        <marker
          id="fg-arrow"
          viewBox="0 0 10 10"
          refX="8"
          refY="5"
          markerWidth="11"
          markerHeight="11"
          markerUnits="userSpaceOnUse"
          orient="auto-start-reverse"
        >
          <path d="M0,0 L10,5 L0,10 z" class="arrow" />
        </marker>
        <marker
          id="fg-arrow-acts"
          viewBox="0 0 10 10"
          refX="8"
          refY="5"
          markerWidth="11"
          markerHeight="11"
          markerUnits="userSpaceOnUse"
          orient="auto-start-reverse"
        >
          <path d="M0,0 L10,5 L0,10 z" class="arrow acts" />
        </marker>
      </defs>
    </svg>
    <VueFlow
      v-if="measured"
      :id="id"
      :nodes="nodes"
      :edges="edges"
      :nodes-draggable="false"
      :nodes-connectable="false"
      :elements-selectable="false"
      :zoom-on-scroll="false"
      :zoom-on-double-click="false"
      :prevent-scrolling="false"
      :min-zoom="0.3"
      :max-zoom="1.6"
      @nodes-initialized="fit"
    >
      <template #node-tool="p">
        <FlowNode :data="p.data" :vertical="direction === 'TB'" />
      </template>
      <template #edge-share="p">
        <ShareEdge v-bind="p" />
      </template>
      <template #edge-loop="p">
        <LoopEdge v-bind="p" />
      </template>
    </VueFlow>
    <div class="fg-controls">
      <button type="button" aria-label="Zoom in" title="Zoom in" @click="zoomIn()">
        <Plus :size="15" :stroke-width="2" aria-hidden="true" />
      </button>
      <button type="button" aria-label="Zoom out" title="Zoom out" @click="zoomOut()">
        <Minus :size="15" :stroke-width="2" aria-hidden="true" />
      </button>
      <button type="button" aria-label="Fit the graph" title="Fit" @click="fit">
        <Maximize :size="14" :stroke-width="2" aria-hidden="true" />
      </button>
    </div>
  </div>
</template>

<style scoped>
.fg {
  position: relative;
  border-radius: var(--c-radius-control);
  border: 1px solid var(--stretto-border);
  background-color: var(--stretto-bg);
  background-image: radial-gradient(
    color-mix(in oklab, var(--stretto-border-strong) 35%, transparent) 1px,
    transparent 1px
  );
  background-size: 18px 18px;
  overflow: hidden;
}

.fg-defs {
  position: absolute;
}

.arrow {
  fill: var(--stretto-border-strong);
}

.arrow.acts {
  fill: var(--stretto-chart-accent);
}

.fg-controls {
  position: absolute;
  right: 8px;
  bottom: 8px;
  display: flex;
  border-radius: 8px;
  border: 1px solid var(--stretto-border);
  background: var(--stretto-surface);
  box-shadow: var(--c-shadow-sm);
  overflow: hidden;
  z-index: 5;
}

.fg-controls button {
  display: grid;
  place-items: center;
  width: 30px;
  height: 28px;
  border: 0;
  border-right: 1px solid var(--stretto-border);
  background: none;
  color: var(--stretto-text-muted);
}

.fg-controls button:last-child {
  border-right: 0;
}

.fg-controls button:hover {
  background: var(--stretto-surface-2);
  color: var(--stretto-text);
}

.fg :deep(.vue-flow__edge-path) {
  fill: none;
}
</style>

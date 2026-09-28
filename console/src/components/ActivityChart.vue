<script setup lang="ts">
/**
 * Tool calls per day over the last 14 days: the agent's own calls (the
 * baseline, slate) with the reads stretto made on top (petrol), as the brand
 * charts stretto against a baseline. One axis; sessions per day ride in the
 * tooltip and the table, not on a second scale. Hover or focus a day for its
 * numbers; the table view has every value without hovering.
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { ChartColumnStacked, Table2 } from '@lucide/vue'
import type { DayActivity } from '@/api/types'
import { formatCount, formatDay } from '@/lib/format'

const props = defineProps<{ days: DayActivity[] }>()

const root = ref<HTMLElement | null>(null)
const width = ref(640)
const height = ref(232)
const active = ref<number | null>(null)
const view = ref<'chart' | 'table'>('chart')
let observer: ResizeObserver | null = null

onMounted(() => {
  if (!root.value) return
  width.value = root.value.clientWidth || 640
  height.value = Math.max(MIN_H, root.value.clientHeight || MIN_H)
  if (typeof ResizeObserver !== 'undefined') {
    observer = new ResizeObserver((entries) => {
      const box = entries[0]?.contentRect
      if (box?.width) width.value = box.width
      if (box?.height) height.value = Math.max(MIN_H, Math.min(420, box.height))
    })
    observer.observe(root.value)
  }
})
onBeforeUnmount(() => observer?.disconnect())

/** The plot fills its card, from this height up. */
const MIN_H = 232
const M = { top: 10, right: 6, bottom: 28, left: 40 }

/** A round top for the axis: 1, 2, 2.5 or 5 times a power of ten. */
function niceMax(max: number): { top: number; step: number } {
  if (max <= 0) return { top: 4, step: 1 }
  const rough = max / 4
  const pow = 10 ** Math.floor(Math.log10(rough))
  const step = [1, 2, 2.5, 5, 10].map((m) => m * pow).find((s) => s >= rough) ?? 10 * pow
  const clean = step < 1 ? 1 : step
  return { top: Math.ceil(max / clean) * clean, step: clean }
}

const total = (d: DayActivity) => d.tool_calls + d.flow_lookups
const scale = computed(() => niceMax(Math.max(0, ...props.days.map(total))))
const plotW = computed(() => Math.max(120, width.value - M.left - M.right))
const H = computed(() => height.value)
const plotH = computed(() => H.value - M.top - M.bottom)
const band = computed(() => plotW.value / Math.max(1, props.days.length))
const barW = computed(() => Math.max(6, Math.min(24, band.value * 0.56)))
const empty = computed(() => props.days.every((d) => total(d) === 0))

const ticks = computed(() => {
  const out: { value: number; y: number }[] = []
  // With nothing to plot, only the baseline: no scale to read.
  if (empty.value) return [{ value: 0, y: M.top + plotH.value }]
  for (let v = 0; v <= scale.value.top + 1e-9; v += scale.value.step) {
    out.push({ value: v, y: M.top + plotH.value - (v / scale.value.top) * plotH.value })
  }
  return out
})

/** A bar segment with rounded top corners (the top of the stack), square at the baseline. */
function topRounded(x: number, y: number, w: number, h: number, r: number): string {
  const rr = Math.min(r, h, w / 2)
  return `M${x},${y + h}V${y + rr}Q${x},${y} ${x + rr},${y}H${x + w - rr}Q${x + w},${y} ${x + w},${y + rr}V${y + h}Z`
}

const bars = computed(() =>
  props.days.map((d, i) => {
    const x = M.left + band.value * i + (band.value - barW.value) / 2
    const h = (n: number) => (n > 0 ? Math.max(2, (n / scale.value.top) * plotH.value) : 0)
    const agentH = h(d.tool_calls)
    const lookH = h(d.flow_lookups)
    const base = M.top + plotH.value
    const agentY = base - agentH
    const gap = agentH > 0 && lookH > 0 ? 2 : 0
    const lookY = agentY - gap - lookH
    return {
      day: d,
      x,
      agent:
        agentH > 0
          ? lookH > 0
            ? `M${x},${base}V${agentY}H${x + barW.value}V${base}Z`
            : topRounded(x, agentY, barW.value, agentH, 4)
          : '',
      lookups: lookH > 0 ? topRounded(x, lookY, barW.value, lookH, 4) : '',
      top: lookH > 0 ? lookY : agentY,
    }
  }),
)

/** Label every day that fits, always the last. */
const labelEvery = computed(() => Math.max(1, Math.ceil(52 / band.value)))
function labelled(i: number): boolean {
  return (props.days.length - 1 - i) % labelEvery.value === 0
}

const tip = computed(() => {
  if (active.value === null) return null
  const bar = bars.value[active.value]
  if (!bar) return null
  const left = bar.x + barW.value / 2
  const flip = left > width.value - 190
  return { bar, style: { left: `${left}px`, top: `${Math.max(0, bar.top - 8)}px` }, flip }
})

const summary = computed(() => {
  const calls = props.days.reduce((s, d) => s + d.tool_calls, 0)
  const lookups = props.days.reduce((s, d) => s + d.flow_lookups, 0)
  return `Tool calls per day over the last ${props.days.length} days: ${formatCount(calls)} by the agent and ${formatCount(lookups)} read ahead by stretto.`
})

function onKey(event: KeyboardEvent) {
  const n = props.days.length
  if (!n) return
  if (event.key === 'ArrowRight' || event.key === 'ArrowLeft') {
    event.preventDefault()
    const step = event.key === 'ArrowRight' ? 1 : -1
    active.value =
      active.value === null
        ? step > 0
          ? 0
          : n - 1
        : Math.max(0, Math.min(n - 1, active.value + step))
  } else if (event.key === 'Home') {
    active.value = 0
  } else if (event.key === 'End') {
    active.value = n - 1
  } else if (event.key === 'Escape') {
    active.value = null
  }
}
</script>

<template>
  <figure class="activity">
    <div class="activity-top">
      <ul class="legend" aria-label="Legend">
        <li><span class="key key-agent" aria-hidden="true" />Agent’s calls</li>
        <li><span class="key key-lookups" aria-hidden="true" />Read ahead by stretto</li>
      </ul>
      <button
        type="button"
        class="view-toggle"
        :aria-pressed="view === 'table'"
        @click="view = view === 'chart' ? 'table' : 'chart'"
      >
        <component
          :is="view === 'chart' ? Table2 : ChartColumnStacked"
          :size="14"
          :stroke-width="1.9"
          aria-hidden="true"
        />
        {{ view === 'chart' ? 'Table' : 'Chart' }}
      </button>
    </div>

    <div v-show="view === 'chart'" ref="root" class="plot">
      <div
        class="plot-focus"
        tabindex="0"
        role="img"
        :aria-label="`${summary} Use the arrow keys to read each day.`"
        @keydown="onKey"
        @blur="active = null"
      >
        <svg :width="width" :height="H" :viewBox="`0 0 ${width} ${H}`" aria-hidden="true">
          <g class="grid">
            <line
              v-for="t in ticks"
              :key="t.value"
              :x1="M.left"
              :x2="width - M.right"
              :y1="t.y"
              :y2="t.y"
              :class="{ base: t.value === 0 }"
            />
          </g>
          <g class="y-labels">
            <text v-for="t in ticks" :key="t.value" :x="M.left - 8" :y="t.y + 4" text-anchor="end">
              {{ formatCount(t.value) }}
            </text>
          </g>
          <g class="bars" :class="{ dimmed: active !== null }">
            <g v-for="(bar, i) in bars" :key="bar.day.day" :class="{ on: active === i }">
              <path v-if="bar.agent" :d="bar.agent" class="bar-agent" />
              <path v-if="bar.lookups" :d="bar.lookups" class="bar-lookups" />
            </g>
          </g>
          <g class="x-labels">
            <template v-for="(bar, i) in bars" :key="bar.day.day">
              <text v-if="labelled(i)" :x="bar.x + barW / 2" :y="H - 8" text-anchor="middle">
                {{ formatDay(bar.day.day, false) }}
              </text>
            </template>
          </g>
          <g class="hits">
            <rect
              v-for="(bar, i) in bars"
              :key="bar.day.day"
              :x="M.left + band * i"
              :y="M.top"
              :width="band"
              :height="plotH"
              @pointerenter="active = i"
              @pointerleave="active = null"
            />
          </g>
          <text
            v-if="empty"
            class="empty-note"
            :x="M.left + plotW / 2"
            :y="M.top + plotH / 2"
            text-anchor="middle"
          >
            No tool calls in these 14 days
          </text>
        </svg>
      </div>
      <div
        v-if="tip"
        class="tip"
        :class="{ flip: tip.flip }"
        :style="tip.style"
        role="presentation"
      >
        <p class="tip-day">{{ formatDay(tip.bar.day.day) }}</p>
        <p class="tip-row">
          <span class="line line-lookups" /><strong>{{
            formatCount(tip.bar.day.flow_lookups)
          }}</strong>
          read ahead by stretto
        </p>
        <p class="tip-row">
          <span class="line line-agent" /><strong>{{ formatCount(tip.bar.day.tool_calls) }}</strong>
          agent’s calls
        </p>
        <p class="tip-row tip-sessions">
          <strong>{{ formatCount(tip.bar.day.sessions) }}</strong>
          {{ tip.bar.day.sessions === 1 ? 'session' : 'sessions' }}
        </p>
      </div>
    </div>

    <div v-if="view === 'table'" class="table-wrap">
      <table class="table table-compact">
        <caption class="sr-only">
          {{
            summary
          }}
        </caption>
        <thead>
          <tr>
            <th scope="col">Day</th>
            <th scope="col" class="num">Sessions</th>
            <th scope="col" class="num">Agent’s calls</th>
            <th scope="col" class="num">Read ahead by stretto</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="d in [...days].reverse()" :key="d.day">
            <td>{{ formatDay(d.day) }}</td>
            <td class="num">{{ formatCount(d.sessions) }}</td>
            <td class="num">{{ formatCount(d.tool_calls) }}</td>
            <td class="num">{{ formatCount(d.flow_lookups) }}</td>
          </tr>
        </tbody>
      </table>
    </div>
  </figure>
</template>

<style scoped>
.activity {
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-width: 0;
  flex: 1 1 auto;
}

.activity-top {
  display: flex;
  align-items: center;
  gap: 12px;
}

.legend {
  display: flex;
  flex-wrap: wrap;
  gap: 6px 16px;
  list-style: none;
  flex: 1;
  font-size: 12.5px;
  color: var(--stretto-text-muted);
}

.legend li {
  display: inline-flex;
  align-items: center;
  gap: 7px;
}

.key {
  width: 10px;
  height: 10px;
  border-radius: 2px;
}

.key-agent {
  background: var(--stretto-chart-baseline);
}

.key-lookups {
  background: var(--stretto-chart-accent);
}

.view-toggle {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 28px;
  padding: 0 9px;
  border: 1px solid var(--stretto-border);
  border-radius: 7px;
  background: var(--stretto-surface);
  color: var(--stretto-text-muted);
  font-size: 12.5px;
  font-weight: 500;
}

.view-toggle:hover {
  color: var(--stretto-text);
  background: var(--stretto-surface-2);
}

.plot {
  position: relative;
  flex: 1 1 auto;
  width: 100%;
  min-width: 0;
  min-height: 232px;
}

.plot-focus {
  position: absolute;
  inset: 0;
}

.plot-focus {
  border-radius: 8px;
}

.table-wrap {
  flex: 1 1 auto;
}

.plot svg {
  display: block;
  overflow: visible;
}

.grid line {
  stroke: var(--stretto-border);
  stroke-width: 1;
  shape-rendering: crispEdges;
}

.grid line.base {
  stroke: color-mix(in oklab, var(--stretto-border-strong) 55%, var(--stretto-border));
}

.y-labels text,
.x-labels text {
  font-size: 11.5px;
  fill: var(--stretto-text-subtle);
  font-variant-numeric: tabular-nums;
}

.bar-agent {
  fill: var(--stretto-chart-baseline);
}

.bar-lookups {
  fill: var(--stretto-chart-accent);
}

.bars g {
  transition: opacity 0.12s var(--c-ease);
}

.bars.dimmed g:not(.on) {
  opacity: 0.42;
}

.hits rect {
  fill: transparent;
}

.empty-note {
  font-size: 13px;
  fill: var(--stretto-text-subtle);
}

.tip {
  position: absolute;
  z-index: 5;
  min-width: 176px;
  padding: 10px 12px;
  border-radius: 10px;
  background: var(--stretto-surface);
  border: 1px solid var(--stretto-border);
  box-shadow: var(--c-shadow-md);
  transform: translate(-50%, -100%);
  pointer-events: none;
  font-size: 12.5px;
}

.tip.flip {
  transform: translate(-100%, -100%);
}

.tip-day {
  margin-bottom: 6px;
  font-weight: 600;
  color: var(--stretto-text);
}

.tip-row {
  display: flex;
  align-items: center;
  gap: 7px;
  color: var(--stretto-text-muted);
  white-space: nowrap;
}

.tip-row + .tip-row {
  margin-top: 3px;
}

.tip-row strong {
  color: var(--stretto-text);
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}

.tip-sessions {
  margin-top: 6px !important;
  padding-top: 6px;
  border-top: 1px solid var(--stretto-border);
}

.line {
  width: 12px;
  height: 3px;
  border-radius: 2px;
}

.line-agent {
  background: var(--stretto-chart-baseline);
}

.line-lookups {
  background: var(--stretto-chart-accent);
}
</style>

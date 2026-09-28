<script setup lang="ts">
/**
 * Every decision the flow logged in the session, under the agent's call it
 * followed, with its value against the threshold.
 */
import { computed } from 'vue'
import ProbBar from '../ui/ProbBar.vue'
import UiBadge from '../ui/UiBadge.vue'
import type { FlowDecision, SessionDetail } from '@/api/types'
import { decisionValue } from '@/lib/timeline'
import { compactJson } from '@/lib/json'
import { formatProb } from '@/lib/format'

const props = defineProps<{ detail: SessionDetail; threshold: number }>()

interface Group {
  after: string
  tool: string
  decisions: FlowDecision[]
}

/** The decisions in log order, grouped by the call they followed. */
const groups = computed<Group[]>(() => {
  const tools = new Map(props.detail.calls.map((c) => [c.id, c.tool]))
  const out: Group[] = []
  for (const d of props.detail.decisions) {
    const last = out[out.length - 1]
    if (last && last.after === d.after) last.decisions.push(d)
    else out.push({ after: d.after, tool: tools.get(d.after) ?? '?', decisions: [d] })
  }
  return out
})
</script>

<template>
  <div class="table-wrap">
    <table class="table table-stack dt">
      <thead>
        <tr>
          <th scope="col">Site</th>
          <th scope="col">Decision</th>
          <th scope="col">Arguments</th>
          <th scope="col" class="num">p</th>
          <th scope="col" class="num">Binding</th>
          <th scope="col">Against the threshold</th>
        </tr>
      </thead>
      <tbody v-for="(g, gi) in groups" :key="`${g.after}-${gi}`">
        <tr class="group">
          <th scope="rowgroup" colspan="6">
            <span class="cell">
              <span class="group-label">After the agent’s call</span>
              <span class="tool">{{ g.tool }}</span>
              <span class="caption mono">#{{ g.after }}</span>
            </span>
          </th>
        </tr>
        <tr v-for="d in g.decisions" :key="d.address" class="decision">
          <td class="nowrap">
            <span class="cell">
              <span class="mono small">{{ d.address }}</span>
              <span class="caption">after</span>
              <span class="tool">{{ d.site }}</span>
            </span>
          </td>
          <td class="what">
            <span v-if="d.action === 'lookup'" class="cell">
              <UiBadge :tone="d.shadow ? 'dashed' : 'accent'">{{
                d.shadow ? 'would look up' : 'looks up'
              }}</UiBadge>
              <span class="tool">{{ d.tool }}</span>
            </span>
            <UiBadge v-else tone="neutral">hands back</UiBadge>
            <p v-if="d.reason" class="reason">{{ d.reason }}</p>
          </td>
          <td class="args" data-label="Arguments" :class="{ 'stack-hide': d.arguments === null }">
            <span
              v-if="d.arguments !== null"
              class="args-text mono"
              :title="compactJson(d.arguments, 4000)"
              >{{ compactJson(d.arguments, 200) }}</span
            >
            <span v-else class="subtle">—</span>
          </td>
          <td class="num inline" data-label="p">{{ formatProb(d.prob) }}</td>
          <td class="num inline" data-label="Binding">{{ formatProb(d.binding) }}</td>
          <td class="inline">
            <ProbBar
              v-if="decisionValue(d) !== null"
              :value="decisionValue(d)"
              :threshold="threshold"
              :acts="d.action === 'lookup'"
              :width="72"
            />
            <span v-else class="subtle">no lookup weighed</span>
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<style scoped>
.dt td {
  vertical-align: middle;
}

/* Words and names in a cell, spaced by the layout, not by whitespace in the template. */
.cell {
  display: inline-flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 4px 6px;
}

.group th {
  padding-top: 9px;
  padding-bottom: 9px;
  background: var(--stretto-surface-2);
  color: var(--stretto-text);
  font-size: 13px;
  white-space: normal;
}

.dt tbody + tbody .group th {
  border-top: 1px solid var(--stretto-border);
}

.group-label {
  color: var(--stretto-text-subtle);
  font-weight: 500;
}

.small {
  font-size: 11.5px;
  color: var(--stretto-text-subtle);
}

.reason {
  margin-top: 4px;
  max-width: 320px;
  font-size: 12.5px;
  color: var(--stretto-text-muted);
}

.args-text {
  display: block;
  max-width: 220px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
  color: var(--stretto-text-muted);
}

@media (max-width: 640px) {
  .dt .group {
    padding: 0;
  }

  .dt .group th,
  .dt tbody + tbody .group th {
    padding: 9px 16px;
    border-top: 0;
  }

  .args-text {
    max-width: 100%;
  }
}
</style>

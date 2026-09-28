<script setup lang="ts">
/** One of the agent's calls: what it asked and what came back, and under it what the flow read ahead. */
import { computed, inject, ref, watch, type Ref } from 'vue'
import { ArrowRight, ChevronRight, CircleX, Waypoints } from '@lucide/vue'
import KindBadge from '../KindBadge.vue'
import UiBadge from '../ui/UiBadge.vue'
import CallPayload from './CallPayload.vue'
import DecisionRow from './DecisionRow.vue'
import type { CallItem } from '@/lib/timeline'
import type { SessionMode } from '@/api/types'
import { compactJson } from '@/lib/json'
import { formatDuration, formatOffset } from '@/lib/format'

const props = defineProps<{ item: CallItem; threshold: number; mode: SessionMode }>()
const expandAll = inject<Ref<{ open: boolean; tick: number }> | null>('timeline-expand', null)
const open = ref(false)
if (expandAll)
  watch(
    () => expandAll.value.tick,
    () => (open.value = expandAll.value.open),
  )

const call = computed(() => props.item.call)
const made = computed(
  () => props.item.decisions.filter((d) => d.lookup).length + props.item.extraLookups.length,
)
const shadow = computed(() => props.item.decisions.some((d) => d.decision.shadow))
const result = computed(() => {
  const c = call.value
  if (c.ok === null) return 'no answer'
  return compactJson(c.result_json ?? c.result_text ?? '', 110)
})
const heading = computed(() => {
  if (shadow.value) {
    return props.item.decisions.some((d) => d.decision.action === 'lookup')
      ? 'In shadow: what the flow would have read ahead'
      : 'In shadow: the flow’s decision'
  }
  if (made.value)
    return `Read ahead by stretto: ${made.value} ${made.value === 1 ? 'lookup' : 'lookups'}`
  return 'The flow’s decision'
})
</script>

<template>
  <article
    class="call"
    :class="{ failed: call.ok === false, pending: call.ok === null }"
    :data-testid="`call-${call.id}`"
  >
    <header class="call-head">
      <button type="button" class="call-toggle" :aria-expanded="open" @click="open = !open">
        <ChevronRight
          class="caret"
          :class="{ open }"
          :size="16"
          :stroke-width="2"
          aria-hidden="true"
        />
        <span class="tool call-tool">{{ call.tool }}</span>
        <span class="sr-only">{{ open ? 'Hide' : 'Show' }} the arguments and result</span>
      </button>
      <KindBadge v-if="call.kind !== 'read'" :kind="call.kind" />
      <UiBadge v-if="call.ok === false" tone="danger" :icon="CircleX">Failed</UiBadge>
      <UiBadge v-else-if="call.ok === null" tone="warn">No answer</UiBadge>
      <span class="spacer" />
      <span
        class="call-time caption num"
        :title="`Answered in ${formatDuration(call.latency_ms)}`"
        >{{ formatDuration(call.latency_ms) }}</span
      >
    </header>
    <p v-if="!open" class="call-line">
      <span class="mono call-args">{{ compactJson(call.arguments, 90) }}</span>
      <ArrowRight class="call-arrow" :size="13" :stroke-width="2" aria-hidden="true" />
      <span class="mono call-result" :class="{ 'text-danger': call.ok === false }">{{
        result
      }}</span>
    </p>
    <div v-else class="call-payload">
      <CallPayload :call="call" />
    </div>

    <section
      v-if="item.decisions.length || item.extraLookups.length"
      class="ahead"
      :aria-label="heading"
    >
      <p class="ahead-title">
        <Waypoints :size="14" :stroke-width="2" aria-hidden="true" />
        {{ heading }}
        <span
          v-if="item.run?.surprise"
          class="caption num ahead-surprise"
          title="How unexpected the server's answers were under the flow's statistics"
        >
          surprise {{ item.run.surprise.toFixed(2) }} nats
        </span>
      </p>
      <ul class="ahead-list">
        <DecisionRow v-for="d in item.decisions" :key="d.index" :item="d" :threshold="threshold" />
        <DecisionRow
          v-for="l in item.extraLookups"
          :key="l.id"
          :item="{
            index: -1,
            lookup: l,
            decision: {
              after: call.id,
              address: 'lookup',
              site: call.tool,
              action: 'lookup',
              tool: l.tool,
              arguments: l.arguments,
              prob: null,
              probs: {},
              binding: null,
              reason: null,
              shadow: false,
              ms: null,
            },
          }"
          :threshold="threshold"
        />
      </ul>
    </section>
    <p class="sr-only">{{ formatOffset(call.t_ms) }}</p>
  </article>
</template>

<style scoped>
.call {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px 14px;
  border-radius: 12px;
  border: 1px solid var(--stretto-border);
  background: var(--stretto-surface);
  box-shadow: var(--c-shadow-sm);
  min-width: 0;
}

.call.failed {
  border-color: var(--c-danger-border);
}

.call-head {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
  min-width: 0;
}

.call-toggle {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  margin-left: -6px;
  padding: 2px 6px;
  border: 0;
  border-radius: 7px;
  background: none;
  text-align: left;
}

.call-toggle:hover {
  background: var(--c-hover);
}

.caret {
  flex: none;
  color: var(--stretto-text-subtle);
  transition: transform 0.12s var(--c-ease);
}

.caret.open {
  transform: rotate(90deg);
}

.call-tool {
  font-size: 13.5px;
  font-weight: 700;
  overflow-wrap: anywhere;
}

.call-time {
  white-space: nowrap;
}

.call-line {
  display: flex;
  align-items: baseline;
  gap: 8px;
  min-width: 0;
  padding-left: 22px;
  font-size: 12.5px;
}

.call-args,
.call-result {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
}

.call-args {
  flex: 0 1 auto;
  max-width: 45%;
  color: var(--stretto-text-muted);
}

.call-result {
  flex: 1 1 0;
  color: var(--stretto-text);
}

.call-arrow {
  flex: none;
  align-self: center;
  color: var(--stretto-text-subtle);
}

.call-payload {
  padding-left: 22px;
}

.ahead {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin: 4px 0 0 22px;
  padding: 10px;
  border-radius: 10px;
  background: color-mix(in oklab, var(--stretto-accent-soft) 55%, var(--stretto-surface));
}

.ahead-title {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 6px;
  font-size: 12.5px;
  font-weight: 600;
  color: var(--stretto-accent);
}

.ahead-surprise {
  margin-left: auto;
  font-weight: 400;
}

.ahead-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

@media (max-width: 720px) {
  .call {
    padding: 12px;
  }

  .call-line {
    flex-direction: column;
    gap: 2px;
    padding-left: 0;
  }

  .call-args {
    max-width: 100%;
  }

  .call-arrow {
    display: none;
  }

  .call-payload,
  .ahead {
    padding-left: 0;
    margin-left: 0;
  }

  .ahead {
    padding: 8px;
  }
}
</style>

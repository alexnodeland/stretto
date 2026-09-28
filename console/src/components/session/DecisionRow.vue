<script setup lang="ts">
/**
 * One of the flow's decisions after a call: a lookup it made (petrol), one
 * it would have made in shadow (a dashed petrol outline), or handing back,
 * with the decision's value against the threshold and the lookup's call.
 */
import { computed, inject, ref, watch, type Ref } from 'vue'
import { ChevronRight, CornerDownRight, CornerUpLeft } from '@lucide/vue'
import ProbBar from '../ui/ProbBar.vue'
import CallPayload from './CallPayload.vue'
import type { DecisionItem } from '@/lib/timeline'
import { decisionValue } from '@/lib/timeline'
import { compactJson } from '@/lib/json'
import { formatDuration, formatProb } from '@/lib/format'

const props = defineProps<{ item: DecisionItem; threshold: number }>()
const expandAll = inject<Ref<{ open: boolean; tick: number }> | null>('timeline-expand', null)
const open = ref(false)
if (expandAll)
  watch(
    () => expandAll.value.tick,
    () => (open.value = expandAll.value.open),
  )

const d = computed(() => props.item.decision)
const kind = computed(() =>
  d.value.action === 'hand_back'
    ? 'back'
    : d.value.shadow || !props.item.lookup
      ? 'shadow'
      : 'made',
)
const value = computed(() => decisionValue(d.value))
const factors = computed(() =>
  d.value.prob !== null && d.value.binding !== null
    ? `${formatProb(d.value.prob)} × ${formatProb(d.value.binding)} binding`
    : null,
)
</script>

<template>
  <li class="dr" :class="`dr-${kind}`" :data-testid="`decision-${kind}`">
    <div class="dr-row">
      <span class="dr-icon" aria-hidden="true">
        <CornerUpLeft v-if="kind === 'back'" :size="15" :stroke-width="2" />
        <CornerDownRight v-else :size="15" :stroke-width="2" />
      </span>
      <div class="dr-main">
        <p class="dr-title">
          <template v-if="kind === 'made'"
            >Looked up <span class="tool">{{ d.tool }}</span></template
          >
          <template v-else-if="kind === 'shadow'"
            >Would look up <span class="tool">{{ d.tool }}</span></template
          >
          <template v-else>Handed back</template>
          <span v-if="d.arguments !== null" class="dr-args mono">{{
            compactJson(d.arguments, 90)
          }}</span>
        </p>
        <p class="dr-sub">
          <span class="mono">{{ d.address }}</span>
          after <span class="mono">{{ d.site }}</span>
          <template v-if="d.reason"> · {{ d.reason }}</template>
          <template v-if="kind === 'shadow'"> · in shadow, nothing looked up</template>
          <template v-if="item.lookup">
            · {{ item.lookup.ok === false ? 'failed' : 'answered' }} in
            {{ formatDuration(item.lookup.latency_ms) }}
          </template>
        </p>
      </div>
      <div class="dr-prob">
        <ProbBar
          v-if="value !== null"
          :value="value"
          :threshold="threshold"
          :acts="d.action === 'lookup'"
          :width="84"
        />
        <span v-if="factors" class="dr-factors num">{{ factors }}</span>
      </div>
      <button
        v-if="item.lookup"
        type="button"
        class="dr-toggle"
        :aria-expanded="open"
        :aria-label="`${open ? 'Hide' : 'Show'} the lookup's arguments and result`"
        @click="open = !open"
      >
        <ChevronRight :class="{ open }" :size="16" :stroke-width="2" aria-hidden="true" />
      </button>
      <span v-else class="dr-toggle-space" aria-hidden="true" />
    </div>
    <div v-if="open && item.lookup" class="dr-payload">
      <CallPayload :call="item.lookup" />
    </div>
  </li>
</template>

<style scoped>
.dr {
  list-style: none;
  border-radius: 10px;
  border: 1px solid var(--stretto-border);
  background: var(--stretto-surface);
}

.dr-made {
  border-color: color-mix(in oklab, var(--stretto-accent-graphic) 55%, var(--stretto-border));
  box-shadow: inset 3px 0 0 var(--stretto-accent-graphic);
}

.dr-shadow {
  border: 1px dashed var(--stretto-accent-graphic);
  background: transparent;
}

.dr-back {
  background: transparent;
  border-style: solid;
}

.dr-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 10px 9px 12px;
  min-width: 0;
}

.dr-icon {
  display: grid;
  place-items: center;
  flex: none;
  width: 24px;
  height: 24px;
  border-radius: 7px;
}

.dr-made .dr-icon,
.dr-shadow .dr-icon {
  background: var(--stretto-accent-soft);
  color: var(--stretto-accent);
}

.dr-back .dr-icon {
  background: var(--stretto-surface-2);
  color: var(--stretto-text-subtle);
}

.dr-main {
  flex: 1 1 auto;
  min-width: 0;
}

.dr-title {
  display: flex;
  align-items: baseline;
  flex-wrap: wrap;
  gap: 2px 8px;
  font-size: 13.5px;
  font-weight: 500;
}

.dr-back .dr-title {
  color: var(--stretto-text-muted);
}

.dr-args {
  font-size: 12px;
  font-weight: 400;
  color: var(--stretto-text-muted);
  overflow-wrap: anywhere;
}

.dr-sub {
  margin-top: 2px;
  font-size: 12px;
  color: var(--stretto-text-subtle);
  overflow-wrap: anywhere;
}

.dr-sub .mono {
  font-size: 11.5px;
}

.dr-prob {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 3px;
  flex: none;
}

.dr-factors {
  font-size: 11.5px;
  color: var(--stretto-text-subtle);
}

.dr-toggle {
  display: grid;
  place-items: center;
  flex: none;
  width: 28px;
  height: 28px;
  border: 0;
  border-radius: 7px;
  background: none;
  color: var(--stretto-text-subtle);
}

.dr-toggle-space {
  flex: none;
  width: 28px;
}

.dr-toggle:hover {
  background: var(--c-hover);
  color: var(--stretto-text);
}

.dr-toggle svg {
  transition: transform 0.12s var(--c-ease);
}

.dr-toggle svg.open {
  transform: rotate(90deg);
}

.dr-payload {
  padding: 0 12px 12px 46px;
}

@media (max-width: 720px) {
  .dr-row {
    flex-wrap: wrap;
  }

  .dr-main {
    flex-basis: calc(100% - 80px);
  }

  .dr-prob {
    order: 3;
    flex-basis: 100%;
    align-items: flex-start;
    padding-left: 34px;
  }

  .dr-payload {
    padding-left: 12px;
  }
}
</style>

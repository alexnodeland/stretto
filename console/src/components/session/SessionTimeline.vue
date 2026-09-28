<script setup lang="ts">
/** The session turn by turn: the conversation where it fell, and each turn's calls with what the flow did after them. */
import { computed, provide, ref } from 'vue'
import { ChevronsUpDown } from '@lucide/vue'
import ChatBubble from './ChatBubble.vue'
import TimelineCall from './TimelineCall.vue'
import type { SessionDetail } from '@/api/types'
import { buildTimeline } from '@/lib/timeline'
import { formatOffset } from '@/lib/format'

const props = defineProps<{ detail: SessionDetail; threshold: number }>()
const entries = computed(() => buildTimeline(props.detail))

const expand = ref({ open: false, tick: 0 })
provide('timeline-expand', expand)
function toggleAll() {
  expand.value = { open: !expand.value.open, tick: expand.value.tick + 1 }
}
</script>

<template>
  <div class="tl">
    <div class="tl-tools">
      <ul class="tl-legend" aria-label="Legend">
        <li><span class="key key-agent" aria-hidden="true" />The agent’s call</li>
        <li><span class="key key-made" aria-hidden="true" />Read ahead by stretto</li>
        <li><span class="key key-shadow" aria-hidden="true" />Would read ahead (shadow)</li>
        <li><span class="key key-back" aria-hidden="true" />Handed back</li>
      </ul>
      <button type="button" class="tl-expand" @click="toggleAll">
        <ChevronsUpDown :size="14" :stroke-width="2" aria-hidden="true" />
        {{ expand.open ? 'Collapse all' : 'Expand all' }}
      </button>
    </div>

    <ol v-if="entries.length" class="tl-list">
      <li v-for="(entry, i) in entries" :key="i" class="tl-entry" :class="`tl-${entry.type}`">
        <div class="tl-gutter" aria-hidden="true">
          <span class="tl-dot" />
        </div>
        <div class="tl-body">
          <ChatBubble v-if="entry.type === 'message'" :message="entry.message" />
          <template v-else>
            <p class="tl-turn-label">
              <span class="tl-turn-name">Turn {{ entry.turn.index + 1 }}</span>
              <span class="caption num">{{ formatOffset(entry.turn.start_ms) }}</span>
              <span v-if="entry.calls.length > 1" class="caption"
                >· {{ entry.calls.length }} calls in parallel</span
              >
            </p>
            <div class="tl-calls">
              <TimelineCall
                v-for="c in entry.calls"
                :key="c.call.id"
                :item="c"
                :threshold="threshold"
                :mode="detail.summary.mode"
              />
            </div>
          </template>
        </div>
      </li>
    </ol>
    <p v-else class="muted">
      This session has no calls and no conversation: the host connected and listed the tools, and
      nothing more.
    </p>
  </div>
</template>

<style scoped>
.tl {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.tl-tools {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 10px 16px;
}

.tl-legend {
  display: flex;
  flex-wrap: wrap;
  gap: 6px 16px;
  flex: 1;
  list-style: none;
  font-size: 12.5px;
  color: var(--stretto-text-muted);
}

.tl-legend li {
  display: inline-flex;
  align-items: center;
  gap: 7px;
}

.key {
  width: 14px;
  height: 10px;
  border-radius: 3px;
}

.key-agent {
  background: var(--stretto-surface);
  border: 1px solid var(--stretto-border-strong);
}

.key-made {
  background: var(--stretto-accent-soft);
  border: 1px solid var(--stretto-accent-graphic);
  box-shadow: inset 3px 0 0 var(--stretto-accent-graphic);
}

.key-shadow {
  border: 1px dashed var(--stretto-accent-graphic);
}

.key-back {
  border: 1px solid var(--stretto-border-strong);
  background: var(--stretto-surface-2);
}

.tl-expand {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 10px;
  border: 1px solid var(--stretto-border);
  border-radius: 8px;
  background: var(--stretto-surface);
  color: var(--stretto-text-muted);
  font-size: 12.5px;
  font-weight: 500;
}

.tl-expand:hover {
  color: var(--stretto-text);
  background: var(--stretto-surface-2);
}

.tl-list {
  list-style: none;
  display: flex;
  flex-direction: column;
}

.tl-entry {
  position: relative;
  display: grid;
  grid-template-columns: 28px minmax(0, 1fr);
  gap: 0 12px;
  padding-bottom: 16px;
}

.tl-gutter {
  position: relative;
  display: flex;
  justify-content: center;
}

.tl-gutter::before {
  content: '';
  position: absolute;
  top: 0;
  bottom: -16px;
  width: 2px;
  border-radius: 1px;
  background: var(--stretto-border);
}

.tl-entry:first-child .tl-gutter::before {
  top: 12px;
}

.tl-entry:last-child .tl-gutter::before {
  bottom: auto;
  height: 12px;
}

.tl-dot {
  position: relative;
  z-index: 1;
  width: 12px;
  height: 12px;
  margin-top: 6px;
  border-radius: 50%;
  background: var(--stretto-surface);
  border: 2px solid var(--stretto-border-strong);
}

.tl-turn .tl-dot {
  background: var(--stretto-text);
  border-color: var(--stretto-text);
  box-shadow: 0 0 0 3px var(--stretto-bg);
}

.tl-body {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}

.tl-message .tl-body {
  padding-top: 1px;
}

.tl-turn-label {
  display: flex;
  align-items: baseline;
  gap: 8px;
  margin-top: 2px;
}

.tl-turn-name {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--stretto-text);
}

.tl-calls {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

@media (max-width: 720px) {
  .tl-entry {
    grid-template-columns: 16px minmax(0, 1fr);
    gap: 0 8px;
  }

  .tl-dot {
    width: 10px;
    height: 10px;
  }
}
</style>

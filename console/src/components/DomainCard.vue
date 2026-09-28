<script setup lang="ts">
/** A domain at a glance: its sessions by mode, its flow and server, and when it was last used. */
import { computed } from 'vue'
import { ArrowUpRight, Server, Workflow } from '@lucide/vue'
import type { DomainSummary } from '@/api/types'
import { formatCount, plural } from '@/lib/format'
import UiRelTime from './ui/UiRelTime.vue'

const props = defineProps<{ domain: DomainSummary }>()

const segments = computed(() => {
  const m = props.domain.modes
  const total = Math.max(1, m.recorded + m.shadow + m.served)
  return [
    { key: 'served', label: 'served', n: m.served, share: m.served / total },
    { key: 'shadow', label: 'in shadow', n: m.shadow, share: m.shadow / total },
    { key: 'recorded', label: 'recorded', n: m.recorded, share: m.recorded / total },
  ].filter((s) => s.n > 0)
})
</script>

<template>
  <article class="dc card">
    <header class="dc-head">
      <RouterLink :to="{ name: 'sessions', query: { domain: domain.name } }" class="dc-name mono">
        {{ domain.name }}
        <ArrowUpRight :size="14" :stroke-width="2" aria-hidden="true" />
      </RouterLink>
      <span class="caption">last <UiRelTime :ms="domain.last_session_unix_ms" /></span>
    </header>

    <p class="dc-count">
      <strong>{{ formatCount(domain.sessions) }}</strong>
      {{ domain.sessions === 1 ? 'session' : 'sessions' }}
    </p>
    <div
      class="dc-bar"
      role="img"
      :aria-label="segments.map((s) => `${s.n} ${s.label}`).join(', ')"
    >
      <span
        v-for="s in segments"
        :key="s.key"
        class="dc-seg"
        :class="`seg-${s.key}`"
        :style="{ flexGrow: s.share }"
      />
    </div>
    <ul class="dc-legend">
      <li v-for="s in segments" :key="s.key">
        <span class="dc-key" :class="`seg-${s.key}`" aria-hidden="true" />{{ formatCount(s.n) }}
        {{ s.label }}
      </li>
    </ul>

    <dl class="dc-links">
      <div>
        <dt>
          <Workflow :size="14" :stroke-width="1.9" aria-hidden="true" /><span class="sr-only"
            >Flows</span
          >
        </dt>
        <dd>
          <template v-if="domain.flows.length">
            <RouterLink
              v-for="f in domain.flows"
              :key="f"
              :to="{ name: 'flow', params: { key: f } }"
              class="mono"
              >{{ f }}</RouterLink
            >
          </template>
          <RouterLink
            v-else
            :to="{ name: 'job-new', query: { kind: 'learn', domain: domain.name } }"
            >Learn a flow</RouterLink
          >
        </dd>
      </div>
      <div>
        <dt>
          <Server :size="14" :stroke-width="1.9" aria-hidden="true" /><span class="sr-only"
            >Servers</span
          >
        </dt>
        <dd>
          <template v-if="domain.servers.length">
            <RouterLink
              v-for="s in domain.servers"
              :key="s"
              :to="{ name: 'server', params: { name: s } }"
              class="mono"
              >{{ s }}</RouterLink
            >
          </template>
          <RouterLink v-else :to="{ name: 'server-new', query: { domain: domain.name } }"
            >Register its server</RouterLink
          >
        </dd>
      </div>
    </dl>
    <p class="sr-only">{{ plural(domain.flows.length, 'flow') }}</p>
  </article>
</template>

<style scoped>
.dc {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 16px 18px;
}

.dc-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 8px;
}

.dc-name {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 14px;
  font-weight: 700;
  color: var(--stretto-text);
}

.dc-name:hover {
  color: var(--stretto-accent);
}

.dc-name svg {
  color: var(--stretto-text-subtle);
}

.dc-count {
  font-size: 13px;
  color: var(--stretto-text-muted);
}

.dc-count strong {
  font-size: 22px;
  font-weight: 600;
  letter-spacing: -0.02em;
  color: var(--stretto-text);
  margin-right: 2px;
}

.dc-bar {
  display: flex;
  gap: 2px;
  height: 8px;
}

.dc-seg {
  flex-basis: 0;
  min-width: 4px;
  border-radius: 3px;
}

.seg-served {
  background: var(--stretto-chart-accent);
}

.seg-shadow {
  background: transparent;
  box-shadow: inset 0 0 0 1.5px var(--stretto-chart-accent);
  background-image: repeating-linear-gradient(
    135deg,
    color-mix(in oklab, var(--stretto-chart-accent) 45%, transparent) 0 2px,
    transparent 2px 5px
  );
}

.seg-recorded {
  background: var(--stretto-chart-baseline);
}

.dc-legend {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 14px;
  list-style: none;
  font-size: 12.5px;
  color: var(--stretto-text-muted);
}

.dc-legend li {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.dc-key {
  width: 9px;
  height: 9px;
  border-radius: 2px;
}

.dc-links {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 4px;
  padding-top: 12px;
  border-top: 1px solid var(--stretto-border);
  font-size: 13px;
}

.dc-links div {
  display: flex;
  align-items: flex-start;
  gap: 8px;
}

.dc-links dt {
  display: flex;
  padding-top: 2px;
  color: var(--stretto-text-subtle);
}

.dc-links dd {
  display: flex;
  flex-wrap: wrap;
  gap: 2px 10px;
  min-width: 0;
}

.dc-links .mono {
  font-size: 12.5px;
  overflow-wrap: anywhere;
}
</style>

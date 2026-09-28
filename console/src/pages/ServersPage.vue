<script setup lang="ts">
/** The registry: each MCP server stretto fronts, and the upstreams its sessions came through that are not registered. */
import { computed } from 'vue'
import { Globe, Plus, Radar, Server, SquareTerminal, TriangleAlert, Workflow } from '@lucide/vue'
import UiPageHeader from '@/components/ui/UiPageHeader.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiBadge from '@/components/ui/UiBadge.vue'
import UiCard from '@/components/ui/UiCard.vue'
import UiEmpty from '@/components/ui/UiEmpty.vue'
import UiError from '@/components/ui/UiError.vue'
import UiSkeleton from '@/components/ui/UiSkeleton.vue'
import UiRelTime from '@/components/ui/UiRelTime.vue'
import ModeBadge from '@/components/ModeBadge.vue'
import { api } from '@/api/client'
import { useResource } from '@/composables/useResource'
import { useTitle } from '@/composables/useTitle'
import { readOnly } from '@/stores/auth'
import { domainFilter, inDomain } from '@/stores/domain'
import { formatCount, upstreamText } from '@/lib/format'

useTitle(() => 'Servers')

const list = useResource((o) => api.servers(o), { events: ['servers', 'sessions', 'flows'] })
const servers = computed(() => list.data.value?.items.filter((s) => inDomain(s.name)) ?? [])
const discovered = computed(
  () => list.data.value?.discovered.filter((d) => inDomain(d.domain)) ?? [],
)
const unregistered = computed(() => discovered.value.filter((d) => !d.registered).length)
</script>

<template>
  <div class="page">
    <UiPageHeader title="Servers">
      The MCP servers stretto fronts. Each runs behind stretto-proxy in your host, which records its
      sessions and serves its flow.
      <template #actions>
        <UiButton
          variant="primary"
          :icon="Plus"
          :to="{ name: 'server-new' }"
          :disabled="readOnly"
          reason="The console is read-only"
          >Add a server</UiButton
        >
      </template>
    </UiPageHeader>

    <div v-if="list.error.value" class="card">
      <UiError
        :message="list.error.value.message"
        :status="list.error.value.status"
        @retry="list.refresh()"
      />
    </div>
    <div v-else-if="!list.data.value" class="grid grid-2">
      <div v-for="i in 2" :key="i" class="card pad"><UiSkeleton :lines="5" /></div>
    </div>
    <template v-else>
      <div v-if="servers.length" class="grid grid-2" :class="{ refreshing: list.refreshing.value }">
        <article
          v-for="s in servers"
          :key="s.name"
          class="srv card"
          :data-testid="`server-${s.name}`"
        >
          <header class="srv-head">
            <span class="srv-icon" aria-hidden="true">
              <Globe v-if="s.upstream.kind === 'http'" :size="17" :stroke-width="1.8" />
              <SquareTerminal v-else :size="17" :stroke-width="1.8" />
            </span>
            <div class="srv-titles">
              <RouterLink
                :to="{ name: 'server', params: { name: s.name } }"
                class="srv-name mono"
                >{{ s.name }}</RouterLink
              >
              <p v-if="s.description" class="srv-desc">{{ s.description }}</p>
            </div>
            <ModeBadge :mode="s.mode" />
          </header>
          <p class="srv-up mono" :title="upstreamText(s.upstream)">
            {{ upstreamText(s.upstream) }}
          </p>
          <dl class="srv-facts">
            <div>
              <dt>Flow</dt>
              <dd>
                <RouterLink
                  v-if="s.flow_summary"
                  :to="{ name: 'flow', params: { key: s.flow_summary.key } }"
                  class="mono"
                >
                  <Workflow :size="13" :stroke-width="2" aria-hidden="true" />{{
                    s.flow_summary.key
                  }}
                </RouterLink>
                <span v-else-if="s.flow" class="mono subtle">{{ s.flow }}</span>
                <span v-else class="subtle">none</span>
              </dd>
            </div>
            <div>
              <dt>Sessions</dt>
              <dd class="num">
                <RouterLink :to="{ name: 'sessions', query: { domain: s.name } }">{{
                  formatCount(s.sessions)
                }}</RouterLink>
              </dd>
            </div>
            <div>
              <dt>Last seen</dt>
              <dd><UiRelTime :ms="s.last_session_unix_ms" /></dd>
            </div>
          </dl>
          <ul v-if="s.issues.length" class="srv-issues">
            <li v-for="issue in s.issues" :key="issue">
              <TriangleAlert :size="14" :stroke-width="2" aria-hidden="true" />
              <span>{{ issue }}</span>
            </li>
          </ul>
        </article>
      </div>
      <div v-else class="card">
        <UiEmpty
          :icon="Server"
          :title="domainFilter ? `No server named ${domainFilter}` : 'No servers yet'"
        >
          Add the MCP server your agent uses. The console gives you your host’s configuration, with
          stretto-proxy in front: use your agent through it, then learn a flow.
          <template #actions>
            <UiButton
              variant="primary"
              :icon="Plus"
              :to="{ name: 'server-new', query: domainFilter ? { domain: domainFilter } : {} }"
              :disabled="readOnly"
            >
              Add a server
            </UiButton>
          </template>
        </UiEmpty>
      </div>

      <UiCard
        title="Discovered upstreams"
        :caption="
          unregistered
            ? `Servers seen in session headers. ${unregistered} not in the registry: add one to get its configuration and test it.`
            : 'Servers seen in session headers, all in the registry.'
        "
        flush
      >
        <div v-if="discovered.length" class="table-wrap">
          <table class="table stack">
            <thead>
              <tr>
                <th scope="col">Domain</th>
                <th scope="col">Upstream</th>
                <th scope="col" class="num">Sessions</th>
                <th scope="col" class="num">Last seen</th>
                <th scope="col"><span class="sr-only">Action</span></th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="d in discovered" :key="d.domain" :data-testid="`discovered-${d.domain}`">
                <td class="domain">
                  <span class="mono">{{ d.domain }}</span>
                </td>
                <td class="up mono">{{ upstreamText(d.upstream) }}</td>
                <td class="num inline" data-label="Sessions">{{ formatCount(d.sessions) }}</td>
                <td class="num subtle inline" data-label="Last seen">
                  <UiRelTime :ms="d.last_seen_unix_ms" />
                </td>
                <td class="action inline">
                  <UiBadge v-if="d.registered" tone="neutral">registered</UiBadge>
                  <UiButton
                    v-else
                    size="sm"
                    :icon="Plus"
                    :to="{ name: 'server-new', query: { domain: d.domain } }"
                    :disabled="readOnly"
                    reason="The console is read-only"
                  >
                    Add
                  </UiButton>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
        <UiEmpty v-else :icon="Radar" title="Nothing discovered yet" compact
          >Upstreams appear here from the headers of recorded sessions.</UiEmpty
        >
      </UiCard>
    </template>
  </div>
</template>

<style scoped>
.pad {
  padding: 20px;
}

.srv {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 18px 20px 16px;
  transition:
    border-color 0.15s var(--c-ease),
    box-shadow 0.15s var(--c-ease);
}

.srv:hover {
  border-color: color-mix(in oklab, var(--stretto-border-strong) 45%, var(--stretto-border));
  box-shadow: var(--c-shadow-md);
}

.srv-head {
  display: flex;
  align-items: flex-start;
  gap: 12px;
}

.srv-icon {
  display: grid;
  place-items: center;
  flex: none;
  width: 34px;
  height: 34px;
  border-radius: 9px;
  background: var(--stretto-surface-2);
  border: 1px solid var(--stretto-border);
  color: var(--stretto-text-muted);
}

.srv-titles {
  flex: 1;
  min-width: 0;
}

.srv-name {
  font-size: 15px;
  font-weight: 700;
  color: var(--stretto-text);
}

.srv-name::after {
  content: '';
  position: absolute;
  inset: 0;
}

.srv-name:hover {
  color: var(--stretto-accent);
}

.srv a:not(.srv-name) {
  position: relative;
  z-index: 1;
}

.srv-desc {
  margin-top: 2px;
  font-size: 13px;
  color: var(--stretto-text-muted);
}

.srv-up {
  padding: 7px 10px;
  border-radius: 8px;
  background: var(--stretto-surface-2);
  border: 1px solid var(--stretto-border);
  font-size: 12px;
  color: var(--stretto-text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.srv-facts {
  display: flex;
  flex-wrap: wrap;
  gap: 8px 28px;
}

.srv-facts dt {
  font-size: 12px;
  color: var(--stretto-text-subtle);
}

.srv-facts dd {
  margin: 2px 0 0;
  font-size: 13.5px;
}

.srv-facts dd a {
  display: inline-flex;
  align-items: center;
  gap: 5px;
}

.srv-facts .mono {
  font-size: 12.5px;
}

.srv-issues {
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 10px 12px;
  border-radius: 8px;
  background: var(--c-warn-soft);
  border: 1px solid var(--c-warn-border);
  font-size: 13px;
}

.srv-issues li {
  display: flex;
  align-items: flex-start;
  gap: 8px;
}

.srv-issues svg {
  flex: none;
  margin-top: 2px;
  color: var(--c-warn);
}

.up {
  font-size: 12px;
  color: var(--stretto-text-muted);
  max-width: 420px;
  overflow-wrap: anywhere;
}

.action {
  text-align: right;
  white-space: nowrap;
}

@media (max-width: 640px) {
  .action {
    margin-left: auto;
  }
}
</style>

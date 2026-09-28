<script setup lang="ts">
/** One server: the configuration to paste into each host, a live connection test, and how it is set up. */
import { computed, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { Pencil, ScrollText, Server, Trash, TriangleAlert, Workflow } from '@lucide/vue'
import UiPageHeader from '@/components/ui/UiPageHeader.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiCard from '@/components/ui/UiCard.vue'
import UiCode from '@/components/ui/UiCode.vue'
import UiEmpty from '@/components/ui/UiEmpty.vue'
import UiError from '@/components/ui/UiError.vue'
import UiSkeleton from '@/components/ui/UiSkeleton.vue'
import UiDialog from '@/components/ui/UiDialog.vue'
import UiRelTime from '@/components/ui/UiRelTime.vue'
import ModeBadge from '@/components/ModeBadge.vue'
import HostConfigTabs from '@/components/server/HostConfigTabs.vue'
import ProbePanel from '@/components/server/ProbePanel.vue'
import { api, ApiError } from '@/api/client'
import { useResource } from '@/composables/useResource'
import { useTitle } from '@/composables/useTitle'
import { readOnly } from '@/stores/auth'
import { toast } from '@/stores/toasts'
import { commandText, formatCount, formatDateTime, plural, upstreamText } from '@/lib/format'

const route = useRoute()
const router = useRouter()
const name = computed(() => String(route.params.name))
useTitle(() => name.value)

const list = useResource((o) => api.servers(o), { events: ['servers', 'sessions', 'flows'] })
const server = computed(() => list.data.value?.items.find((s) => s.name === name.value) ?? null)
const missing = computed(() => !!list.data.value && !server.value)
const env = computed(() =>
  server.value?.upstream.kind === 'stdio' ? server.value.upstream.env : [],
)
const deciderText = computed(() => {
  const s = server.value
  if (!s?.flow) return '—'
  if (s.decider) return s.decider
  const f = s.flow_summary
  return f
    ? `${f.has_arbiter ? 'arbiter' : f.has_reach ? 'reach' : 'habit'} (the flow’s default)`
    : 'the flow’s default'
})

/** The flow's surprise gate as the proxy serves it: the server's, else the flow's own. */
const surpriseText = computed(() => {
  const s = server.value
  if (!s?.flow) return '—'
  if (s.surprise?.kind === 'off') return 'off'
  if (s.surprise) return `${s.surprise.nats} nats, in place of the flow’s`
  const g = s.flow_summary?.surprise
  if (g)
    return `${g.window} steps above ${g.threshold.toFixed(2)} nats (the flow’s${g.quantile == null ? '' : `, learned at the ${g.quantile} quantile`})`
  return 'none: the flow never hands back for surprise'
})

const confirmDelete = ref(false)
const deleting = ref(false)
async function remove() {
  deleting.value = true
  try {
    await api.deleteServer(name.value, { quiet: true })
    confirmDelete.value = false
    toast({
      kind: 'success',
      title: `Server ${name.value} removed from the registry`,
      message: 'Its sessions and flows stay where they are.',
    })
    void router.push({ name: 'servers' })
  } catch (e) {
    toast({
      kind: 'error',
      title: 'Could not remove the server',
      message: e instanceof ApiError ? e.message : String(e),
    })
  } finally {
    deleting.value = false
  }
}
</script>

<template>
  <div class="page">
    <UiPageHeader :back="{ name: 'servers' }" back-label="Servers" mono>
      <template #title>
        {{ name }}
        <ModeBadge v-if="server" :mode="server.mode" />
      </template>
      <template v-if="server?.description">{{ server.description }}</template>
      <template #actions>
        <UiButton
          :icon="Pencil"
          :to="{ name: 'server-edit', params: { name } }"
          :disabled="readOnly || !server"
          reason="The console is read-only"
          >Edit</UiButton
        >
        <UiButton
          variant="danger"
          :icon="Trash"
          :disabled="readOnly || !server"
          reason="The console is read-only"
          @click="confirmDelete = true"
          >Remove</UiButton
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
    <div v-else-if="missing" class="card">
      <UiEmpty :icon="Server" :title="`No server named ${name}`">
        It is not in the registry. It may have been removed, or its sessions came through a server
        that was never added.
        <template #actions>
          <UiButton
            variant="primary"
            :to="{ name: 'server-new', query: { domain: name } }"
            :disabled="readOnly"
            >Add it</UiButton
          >
        </template>
      </UiEmpty>
    </div>

    <template v-else>
      <div v-for="issue in server?.issues ?? []" :key="issue" class="notice warn" role="status">
        <TriangleAlert :size="16" :stroke-width="2" aria-hidden="true" />
        <span>{{ issue }}</span>
      </div>

      <div class="grid layout">
        <div class="stack">
          <UiCard
            title="Host configuration"
            caption="What stretto init prints: paste it into your MCP host, in place of the server’s own entry"
          >
            <HostConfigTabs v-if="server" :name="name" :env="env" />
            <UiSkeleton v-else :lines="6" />
          </UiCard>

          <UiCard
            title="Test connection"
            caption="Is the server there, and which tools does it list now?"
          >
            <ProbePanel :name="name" />
          </UiCard>
        </div>

        <div class="stack">
          <UiCard title="Setup">
            <dl v-if="server" class="kv">
              <dt>Upstream</dt>
              <dd class="mono small">{{ upstreamText(server.upstream) }}</dd>
              <template v-if="server.upstream.kind === 'http' && server.upstream.headers.length">
                <dt>Headers</dt>
                <dd>
                  <span v-for="h in server.upstream.headers" :key="h.name" class="chip"
                    >{{ h.name }} ← ${{ h.env }}</span
                  >
                </dd>
              </template>
              <template v-if="env.length">
                <dt>Variables</dt>
                <dd class="chips">
                  <span v-for="v in env" :key="v" class="chip">{{ v }}</span>
                </dd>
              </template>
              <dt>Mode</dt>
              <dd><ModeBadge :mode="server.mode" /></dd>
              <dt>Flow</dt>
              <dd>
                <RouterLink
                  v-if="server.flow_summary"
                  :to="{ name: 'flow', params: { key: server.flow_summary.key } }"
                  class="link-icon mono small"
                >
                  <Workflow :size="13" :stroke-width="2" aria-hidden="true" />{{
                    server.flow_summary.key
                  }}
                </RouterLink>
                <span v-else-if="server.flow" class="mono small">{{ server.flow }}</span>
                <span v-else class="subtle">none</span>
              </dd>
              <template v-if="server.flow">
                <dt>Decider</dt>
                <dd>{{ deciderText }}</dd>
                <dt>Threshold</dt>
                <dd class="num">{{ server.threshold ?? '0.3 (the proxy’s default)' }}</dd>
                <dt>Surprise gate</dt>
                <dd data-testid="server-surprise">{{ surpriseText }}</dd>
                <dt>Tools it may call</dt>
                <dd data-testid="server-flow-tools">
                  <template v-if="server.flow_tools.length"
                    >only
                    <span class="mono small">{{ server.flow_tools.join(', ') }}</span></template
                  >
                  <span v-else class="subtle">every tool it looks up</span>
                </dd>
              </template>
              <dt>Guards</dt>
              <dd data-testid="server-guards">
                <template v-if="server.guards">
                  on<template v-if="server.judge"
                    >, with the confirmation judge ({{ server.judge.mode }}) reading
                    <span class="mono small">{{ server.judge.context }}</span></template
                  >
                </template>
                <span v-else class="subtle">off</span>
              </dd>
              <dt><span class="mono">stretto_commit</span></dt>
              <dd>
                <template v-if="server.commit">offered</template>
                <span v-else class="subtle">not offered</span>
              </dd>
              <dt>Records to</dt>
              <dd class="mono small">
                {{
                  server.record_dir ??
                  `~/.stretto/${server.mode === 'shadow' ? 'shadow' : 'logs'}/${server.name}`
                }}
              </dd>
              <dt>Keeps sessions</dt>
              <dd data-testid="server-retain">
                <template v-if="server.retain_days !== null">{{
                  plural(server.retain_days, 'day')
                }}</template>
                <span v-else class="subtle">all</span>
              </dd>
              <dt>Sessions</dt>
              <dd>
                <RouterLink
                  :to="{ name: 'sessions', query: { domain: server.name } }"
                  class="link-icon"
                >
                  <ScrollText :size="13" :stroke-width="2" aria-hidden="true" />{{
                    formatCount(server.sessions)
                  }}
                </RouterLink>
                <span v-if="server.last_session_unix_ms" class="subtle">
                  · last <UiRelTime :ms="server.last_session_unix_ms"
                /></span>
              </dd>
              <dt>Added</dt>
              <dd>{{ formatDateTime(server.created_unix_ms) }}</dd>
              <dt>Changed</dt>
              <dd>{{ formatDateTime(server.updated_unix_ms) }}</dd>
            </dl>
            <UiSkeleton v-else :lines="8" />
          </UiCard>

          <UiCard title="The proxy’s command line" caption="What the host runs, for every host">
            <UiCode
              v-if="server"
              :code="commandText(['stretto-proxy', ...server.proxy_args])"
              language="shell"
              what="Command"
              wrap
            />
            <UiSkeleton v-else :lines="2" />
          </UiCard>
        </div>
      </div>
    </template>

    <UiDialog v-model:open="confirmDelete" :title="`Remove ${name} from the registry?`">
      <p>
        The console forgets this server’s setup. Its sessions and its flow stay in the data dir, and
        a host that runs it keeps running it until you change the host’s configuration.
      </p>
      <template #actions>
        <UiButton @click="confirmDelete = false">Cancel</UiButton>
        <UiButton variant="danger" :icon="Trash" :loading="deleting" @click="remove"
          >Remove</UiButton
        >
      </template>
    </UiDialog>
  </div>
</template>

<style scoped>
.layout {
  grid-template-columns: minmax(0, 1.55fr) minmax(0, 1fr);
  align-items: start;
}

.small {
  font-size: 12px;
}

.link-icon {
  display: inline-flex;
  align-items: center;
  gap: 5px;
}

.kv dd .chip + .chip {
  margin-left: 6px;
}

@media (max-width: 1100px) {
  .layout {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>

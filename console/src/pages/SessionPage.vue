<script setup lang="ts">
/** One session: who and where, the numbers, then the timeline, the flow's decisions, the raw log and the tools. */
import { computed, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { Bot, Download, FileText, Scale, ScrollText, Trash, Wrench, Zap } from '@lucide/vue'
import UiPageHeader from '@/components/ui/UiPageHeader.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiTabs from '@/components/ui/UiTabs.vue'
import UiSkeleton from '@/components/ui/UiSkeleton.vue'
import UiError from '@/components/ui/UiError.vue'
import UiDialog from '@/components/ui/UiDialog.vue'
import UiEmpty from '@/components/ui/UiEmpty.vue'
import ModeBadge from '@/components/ModeBadge.vue'
import ToolsTable from '@/components/ToolsTable.vue'
import SessionTimeline from '@/components/session/SessionTimeline.vue'
import DecisionsTable from '@/components/session/DecisionsTable.vue'
import RawLog from '@/components/session/RawLog.vue'
import { api, ApiError } from '@/api/client'
import { useResource } from '@/composables/useResource'
import { useTitle } from '@/composables/useTitle'
import { readOnly } from '@/stores/auth'
import { toast } from '@/stores/toasts'
import {
  formatBytes,
  formatCount,
  formatDateTime,
  formatDuration,
  upstreamText,
} from '@/lib/format'
import { surprisedAt } from '@/lib/timeline'

const route = useRoute()
const router = useRouter()
const key = computed(() => String(route.params.key))

const session = useResource((o) => api.session(key.value, o), {
  events: ['sessions'],
  filter: (e) => e.keys.length === 0 || e.keys.includes(key.value),
})
const servers = useResource((o) => api.servers(o), { events: ['servers'] })
const detail = session.data
const summary = computed(() => detail.value?.summary ?? null)
const surprised = computed(() => (detail.value ? surprisedAt(detail.value.decisions) : null))

useTitle(() => summary.value?.session_id ?? 'Session')

/** The threshold the domain's server serves its flow at, else the proxy's default. */
const server = computed(
  () => servers.data.value?.items.find((s) => s.name === summary.value?.domain) ?? null,
)
const threshold = computed(() => server.value?.threshold ?? 0.3)
const thresholdNote = computed(() => {
  if (server.value?.threshold !== null && server.value?.threshold !== undefined) {
    return `the threshold server ${server.value.name} sets`
  }
  return 'the proxy’s default (--flow-threshold 0.3)'
})

const tab = ref(String(route.query.tab ?? 'timeline'))
watch(
  tab,
  (t) => void router.replace({ query: { ...route.query, tab: t === 'timeline' ? undefined : t } }),
)
const tabs = computed(() => [
  { id: 'timeline', label: 'Timeline', icon: ScrollText },
  {
    id: 'decisions',
    label: 'Decisions',
    icon: Scale,
    count: detail.value?.decisions.length ?? null,
  },
  { id: 'raw', label: 'Raw log', icon: FileText },
  { id: 'tools', label: 'Tools', icon: Wrench, count: detail.value?.tools.length ?? null },
])

const stats = computed(() => {
  const s = summary.value
  if (!s) return []
  const list = [
    { label: 'Tool calls', value: formatCount(s.tool_calls), hint: 'the agent’s own' },
    { label: 'LLM turns', value: formatCount(s.llm_turns), hint: 'inferred from timing' },
  ]
  if (s.mode === 'shadow')
    list.push({
      label: 'In shadow',
      value: formatCount(s.shadow_lookups),
      hint: 'lookups decided, none made',
    })
  else
    list.push({
      label: 'Lookups',
      value: formatCount(s.flow_lookups),
      hint: 'read ahead by the flow',
    })
  list.push(
    { label: 'Hand-backs', value: formatCount(s.hand_backs), hint: 'the flow’s decisions to stop' },
    { label: 'Errors', value: formatCount(s.errors), hint: 'failed calls' },
    { label: 'Writes', value: formatCount(s.writes), hint: 'calls to write tools' },
  )
  return list
})

const confirmDelete = ref(false)
const deleting = ref(false)
async function remove() {
  deleting.value = true
  try {
    await api.deleteSession(key.value, { quiet: true })
    confirmDelete.value = false
    toast({
      kind: 'success',
      title: 'Session moved to the trash',
      message: 'It is in console/trash in the data dir.',
    })
    void router.push({ name: 'sessions' })
  } catch (e) {
    toast({
      kind: 'error',
      title: 'Could not delete the session',
      message: e instanceof ApiError ? e.message : String(e),
    })
  } finally {
    deleting.value = false
  }
}
</script>

<template>
  <div class="page">
    <UiPageHeader :back="{ name: 'sessions' }" back-label="Sessions" mono>
      <template #title>{{ summary?.session_id ?? key }}</template>
      <div v-if="summary" class="meta">
        <ModeBadge :mode="summary.mode" />
        <RouterLink
          v-if="summary.domain"
          :to="{ name: 'sessions', query: { domain: summary.domain } }"
          class="mono meta-domain"
          >{{ summary.domain }}</RouterLink
        >
        <span v-if="summary.agent" class="meta-item"
          ><Bot :size="14" :stroke-width="1.9" aria-hidden="true" />{{ summary.agent }}</span
        >
        <span class="meta-item"
          >{{ formatDateTime(summary.started_unix_ms) }} ·
          {{ formatDuration(summary.duration_ms) }}</span
        >
        <span class="meta-item subtle">{{ formatBytes(summary.size_bytes) }}</span>
      </div>
      <template #actions>
        <UiButton
          :icon="Download"
          :href="api.sessionRawUrl(key)"
          :download="`${summary?.session_id ?? key}.jsonl`"
          >Download</UiButton
        >
        <UiButton
          variant="danger"
          :icon="Trash"
          :disabled="readOnly || !summary"
          :reason="readOnly ? 'The console is read-only' : undefined"
          @click="confirmDelete = true"
        >
          Delete
        </UiButton>
      </template>
    </UiPageHeader>

    <div v-if="session.error.value" class="card">
      <UiError
        :title="session.error.value.status === 404 ? 'No such session' : undefined"
        :message="session.error.value.message"
        :status="session.error.value.status"
        @retry="session.refresh()"
      />
    </div>

    <template v-else>
      <p v-if="summary?.upstream" class="upstream caption">
        Server: <span class="mono">{{ upstreamText(summary.upstream) }}</span> ·
        <span class="mono">{{ summary.path }}</span>
      </p>

      <div v-if="surprised" class="notice warn" role="status" data-testid="surprised-notice">
        <Zap :size="16" :stroke-width="2" aria-hidden="true" />
        <span
          >The session surprised the flow after call <span class="mono">{{ surprised.after }}</span
          ><template v-if="surprised.measured">: {{ surprised.measured }}</template
          >. From there the flow handed back after every call.</span
        >
      </div>

      <section class="stats card" aria-label="Numbers for this session">
        <template v-if="summary">
          <div v-for="s in stats" :key="s.label" class="stat">
            <p class="stat-label">{{ s.label }}</p>
            <p class="stat-value num">{{ s.value }}</p>
            <p class="stat-hint">{{ s.hint }}</p>
          </div>
        </template>
        <template v-else>
          <div v-for="i in 6" :key="i" class="stat"><UiSkeleton :lines="2" /></div>
        </template>
      </section>

      <div class="tabs-wrap">
        <UiTabs v-model="tab" :tabs="tabs" id-prefix="session" label="Views of the session" />
      </div>

      <section
        :id="`session-panel-${tab}`"
        role="tabpanel"
        :aria-labelledby="`session-tab-${tab}`"
        class="panel"
        :class="{ refreshing: session.refreshing.value }"
      >
        <div v-if="!detail" class="card pad"><UiSkeleton :lines="8" height="18px" /></div>
        <template v-else-if="tab === 'timeline'">
          <p v-if="detail.decisions.length" class="caption threshold-note">
            Each decision’s value is the lookup’s probability times the chance its bound arguments
            are the agent’s, against a threshold of
            <strong class="num">{{ threshold.toFixed(2) }}</strong
            >, {{ thresholdNote }}.
          </p>
          <SessionTimeline :detail="detail" :threshold="threshold" />
        </template>
        <div v-else-if="tab === 'decisions'" class="card">
          <DecisionsTable v-if="detail.decisions.length" :detail="detail" :threshold="threshold" />
          <UiEmpty v-else :icon="Scale" title="No flow ran in this session" compact>
            The proxy recorded it without a flow. Serve a flow, or run one in shadow, and its
            decisions show here.
          </UiEmpty>
        </div>
        <div v-else-if="tab === 'raw'" class="card pad">
          <RawLog :detail="detail" />
        </div>
        <div v-else class="card">
          <ToolsTable v-if="detail.tools.length" :tools="detail.tools" />
          <UiEmpty v-else :icon="Wrench" title="No tools/list in this session" compact
            >The host never asked the server for its tools.</UiEmpty
          >
        </div>
      </section>
    </template>

    <UiDialog v-model:open="confirmDelete" title="Delete this session?">
      <p>
        The log and its flow and confirmation logs move to
        <span class="mono">console/trash</span> in the data dir. Nothing is unlinked, so you can
        move them back.
      </p>
      <template #actions>
        <UiButton @click="confirmDelete = false">Cancel</UiButton>
        <UiButton variant="danger" :icon="Trash" :loading="deleting" @click="remove"
          >Move to the trash</UiButton
        >
      </template>
    </UiDialog>
  </div>
</template>

<style scoped>
.meta {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 6px 14px;
  font-size: 13.5px;
}

.meta-domain {
  font-size: 13px;
  font-weight: 700;
}

.meta-item {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  color: var(--stretto-text-muted);
}

.upstream {
  margin-top: -12px;
  overflow-wrap: anywhere;
}

.upstream .mono {
  font-size: 11.5px;
}

.stats {
  display: grid;
  grid-template-columns: repeat(6, minmax(0, 1fr));
}

.stat {
  padding: 14px 18px;
  border-right: 1px solid var(--stretto-border);
  min-width: 0;
}

.stat:last-child {
  border-right: 0;
}

.stat-label {
  font-size: 12.5px;
  font-weight: 500;
  color: var(--stretto-text-muted);
}

.stat-value {
  margin-top: 4px;
  font-size: 22px;
  font-weight: 600;
  letter-spacing: -0.02em;
}

.stat-hint {
  margin-top: 2px;
  font-size: 11.5px;
  color: var(--stretto-text-subtle);
}

.tabs-wrap {
  margin-bottom: -12px;
}

.panel {
  min-width: 0;
}

.pad {
  padding: 18px 20px;
}

.threshold-note {
  margin-bottom: 14px;
}

.threshold-note strong {
  color: var(--stretto-text);
  font-weight: 600;
}

@media (max-width: 1100px) {
  .stats {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  .stat:nth-child(3n) {
    border-right: 0;
  }

  .stat:nth-child(-n + 3) {
    border-bottom: 1px solid var(--stretto-border);
  }
}

@media (max-width: 520px) {
  .stats {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .stat {
    padding: 12px 14px;
    border-right: 0;
    border-bottom: 1px solid var(--stretto-border);
  }

  .stat:nth-child(odd) {
    border-right: 1px solid var(--stretto-border);
  }

  .stat:nth-last-child(-n + 2) {
    border-bottom: 0;
  }

  .pad {
    padding: 14px;
  }
}
</style>

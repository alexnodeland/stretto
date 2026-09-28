<script setup lang="ts">
/** The overview: the totals, the last 14 days, each domain, recent sessions, health and jobs. */
import { computed, watch } from 'vue'
import { ArrowRight, Plus, ScrollText, SquareTerminal } from '@lucide/vue'
import UiPageHeader from '@/components/ui/UiPageHeader.vue'
import UiCard from '@/components/ui/UiCard.vue'
import UiStat from '@/components/ui/UiStat.vue'
import UiSkeleton from '@/components/ui/UiSkeleton.vue'
import UiEmpty from '@/components/ui/UiEmpty.vue'
import UiError from '@/components/ui/UiError.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiRelTime from '@/components/ui/UiRelTime.vue'
import ActivityChart from '@/components/ActivityChart.vue'
import DomainCard from '@/components/DomainCard.vue'
import FirstRun from '@/components/FirstRun.vue'
import HealthList from '@/components/HealthList.vue'
import JobStatus from '@/components/JobStatus.vue'
import SessionsTable from '@/components/SessionsTable.vue'
import { api } from '@/api/client'
import { useResource } from '@/composables/useResource'
import { useTitle } from '@/composables/useTitle'
import { meta, readOnly } from '@/stores/auth'
import { domainFilter, domainNames, inDomain, setDomainNames } from '@/stores/domain'

useTitle(() => 'Overview')

const overview = useResource((o) => api.overview(o), {
  events: ['sessions', 'flows', 'servers', 'jobs'],
})
const data = overview.data

watch(data, (o) => {
  if (o) setDomainNames([...domainNames.value, ...o.domains.map((d) => d.name)])
})

const totals = computed(() => data.value?.totals ?? null)
const firstRun = computed(() => {
  const t = totals.value
  return !!t && (t.servers === 0 || t.sessions === 0 || t.flows === 0)
})
const domains = computed(() => data.value?.domains.filter((d) => inDomain(d.name)) ?? [])
const recent = computed(() => data.value?.recent_sessions.filter((s) => inDomain(s.domain)) ?? [])
const series = (key: 'sessions' | 'tool_calls' | 'flow_lookups') =>
  data.value?.activity.map((d) => d[key]) ?? []
const problems = computed(
  () => data.value?.health.filter((h) => h.level === 'warn' || h.level === 'error').length ?? 0,
)
</script>

<template>
  <div class="page">
    <UiPageHeader title="Overview">
      What stretto has recorded and learned in
      <span class="mono">{{ meta?.data_dir ?? '~/.stretto' }}</span
      >.
      <template #actions>
        <UiButton
          :icon="SquareTerminal"
          :to="{ name: 'job-new' }"
          :disabled="readOnly"
          reason="The console is read-only"
          >New job</UiButton
        >
        <UiButton
          variant="primary"
          :icon="Plus"
          :to="{ name: 'server-new' }"
          :disabled="readOnly"
          reason="The console is read-only"
        >
          Add a server
        </UiButton>
      </template>
    </UiPageHeader>

    <div v-if="overview.error.value" class="card">
      <UiError
        :message="overview.error.value.message"
        :status="overview.error.value.status"
        @retry="overview.refresh()"
      />
    </div>

    <template v-else>
      <FirstRun v-if="firstRun && totals" :totals="totals" :read-only="readOnly" />

      <section aria-label="Totals" :class="{ refreshing: overview.refreshing.value }">
        <p v-if="domainFilter" class="scope-note caption">
          The totals and the chart cover every domain; the cards and lists below show
          <span class="mono">{{ domainFilter }}</span
          >.
        </p>
        <div class="grid grid-4 kpis">
          <template v-if="totals">
            <UiStat
              label="Sessions this week"
              :value="totals.sessions_7d"
              :scope="`last 7 days · ${totals.sessions.toLocaleString('en-US')} in all`"
              :trend="series('sessions')"
              trend-label="Sessions per day, last 14 days"
              :to="{ name: 'sessions' }"
            />
            <UiStat
              label="Tool calls"
              :value="totals.tool_calls"
              scope="the agent’s own, every session"
              :trend="series('tool_calls')"
              trend-label="The agent’s tool calls per day, last 14 days"
            />
            <UiStat
              label="Lookups served"
              :value="totals.flow_lookups"
              scope="read ahead by flows, every session"
              :trend="series('flow_lookups')"
              trend-label="Lookups served per day, last 14 days"
              accent
            />
            <UiStat
              label="Shadow decisions"
              :value="totals.shadow_decisions"
              scope="decided in shadow, nothing looked up"
              :to="{ name: 'sessions', query: { mode: 'shadow' } }"
            />
          </template>
          <template v-else>
            <div v-for="i in 4" :key="i" class="stat-skeleton card">
              <UiSkeleton width="45%" height="12px" />
              <UiSkeleton width="38%" height="28px" />
              <UiSkeleton width="70%" height="11px" />
            </div>
          </template>
        </div>
      </section>

      <div class="grid split">
        <UiCard
          title="Activity"
          caption="Tool calls per day, last 14 days, every domain"
          fill
          :class="{ refreshing: overview.refreshing.value }"
        >
          <ActivityChart v-if="data" :days="data.activity" />
          <UiSkeleton v-else height="240px" />
        </UiCard>

        <UiCard
          title="Health"
          :caption="problems ? `${problems} to look at` : 'What stretto doctor would check'"
        >
          <HealthList v-if="data && data.health.length" :items="data.health" />
          <p v-else-if="data" class="muted">Nothing to report.</p>
          <UiSkeleton v-else :lines="5" />
        </UiCard>
      </div>

      <section class="stack" aria-labelledby="domains-title">
        <div class="section-head">
          <h2 id="domains-title" class="section-title">Domains</h2>
          <span class="caption"
            >A domain is a server’s name: its sessions, flows and registry entry.</span
          >
        </div>
        <div v-if="!data" class="grid grid-3">
          <div v-for="i in 3" :key="i" class="card domain-skeleton"><UiSkeleton :lines="4" /></div>
        </div>
        <div v-else-if="domains.length" class="grid grid-3">
          <DomainCard v-for="d in domains" :key="d.name" :domain="d" />
        </div>
        <div v-else class="card">
          <UiEmpty :icon="ScrollText" title="No domains yet" compact>
            A domain appears with its first recorded session. Add a server, use your agent through
            it, then learn a flow.
          </UiEmpty>
        </div>
      </section>

      <div class="grid split">
        <UiCard title="Recent sessions" caption="The newest, with what the flow did in each" flush>
          <template #actions>
            <UiButton variant="ghost" size="sm" :icon-right="ArrowRight" :to="{ name: 'sessions' }"
              >All sessions</UiButton
            >
          </template>
          <SessionsTable
            v-if="recent.length"
            :sessions="recent"
            compact
            :show-domain="!domainFilter"
          />
          <div v-else-if="!data" class="pad"><UiSkeleton :lines="6" height="18px" /></div>
          <UiEmpty v-else :icon="ScrollText" title="No sessions yet" compact>
            Put a server behind stretto-proxy and use your agent as usual: each session is recorded
            and shows here.
          </UiEmpty>
        </UiCard>

        <UiCard
          title="Jobs"
          caption="The newest learn, promote, audit, redact and doctor runs"
          flush
        >
          <template #actions>
            <UiButton variant="ghost" size="sm" :icon-right="ArrowRight" :to="{ name: 'jobs' }"
              >All jobs</UiButton
            >
          </template>
          <ul v-if="data && data.jobs.length" class="jobs">
            <li v-for="job in data.jobs" :key="job.id">
              <RouterLink :to="{ name: 'job', params: { id: job.id } }" class="job">
                <span class="job-title">{{ job.title }}</span>
                <span class="job-meta">
                  <JobStatus :status="job.status" />
                  <span class="caption"><UiRelTime :ms="job.created_unix_ms" /></span>
                </span>
              </RouterLink>
            </li>
          </ul>
          <div v-else-if="!data" class="pad"><UiSkeleton :lines="5" height="18px" /></div>
          <UiEmpty v-else :icon="SquareTerminal" title="No jobs yet" compact>
            Jobs run the stretto CLI for you: learn a flow from sessions, promote it, audit it.
          </UiEmpty>
        </UiCard>
      </div>
    </template>
  </div>
</template>

<style scoped>
.scope-note {
  margin-bottom: 10px;
}

@media (max-width: 720px) {
  .kpis {
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
  }

  .kpis :deep(.stat-spark) {
    display: none;
  }

  .kpis :deep(.stat) {
    padding: 14px;
  }

  .kpis :deep(.stat-value) {
    font-size: 26px;
  }
}

.stat-skeleton {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 18px;
}

.split {
  grid-template-columns: minmax(0, 1.65fr) minmax(0, 1fr);
  align-items: stretch;
}

@media (max-width: 1100px) {
  .split {
    grid-template-columns: minmax(0, 1fr);
  }
}

.section-head {
  display: flex;
  align-items: baseline;
  flex-wrap: wrap;
  gap: 4px 12px;
}

.section-title {
  font-size: 16px;
  letter-spacing: -0.01em;
}

.domain-skeleton {
  padding: 18px;
}

.pad {
  padding: 18px 20px;
}

.jobs {
  list-style: none;
}

.job {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 12px 20px;
  border-bottom: 1px solid var(--stretto-border);
  color: var(--stretto-text);
}

.jobs li:last-child .job {
  border-bottom: 0;
}

.job:hover {
  background: var(--c-hover);
  color: var(--stretto-text);
}

.job-title {
  flex: 1 1 auto;
  font-size: 13.5px;
  font-weight: 500;
  min-width: 0;
  overflow-wrap: anywhere;
}

.job-meta {
  display: inline-flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 3px;
  flex: none;
}

@media (max-width: 720px) {
  .job {
    padding: 12px 16px;
  }

  .pad {
    padding: 16px;
  }
}
</style>

<script setup lang="ts">
/** Every flow in the data dir: what it was learned from, how it decides, where it is served. */
import { computed } from 'vue'
import { CircleX, Server, Sparkles, Workflow } from '@lucide/vue'
import UiPageHeader from '@/components/ui/UiPageHeader.vue'
import UiBadge from '@/components/ui/UiBadge.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiEmpty from '@/components/ui/UiEmpty.vue'
import UiError from '@/components/ui/UiError.vue'
import UiSkeleton from '@/components/ui/UiSkeleton.vue'
import UiRelTime from '@/components/ui/UiRelTime.vue'
import { api } from '@/api/client'
import type { FlowSummary } from '@/api/types'
import { useResource } from '@/composables/useResource'
import { useTitle } from '@/composables/useTitle'
import { readOnly } from '@/stores/auth'
import { domainFilter, inDomain } from '@/stores/domain'
import { formatBytes, formatCount, plural } from '@/lib/format'

useTitle(() => 'Flows')

const flows = useResource((o) => api.flows(o), { events: ['flows', 'servers'] })
const items = computed(
  () => flows.data.value?.items.filter((f) => f.error || inDomain(f.domain)) ?? [],
)

const decider = (f: FlowSummary) =>
  ({
    reach: {
      label: 'reach',
      title:
        'Decides by counting: the chance a lookup is used before the next write. Asks no model.',
    },
    habit: {
      label: 'habit',
      title: 'Decides with the habit alone: what the agent did next. Asks no model.',
    },
    arbiter: {
      label: 'arbiter',
      title: 'Weighs the habit with a System-One model’s answers (needs TYPESAFE_API_KEY).',
    },
  })[f.decider]

function learned(f: FlowSummary): string {
  const from = f.sources.length ? f.sources.join(', ') : 'no named source'
  const arbiter = f.has_arbiter
    ? `; its arbiter was fitted on ${plural(f.arbiter_cases, 'held-out decision')}`
    : ''
  return `Learned from ${plural(f.habit_episodes, 'session')} of ${from}${arbiter}.`
}
</script>

<template>
  <div class="page">
    <UiPageHeader title="Flows">
      A flow is a JSON file of counts and bindings: which reads follow which calls, and where their
      arguments come from.
      <template #actions>
        <UiButton
          variant="primary"
          :icon="Sparkles"
          :to="{ name: 'job-new', query: { kind: 'learn' } }"
          :disabled="readOnly"
          reason="The console is read-only"
        >
          Learn a flow
        </UiButton>
      </template>
    </UiPageHeader>

    <div v-if="flows.error.value" class="card">
      <UiError
        :message="flows.error.value.message"
        :status="flows.error.value.status"
        @retry="flows.refresh()"
      />
    </div>
    <div v-else-if="!flows.data.value" class="grid grid-2">
      <div v-for="i in 4" :key="i" class="card pad"><UiSkeleton :lines="5" /></div>
    </div>
    <div v-else-if="!items.length" class="card">
      <UiEmpty
        :icon="Workflow"
        :title="domainFilter ? `No flows for ${domainFilter}` : 'No flows yet'"
      >
        Add a server, use your agent through it, then learn a flow from its recorded sessions.
        Learning with --habit-only needs no key.
        <template #actions>
          <UiButton
            variant="primary"
            :icon="Sparkles"
            :to="{ name: 'job-new', query: { kind: 'learn', domain: domainFilter ?? undefined } }"
            :disabled="readOnly"
          >
            Learn a flow
          </UiButton>
        </template>
      </UiEmpty>
    </div>
    <div v-else class="grid grid-2" :class="{ refreshing: flows.refreshing.value }">
      <article v-for="f in items" :key="f.key" class="flow card" :class="{ broken: f.error }">
        <header class="flow-head">
          <RouterLink
            v-if="!f.error"
            :to="{ name: 'flow', params: { key: f.key } }"
            class="flow-name mono"
            >{{ f.key }}</RouterLink
          >
          <span v-else class="flow-name mono">{{ f.key }}</span>
          <span class="spacer" />
          <template v-if="!f.error">
            <UiBadge :title="decider(f).title" mono>{{ decider(f).label }}</UiBadge>
            <UiBadge
              v-if="f.promoted"
              tone="accent"
              :title="`Acts at ${f.promoted.sites_promoted} of the ${f.promoted.sites_scored} sites promote scored`"
            >
              promoted {{ f.promoted.sites_promoted }}/{{ f.promoted.sites_scored }}
            </UiBadge>
            <UiBadge
              v-if="f.stage?.staged"
              tone="dashed"
              :title="
                f.stage.other
                  ? `The staged flow of ${f.stage.other}: commit it on that flow's Staged tab`
                  : 'A staged flow, never committed'
              "
              :data-testid="`staged-${f.key}`"
              >staged</UiBadge
            >
            <UiBadge
              v-else-if="f.stage?.pending"
              tone="accent"
              :title="`A staged flow waits beside it: ${f.stage.other}`"
              :data-testid="`staged-${f.key}`"
              >staged changes</UiBadge
            >
          </template>
        </header>
        <p class="flow-path caption">
          <span class="mono">{{ f.path }}</span> · {{ formatBytes(f.size_bytes) }}
        </p>

        <p v-if="f.error" class="notice danger">
          <CircleX :size="16" :stroke-width="2" aria-hidden="true" />
          <span>This file does not load: {{ f.error }}</span>
        </p>
        <template v-else>
          <p class="flow-learned">
            <span class="mono flow-domain">{{ f.domain }}</span>
            {{ learned(f) }}
          </p>
          <dl class="flow-nums">
            <div>
              <dt>Sites</dt>
              <dd class="num">{{ formatCount(f.sites) }}</dd>
            </div>
            <div>
              <dt>Lookups</dt>
              <dd class="num">{{ formatCount(f.lookups) }}</dd>
            </div>
            <div>
              <dt>Tools</dt>
              <dd class="num">
                {{ f.tools.read }} read · {{ f.tools.write }} write<template v-if="f.tools.generic">
                  · {{ f.tools.generic }} neither</template
                >
              </dd>
            </div>
          </dl>
        </template>
        <footer class="flow-foot caption">
          <span v-if="f.served_by.length" class="served">
            <Server :size="13" :stroke-width="2" aria-hidden="true" />
            Served by
            <RouterLink
              v-for="s in f.served_by"
              :key="s"
              :to="{ name: 'server', params: { name: s } }"
              class="mono"
              >{{ s }}</RouterLink
            >
          </span>
          <span v-else-if="!f.error">No server runs it</span>
          <span class="spacer" />
          <span>modified <UiRelTime :ms="f.modified_unix_ms" /></span>
        </footer>
      </article>
    </div>
  </div>
</template>

<style scoped>
.pad {
  padding: 20px;
}

.flow {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 18px 20px 14px;
  transition:
    border-color 0.15s var(--c-ease),
    box-shadow 0.15s var(--c-ease);
}

.flow:not(.broken):hover {
  border-color: color-mix(in oklab, var(--stretto-border-strong) 45%, var(--stretto-border));
  box-shadow: var(--c-shadow-md);
}

.flow-head {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
}

.flow-name {
  font-size: 15px;
  font-weight: 700;
  color: var(--stretto-text);
  overflow-wrap: anywhere;
}

a.flow-name::after {
  content: '';
  position: absolute;
  inset: 0;
}

.flow {
  position: relative;
}

.flow a:not(.flow-name),
.flow button {
  position: relative;
  z-index: 1;
}

a.flow-name:hover {
  color: var(--stretto-accent);
}

.flow-path {
  margin-top: -6px;
  overflow-wrap: anywhere;
}

.flow-path .mono {
  font-size: 11.5px;
}

.flow-learned {
  font-size: 13.5px;
  color: var(--stretto-text-muted);
}

.flow-domain {
  display: inline-block;
  margin-right: 4px;
  padding: 0 6px;
  border-radius: 5px;
  background: var(--stretto-surface-2);
  border: 1px solid var(--stretto-border);
  font-size: 12px;
  color: var(--stretto-text);
}

.flow-nums {
  display: flex;
  flex-wrap: wrap;
  gap: 8px 28px;
  padding: 10px 0 2px;
  border-top: 1px solid var(--stretto-border);
}

.flow-nums dt {
  font-size: 12px;
  color: var(--stretto-text-subtle);
}

.flow-nums dd {
  margin: 2px 0 0;
  font-size: 15px;
  font-weight: 600;
}

.flow-foot {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 6px 10px;
  margin-top: auto;
  padding-top: 10px;
  border-top: 1px solid var(--stretto-border);
}

.served {
  display: inline-flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 5px;
}

.served .mono {
  font-size: 12px;
}
</style>

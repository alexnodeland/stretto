<script setup lang="ts">
/**
 * One flow: what it may call, where, with what arguments and how it decides.
 * The threshold slider previews which lookups act; the server confirms each
 * threshold with review::show's own computation.
 */
import { computed, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import {
  BadgeCheck,
  Download,
  FileText,
  GitCompareArrows,
  Braces,
  Scale,
  Server,
  FileSearch,
  Trash,
  TriangleAlert,
  Workflow,
} from '@lucide/vue'
import UiPageHeader from '@/components/ui/UiPageHeader.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiBadge from '@/components/ui/UiBadge.vue'
import UiCard from '@/components/ui/UiCard.vue'
import UiTabs from '@/components/ui/UiTabs.vue'
import UiSkeleton from '@/components/ui/UiSkeleton.vue'
import UiError from '@/components/ui/UiError.vue'
import UiJson from '@/components/ui/UiJson.vue'
import UiDialog from '@/components/ui/UiDialog.vue'
import MarkdownView from '@/components/MarkdownView.vue'
import ThresholdSlider from '@/components/flow/ThresholdSlider.vue'
import FlowGraph from '@/components/flow/FlowGraph.vue'
import SitesTable from '@/components/flow/SitesTable.vue'
import BindingsList from '@/components/flow/BindingsList.vue'
import FlowToolsTable from '@/components/flow/FlowToolsTable.vue'
import FlowCompare from '@/components/flow/FlowCompare.vue'
import { api, ApiError } from '@/api/client'
import { useResource } from '@/composables/useResource'
import { useTitle } from '@/composables/useTitle'
import { readOnly } from '@/stores/auth'
import { toast } from '@/stores/toasts'
import { previewSites } from '@/lib/flow'
import { formatBytes, formatCount, formatDateTime, plural } from '@/lib/format'

const route = useRoute()
const router = useRouter()
const key = computed(() => String(route.params.key))
useTitle(() => key.value)

const first = (v: unknown) => (Array.isArray(v) ? v[0] : v) as string | undefined
const initial = Number(first(route.query.threshold))
const threshold = ref(Number.isFinite(initial) && initial >= 0 && initial <= 1 ? initial : 0.3)
/** The threshold the server was last asked about, a moment after the slider stops. */
const asked = ref(threshold.value)
let timer: ReturnType<typeof setTimeout> | null = null
watch(threshold, (t) => {
  if (timer) clearTimeout(timer)
  timer = setTimeout(() => (asked.value = t), 220)
})

const tab = ref(first(route.query.tab) ?? 'flow')
const compareWith = ref<string | null>(first(route.query.compare) ?? null)
watch([asked, tab, compareWith], () => {
  void router.replace({
    query: {
      ...(Math.abs(asked.value - 0.3) > 1e-9 ? { threshold: asked.value.toFixed(2) } : {}),
      ...(tab.value !== 'flow' ? { tab: tab.value } : {}),
      ...(tab.value === 'compare' && compareWith.value ? { compare: compareWith.value } : {}),
    },
  })
})

const flow = useResource((o) => api.flow(key.value, asked.value, o), {
  watch: [asked],
  events: ['flows', 'servers'],
  filter: (e) => e.what === 'servers' || e.keys.length === 0 || e.keys.includes(key.value),
})
const all = useResource((o) => api.flows(o), { events: ['flows'] })
const detail = flow.data
const summary = computed(() => detail.value?.summary ?? null)

const previews = computed(() => (detail.value ? previewSites(detail.value, threshold.value) : []))
const acting = computed(() => previews.value.filter((p) => p.acts).length)
const promoted = computed(() => !!detail.value?.promotion)
const marks = computed(() => {
  const bar = detail.value?.promotion?.bar.threshold
  return bar !== undefined && Math.abs(bar - 0.3) > 1e-9
    ? [{ value: bar, label: `${bar} promoted at` }]
    : []
})
const others = computed(
  () => all.data.value?.items.filter((f) => f.key !== key.value && !f.error) ?? [],
)

const raw = ref<unknown>(null)
const rawError = ref<ApiError | null>(null)
async function loadRaw() {
  rawError.value = null
  try {
    raw.value = JSON.parse(await api.flowRaw(key.value, { quiet: true }))
  } catch (e) {
    rawError.value =
      e instanceof ApiError ? e : new ApiError(0, e instanceof Error ? e.message : String(e))
  }
}
watch(
  tab,
  (t) => {
    if (t === 'raw' && raw.value === null) void loadRaw()
  },
  { immediate: true },
)

const tabs = computed(() => [
  { id: 'flow', label: 'Flow', icon: Workflow },
  { id: 'review', label: 'Review', icon: FileText },
  { id: 'raw', label: 'Raw JSON', icon: Braces },
  { id: 'compare', label: 'Compare', icon: GitCompareArrows },
])

const deciderText = computed(() => {
  const s = summary.value
  if (!s) return ''
  if (s.decider === 'arbiter')
    return `Served with its arbiter, fitted on ${plural(s.arbiter_cases, 'held-out decision')}: it asks a System-One model and needs TYPESAFE_API_KEY.`
  if (s.decider === 'reach')
    return 'Served with reach: it counts how often each lookup came before the agent’s next write, and asks no model.'
  return 'Served with the habit alone: it counts what the agent did next, and asks no model.'
})

const confirmDelete = ref(false)
const deleting = ref(false)
async function remove() {
  deleting.value = true
  try {
    await api.deleteFlow(key.value, { quiet: true })
    confirmDelete.value = false
    toast({
      kind: 'success',
      title: 'Flow moved to the trash',
      message: 'It is in console/trash in the data dir.',
    })
    void router.push({ name: 'flows' })
  } catch (e) {
    toast({
      kind: 'error',
      title: 'Could not delete the flow',
      message: e instanceof ApiError ? e.message : String(e),
    })
  } finally {
    deleting.value = false
  }
}
</script>

<template>
  <div class="page">
    <UiPageHeader :back="{ name: 'flows' }" back-label="Flows" mono>
      <template #title>
        {{ key }}
        <template v-if="summary">
          <UiBadge mono :title="deciderText">{{ summary.decider }}</UiBadge>
          <UiBadge v-if="summary.promoted" tone="accent"
            >promoted {{ summary.promoted.sites_promoted }}/{{
              summary.promoted.sites_scored
            }}</UiBadge
          >
        </template>
      </template>
      <p v-if="summary" class="sub">
        Domain <span class="mono">{{ summary.domain }}</span
        >. Learned by stretto {{ summary.stretto_version }} from
        {{ plural(summary.habit_episodes, 'session') }} of
        {{ summary.sources.join(', ') || 'no named source' }}, on
        {{ formatDateTime(summary.compiled_unix_ms) }}. {{ deciderText }}
      </p>
      <template #actions>
        <UiButton
          :icon="Download"
          :href="api.flowRawUrl(key)"
          :download="summary?.path.split('/').pop() ?? `${key}.flow.json`"
          >Download</UiButton
        >
        <UiButton
          :icon="BadgeCheck"
          :to="{ name: 'job-new', query: { kind: 'promote', flow: key } }"
          :disabled="readOnly"
          reason="The console is read-only"
        >
          Promote
        </UiButton>
        <UiButton
          :icon="FileSearch"
          :to="{ name: 'job-new', query: { kind: 'audit', flow: key } }"
          :disabled="readOnly"
          reason="The console is read-only"
        >
          Audit
        </UiButton>
        <UiButton
          variant="danger"
          :icon="Trash"
          square
          aria-label="Delete the flow"
          :disabled="readOnly || !summary"
          :reason="readOnly ? 'The console is read-only' : 'Delete'"
          @click="confirmDelete = true"
        />
      </template>
    </UiPageHeader>

    <div v-if="flow.error.value" class="card">
      <UiError
        :title="flow.error.value.status === 404 ? 'No such flow' : 'This flow could not be loaded'"
        :message="flow.error.value.message"
        :status="flow.error.value.status"
        @retry="flow.refresh()"
      />
    </div>

    <template v-else>
      <div v-for="w in detail?.warnings ?? []" :key="w" class="notice warn" role="status">
        <TriangleAlert :size="16" :stroke-width="2" aria-hidden="true" />
        <span>{{ w }}</span>
      </div>

      <section class="facts card" aria-label="The flow in numbers">
        <template v-if="summary">
          <div class="fact">
            <p class="fact-label">Sites</p>
            <p class="fact-value num">{{ formatCount(summary.sites) }}</p>
            <p class="fact-hint">calls it decides after</p>
          </div>
          <div class="fact">
            <p class="fact-label">Lookups</p>
            <p class="fact-value num">{{ formatCount(summary.lookups) }}</p>
            <p class="fact-hint">distinct tools it may read</p>
          </div>
          <div class="fact">
            <p class="fact-label">Tools</p>
            <p class="fact-value num">
              {{ summary.tools.read }}<span class="fact-of"> read</span> · {{ summary.tools.write
              }}<span class="fact-of"> write</span>
            </p>
            <p class="fact-hint">
              {{ summary.tools.generic ? `${summary.tools.generic} neither; ` : '' }}it calls reads
              only
            </p>
          </div>
          <div class="fact">
            <p class="fact-label">Served by</p>
            <p class="fact-value served">
              <template v-if="summary.served_by.length">
                <RouterLink
                  v-for="s in summary.served_by"
                  :key="s"
                  :to="{ name: 'server', params: { name: s } }"
                  class="mono"
                >
                  <Server :size="14" :stroke-width="2" aria-hidden="true" />{{ s }}
                </RouterLink>
              </template>
              <span v-else class="subtle">no server</span>
            </p>
            <p class="fact-hint mono">{{ summary.path }} · {{ formatBytes(summary.size_bytes) }}</p>
          </div>
        </template>
        <template v-else>
          <div v-for="i in 4" :key="i" class="fact"><UiSkeleton :lines="2" /></div>
        </template>
      </section>

      <div class="tabs-wrap">
        <UiTabs v-model="tab" :tabs="tabs" id-prefix="flow" label="Views of the flow" />
      </div>

      <section
        :id="`flow-panel-${tab}`"
        role="tabpanel"
        :aria-labelledby="`flow-tab-${tab}`"
        class="stack"
      >
        <template v-if="tab === 'flow'">
          <UiCard>
            <template #head>
              <h2 class="card-title">What it does at a threshold</h2>
              <p class="caption">
                The flow takes the likeliest lookup when its share, times the chance its bound
                arguments are the agent’s, reaches the threshold the proxy serves it with (<span
                  class="mono"
                  >--flow-threshold</span
                >).
              </p>
            </template>
            <div class="stack">
              <ThresholdSlider v-model="threshold" :marks="marks" />
              <p v-if="detail" class="acting" aria-live="polite" data-testid="acting">
                <Scale :size="15" :stroke-width="2" aria-hidden="true" />
                At <strong class="num">{{ threshold.toFixed(2) }}</strong> the flow looks something
                up after <strong class="num">{{ acting }}</strong> of its
                <strong class="num">{{ previews.length }}</strong>
                {{ previews.length === 1 ? 'site' : 'sites'
                }}{{ promoted ? ', where promoted' : '' }}, and hands back after the rest.
              </p>
              <UiSkeleton v-else height="16px" width="60%" />
              <FlowGraph
                v-if="detail && previews.length"
                :id="`flow-${key}`"
                :previews="previews"
                :tools="detail.tools"
                :class="{ refreshing: flow.refreshing.value }"
              />
              <UiSkeleton v-else-if="!detail" height="320px" />
              <p v-else class="muted">
                This flow has no sites: training saw no lookup after any call, so it always hands
                back.
              </p>
              <ul class="graph-legend" aria-label="Legend">
                <li><span class="lg-line acts" aria-hidden="true" />looks up at this threshold</li>
                <li><span class="lg-line" aria-hidden="true" />hands back</li>
                <li>
                  <span class="lg-width" aria-hidden="true" />width: its share of what the agent did
                  next
                </li>
                <li><span class="lg-pill" aria-hidden="true">0.88</span>share × binding chance</li>
              </ul>
            </div>
          </UiCard>

          <UiCard title="Sites" caption="After each call, the lookups the flow may make next" flush>
            <SitesTable v-if="detail" :previews="previews" :promoted="promoted" />
            <div v-else class="pad"><UiSkeleton :lines="5" /></div>
          </UiCard>

          <div class="grid two">
            <UiCard title="Bindings" caption="Where each lookup’s arguments come from, in training">
              <BindingsList v-if="detail" :bindings="detail.bindings" />
              <UiSkeleton v-else :lines="6" />
            </UiCard>
            <UiCard
              v-if="detail?.promotion"
              title="Promotion"
              caption="The bar stretto promote held each site to"
            >
              <dl class="kv">
                <dt>Threshold</dt>
                <dd class="num">{{ detail.promotion.bar.threshold }}</dd>
                <dt>Least share used</dt>
                <dd class="num">{{ detail.promotion.bar.min_used }}</dd>
                <dt>Least lower bound</dt>
                <dd class="num">{{ detail.promotion.bar.min_lower }} (Wilson, 90% two-sided)</dd>
                <dt>Fewest tasks</dt>
                <dd class="num">{{ detail.promotion.bar.min_tasks }}</dd>
                <dt>Promoted</dt>
                <dd class="num">
                  {{ detail.promotion.sites_promoted }} of the
                  {{ detail.promotion.sites_scored }} sites scored
                </dd>
              </dl>
            </UiCard>
          </div>

          <UiCard
            title="Tools"
            caption="The domain’s tools, from the servers’ readOnlyHint: the flow calls only reads"
            flush
          >
            <FlowToolsTable v-if="detail" :tools="detail.tools" />
            <div v-else class="pad"><UiSkeleton :lines="4" /></div>
          </UiCard>
        </template>

        <UiCard
          v-else-if="tab === 'review'"
          title="Review"
          :caption="`stretto flow-show, at a threshold of ${asked.toFixed(2)}`"
        >
          <MarkdownView v-if="detail" :text="detail.review_markdown" />
          <UiSkeleton v-else :lines="10" />
        </UiCard>

        <div v-else-if="tab === 'raw'" class="card pad">
          <UiError
            v-if="rawError"
            :message="rawError.message"
            :status="rawError.status"
            @retry="loadRaw"
          />
          <UiJson
            v-else-if="raw !== null"
            :value="raw"
            :what="'Flow'"
            :label="summary?.path"
            :depth="1"
            max-height="640px"
          />
          <UiSkeleton v-else :lines="10" />
        </div>

        <FlowCompare
          v-else-if="summary"
          :flow="summary"
          :others="others"
          :threshold="asked"
          :initial="compareWith"
          @other="compareWith = $event"
        />
      </section>
    </template>

    <UiDialog v-model:open="confirmDelete" title="Delete this flow?">
      <p>
        <span class="mono">{{ summary?.path }}</span> moves to
        <span class="mono">console/trash</span> in the data dir.
        <template v-if="summary?.served_by.length">
          Server {{ summary.served_by.join(', ') }} runs it: its proxy will not start with the flow
          gone.
        </template>
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
.sub {
  max-width: 900px;
}

.sub .mono {
  font-size: 13px;
}

.facts {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
}

.fact {
  padding: 14px 18px;
  border-right: 1px solid var(--stretto-border);
  min-width: 0;
}

.fact:last-child {
  border-right: 0;
}

.fact-label {
  font-size: 12.5px;
  font-weight: 500;
  color: var(--stretto-text-muted);
}

.fact-value {
  margin-top: 4px;
  font-size: 20px;
  font-weight: 600;
  letter-spacing: -0.015em;
}

.fact-of {
  font-size: 13px;
  font-weight: 500;
  color: var(--stretto-text-muted);
  letter-spacing: 0;
}

.fact-value.served {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 12px;
  font-size: 14px;
  padding-top: 5px;
}

.fact-value.served a {
  display: inline-flex;
  align-items: center;
  gap: 5px;
}

.fact-hint {
  margin-top: 2px;
  font-size: 11.5px;
  color: var(--stretto-text-subtle);
  overflow-wrap: anywhere;
}

.tabs-wrap {
  margin-bottom: -12px;
}

.acting {
  font-size: 13.5px;
  color: var(--stretto-text-muted);
}

.acting svg {
  margin-right: 6px;
  vertical-align: -3px;
  color: var(--stretto-accent-graphic);
}

.acting strong {
  color: var(--stretto-text);
  font-weight: 600;
}

.graph-legend {
  display: flex;
  flex-wrap: wrap;
  gap: 6px 18px;
  list-style: none;
  font-size: 12.5px;
  color: var(--stretto-text-muted);
}

.graph-legend li {
  display: inline-flex;
  align-items: center;
  gap: 7px;
}

.lg-line {
  width: 22px;
  height: 3px;
  border-radius: 2px;
  background: var(--stretto-border-strong);
}

.lg-line.acts {
  background: var(--stretto-chart-accent);
}

.lg-width {
  width: 22px;
  height: 8px;
  background: linear-gradient(to bottom right, transparent 45%, var(--stretto-border-strong) 45%);
  border-radius: 1px;
}

.lg-pill {
  padding: 0 6px;
  border-radius: 999px;
  border: 1px solid var(--stretto-border);
  background: var(--stretto-surface);
  font-size: 11px;
  font-weight: 600;
  color: var(--stretto-text);
}

.two {
  grid-template-columns: minmax(0, 1.4fr) minmax(0, 1fr);
  align-items: start;
}

.two > :only-child {
  grid-column: 1 / -1;
}

.pad {
  padding: 18px 20px;
}

@media (max-width: 1100px) {
  .facts {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .fact:nth-child(2n) {
    border-right: 0;
  }

  .fact:nth-child(-n + 2) {
    border-bottom: 1px solid var(--stretto-border);
  }

  .two {
    grid-template-columns: minmax(0, 1fr);
  }
}

@media (max-width: 520px) {
  .fact {
    padding: 12px 14px;
  }

  .pad {
    padding: 14px;
  }
}
</style>

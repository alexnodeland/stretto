<script setup lang="ts">
/** One job: its output as it runs, what it wrote, and the parameters it ran with. */
import { computed, nextTick, onScopeDispose, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ArrowDownToLine, FileText, FolderOpen, RotateCcw, Workflow } from '@lucide/vue'
import UiPageHeader from '@/components/ui/UiPageHeader.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiCard from '@/components/ui/UiCard.vue'
import UiCopy from '@/components/ui/UiCopy.vue'
import UiError from '@/components/ui/UiError.vue'
import UiSkeleton from '@/components/ui/UiSkeleton.vue'
import UiSegmented from '@/components/ui/UiSegmented.vue'
import UiBadge from '@/components/ui/UiBadge.vue'
import JobStatus from '@/components/JobStatus.vue'
import MarkdownView from '@/components/MarkdownView.vue'
import { api, ApiError } from '@/api/client'
import { onJob } from '@/api/events'
import type { Job, JobRequest } from '@/api/types'
import { useResource } from '@/composables/useResource'
import { useTitle } from '@/composables/useTitle'
import { readOnly } from '@/stores/auth'
import { toast } from '@/stores/toasts'
import { formatDateTime, formatDuration } from '@/lib/format'
import { isRecord } from '@/lib/json'

const route = useRoute()
const router = useRouter()
const id = computed(() => String(route.params.id))

const resource = useResource((o) => api.job(id.value, o), {
  events: ['jobs'],
  filter: (e) => e.keys.length === 0 || e.keys.includes(id.value),
})
/** The job as the latest event or load has it. */
const live = ref<Job | null>(null)
watch(resource.data, (j) => {
  if (
    j &&
    (!live.value || j.output.length >= live.value.output.length || j.status !== live.value.status)
  )
    live.value = j
})
const stop = onJob((j) => {
  if (j.id === id.value) live.value = j
})
onScopeDispose(stop)
const job = computed(() => live.value ?? resource.data.value)
useTitle(() => job.value?.title ?? 'Job')

// While a job runs, ask again now and then, in case the live stream is down.
const poll = setInterval(() => {
  const s = job.value?.status
  if (s === 'running' || s === 'queued') void resource.refresh()
}, 2500)
onScopeDispose(() => clearInterval(poll))

const running = computed(() => job.value?.status === 'running' || job.value?.status === 'queued')
const took = computed(() => {
  const j = job.value
  if (!j?.started_unix_ms) return null
  return formatDuration((j.finished_unix_ms ?? Date.now()) - j.started_unix_ms)
})

const follow = ref(true)
const out = ref<HTMLElement | null>(null)
watch(
  () => job.value?.output,
  async () => {
    if (!follow.value) return
    await nextTick()
    if (out.value) out.value.scrollTop = out.value.scrollHeight
  },
)
function onScroll() {
  const el = out.value
  if (!el) return
  follow.value = el.scrollHeight - el.scrollTop - el.clientHeight < 24
}

/** Audit and promote write a Markdown report: offer it rendered, once the job is done. */
const reportIndex = computed(() => {
  const j = job.value
  if (!j || j.status !== 'succeeded') return -1
  return j.artifacts.findIndex((a) => a.kind === 'report' && a.path.endsWith('.md'))
})
const report = ref<string | null>(null)
watch(
  [id, reportIndex],
  async ([jobId, index]) => {
    report.value = null
    if (index < 0) return
    try {
      const text = await api.jobArtifact(jobId, index, { quiet: true })
      if (jobId === id.value) report.value = text.trim() || null
    } catch {
      // The output is still there.
    }
  },
  { immediate: true },
)
const view = ref<'output' | 'report'>('report')
const shownView = computed(() => (report.value ? view.value : 'output'))

const params = computed(() => {
  const p = job.value?.params
  if (!isRecord(p)) return []
  return Object.entries(p).filter(([k]) => k !== 'kind')
})

const again = ref(false)
async function runAgain() {
  const p = job.value?.params
  if (!isRecord(p)) return
  again.value = true
  try {
    const next = await api.createJob(p as unknown as JobRequest, { quiet: true })
    void router.push({ name: 'job', params: { id: next.id } })
  } catch (e) {
    toast({
      kind: 'error',
      title: 'The job did not start',
      message: e instanceof ApiError ? e.message : String(e),
    })
  } finally {
    again.value = false
  }
}

const icons = { flow: Workflow, report: FileText, dir: FolderOpen }
</script>

<template>
  <div class="page">
    <UiPageHeader :back="{ name: 'jobs' }" back-label="Jobs">
      <template #title>
        {{ job?.title ?? id }}
        <JobStatus v-if="job" :status="job.status" />
      </template>
      <p v-if="job" class="meta">
        <UiBadge mono>{{ job.kind }}</UiBadge>
        <span>started {{ formatDateTime(job.created_unix_ms) }}</span>
        <span v-if="took">{{ running ? 'running for' : 'took' }} {{ took }}</span>
        <span v-if="job.exit_code !== null"
          >exit code <span class="mono">{{ job.exit_code }}</span></span
        >
        <span class="mono subtle">{{ job.id }}</span>
      </p>
      <template #actions>
        <UiButton
          :icon="RotateCcw"
          :loading="again"
          :disabled="readOnly || !job || running"
          :reason="readOnly ? 'The console is read-only' : undefined"
          @click="runAgain"
        >
          Run again
        </UiButton>
      </template>
    </UiPageHeader>

    <div v-if="resource.error.value && !job" class="card">
      <UiError
        :title="resource.error.value.status === 404 ? 'No such job' : undefined"
        :message="resource.error.value.message"
        :status="resource.error.value.status"
        @retry="resource.refresh()"
      />
    </div>

    <template v-else>
      <div class="grid layout">
        <UiCard
          :title="shownView === 'report' ? 'Report' : 'Output'"
          :caption="
            running ? 'Live, as the job writes it' : 'stdout and stderr, the last 64 KiB in lists'
          "
          flush
        >
          <template #actions>
            <UiSegmented
              v-if="report"
              v-model="view"
              :options="[
                { value: 'output', label: 'Output' },
                { value: 'report', label: 'Report' },
              ]"
              label="View"
              size="sm"
            />
            <button v-if="running && !follow" type="button" class="follow" @click="follow = true">
              <ArrowDownToLine :size="14" :stroke-width="2" aria-hidden="true" />Follow
            </button>
            <UiCopy v-if="job?.output" :text="job.output" what="Output" />
          </template>
          <div v-if="shownView === 'report' && report" class="report">
            <MarkdownView :text="report" />
          </div>
          <pre
            v-else-if="job"
            ref="out"
            class="terminal"
            tabindex="0"
            aria-label="Output"
            data-testid="job-output"
            @scroll="
              onScroll
            ">{{ job.output || (running ? 'Waiting for the job to start…' : 'No output.') }}<span v-if="running" class="cursor" aria-hidden="true" /></pre>
          <div v-else class="pad"><UiSkeleton :lines="8" /></div>
        </UiCard>

        <div class="stack">
          <UiCard title="What it wrote">
            <ul v-if="job?.artifacts.length" class="arts" data-testid="job-artifacts">
              <li v-for="(a, i) in job.artifacts" :key="a.path">
                <component :is="icons[a.kind]" :size="16" :stroke-width="1.8" aria-hidden="true" />
                <div>
                  <RouterLink
                    v-if="a.kind === 'flow' && a.key"
                    :to="{ name: 'flow', params: { key: a.key } }"
                    class="mono"
                    >{{ a.path }}</RouterLink
                  >
                  <a
                    v-else-if="a.kind === 'report' && !running"
                    :href="api.jobArtifactUrl(job.id, i)"
                    target="_blank"
                    rel="noopener"
                    class="mono"
                    >{{ a.path }}</a
                  >
                  <span v-else class="mono">{{ a.path }}</span>
                  <p class="caption">
                    {{
                      a.kind === 'flow'
                        ? running
                          ? 'The flow it writes.'
                          : 'A flow: open it to review what it may do.'
                        : a.kind === 'report'
                          ? a.path.endsWith('.md')
                            ? 'The report, in Markdown.'
                            : 'The report, as JSON.'
                          : 'A directory.'
                    }}
                  </p>
                </div>
              </li>
            </ul>
            <p v-else-if="job" class="muted">{{ running ? 'Nothing yet.' : 'Nothing.' }}</p>
            <UiSkeleton v-else :lines="2" />
          </UiCard>

          <UiCard title="Parameters">
            <dl v-if="params.length" class="kv">
              <template v-for="[k, v] in params" :key="k">
                <dt class="mono">{{ k }}</dt>
                <dd class="mono small">
                  {{
                    v === null ? 'default' : Array.isArray(v) ? v.join(', ') || 'none' : String(v)
                  }}
                </dd>
              </template>
            </dl>
            <p v-else-if="job" class="muted">None.</p>
            <UiSkeleton v-else :lines="3" />
          </UiCard>
        </div>
      </div>
    </template>
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

.meta .mono {
  font-size: 12px;
}

.layout {
  grid-template-columns: minmax(0, 1.8fr) minmax(0, 1fr);
  align-items: start;
}

.terminal {
  margin: 0;
  min-height: 280px;
  max-height: 620px;
  overflow: auto;
  padding: 16px 20px;
  background: var(--stretto-surface-2);
  border-radius: 0 0 var(--c-radius-card) var(--c-radius-card);
  font-family: var(--stretto-font-mono);
  font-size: 12.5px;
  line-height: 1.65;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.cursor {
  display: inline-block;
  width: 7px;
  height: 14px;
  margin-left: 2px;
  vertical-align: -2px;
  background: var(--stretto-accent-graphic);
  animation: blink 1s steps(2, start) infinite;
}

@keyframes blink {
  to {
    visibility: hidden;
  }
}

.report {
  padding: 18px 20px;
}

.follow {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 28px;
  padding: 0 9px;
  border: 1px solid var(--stretto-border);
  border-radius: 7px;
  background: var(--stretto-surface);
  color: var(--stretto-accent);
  font-size: 12.5px;
  font-weight: 500;
}

.arts {
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.arts li {
  display: flex;
  gap: 10px;
  align-items: flex-start;
}

.arts svg {
  flex: none;
  margin-top: 2px;
  color: var(--stretto-accent-graphic);
}

.arts .mono {
  font-size: 12.5px;
  overflow-wrap: anywhere;
}

.small {
  font-size: 12px;
}

.pad {
  padding: 18px 20px;
}

@media (max-width: 1100px) {
  .layout {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>

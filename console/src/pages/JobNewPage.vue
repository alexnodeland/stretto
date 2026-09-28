<script setup lang="ts">
/** Start a job: learn a flow, promote it, audit it, stage its next version, redact sessions to share, or run stretto doctor. */
import { computed, reactive, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import {
  BadgeCheck,
  Eraser,
  FileSearch,
  GitBranch,
  History,
  Play,
  Plus,
  Sparkles,
  Stethoscope,
  TriangleAlert,
  X,
} from '@lucide/vue'
import UiPageHeader from '@/components/ui/UiPageHeader.vue'
import UiField from '@/components/ui/UiField.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiSwitch from '@/components/ui/UiSwitch.vue'
import UiCode from '@/components/ui/UiCode.vue'
import { api, ApiError } from '@/api/client'
import type { DeciderName, Job, JobKind, JobRequest } from '@/api/types'
import { useResource } from '@/composables/useResource'
import { useTitle } from '@/composables/useTitle'
import { meta, readOnly } from '@/stores/auth'
import { domainNames } from '@/stores/domain'
import { commandText } from '@/lib/format'
import { checkServerName } from '@/lib/validate'
import { isRecord } from '@/lib/json'

useTitle(() => 'New job')
const route = useRoute()
const router = useRouter()
const q = (k: string) => (typeof route.query[k] === 'string' ? (route.query[k] as string) : '')

const kinds: { value: JobKind; label: string; icon: typeof Sparkles; body: string }[] = [
  { value: 'learn', label: 'Learn', icon: Sparkles, body: 'Learn a flow from recorded sessions.' },
  {
    value: 'promote',
    label: 'Promote',
    icon: BadgeCheck,
    body: 'Keep a flow to the sites where its lookups were the agent’s own.',
  },
  {
    value: 'audit',
    label: 'Audit',
    icon: FileSearch,
    body: 'Score a flow against sessions it did not learn from.',
  },
  {
    value: 'stage',
    label: 'Stage',
    icon: GitBranch,
    body: 'Learn a flow’s next version beside it, and compare the two.',
  },
  {
    value: 'redact',
    label: 'Redact',
    icon: Eraser,
    body: 'Write a pseudonymized copy of sessions, to share.',
  },
  { value: 'doctor', label: 'Doctor', icon: Stethoscope, body: 'Check the installation.' },
]
const kind = ref<JobKind>(
  (kinds.some((k) => k.value === q('kind')) ? q('kind') : 'learn') as JobKind,
)
watch(kind, (k) => void router.replace({ query: { ...route.query, kind: k } }))

const flows = useResource((o) => api.flows(o), { events: ['flows'] })
const sessions = useResource((o) => api.sessions({ limit: 200 }, o), { events: ['sessions'] })
const goodFlows = computed(() => flows.data.value?.items.filter((f) => !f.error) ?? [])
const dirs = computed(() => {
  const set = new Set<string>()
  for (const s of sessions.data.value?.items ?? [])
    set.add(s.path.split('/').slice(0, -1).join('/'))
  return [...set].sort()
})
const domains = computed(() =>
  [...new Set([...domainNames.value, ...dirs.value.map((d) => d.split('/').pop() ?? '')])]
    .filter(Boolean)
    .sort(),
)

const learn = reactive({
  domain: q('domain'),
  sessions: '',
  out: '',
  overwrite: false,
  habit_only: true,
  constants: false,
})
const promote = reactive({
  flow: q('flow'),
  sessions: '',
  oracle_cache: '',
  threshold: 0.3,
  min_used: 0.7,
  min_lower: 0.5,
  min_tasks: 3,
  out: '',
  overwrite: false,
})
const audit = reactive({ flow: q('flow'), sessions: '', decider: '' as '' | DeciderName })
const stage = reactive({
  flow: q('flow'),
  sessions: '',
  window: 50,
  decider: '' as '' | DeciderName,
  half_life: null as number | null,
  constants: false,
})
const redact = reactive({ sessions: '', out: '', keep_shared: 3, hash_fields: [] as string[] })
const hashDraft = ref('')

/**
 * "Run again" on a job that writes where you say opens this form with that
 * job's parameters (`?from=<id>`), so that its path can change, or the file
 * be replaced.
 */
const from = ref<Job | null>(null)
if (q('from')) {
  api
    .job(q('from'), { quiet: true })
    .then((j) => {
      from.value = j
      fill(j.params)
    })
    .catch(() => {
      // The form stays as it is.
    })
}
function fill(p: unknown) {
  if (!isRecord(p)) return
  const str = (v: unknown) => (typeof v === 'string' ? v : '')
  const num = (v: unknown, d: number) => (typeof v === 'number' ? v : d)
  switch (p.kind) {
    case 'learn':
      kind.value = 'learn'
      Object.assign(learn, {
        domain: str(p.domain),
        sessions: str(p.sessions),
        out: str(p.out),
        overwrite: p.overwrite === true,
        habit_only: p.habit_only !== false,
        constants: p.constants === true,
      })
      break
    case 'promote':
      kind.value = 'promote'
      Object.assign(promote, {
        flow: str(p.flow),
        sessions: str(p.sessions),
        oracle_cache: str(p.oracle_cache),
        threshold: num(p.threshold, promote.threshold),
        min_used: num(p.min_used, promote.min_used),
        min_lower: num(p.min_lower, promote.min_lower),
        min_tasks: num(p.min_tasks, promote.min_tasks),
        out: str(p.out),
        overwrite: p.overwrite === true,
      })
      break
    case 'audit':
      kind.value = 'audit'
      Object.assign(audit, {
        flow: str(p.flow),
        sessions: str(p.sessions),
        decider: str(p.decider) as '' | DeciderName,
      })
      break
    case 'stage':
      kind.value = 'stage'
      Object.assign(stage, {
        flow: str(p.flow),
        sessions: str(p.sessions),
        window: num(p.window, stage.window),
        decider: str(p.decider) as '' | DeciderName,
        half_life: typeof p.half_life === 'number' ? p.half_life : null,
        constants: p.constants === true,
      })
      break
    case 'redact':
      kind.value = 'redact'
      Object.assign(redact, {
        sessions: str(p.sessions),
        out: str(p.out),
        keep_shared: num(p.keep_shared, redact.keep_shared),
        hash_fields: Array.isArray(p.hash_fields)
          ? p.hash_fields.filter((f): f is string => typeof f === 'string')
          : [],
      })
      break
    case 'doctor':
      kind.value = 'doctor'
  }
}

const flowOf = (key: string) => goodFlows.value.find((f) => f.key === key)
const learnSessions = computed(() => learn.sessions || (learn.domain ? `logs/${learn.domain}` : ''))
const promoteSessions = computed(
  () => promote.sessions || (flowOf(promote.flow) ? `shadow/${flowOf(promote.flow)!.domain}` : ''),
)
const auditSessions = computed(
  () => audit.sessions || (flowOf(audit.flow) ? `logs/${flowOf(audit.flow)!.domain}` : ''),
)
/** The half-life asked for: an emptied number field holds '', which is none. */
const halfLife = computed(() => (typeof stage.half_life === 'number' ? stage.half_life : null))
const stageSessions = computed(
  () => stage.sessions || (flowOf(stage.flow) ? `logs/${flowOf(stage.flow)!.domain}` : ''),
)
const redactOut = computed(
  () =>
    redact.out ||
    (redact.sessions ? `redacted/${redact.sessions.split('/').filter(Boolean).pop()}` : ''),
)

const tried = ref(false)
const errors = computed(() => {
  const e: Record<string, string> = {}
  if (kind.value === 'learn') {
    const n = checkServerName(learn.domain)
    if (n) e.domain = n.replace('Give the server a name.', 'Name the domain.')
    if (!learnSessions.value) e.sessions = 'Give the directory of sessions.'
  } else if (kind.value === 'promote') {
    if (!promote.flow) e.flow = 'Choose the flow to promote.'
    if (!promoteSessions.value) e.sessions = 'Give the directory of sessions.'
    for (const [k, v] of [
      ['threshold', promote.threshold],
      ['min_used', promote.min_used],
      ['min_lower', promote.min_lower],
    ] as const) {
      if (!(v >= 0 && v <= 1)) e[k] = 'A number from 0 to 1.'
    }
    if (!(promote.min_tasks >= 1 && Number.isInteger(promote.min_tasks)))
      e.min_tasks = 'A whole number, 1 or more.'
  } else if (kind.value === 'audit') {
    if (!audit.flow) e.flow = 'Choose the flow to audit.'
    if (!auditSessions.value) e.sessions = 'Give the directory of sessions.'
  } else if (kind.value === 'stage') {
    if (!stage.flow) e.flow = 'Choose the flow to stage.'
    if (!stageSessions.value) e.sessions = 'Give the directory of sessions.'
    if (!(stage.window >= 1 && Number.isInteger(stage.window)))
      e.window = 'A whole number, 1 or more.'
    if (halfLife.value !== null && !(halfLife.value > 0))
      e.half_life = 'A number of sessions, more than 0.'
  } else if (kind.value === 'redact') {
    if (!redact.sessions) e.sessions = 'Give the directory of sessions.'
    if (!redactOut.value) e.out = 'Give a directory to write to.'
    if (!(redact.keep_shared >= 1 && Number.isInteger(redact.keep_shared)))
      e.keep_shared = 'A whole number, 1 or more.'
  }
  return e
})
const err = (k: string) => (tried.value ? (errors.value[k] ?? null) : null)

const body = computed<JobRequest>(() => {
  switch (kind.value) {
    case 'learn':
      return {
        kind: 'learn',
        domain: learn.domain.trim(),
        sessions: learnSessions.value,
        out: learn.out.trim() || null,
        overwrite: learn.overwrite,
        habit_only: learn.habit_only,
        constants: learn.constants,
      }
    case 'promote':
      return {
        kind: 'promote',
        flow: promote.flow,
        sessions: promoteSessions.value,
        oracle_cache: promote.oracle_cache.trim() || null,
        threshold: promote.threshold,
        min_used: promote.min_used,
        min_lower: promote.min_lower,
        min_tasks: promote.min_tasks,
        out: promote.out.trim() || null,
        overwrite: promote.overwrite,
      }
    case 'audit':
      return {
        kind: 'audit',
        flow: audit.flow,
        sessions: auditSessions.value,
        decider: audit.decider || null,
      }
    case 'stage':
      return {
        kind: 'stage',
        flow: stage.flow,
        sessions: stageSessions.value,
        window: stage.window,
        decider: stage.decider || null,
        half_life: halfLife.value,
        constants: stage.constants,
      }
    case 'redact':
      return {
        kind: 'redact',
        sessions: redact.sessions.trim(),
        out: redactOut.value,
        keep_shared: redact.keep_shared,
        hash_fields: [...redact.hash_fields],
      }
    default:
      return { kind: 'doctor' }
  }
})

const preview = computed(() => {
  const b = body.value
  const flowPath = (key: string) => flowOf(key)?.path ?? '<flow>'
  switch (b.kind) {
    case 'learn':
      return commandText([
        'stretto',
        'learn',
        '--sessions',
        b.sessions || '<dir>',
        '--domain',
        b.domain || '<domain>',
        ...(b.habit_only ? ['--habit-only'] : []),
        ...(b.constants ? ['--constants'] : []),
        '--out',
        b.out ?? `${b.domain || '<domain>'}.flow.json`,
      ])
    case 'promote':
      return commandText([
        'stretto',
        'promote',
        '--flow',
        flowPath(b.flow),
        '--sessions',
        b.sessions || '<dir>',
        ...(b.oracle_cache ? ['--oracle-cache', b.oracle_cache] : []),
        '--threshold',
        String(b.threshold),
        '--min-used',
        String(b.min_used),
        '--min-lower',
        String(b.min_lower),
        '--min-tasks',
        String(b.min_tasks),
        '--out',
        b.out ?? `${flowOf(b.flow)?.name ?? '<name>'}.promoted.flow.json`,
      ])
    case 'audit':
      return commandText([
        'stretto',
        'audit',
        '--flow',
        flowPath(b.flow),
        '--sessions',
        b.sessions || '<dir>',
        ...(b.decider ? ['--decider', b.decider] : []),
        '--json',
        '<report>',
      ])
    case 'stage':
      return commandText([
        'stretto',
        'stage',
        '--flow',
        flowPath(b.flow),
        '--sessions',
        b.sessions || '<dir>',
        '--window',
        String(b.window),
        ...(b.decider ? ['--decider', b.decider] : []),
        ...(b.half_life ? ['--half-life', String(b.half_life)] : []),
        ...(b.constants ? ['--constants'] : []),
      ])
    case 'redact':
      return commandText([
        'stretto',
        'redact',
        '--sessions',
        b.sessions || '<dir>',
        '--out',
        b.out || '<dir>',
        '--keep-shared',
        String(b.keep_shared),
        ...(b.hash_fields?.length ? ['--hash-field', b.hash_fields.join(',')] : []),
      ])
    default:
      return 'stretto doctor'
  }
})

function addHash() {
  const f = hashDraft.value.trim()
  if (f && !redact.hash_fields.includes(f))
    redact.hash_fields.push(
      ...f
        .split(',')
        .map((x) => x.trim())
        .filter(Boolean),
    )
  hashDraft.value = ''
}

const busy = ref(false)
const serverError = ref<string | null>(null)
async function submit() {
  tried.value = true
  if (Object.keys(errors.value).length) return
  busy.value = true
  serverError.value = null
  try {
    const job = await api.createJob(body.value, { quiet: true })
    void router.push({ name: 'job', params: { id: job.id } })
  } catch (e) {
    serverError.value = e instanceof ApiError ? e.message : String(e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div class="page narrow">
    <UiPageHeader title="New job" :back="{ name: 'jobs' }" back-label="Jobs">
      Runs the stretto CLI in the data dir,
      <span class="mono">{{ meta?.data_dir ?? '~/.stretto' }}</span
      >. Paths are relative to it, or start with <span class="mono">~/</span>.
    </UiPageHeader>

    <div class="kinds" role="radiogroup" aria-label="What to run">
      <label v-for="k in kinds" :key="k.value" class="kind" :class="{ on: kind === k.value }">
        <input
          v-model="kind"
          type="radio"
          name="kind"
          :value="k.value"
          class="sr-only"
          :data-testid="`kind-${k.value}`"
        />
        <component :is="k.icon" :size="17" :stroke-width="1.8" aria-hidden="true" />
        <span class="kind-label">{{ k.label }}</span>
        <span class="kind-body">{{ k.body }}</span>
      </label>
    </div>

    <form class="jf card" novalidate @submit.prevent="submit">
      <p v-if="from" class="notice accent" data-testid="job-from">
        <History :size="16" :stroke-width="2" aria-hidden="true" />
        <span
          >The parameters of
          <RouterLink :to="{ name: 'job', params: { id: from.id } }">{{ from.title }}</RouterLink
          >. Change what it writes, or let it replace the file.</span
        >
      </p>
      <p v-if="serverError" class="notice danger" role="alert">
        <TriangleAlert :size="16" :stroke-width="2" aria-hidden="true" />
        <span>{{ serverError }}</span>
      </p>

      <template v-if="kind === 'learn'">
        <div class="row2">
          <UiField
            v-slot="{ id, describedby, invalid }"
            label="Domain"
            hint="The flow’s domain: the server’s name."
            :error="err('domain')"
          >
            <input
              :id="id"
              v-model="learn.domain"
              class="input mono"
              list="job-domains"
              :aria-describedby="describedby"
              :aria-invalid="invalid"
              placeholder="shop"
              spellcheck="false"
              data-testid="learn-domain"
            />
          </UiField>
          <UiField
            v-slot="{ id, describedby, invalid }"
            label="Sessions"
            hint="A directory of session logs."
            :error="err('sessions')"
          >
            <input
              :id="id"
              v-model="learn.sessions"
              class="input mono"
              list="job-dirs"
              :aria-describedby="describedby"
              :aria-invalid="invalid"
              :placeholder="learn.domain ? `logs/${learn.domain}` : 'logs/<domain>'"
              spellcheck="false"
            />
          </UiField>
        </div>
        <UiField
          v-slot="{ id, describedby }"
          label="Write the flow to"
          optional
          :hint="`Default ${learn.domain || '<domain>'}.flow.json in the data dir. An existing flow is kept unless you replace it.`"
        >
          <input
            :id="id"
            v-model="learn.out"
            class="input mono"
            :aria-describedby="describedby"
            :placeholder="`${learn.domain || '<domain>'}.flow.json`"
            spellcheck="false"
          />
        </UiField>
        <div class="switches">
          <UiSwitch
            v-model="learn.habit_only"
            label="With no key (--habit-only)"
            hint="Ask no System-One model: the flow has no arbiter and is served with reach."
          />
          <UiSwitch
            v-model="learn.constants"
            label="Constants (--constants)"
            hint="Also pass arguments the agent always passed with one value, such as a page size."
          />
          <UiSwitch
            v-model="learn.overwrite"
            label="Replace an existing flow"
            hint="Without it, the job refuses to write over a flow."
          />
        </div>
        <p v-if="!learn.habit_only && !meta?.key_set" class="notice warn">
          <TriangleAlert :size="16" :stroke-width="2" aria-hidden="true" />
          <span
            >Fitting an arbiter asks Jev, and TYPESAFE_API_KEY is not set in the console’s
            environment: this job will fail.</span
          >
        </p>
      </template>

      <template v-else-if="kind === 'promote' || kind === 'audit'">
        <div class="row2">
          <UiField v-slot="{ id, describedby, invalid }" label="Flow" :error="err('flow')">
            <select
              v-if="kind === 'promote'"
              :id="id"
              v-model="promote.flow"
              class="select"
              :aria-describedby="describedby"
              :aria-invalid="invalid"
              data-testid="job-flow"
            >
              <option value="">Choose a flow</option>
              <option v-for="f in goodFlows" :key="f.key" :value="f.key">
                {{ f.key }} · {{ f.domain }}
              </option>
            </select>
            <select
              v-else
              :id="id"
              v-model="audit.flow"
              class="select"
              :aria-describedby="describedby"
              :aria-invalid="invalid"
              data-testid="job-flow"
            >
              <option value="">Choose a flow</option>
              <option v-for="f in goodFlows" :key="f.key" :value="f.key">
                {{ f.key }} · {{ f.domain }}
              </option>
            </select>
          </UiField>
          <UiField
            v-slot="{ id, describedby, invalid }"
            label="Sessions"
            :hint="
              kind === 'promote'
                ? 'Sessions recorded with the flow in shadow, usually.'
                : 'Sessions the flow did not learn from.'
            "
            :error="err('sessions')"
          >
            <input
              v-if="kind === 'promote'"
              :id="id"
              v-model="promote.sessions"
              class="input mono"
              list="job-dirs"
              :aria-describedby="describedby"
              :aria-invalid="invalid"
              :placeholder="promoteSessions || 'shadow/<domain>'"
              spellcheck="false"
            />
            <input
              v-else
              :id="id"
              v-model="audit.sessions"
              class="input mono"
              list="job-dirs"
              :aria-describedby="describedby"
              :aria-invalid="invalid"
              :placeholder="auditSessions || 'logs/<domain>'"
              spellcheck="false"
            />
          </UiField>
        </div>
        <template v-if="kind === 'promote'">
          <fieldset class="bar">
            <legend class="label">The bar a site must meet</legend>
            <div class="row4">
              <UiField v-slot="{ id }" label="Threshold" :error="err('threshold')">
                <input
                  :id="id"
                  v-model.number="promote.threshold"
                  class="input num"
                  type="number"
                  min="0"
                  max="1"
                  step="0.01"
                />
              </UiField>
              <UiField v-slot="{ id }" label="Least share used" :error="err('min_used')">
                <input
                  :id="id"
                  v-model.number="promote.min_used"
                  class="input num"
                  type="number"
                  min="0"
                  max="1"
                  step="0.05"
                />
              </UiField>
              <UiField v-slot="{ id }" label="Least lower bound" :error="err('min_lower')">
                <input
                  :id="id"
                  v-model.number="promote.min_lower"
                  class="input num"
                  type="number"
                  min="0"
                  max="1"
                  step="0.05"
                />
              </UiField>
              <UiField v-slot="{ id }" label="Fewest tasks" :error="err('min_tasks')">
                <input
                  :id="id"
                  v-model.number="promote.min_tasks"
                  class="input num"
                  type="number"
                  min="1"
                  step="1"
                />
              </UiField>
            </div>
            <p class="caption">
              The threshold is the one the flow will be served with. The lower bound is Wilson’s,
              90% two-sided.
            </p>
          </fieldset>
          <div class="row2">
            <UiField
              v-slot="{ id, describedby }"
              label="Oracle cache"
              optional
              hint="For a flow with an arbiter: the proxy’s cache from the shadow sessions."
            >
              <input
                :id="id"
                v-model="promote.oracle_cache"
                class="input mono"
                :aria-describedby="describedby"
                placeholder="oracle-cache"
                spellcheck="false"
              />
            </UiField>
            <UiField v-slot="{ id }" label="Write the promoted flow to" optional>
              <input
                :id="id"
                v-model="promote.out"
                class="input mono"
                :placeholder="`${flowOf(promote.flow)?.name ?? '<name>'}.promoted.flow.json`"
                spellcheck="false"
              />
            </UiField>
          </div>
          <UiSwitch
            v-model="promote.overwrite"
            label="Replace an existing flow"
            hint="Without it, the job refuses to write over a flow."
          />
        </template>
        <UiField v-else v-slot="{ id }" label="Decider" optional>
          <select :id="id" v-model="audit.decider" class="select">
            <option value="">As the flow is served by default</option>
            <option value="reach">reach</option>
            <option value="habit">habit</option>
            <option value="arbiter">arbiter (asks the replay cache)</option>
          </select>
        </UiField>
      </template>

      <template v-else-if="kind === 'stage'">
        <div class="row2">
          <UiField
            v-slot="{ id, describedby, invalid }"
            label="Flow"
            hint="The committed flow: the staged one is learned beside it."
            :error="err('flow')"
          >
            <select
              :id="id"
              v-model="stage.flow"
              class="select"
              :aria-describedby="describedby"
              :aria-invalid="invalid"
              data-testid="job-flow"
            >
              <option value="">Choose a flow</option>
              <option v-for="f in goodFlows" :key="f.key" :value="f.key">
                {{ f.key }} · {{ f.domain }}
              </option>
            </select>
          </UiField>
          <UiField
            v-slot="{ id, describedby, invalid }"
            label="Sessions"
            hint="Where the proxy records the flow’s sessions, served or in shadow."
            :error="err('sessions')"
          >
            <input
              :id="id"
              v-model="stage.sessions"
              class="input mono"
              list="job-dirs"
              :aria-describedby="describedby"
              :aria-invalid="invalid"
              :placeholder="stageSessions || 'logs/<domain>'"
              spellcheck="false"
            />
          </UiField>
        </div>
        <div class="row2">
          <UiField
            v-slot="{ id, describedby }"
            label="Compare on the last"
            hint="Sessions both flows were scored on (--window)."
            :error="err('window')"
          >
            <input
              :id="id"
              v-model.number="stage.window"
              class="input num"
              type="number"
              min="1"
              step="1"
              :aria-describedby="describedby"
            />
          </UiField>
          <UiField
            v-slot="{ id, describedby }"
            label="Forget old sessions"
            optional
            hint="A half-life, in sessions: one this many older than the newest counts half."
            :error="err('half_life')"
          >
            <input
              :id="id"
              v-model.number="stage.half_life"
              class="input num"
              type="number"
              min="1"
              step="1"
              placeholder="never"
              :aria-describedby="describedby"
            />
          </UiField>
        </div>
        <UiField v-slot="{ id }" label="Decider" optional>
          <select :id="id" v-model="stage.decider" class="select">
            <option value="">Each flow as it is served by default</option>
            <option value="reach">reach</option>
            <option value="habit">habit</option>
            <option value="arbiter">arbiter (asks the replay cache)</option>
          </select>
        </UiField>
        <UiSwitch
          v-model="stage.constants"
          label="Constants (--constants)"
          hint="Also pass arguments the agent always passed with one value, as learn does."
        />
      </template>

      <template v-else-if="kind === 'redact'">
        <div class="row2">
          <UiField v-slot="{ id, describedby, invalid }" label="Sessions" :error="err('sessions')">
            <input
              :id="id"
              v-model="redact.sessions"
              class="input mono"
              list="job-dirs"
              :aria-describedby="describedby"
              :aria-invalid="invalid"
              placeholder="logs/shop"
              spellcheck="false"
            />
          </UiField>
          <UiField
            v-slot="{ id, describedby, invalid }"
            label="Write the copy to"
            :error="err('out')"
          >
            <input
              :id="id"
              v-model="redact.out"
              class="input mono"
              :aria-describedby="describedby"
              :aria-invalid="invalid"
              :placeholder="redactOut || 'redacted/<dir>'"
              spellcheck="false"
            />
          </UiField>
        </div>
        <div class="row2">
          <UiField
            v-slot="{ id, describedby }"
            label="Keep values this many sessions share"
            :error="err('keep_shared')"
            hint="A value fewer sessions contain becomes a salted hash."
          >
            <input
              :id="id"
              v-model.number="redact.keep_shared"
              class="input num"
              type="number"
              min="1"
              step="1"
              :aria-describedby="describedby"
            />
          </UiField>
          <UiField
            v-slot="{ id, describedby }"
            label="Always hash these fields"
            optional
            hint="JSON keys whose values identify people, such as user_id or email."
          >
            <div class="hash">
              <div v-if="redact.hash_fields.length" class="chips">
                <span v-for="f in redact.hash_fields" :key="f" class="chip">
                  {{ f }}
                  <button
                    type="button"
                    class="chip-x"
                    :aria-label="`Remove ${f}`"
                    @click="redact.hash_fields = redact.hash_fields.filter((x) => x !== f)"
                  >
                    <X :size="12" :stroke-width="2.4" />
                  </button>
                </span>
              </div>
              <div class="hash-add">
                <input
                  :id="id"
                  v-model="hashDraft"
                  class="input mono"
                  :aria-describedby="describedby"
                  placeholder="user_id"
                  spellcheck="false"
                  @keydown.enter.prevent="addHash"
                />
                <UiButton :icon="Plus" @click="addHash">Add</UiButton>
              </div>
            </div>
          </UiField>
        </div>
        <p class="caption">
          The salt comes from STRETTO_REDACT_SALT in the console’s environment. Keep it secret, and
          the same for copies whose hashes should match.
        </p>
      </template>

      <p v-else class="muted">
        Checks the versions of stretto-proxy, stretto-procedure and stretto-mcp-demo, whether the
        data dir is writable, whether a key is set (never its value), and the flows and sessions in
        the data dir. It makes no network request.
      </p>

      <div class="runs">
        <p class="label">Runs</p>
        <UiCode :code="preview" language="shell" what="Command" wrap />
      </div>

      <div class="actions">
        <UiButton :to="{ name: 'jobs' }">Cancel</UiButton>
        <UiButton
          type="submit"
          variant="primary"
          :icon="Play"
          :loading="busy"
          :disabled="readOnly"
          reason="The console is read-only"
          data-testid="job-start"
          >Start the job</UiButton
        >
      </div>
    </form>

    <datalist id="job-dirs">
      <option v-for="d in dirs" :key="d" :value="d" />
    </datalist>
    <datalist id="job-domains">
      <option v-for="d in domains" :key="d" :value="d" />
    </datalist>
  </div>
</template>

<style scoped>
.narrow {
  max-width: 920px;
}

.kinds {
  display: grid;
  grid-template-columns: repeat(5, minmax(0, 1fr));
  gap: 10px;
}

.kind {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 12px 14px;
  border-radius: 12px;
  border: 1px solid var(--stretto-border);
  background: var(--stretto-surface);
  color: var(--stretto-text-muted);
  cursor: pointer;
  transition:
    border-color 0.12s var(--c-ease),
    box-shadow 0.12s var(--c-ease);
}

.kind:hover {
  border-color: color-mix(in oklab, var(--stretto-border-strong) 55%, var(--stretto-border));
}

.kind.on {
  border-color: var(--stretto-accent-graphic);
  box-shadow: 0 0 0 3px color-mix(in oklab, var(--stretto-accent-graphic) 16%, transparent);
  background: color-mix(in oklab, var(--stretto-accent-soft) 45%, var(--stretto-surface));
  color: var(--stretto-accent);
}

.kind:has(input:focus-visible) {
  outline: 2px solid var(--stretto-focus);
  outline-offset: 2px;
}

.kind-label {
  font-size: 14px;
  font-weight: 600;
  color: var(--stretto-text);
}

.kind-body {
  font-size: 12.5px;
  color: var(--stretto-text-muted);
}

.jf {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 20px;
}

.row2 {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 14px;
}

.row4 {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 12px;
}

.bar {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin: 0;
  padding: 14px;
  border: 1px solid var(--stretto-border);
  border-radius: 10px;
  min-width: 0;
}

.bar legend {
  padding: 0 6px;
}

.switches {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.hash {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.hash-add {
  display: flex;
  gap: 8px;
}

.chip-x {
  display: grid;
  place-items: center;
  width: 16px;
  height: 16px;
  margin-right: -3px;
  border: 0;
  border-radius: 4px;
  background: none;
  color: var(--stretto-text-subtle);
}

.runs {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding-top: 4px;
}

@media (max-width: 900px) {
  .kinds {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  .row4 {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}

@media (max-width: 720px) {
  .kinds {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .kind-body {
    display: none;
  }

  .row2 {
    grid-template-columns: minmax(0, 1fr);
  }

  .jf {
    padding: 16px;
  }
}
</style>

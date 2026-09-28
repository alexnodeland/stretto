<script setup lang="ts">
/**
 * A server's setup: its name (the domain), where it is (a command or an
 * HTTP URL, with the variables it needs by name only), what the proxy does
 * with it (record, shadow, serve), the flow and its settings, and the
 * guards, the confirmation judge, stretto_commit and how long sessions are
 * kept.
 */
import { computed, reactive, ref, toRaw, watch } from 'vue'
import { Globe, Plus, SquareTerminal, X } from '@lucide/vue'
import UiField from '../ui/UiField.vue'
import UiSegmented from '../ui/UiSegmented.vue'
import UiButton from '../ui/UiButton.vue'
import UiCode from '../ui/UiCode.vue'
import type { DeciderName, FlowSummary, JudgeMode, ServerInput, ServerMode } from '@/api/types'
import { splitCommand } from '@/lib/shell'
import { checkEnvName, checkServer, GUARDED_DOMAINS, type ServerErrors } from '@/lib/validate'
import { commandText } from '@/lib/format'

export interface ServerModel {
  name: string
  description: string
  kind: 'stdio' | 'http'
  command: string
  env: string[]
  url: string
  headers: { name: string; env: string }[]
  mode: ServerMode
  flow: string
  record_dir: string
  decider: '' | DeciderName
  /** As typed: a number input's v-model gives a number, an empty one a string. */
  threshold: string | number
  guards: boolean
  /** The confirmation judge on guarded writes: off, or how it acts. */
  judge: '' | JudgeMode
  /** The file the host appends the conversation to, which the judge reads. */
  context: string
  commit: boolean
  /** Days to keep sessions, as typed. */
  retain_days: string | number
}

const props = defineProps<{
  initial: ServerModel
  editing: boolean
  taken: string[]
  flows: FlowSummary[]
  dataDir: string
  saving: boolean
  serverError: string | null
}>()
const emit = defineEmits<{ submit: [input: ServerInput]; cancel: [] }>()

/** A copy the form can change: the model is plain data (strings, and lists of them). */
const copy = (v: ServerModel): ServerModel => JSON.parse(JSON.stringify(toRaw(v))) as ServerModel
const m = reactive<ServerModel>(copy(props.initial))
watch(
  () => props.initial,
  (v) => Object.assign(m, copy(v)),
)
const tried = ref(false)
const envDraft = ref('')
const envError = ref<string | null>(null)

const split = computed(() => splitCommand(m.command))
const flowPath = (f: FlowSummary) =>
  props.dataDir.endsWith('/.stretto') ? `~/.stretto/${f.path}` : `${props.dataDir}/${f.path}`
const flowOptions = computed(() =>
  props.flows
    .filter((f) => !f.error)
    .map((f) => ({
      value: flowPath(f),
      label: `${f.key}${f.domain !== m.name ? ` (domain ${f.domain})` : ''}`,
      domain: f.domain,
    }))
    .sort((a, b) => Number(b.domain === m.name) - Number(a.domain === m.name)),
)
const flowIsOther = ref(!!m.flow && !flowOptions.value.some((o) => o.value === m.flow))

const guarded = computed(() => GUARDED_DOMAINS.includes(m.name.trim()))

function toInput(): ServerInput {
  const typed = String(m.threshold ?? '').trim()
  const threshold = typed === '' ? null : Number(typed)
  const days = String(m.retain_days ?? '').trim()
  return {
    name: m.name.trim(),
    description: m.description.trim() || null,
    upstream:
      m.kind === 'stdio'
        ? { kind: 'stdio', command: split.value.words, env: [...m.env] }
        : {
            kind: 'http',
            url: m.url.trim(),
            headers: m.headers
              .filter((h) => h.name || h.env)
              .map((h) => ({ name: h.name.trim(), env: h.env.trim() })),
          },
    mode: m.mode,
    flow: m.mode === 'record' ? null : m.flow.trim() || null,
    record_dir: m.record_dir.trim() || null,
    decider: m.mode === 'record' ? null : m.decider || null,
    threshold: m.mode === 'record' ? null : threshold,
    guards: m.guards,
    judge: m.guards && m.judge ? { mode: m.judge, context: m.context.trim() } : null,
    commit: m.commit,
    retain_days: days === '' ? null : Number(days),
  }
}

const errors = computed<ServerErrors>(() => {
  const e = checkServer(toInput(), props.editing ? [] : props.taken)
  if (m.kind === 'stdio' && split.value.error) e.command = split.value.error
  return e
})
const touched = reactive(new Set<keyof ServerErrors>())
const shown = (field: keyof ServerErrors) =>
  tried.value || touched.has(field) ? (errors.value[field] ?? null) : null

function onFlowChange(event: Event) {
  const value = (event.target as HTMLSelectElement).value
  if (value === '__other') {
    flowIsOther.value = true
    m.flow = ''
  } else {
    m.flow = value
  }
  touched.add('flow')
}

const defaultRecord = computed(
  () => `~/.stretto/${m.mode === 'shadow' ? 'shadow' : 'logs'}/${m.name || '<name>'}`,
)
const preview = computed(() => {
  const i = toInput()
  const args = [
    'stretto-proxy',
    '--record',
    i.record_dir ?? defaultRecord.value,
    '--domain',
    i.name || '<name>',
  ]
  if (i.flow) {
    args.push('--flow', i.flow)
    if (i.decider) args.push('--flow-decider', i.decider)
    if (i.threshold !== null && !Number.isNaN(i.threshold))
      args.push('--flow-threshold', String(i.threshold))
    if (i.mode === 'shadow') args.push('--flow-shadow')
  }
  if (i.retain_days !== null && i.retain_days !== undefined && !Number.isNaN(i.retain_days))
    args.push('--retain-days', String(i.retain_days))
  if (i.guards) args.push('--guards')
  if (i.judge) args.push('--confirm-judge', i.judge.mode, '--context', i.judge.context || '<file>')
  if (i.commit) args.push('--commit')
  if (i.upstream.kind === 'http') {
    args.push('--upstream', i.upstream.url || '<url>')
    for (const h of i.upstream.headers) args.push('--upstream-header', `${h.name}=${h.env}`)
  } else {
    args.push('--', ...(i.upstream.command.length ? i.upstream.command : ['<command>']))
  }
  return commandText(args)
})

function addEnv() {
  const name = envDraft.value.trim()
  if (!name) return
  envError.value = checkEnvName(name)
  if (envError.value) return
  if (!m.env.includes(name)) m.env.push(name)
  envDraft.value = ''
}

function submit() {
  tried.value = true
  if (Object.keys(errors.value).length) {
    requestAnimationFrame(() =>
      document.querySelector<HTMLElement>('[aria-invalid="true"]')?.focus(),
    )
    return
  }
  emit('submit', toInput())
}

const modes: { value: ServerMode; title: string; body: string }[] = [
  {
    value: 'record',
    title: 'Record',
    body: 'Forward every message and record each session. No flow runs. Start here.',
  },
  {
    value: 'shadow',
    title: 'Shadow',
    body: 'Run a flow that decides and logs what it would look up, and looks nothing up: the sessions stretto promote scores.',
  },
  {
    value: 'serve',
    title: 'Serve',
    body: 'Run a flow: after each of the agent’s calls, its lookups ride in the same result.',
  },
]
</script>

<template>
  <form class="sf" novalidate @submit.prevent="submit">
    <p v-if="serverError" class="notice danger" role="alert">{{ serverError }}</p>

    <section class="sf-section">
      <h2 class="sf-h">The server</h2>
      <div class="sf-row">
        <UiField
          v-slot="{ id, describedby, invalid }"
          label="Name"
          hint="The name the host gives the server, which is also the domain of its sessions and flows: lowercase letters, digits, - and _."
          :error="shown('name')"
        >
          <input
            :id="id"
            v-model="m.name"
            class="input mono"
            :aria-describedby="describedby"
            :aria-invalid="invalid"
            :disabled="editing"
            autocomplete="off"
            spellcheck="false"
            placeholder="orders"
            data-testid="server-name"
            @blur="touched.add('name')"
          />
        </UiField>
        <UiField v-slot="{ id, describedby }" label="Description" optional>
          <input
            :id="id"
            v-model="m.description"
            class="input"
            :aria-describedby="describedby"
            placeholder="What it serves"
          />
        </UiField>
      </div>
    </section>

    <section class="sf-section">
      <h2 class="sf-h">Where it runs</h2>
      <UiSegmented
        v-model="m.kind"
        :options="[
          { value: 'stdio', label: 'A command (stdio)', icon: SquareTerminal },
          { value: 'http', label: 'A URL (Streamable HTTP)', icon: Globe },
        ]"
        label="Upstream"
      />
      <template v-if="m.kind === 'stdio'">
        <UiField
          v-slot="{ id, describedby, invalid }"
          label="Command"
          hint="The server’s command and its arguments, as a shell reads them. The proxy starts it and inherits the host’s environment."
          :error="shown('command')"
        >
          <input
            :id="id"
            v-model="m.command"
            class="input mono"
            :aria-describedby="describedby"
            :aria-invalid="invalid"
            autocomplete="off"
            spellcheck="false"
            placeholder="npx -y @your/mcp-server"
            data-testid="server-command"
            @blur="touched.add('command')"
          />
        </UiField>
        <div
          v-if="split.words.length > 1"
          class="chips words"
          aria-label="The command, word by word"
        >
          <span v-for="(w, i) in split.words" :key="i" class="chip">{{ w }}</span>
        </div>
        <UiField
          v-slot="{ id, describedby }"
          label="Environment variables it needs"
          optional
          hint="Names only, for the record: values stay in the host’s env for the server and are never stored or shown here."
          :error="envError ?? shown('env')"
        >
          <div class="env">
            <div v-if="m.env.length" class="chips">
              <span v-for="v in m.env" :key="v" class="chip">
                {{ v }}
                <button
                  type="button"
                  class="chip-x"
                  :aria-label="`Remove ${v}`"
                  @click="m.env = m.env.filter((x) => x !== v)"
                >
                  <X :size="12" :stroke-width="2.4" />
                </button>
              </span>
            </div>
            <div class="env-add">
              <input
                :id="id"
                v-model="envDraft"
                class="input mono"
                :aria-describedby="describedby"
                placeholder="ORDERS_API_KEY"
                spellcheck="false"
                @keydown.enter.prevent="addEnv"
              />
              <UiButton :icon="Plus" @click="addEnv">Add</UiButton>
            </div>
          </div>
        </UiField>
      </template>
      <template v-else>
        <UiField
          v-slot="{ id, describedby, invalid }"
          label="URL"
          hint="The server’s Streamable HTTP endpoint (MCP 2025-06-18). Leave credentials out: send them in a header."
          :error="shown('url')"
        >
          <input
            :id="id"
            v-model="m.url"
            class="input mono"
            type="url"
            :aria-describedby="describedby"
            :aria-invalid="invalid"
            placeholder="https://example.com/mcp"
            spellcheck="false"
            data-testid="server-url"
            @blur="touched.add('url')"
          />
        </UiField>
        <fieldset class="headers">
          <legend class="label">
            Headers, from environment variables <span class="subtle">optional</span>
          </legend>
          <p class="caption">
            Each is <span class="mono">--upstream-header NAME=VAR</span>: the proxy sends header
            NAME with the value of variable VAR. Values are never logged or stored.
          </p>
          <div v-for="(h, i) in m.headers" :key="i" class="header-row">
            <input
              v-model="h.name"
              class="input mono"
              :aria-label="`Header ${i + 1}: name`"
              placeholder="Authorization"
              spellcheck="false"
            />
            <span class="subtle" aria-hidden="true">←</span>
            <input
              v-model="h.env"
              class="input mono"
              :aria-label="`Header ${i + 1}: variable`"
              placeholder="ORDERS_AUTH"
              spellcheck="false"
            />
            <UiButton
              variant="ghost"
              square
              :icon="X"
              :aria-label="`Remove header ${i + 1}`"
              @click="m.headers.splice(i, 1)"
            />
          </div>
          <p v-if="shown('headers')" class="field-error">{{ shown('headers') }}</p>
          <UiButton size="sm" :icon="Plus" @click="m.headers.push({ name: '', env: '' })"
            >Add a header</UiButton
          >
        </fieldset>
      </template>
    </section>

    <section class="sf-section">
      <h2 class="sf-h">What the proxy does</h2>
      <div class="modes" role="radiogroup" aria-label="Mode">
        <label
          v-for="mode in modes"
          :key="mode.value"
          class="mode"
          :class="{ on: m.mode === mode.value, [mode.value]: true }"
        >
          <input v-model="m.mode" type="radio" name="mode" :value="mode.value" class="sr-only" />
          <span class="mode-title">{{ mode.title }}</span>
          <span class="mode-body">{{ mode.body }}</span>
        </label>
      </div>

      <template v-if="m.mode !== 'record'">
        <UiField
          v-slot="{ id, describedby, invalid }"
          label="Flow"
          :error="shown('flow')"
          :hint="
            flowOptions.length
              ? undefined
              : 'No flow in the data dir yet: learn one from this server’s recorded sessions first.'
          "
        >
          <div class="flow-pick">
            <select
              v-if="!flowIsOther"
              :id="id"
              class="select"
              :value="m.flow"
              :aria-describedby="describedby"
              :aria-invalid="invalid"
              data-testid="server-flow"
              @change="onFlowChange"
            >
              <option value="">Choose a flow</option>
              <option v-for="o in flowOptions" :key="o.value" :value="o.value">
                {{ o.label }}
              </option>
              <option value="__other">Another path…</option>
            </select>
            <template v-else>
              <input
                :id="id"
                v-model="m.flow"
                class="input mono"
                :aria-describedby="describedby"
                :aria-invalid="invalid"
                placeholder="~/.stretto/orders.flow.json"
                spellcheck="false"
              />
              <UiButton v-if="flowOptions.length" @click="flowIsOther = false">Choose</UiButton>
            </template>
          </div>
        </UiField>
        <div class="sf-row three">
          <UiField v-slot="{ id }" label="Decider" optional>
            <select :id="id" v-model="m.decider" class="select">
              <option value="">The flow’s default</option>
              <option value="reach">reach: counts, asks no model</option>
              <option value="habit">habit: counts, asks no model</option>
              <option value="arbiter">arbiter: asks Jev, needs a key</option>
            </select>
          </UiField>
          <UiField
            v-slot="{ id, describedby, invalid }"
            label="Threshold"
            optional
            :error="shown('threshold')"
          >
            <input
              :id="id"
              v-model="m.threshold"
              class="input num"
              type="number"
              min="0"
              max="1"
              step="0.01"
              :aria-describedby="describedby"
              :aria-invalid="invalid"
              placeholder="0.3"
              @blur="touched.add('threshold')"
            />
          </UiField>
          <UiField v-slot="{ id }" label="Record to" optional>
            <input
              :id="id"
              v-model="m.record_dir"
              class="input mono"
              :placeholder="defaultRecord"
              spellcheck="false"
            />
          </UiField>
        </div>
      </template>
      <UiField v-else v-slot="{ id }" label="Record to" optional>
        <input
          :id="id"
          v-model="m.record_dir"
          class="input mono"
          :placeholder="defaultRecord"
          spellcheck="false"
        />
      </UiField>
    </section>

    <section class="sf-section">
      <h2 class="sf-h">Writes and retention</h2>
      <label class="check">
        <input
          v-model="m.guards"
          type="checkbox"
          :disabled="!guarded && !m.guards"
          data-testid="server-guards"
          @change="touched.add('guards')"
        />
        <span class="check-text">
          <span class="check-title">Policy guards</span>
          <span class="caption"
            >Check each of the agent’s calls against the domain’s rules, and refuse the ones they
            fail (<span class="mono">--guards</span>). Retail and airline have guards.</span
          >
        </span>
      </label>
      <p v-if="shown('guards')" class="field-error">{{ shown('guards') }}</p>
      <div v-if="m.guards" class="sf-row">
        <UiField
          v-slot="{ id, describedby }"
          label="Confirmation judge"
          optional
          hint="Also ask the System-One model whether the customer confirmed each write the guards check for a confirmation. The proxy asks Jev, so it needs TYPESAFE_API_KEY."
        >
          <select
            :id="id"
            v-model="m.judge"
            class="select"
            :aria-describedby="describedby"
            data-testid="server-judge"
          >
            <option value="">Off</option>
            <option value="log">log: record its judgments</option>
            <option value="enforce">enforce: refuse the writes it fails</option>
          </select>
        </UiField>
        <UiField
          v-if="m.judge"
          v-slot="{ id, describedby, invalid }"
          label="The conversation"
          hint="The file the host appends the conversation to, as JSON lines, which the judge reads (--context)."
          :error="shown('context')"
        >
          <input
            :id="id"
            v-model="m.context"
            class="input mono"
            :aria-describedby="describedby"
            :aria-invalid="invalid"
            :placeholder="`~/.stretto/context/${m.name || '<name>'}.jsonl`"
            spellcheck="false"
            data-testid="server-context"
            @blur="touched.add('context')"
          />
        </UiField>
      </div>
      <label class="check">
        <input v-model="m.commit" type="checkbox" data-testid="server-commit" />
        <span class="check-text">
          <span class="check-title"><span class="mono">stretto_commit</span></span>
          <span class="caption"
            >Offer the agent a tool that makes several calls in one, in order, each checked by the
            guards (<span class="mono">--commit</span>).</span
          >
        </span>
      </label>
      <UiField
        v-slot="{ id, describedby, invalid }"
        label="Keep sessions for"
        optional
        hint="Days. When the proxy starts, it deletes the sessions it recorded, and the logs beside them, older than this (--retain-days). Empty keeps them all."
        :error="shown('retain_days')"
      >
        <input
          :id="id"
          v-model="m.retain_days"
          class="input num days"
          type="number"
          min="1"
          step="1"
          :aria-describedby="describedby"
          :aria-invalid="invalid"
          placeholder="All"
          data-testid="server-retain"
          @blur="touched.add('retain_days')"
        />
      </UiField>
    </section>

    <section class="sf-section">
      <h2 class="sf-h">The proxy’s command line</h2>
      <p class="caption">
        A preview. The console writes the exact one, and each host’s configuration, when you save.
      </p>
      <UiCode :code="preview" language="shell" what="Command" wrap />
    </section>

    <div class="sf-actions">
      <UiButton @click="emit('cancel')">Cancel</UiButton>
      <UiButton type="submit" variant="primary" :loading="saving" data-testid="server-save">{{
        editing ? 'Save changes' : 'Add the server'
      }}</UiButton>
    </div>
  </form>
</template>

<style scoped>
.sf {
  display: flex;
  flex-direction: column;
  gap: 18px;
}

.sf-section {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 20px;
  border-radius: var(--c-radius-card);
  border: 1px solid var(--stretto-border);
  background: var(--stretto-surface);
}

.sf-h {
  font-size: 14.5px;
}

.sf-row {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 14px;
}

.sf-row.three {
  grid-template-columns: repeat(3, minmax(0, 1fr));
}

.words {
  margin-top: -6px;
}

.env {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.env-add {
  display: flex;
  gap: 8px;
  max-width: 420px;
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

.chip-x:hover {
  background: var(--c-hover);
  color: var(--stretto-text);
}

.headers {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin: 0;
  padding: 0;
  border: 0;
  min-width: 0;
}

.headers legend {
  margin-bottom: 4px;
  padding: 0;
}

.header-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr) auto;
  align-items: center;
  gap: 8px;
  max-width: 620px;
}

.field-error {
  font-size: 12.5px;
  color: var(--c-danger);
}

.modes {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 10px;
}

.mode {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 12px 14px;
  border-radius: 10px;
  border: 1px solid var(--stretto-border);
  background: var(--stretto-surface);
  cursor: pointer;
  transition:
    border-color 0.12s var(--c-ease),
    box-shadow 0.12s var(--c-ease);
}

.mode:hover {
  border-color: color-mix(in oklab, var(--stretto-border-strong) 55%, var(--stretto-border));
}

.mode.on {
  border-color: var(--stretto-accent-graphic);
  box-shadow: 0 0 0 3px color-mix(in oklab, var(--stretto-accent-graphic) 16%, transparent);
  background: color-mix(in oklab, var(--stretto-accent-soft) 45%, var(--stretto-surface));
}

.mode.shadow.on {
  border-style: dashed;
}

.mode:has(input:focus-visible) {
  outline: 2px solid var(--stretto-focus);
  outline-offset: 2px;
}

.mode-title {
  font-size: 14px;
  font-weight: 600;
}

.mode-body {
  font-size: 12.5px;
  color: var(--stretto-text-muted);
}

.flow-pick {
  display: flex;
  gap: 8px;
}

.check-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.check-title {
  font-weight: 600;
}

.check input:disabled {
  cursor: not-allowed;
}

.days {
  max-width: 160px;
}

.sf-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}

@media (max-width: 900px) {
  .sf-row,
  .sf-row.three,
  .modes {
    grid-template-columns: minmax(0, 1fr);
  }
}

@media (max-width: 720px) {
  .sf-section {
    padding: 16px;
  }

  .header-row {
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) auto;
  }

  .header-row > span {
    display: none;
  }
}
</style>

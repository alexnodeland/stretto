<script setup lang="ts">
/** Add a server, or change one: the form, prefilled from a discovered upstream when there is one. */
import { computed, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import UiPageHeader from '@/components/ui/UiPageHeader.vue'
import UiSkeleton from '@/components/ui/UiSkeleton.vue'
import UiError from '@/components/ui/UiError.vue'
import ServerForm, { type ServerModel } from '@/components/server/ServerForm.vue'
import { api, ApiError } from '@/api/client'
import type { ServerInput, ServerView } from '@/api/types'
import { useResource } from '@/composables/useResource'
import { useTitle } from '@/composables/useTitle'
import { meta } from '@/stores/auth'
import { toast } from '@/stores/toasts'
import { commandText } from '@/lib/format'

const route = useRoute()
const router = useRouter()
const editing = computed(() => route.name === 'server-edit')
const name = computed(() => (editing.value ? String(route.params.name) : null))
useTitle(() => (editing.value ? `Edit ${name.value}` : 'Add a server'))

const servers = useResource((o) => api.servers(o))
const flows = useResource((o) => api.flows(o), { events: ['flows'] })

function blank(): ServerModel {
  return {
    name: '',
    description: '',
    kind: 'stdio',
    command: '',
    env: [],
    url: '',
    headers: [],
    mode: 'record',
    flow: '',
    record_dir: '',
    decider: '',
    threshold: '',
    surprise: '',
    surprise_nats: '',
    flow_tools_only: false,
    flow_tools: [],
    guards: false,
    judge: '',
    context: '',
    commit: false,
    retain_days: '',
  }
}

function fromView(s: ServerView): ServerModel {
  return {
    name: s.name,
    description: s.description ?? '',
    kind: s.upstream.kind,
    command: s.upstream.kind === 'stdio' ? commandText(s.upstream.command) : '',
    env: s.upstream.kind === 'stdio' ? [...s.upstream.env] : [],
    url: s.upstream.kind === 'http' ? s.upstream.url : '',
    headers: s.upstream.kind === 'http' ? s.upstream.headers.map((h) => ({ ...h })) : [],
    mode: s.mode,
    flow: s.flow ?? '',
    record_dir: s.record_dir ?? '',
    decider: s.decider ?? '',
    threshold: s.threshold === null ? '' : String(s.threshold),
    surprise: s.surprise?.kind ?? '',
    surprise_nats: s.surprise?.kind === 'threshold' ? String(s.surprise.nats) : '',
    flow_tools_only: s.flow_tools.length > 0,
    flow_tools: [...s.flow_tools],
    guards: s.guards,
    judge: s.judge?.mode ?? '',
    context: s.judge?.context ?? '',
    commit: s.commit,
    retain_days: s.retain_days === null ? '' : String(s.retain_days),
  }
}

const initial = computed<ServerModel | null>(() => {
  const list = servers.data.value
  if (!list) return null
  if (editing.value) {
    const s = list.items.find((x) => x.name === name.value)
    return s ? fromView(s) : null
  }
  const model = blank()
  const domain = typeof route.query.domain === 'string' ? route.query.domain : ''
  if (domain) {
    model.name = domain
    const d = list.discovered.find((x) => x.domain === domain)
    if (d?.upstream?.kind === 'stdio') model.command = commandText(d.upstream.command)
    if (d?.upstream?.kind === 'http') {
      model.kind = 'http'
      model.url = d.upstream.url
    }
  }
  return model
})

const saving = ref(false)
const serverError = ref<string | null>(null)
async function save(input: ServerInput) {
  saving.value = true
  serverError.value = null
  try {
    const saved = editing.value
      ? await api.updateServer(input.name, input, { quiet: true })
      : await api.createServer(input, { quiet: true })
    toast({
      kind: 'success',
      title: editing.value ? `Server ${saved.name} saved` : `Server ${saved.name} added`,
      message: 'Its host configuration is ready to paste.',
    })
    void router.push({ name: 'server', params: { name: saved.name } })
  } catch (e) {
    serverError.value = e instanceof ApiError ? e.message : String(e)
  } finally {
    saving.value = false
  }
}

function cancel() {
  if (editing.value && name.value)
    void router.push({ name: 'server', params: { name: name.value } })
  else void router.push({ name: 'servers' })
}
</script>

<template>
  <div class="page narrow">
    <UiPageHeader
      :title="editing ? `Edit ${name}` : 'Add a server'"
      :back="editing ? { name: 'server', params: { name: name ?? '' } } : { name: 'servers' }"
      :back-label="editing ? (name ?? 'Server') : 'Servers'"
    >
      {{
        editing
          ? 'Changes apply to the configuration the console gives each host: paste it again after saving.'
          : 'The console keeps the setup in servers.json in the data dir, and gives you each host’s configuration, with stretto-proxy in front.'
      }}
    </UiPageHeader>

    <div v-if="servers.error.value" class="card">
      <UiError
        :message="servers.error.value.message"
        :status="servers.error.value.status"
        @retry="servers.refresh()"
      />
    </div>
    <div v-else-if="servers.data.value && !initial" class="card">
      <UiError
        title="No such server"
        :message="`There is no server named ${name} to edit.`"
        @retry="servers.refresh()"
      />
    </div>
    <ServerForm
      v-else-if="initial"
      :initial="initial"
      :editing="editing"
      :taken="servers.data.value?.items.map((s) => s.name) ?? []"
      :flows="flows.data.value?.items ?? []"
      :data-dir="meta?.data_dir ?? '~/.stretto'"
      :saving="saving"
      :server-error="serverError"
      @submit="save"
      @cancel="cancel"
    />
    <div v-else class="stack">
      <div v-for="i in 3" :key="i" class="card pad"><UiSkeleton :lines="3" /></div>
    </div>
  </div>
</template>

<style scoped>
.narrow {
  max-width: 920px;
}

.pad {
  padding: 20px;
}
</style>

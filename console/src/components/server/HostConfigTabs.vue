<script setup lang="ts">
/** The host's configuration for a server, a tab per host, as `stretto init` prints it: the snippet, where it goes, and the next steps. */
import { ref } from 'vue'
import UiTabs from '../ui/UiTabs.vue'
import UiCode from '../ui/UiCode.vue'
import UiSkeleton from '../ui/UiSkeleton.vue'
import UiError from '../ui/UiError.vue'
import RichText from '../RichText.vue'
import { api } from '@/api/client'
import type { HostName } from '@/api/types'
import { useResource } from '@/composables/useResource'

const props = defineProps<{ name: string; env: string[] }>()
const HOST_KEY = 'stretto-console:host'
const hosts: { id: HostName; label: string }[] = [
  { id: 'claude-code', label: 'Claude Code' },
  { id: 'claude-desktop', label: 'Claude Desktop' },
  { id: 'cursor', label: 'Cursor' },
  { id: 'vscode', label: 'VS Code' },
]
function stored(): HostName {
  try {
    const h = localStorage.getItem(HOST_KEY)
    return hosts.some((x) => x.id === h) ? (h as HostName) : 'claude-code'
  } catch {
    return 'claude-code'
  }
}
const host = ref<HostName>(stored())
function choose(h: string) {
  host.value = h as HostName
  try {
    localStorage.setItem(HOST_KEY, h)
  } catch {
    // Not kept.
  }
}

const config = useResource((o) => api.serverConfig(props.name, host.value, o), {
  watch: [host],
  events: ['servers', 'flows'],
})
</script>

<template>
  <div class="hc">
    <UiTabs
      :model-value="host"
      :tabs="hosts"
      id-prefix="host"
      label="Hosts"
      @update:model-value="choose"
    />
    <div
      :id="`host-panel-${host}`"
      role="tabpanel"
      :aria-labelledby="`host-tab-${host}`"
      class="hc-panel"
      :class="{ refreshing: config.refreshing.value }"
    >
      <UiError
        v-if="config.error.value"
        :message="config.error.value.message"
        :status="config.error.value.status"
        @retry="config.refresh()"
      />
      <template v-else-if="config.data.value && !config.loading.value">
        <UiCode
          :code="config.data.value.snippet"
          :language="config.data.value.language"
          :wrap="config.data.value.language === 'shell'"
          what="Configuration"
          data-testid="host-snippet"
        />
        <p class="hc-place"><RichText :text="config.data.value.placement" /></p>
        <p v-if="env.length" class="notice">
          <span>
            The server needs {{ env.length === 1 ? 'the variable' : 'these variables' }}
            <span v-for="(v, i) in env" :key="v"
              ><span class="mono">{{ v }}</span
              >{{ i < env.length - 1 ? ', ' : '' }}</span
            >
            in the host’s environment for the server (its <span class="mono">env</span>). The
            console stores names only, never values.
          </span>
        </p>
        <details class="hc-steps">
          <summary>Next steps from here</summary>
          <pre class="code wrap">{{ config.data.value.next_steps }}</pre>
        </details>
      </template>
      <UiSkeleton v-else :lines="4" height="16px" />
    </div>
  </div>
</template>

<style scoped>
.hc {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.hc-panel {
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-width: 0;
}

.hc-place {
  font-size: 13.5px;
  color: var(--stretto-text-muted);
}

.hc-steps summary {
  cursor: pointer;
  font-size: 13.5px;
  font-weight: 500;
  color: var(--stretto-accent);
  width: fit-content;
  border-radius: 4px;
}

.hc-steps[open] summary {
  margin-bottom: 10px;
}

.hc-steps .code {
  font-size: 12px;
}
</style>

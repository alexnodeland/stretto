<script setup lang="ts">
/** Test connection: start the server (or reach its URL) as the proxy would, and list its tools. */
import { ref } from 'vue'
import { CircleCheck, CircleX, Plug, TriangleAlert } from '@lucide/vue'
import UiButton from '../ui/UiButton.vue'
import ToolsTable from '../ToolsTable.vue'
import { api, ApiError } from '@/api/client'
import type { ProbeResult } from '@/api/types'
import { readOnly } from '@/stores/auth'
import { formatDuration } from '@/lib/format'

const props = defineProps<{ name: string }>()
const result = ref<ProbeResult | null>(null)
const error = ref<string | null>(null)
const busy = ref(false)

async function probe() {
  busy.value = true
  error.value = null
  try {
    result.value = await api.probeServer(props.name, { quiet: true })
  } catch (e) {
    result.value = null
    error.value = e instanceof ApiError ? e.message : String(e)
  } finally {
    busy.value = false
  }
}
defineExpose({ probe })
</script>

<template>
  <div class="probe">
    <div class="probe-bar">
      <p class="probe-text">
        Starts the server’s command with a 15 s limit, or reaches its URL with the named headers,
        then lists its tools, as the proxy would.
      </p>
      <UiButton
        variant="primary"
        :icon="Plug"
        :loading="busy"
        :disabled="readOnly"
        reason="The console is read-only"
        data-testid="probe"
        @click="probe"
      >
        Test connection
      </UiButton>
    </div>

    <p v-if="error" class="notice danger" role="alert">
      <CircleX :size="16" :stroke-width="2" aria-hidden="true" />
      <span>{{ error }}</span>
    </p>

    <template v-if="result">
      <p v-if="result.ok" class="notice accent probe-ok" data-testid="probe-ok">
        <CircleCheck :size="16" :stroke-width="2" aria-hidden="true" />
        <span>
          Connected in {{ formatDuration(result.ms) }}:
          <strong class="mono">{{ result.server_name }} {{ result.server_version }}</strong
          >, protocol <span class="mono">{{ result.protocol_version }}</span
          >, {{ result.tools.length }} tools.
          <span v-if="result.instructions" class="probe-instructions"
            >Its instructions: “{{ result.instructions }}”</span
          >
        </span>
      </p>
      <p v-else class="notice danger" role="alert" data-testid="probe-failed">
        <CircleX :size="16" :stroke-width="2" aria-hidden="true" />
        <span>Could not connect ({{ formatDuration(result.ms) }}): {{ result.error }}</span>
      </p>
      <div v-for="w in result.flow_warnings" :key="w" class="notice warn">
        <TriangleAlert :size="16" :stroke-width="2" aria-hidden="true" />
        <span>{{ w }}</span>
      </div>
      <div v-if="result.tools.length" class="probe-tools card">
        <ToolsTable :tools="result.tools" />
      </div>
    </template>
  </div>
</template>

<style scoped>
.probe {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.probe-bar {
  display: flex;
  align-items: center;
  gap: 16px;
  flex-wrap: wrap;
}

.probe-text {
  flex: 1 1 320px;
  font-size: 13.5px;
  color: var(--stretto-text-muted);
}

.probe-instructions {
  display: block;
  margin-top: 4px;
  color: var(--stretto-text-muted);
}

.probe-tools {
  box-shadow: none;
}
</style>

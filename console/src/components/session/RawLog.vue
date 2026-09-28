<script setup lang="ts">
/** The session's log: every line in brief (the events), or the JSON lines as the proxy wrote them. */
import { computed, ref, watch } from 'vue'
import { Download } from '@lucide/vue'
import UiSegmented from '../ui/UiSegmented.vue'
import UiButton from '../ui/UiButton.vue'
import UiCopy from '../ui/UiCopy.vue'
import UiSkeleton from '../ui/UiSkeleton.vue'
import UiError from '../ui/UiError.vue'
import UiBadge from '../ui/UiBadge.vue'
import { api, ApiError } from '@/api/client'
import type { SessionDetail } from '@/api/types'
import { formatOffset } from '@/lib/format'
import { parseJsonLines, prettyJson } from '@/lib/json'

const props = defineProps<{ detail: SessionDetail }>()
const view = ref<'events' | 'lines'>('events')
const raw = ref<string | null>(null)
const error = ref<ApiError | null>(null)
const loading = ref(false)
const expanded = ref(new Set<number>())
const SHOW = 400
const all = ref(false)

async function load() {
  loading.value = true
  error.value = null
  try {
    raw.value = await api.sessionRaw(props.detail.summary.key, { quiet: true })
  } catch (e) {
    error.value = e instanceof ApiError ? e : new ApiError(0, String(e))
  } finally {
    loading.value = false
  }
}

watch(view, (v) => {
  if (v === 'lines' && raw.value === null && !loading.value) void load()
})

const lines = computed(() => (raw.value ? parseJsonLines(raw.value) : []))
const shown = computed(() => (all.value ? lines.value : lines.value.slice(0, SHOW)))

function toggle(line: number) {
  const next = new Set(expanded.value)
  if (next.has(line)) next.delete(line)
  else next.add(line)
  expanded.value = next
}

function fromOf(value: unknown): string {
  if (value && typeof value === 'object' && 'from' in value)
    return String((value as { from: unknown }).from)
  if (value && typeof value === 'object' && 'stretto_mcp_log' in value) return 'header'
  return ''
}

const tones: Record<string, 'neutral' | 'accent' | 'outline' | 'dashed'> = {
  client: 'outline',
  server: 'neutral',
  proxy: 'accent',
  context: 'dashed',
  header: 'neutral',
}
</script>

<template>
  <div class="raw">
    <div class="raw-tools">
      <UiSegmented
        v-model="view"
        :options="[
          { value: 'events', label: `Events (${detail.events.length})` },
          { value: 'lines', label: 'JSON lines' },
        ]"
        label="View"
        size="sm"
      />
      <span class="spacer" />
      <UiCopy v-if="view === 'lines' && raw" :text="raw" what="Log" label="Copy" />
      <UiButton
        size="sm"
        :icon="Download"
        :href="api.sessionRawUrl(detail.summary.key)"
        :download="`${detail.summary.session_id}.jsonl`"
      >
        Download
      </UiButton>
    </div>

    <div v-if="view === 'events'" class="table-wrap">
      <table class="table table-compact stack ev">
        <thead>
          <tr>
            <th scope="col" class="num">Time</th>
            <th scope="col">From</th>
            <th scope="col">Kind</th>
            <th scope="col">Id</th>
            <th scope="col">Summary</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(e, i) in detail.events" :key="i">
            <td class="num subtle inline">{{ formatOffset(e.t_ms) }}</td>
            <td class="inline">
              <UiBadge :tone="tones[e.from] ?? 'neutral'">{{ e.from }}</UiBadge>
            </td>
            <td class="nowrap inline">
              <span class="kind">
                <span>{{ e.kind }}</span>
                <span v-if="e.method" class="mono subtle">{{ e.method }}</span>
              </span>
            </td>
            <td
              class="mono subtle nowrap inline"
              :class="{ 'stack-hide': e.id === null }"
              data-label="id"
            >
              {{ e.id ?? '' }}
            </td>
            <td class="mono summary">{{ e.summary }}</td>
          </tr>
        </tbody>
      </table>
    </div>

    <template v-else>
      <UiError v-if="error" :message="error.message" :status="error.status" @retry="load" />
      <UiSkeleton v-else-if="loading || raw === null" :lines="12" />
      <ol v-else class="lines">
        <li v-for="l in shown" :key="l.line" class="line" :class="{ bad: !l.ok }">
          <button
            type="button"
            class="line-no num"
            :aria-expanded="expanded.has(l.line)"
            :aria-label="`Line ${l.line}: ${expanded.has(l.line) ? 'fold' : 'unfold'}`"
            @click="toggle(l.line)"
          >
            {{ l.line }}
          </button>
          <span v-if="fromOf(l.value)" class="line-from">
            <UiBadge :tone="tones[fromOf(l.value)] ?? 'neutral'">{{ fromOf(l.value) }}</UiBadge>
          </span>
          <pre v-if="expanded.has(l.line)" class="line-text pretty">{{
            l.ok ? prettyJson(l.value) : l.raw
          }}</pre>
          <code v-else class="line-text" @click="toggle(l.line)">{{ l.raw }}</code>
        </li>
      </ol>
      <UiButton v-if="!all && lines.length > SHOW" size="sm" @click="all = true"
        >Show all {{ lines.length }} lines</UiButton
      >
    </template>
  </div>
</template>

<style scoped>
.raw {
  display: flex;
  flex-direction: column;
  gap: 14px;
  min-width: 0;
}

.raw-tools {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.ev .summary {
  font-size: 12px;
  overflow-wrap: anywhere;
  min-width: 280px;
}

.kind {
  display: inline-flex;
  align-items: baseline;
  gap: 6px;
}

@media (max-width: 640px) {
  /* The card around the log already pads it. */
  .ev tr {
    padding: 10px 0;
  }

  .ev .summary {
    min-width: 0;
  }
}

.lines {
  list-style: none;
  border: 1px solid var(--stretto-border);
  border-radius: var(--c-radius-control);
  background: var(--stretto-surface-2);
  overflow: hidden;
}

.line {
  display: grid;
  grid-template-columns: 52px 76px minmax(0, 1fr);
  align-items: start;
  border-bottom: 1px solid var(--stretto-border);
  font-size: 12px;
}

.line:last-child {
  border-bottom: 0;
}

.line-no {
  padding: 6px 10px 6px 0;
  border: 0;
  background: none;
  color: var(--stretto-text-subtle);
  font-family: var(--stretto-font-mono);
  font-size: 11.5px;
  text-align: right;
}

.line-no:hover {
  color: var(--stretto-accent);
}

.line-from {
  padding: 4px 0;
}

.line-text {
  display: block;
  margin: 0;
  padding: 6px 12px 6px 0;
  font-family: var(--stretto-font-mono);
  font-size: 12px;
  line-height: 1.55;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  cursor: pointer;
}

.line-text.pretty {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  cursor: auto;
}

.line.bad .line-text {
  color: var(--c-warn);
}

@media (max-width: 720px) {
  .line {
    grid-template-columns: 40px minmax(0, 1fr);
  }

  .line-from {
    display: none;
  }
}
</style>

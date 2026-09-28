<script setup lang="ts">
/**
 * Sessions as a table (a list of cards on a phone). A row opens the session;
 * the session id is its link, so the keyboard reaches it too.
 */
import { useRouter } from 'vue-router'
import { TriangleAlert } from '@lucide/vue'
import ModeBadge from './ModeBadge.vue'
import UiRelTime from './ui/UiRelTime.vue'
import type { SessionSummary } from '@/api/types'
import { formatCount, formatDuration, plural } from '@/lib/format'

withDefaults(
  defineProps<{ sessions: SessionSummary[]; compact?: boolean; showDomain?: boolean }>(),
  {
    showDomain: true,
  },
)
const router = useRouter()

function open(session: SessionSummary, event: MouseEvent) {
  if ((event.target as HTMLElement).closest('a, button')) return
  void router.push({ name: 'session', params: { key: session.key } })
}
</script>

<template>
  <div class="st">
    <div class="table-wrap st-table">
      <table class="table">
        <thead>
          <tr>
            <th scope="col">Session</th>
            <th v-if="showDomain" scope="col">Domain</th>
            <th v-if="!compact" scope="col">Agent</th>
            <th scope="col">Mode</th>
            <th scope="col" class="num" title="The agent's own tool calls">Calls</th>
            <th v-if="!compact" scope="col" class="num" title="LLM turns, inferred from timing">
              Turns
            </th>
            <th
              scope="col"
              class="num"
              title="Lookups the flow made (served), or would have made (shadow)"
            >
              Lookups
            </th>
            <th v-if="!compact" scope="col" class="num" title="Failed calls">Errors</th>
            <th v-if="!compact" scope="col" class="num">Duration</th>
            <th scope="col" class="num">Started</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="s in sessions" :key="s.key" class="clickable" @click="open(s, $event)">
            <td>
              <RouterLink :to="{ name: 'session', params: { key: s.key } }" class="st-id mono">{{
                s.session_id
              }}</RouterLink>
            </td>
            <td v-if="showDomain">
              <span v-if="s.domain" class="mono st-domain">{{ s.domain }}</span>
              <span v-else class="subtle">none</span>
            </td>
            <td v-if="!compact" class="st-agent">
              <span class="ellipsis" :title="s.agent ?? undefined">{{ s.agent ?? '—' }}</span>
            </td>
            <td><ModeBadge :mode="s.mode" /></td>
            <td class="num">{{ formatCount(s.tool_calls) }}</td>
            <td v-if="!compact" class="num">{{ formatCount(s.llm_turns) }}</td>
            <td class="num">
              <span
                v-if="s.mode === 'shadow'"
                class="subtle"
                :title="`${s.shadow_lookups} lookups decided in shadow, none made`"
              >
                {{ formatCount(s.shadow_lookups) }} in shadow
              </span>
              <span v-else :class="{ subtle: !s.flow_lookups }">{{
                formatCount(s.flow_lookups)
              }}</span>
            </td>
            <td v-if="!compact" class="num">
              <span v-if="s.errors" class="text-danger st-err">
                <TriangleAlert :size="13" :stroke-width="2.2" aria-hidden="true" />{{
                  formatCount(s.errors)
                }}
              </span>
              <span v-else class="subtle">0</span>
            </td>
            <td v-if="!compact" class="num">{{ formatDuration(s.duration_ms) }}</td>
            <td class="num subtle"><UiRelTime :ms="s.started_unix_ms" /></td>
          </tr>
        </tbody>
      </table>
    </div>

    <ul class="st-list">
      <li v-for="s in sessions" :key="s.key">
        <RouterLink :to="{ name: 'session', params: { key: s.key } }" class="st-card">
          <span class="st-card-top">
            <span class="mono st-id">{{ s.session_id }}</span>
            <ModeBadge :mode="s.mode" />
          </span>
          <span class="st-card-meta">
            <span v-if="showDomain && s.domain" class="mono">{{ s.domain }}</span>
            <span>{{ plural(s.tool_calls, 'call') }}</span>
            <span v-if="s.mode === 'served'">{{ plural(s.flow_lookups, 'lookup') }}</span>
            <span v-if="s.mode === 'shadow'">{{ formatCount(s.shadow_lookups) }} in shadow</span>
            <span v-if="s.errors" class="text-danger">{{ plural(s.errors, 'error') }}</span>
            <UiRelTime :ms="s.started_unix_ms" />
          </span>
        </RouterLink>
      </li>
    </ul>
  </div>
</template>

<style scoped>
.st-id {
  font-size: 12.5px;
  color: var(--stretto-text);
  white-space: nowrap;
}

a.st-id:hover {
  color: var(--stretto-accent);
}

.st-domain {
  font-size: 12.5px;
}

.st-agent {
  max-width: 180px;
}

.st-agent .ellipsis {
  display: block;
}

.st-err {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.st-list {
  display: none;
  list-style: none;
}

.st-card {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 12px 16px;
  border-bottom: 1px solid var(--stretto-border);
  color: inherit;
}

.st-list li:last-child .st-card {
  border-bottom: 0;
}

.st-card:hover {
  background: var(--c-hover);
  color: inherit;
}

.st-card-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  min-width: 0;
}

.st-card-top .st-id {
  overflow: hidden;
  text-overflow: ellipsis;
}

.st-card-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 12px;
  font-size: 12.5px;
  color: var(--stretto-text-muted);
}

.st-card-meta .mono {
  font-size: 12px;
}

@media (max-width: 720px) {
  .st-table {
    display: none;
  }

  .st-list {
    display: block;
  }
}
</style>

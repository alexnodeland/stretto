<script setup lang="ts">
/** Every recorded session, newest first, filtered by domain, mode and text. */
import { computed, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { Plus, ScrollText, Search, X } from '@lucide/vue'
import UiPageHeader from '@/components/ui/UiPageHeader.vue'
import UiSegmented from '@/components/ui/UiSegmented.vue'
import UiPagination from '@/components/ui/UiPagination.vue'
import UiSkeleton from '@/components/ui/UiSkeleton.vue'
import UiEmpty from '@/components/ui/UiEmpty.vue'
import UiError from '@/components/ui/UiError.vue'
import UiButton from '@/components/ui/UiButton.vue'
import SessionsTable from '@/components/SessionsTable.vue'
import { api } from '@/api/client'
import type { SessionMode } from '@/api/types'
import { useResource } from '@/composables/useResource'
import { useTitle } from '@/composables/useTitle'
import { readOnly } from '@/stores/auth'
import { domainFilter } from '@/stores/domain'
import { formatCount } from '@/lib/format'

useTitle(() => 'Sessions')

const LIMIT = 50
const route = useRoute()
const router = useRouter()

type ModeChoice = 'all' | SessionMode
const modes: { value: ModeChoice; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'served', label: 'Served' },
  { value: 'shadow', label: 'Shadow' },
  { value: 'recorded', label: 'Recorded' },
]

const first = (v: unknown) => (Array.isArray(v) ? v[0] : v) as string | undefined
const q = ref(first(route.query.q) ?? '')
const debounced = ref(q.value)
const initialMode = first(route.query.mode)
const mode = ref<ModeChoice>(
  initialMode === 'served' || initialMode === 'shadow' || initialMode === 'recorded'
    ? initialMode
    : 'all',
)
const offset = ref(Math.max(0, Number(first(route.query.offset) ?? 0) || 0))
const queryDomain = first(route.query.domain)
if (queryDomain) domainFilter.value = queryDomain

let timer: ReturnType<typeof setTimeout> | null = null
watch(q, (value) => {
  if (timer) clearTimeout(timer)
  timer = setTimeout(() => (debounced.value = value.trim()), 250)
})
watch([debounced, mode, domainFilter], () => (offset.value = 0))
watch([debounced, mode, offset, domainFilter], () => {
  void router.replace({
    query: {
      ...(debounced.value ? { q: debounced.value } : {}),
      ...(mode.value !== 'all' ? { mode: mode.value } : {}),
      ...(domainFilter.value ? { domain: domainFilter.value } : {}),
      ...(offset.value ? { offset: String(offset.value) } : {}),
    },
  })
})

const page = useResource(
  (o) =>
    api.sessions(
      {
        q: debounced.value || null,
        mode: mode.value === 'all' ? null : mode.value,
        domain: domainFilter.value,
        limit: LIMIT,
        offset: offset.value,
      },
      o,
    ),
  { watch: [debounced, mode, offset, domainFilter], events: ['sessions'] },
)

const filtered = computed(() => !!debounced.value || mode.value !== 'all' || !!domainFilter.value)

function clear() {
  q.value = ''
  debounced.value = ''
  mode.value = 'all'
  domainFilter.value = null
}
</script>

<template>
  <div class="page">
    <UiPageHeader title="Sessions">
      Every session stretto-proxy recorded, newest first: the agent’s calls, and the flow’s lookups
      and why.
    </UiPageHeader>

    <div class="filters" role="search">
      <label class="search">
        <Search class="search-icon" :size="16" :stroke-width="1.9" aria-hidden="true" />
        <span class="sr-only">Search sessions</span>
        <input
          v-model="q"
          class="input"
          type="search"
          placeholder="Session id, domain, agent or tool"
          data-testid="sessions-search"
        />
      </label>
      <UiSegmented v-model="mode" :options="modes" label="Mode" />
      <span class="spacer" />
      <p v-if="page.data.value" class="count caption num" aria-live="polite">
        {{ formatCount(page.data.value.total) }}
        {{ page.data.value.total === 1 ? 'session' : 'sessions'
        }}{{ filtered ? (page.data.value.total === 1 ? ' matches' : ' match') : '' }}
      </p>
    </div>

    <section class="card" :class="{ refreshing: page.refreshing.value }" aria-label="Sessions">
      <UiError
        v-if="page.error.value"
        :message="page.error.value.message"
        :status="page.error.value.status"
        @retry="page.refresh()"
      />
      <div v-else-if="page.loading.value && !page.data.value" class="pad">
        <UiSkeleton :lines="10" height="18px" />
      </div>
      <template v-else-if="page.data.value">
        <template v-if="page.data.value.items.length">
          <SessionsTable :sessions="page.data.value.items" :show-domain="!domainFilter" />
          <UiPagination
            v-if="page.data.value.total > LIMIT"
            v-model:offset="offset"
            :total="page.data.value.total"
            :limit="LIMIT"
            noun="sessions"
          />
        </template>
        <UiEmpty v-else-if="filtered" :icon="Search" title="No sessions match">
          Nothing matches these filters{{ domainFilter ? ` in ${domainFilter}` : '' }}.
          <template #actions>
            <UiButton :icon="X" @click="clear">Clear the filters</UiButton>
          </template>
        </UiEmpty>
        <UiEmpty v-else :icon="ScrollText" title="No sessions recorded yet">
          Add a server, use your agent through it, then learn a flow. stretto-proxy records each
          session, and it shows here as it is written.
          <template #actions>
            <UiButton
              variant="primary"
              :icon="Plus"
              :to="{ name: 'server-new' }"
              :disabled="readOnly"
              reason="The console is read-only"
            >
              Add a server
            </UiButton>
          </template>
        </UiEmpty>
      </template>
    </section>
  </div>
</template>

<style scoped>
.filters {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 10px 12px;
}

.search {
  position: relative;
  flex: 0 1 360px;
  min-width: 220px;
}

.search .input {
  padding-left: 36px;
  background: var(--stretto-surface);
}

.search-icon {
  position: absolute;
  left: 12px;
  top: 10px;
  color: var(--stretto-text-subtle);
  pointer-events: none;
}

.count {
  white-space: nowrap;
}

.pad {
  padding: 20px;
}

@media (max-width: 720px) {
  .search {
    flex: 1 1 100%;
  }

  .spacer {
    display: none;
  }
}
</style>

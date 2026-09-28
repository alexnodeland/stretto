<script setup lang="ts">
/**
 * ⌘K / Ctrl K: jump to any page, server, flow or session, or start an action.
 * Type to filter; the arrow keys move, Enter goes, Escape closes.
 */
import { computed, nextTick, ref, watch, type Component } from 'vue'
import { useRouter, type RouteLocationRaw } from 'vue-router'
import {
  CornerDownLeft,
  LayoutDashboard,
  Plus,
  ScrollText,
  Search,
  Server,
  Settings,
  SquareTerminal,
  SunMoon,
  Workflow,
} from '@lucide/vue'
import UiKbd from '../ui/UiKbd.vue'
import { api } from '@/api/client'
import type { FlowSummary, ServerView, SessionSummary } from '@/api/types'
import { formatRelative } from '@/lib/format'
import { readOnly } from '@/stores/auth'
import { cycleTheme } from '@/stores/theme'

const props = defineProps<{ open: boolean }>()
const emit = defineEmits<{ 'update:open': [open: boolean] }>()
const router = useRouter()

interface Entry {
  id: string
  group: 'Pages' | 'Actions' | 'Servers' | 'Flows' | 'Sessions'
  label: string
  detail?: string
  icon: Component
  mono?: boolean
  to?: RouteLocationRaw
  run?: () => void
}

const dialog = ref<HTMLDialogElement | null>(null)
const input = ref<HTMLInputElement | null>(null)
const q = ref('')
const active = ref(0)
const servers = ref<ServerView[]>([])
const flows = ref<FlowSummary[]>([])
const sessions = ref<SessionSummary[]>([])
const searching = ref(false)

const pages: Entry[] = [
  {
    id: 'p-overview',
    group: 'Pages',
    label: 'Overview',
    icon: LayoutDashboard,
    to: { name: 'overview' },
  },
  { id: 'p-servers', group: 'Pages', label: 'Servers', icon: Server, to: { name: 'servers' } },
  {
    id: 'p-sessions',
    group: 'Pages',
    label: 'Sessions',
    icon: ScrollText,
    to: { name: 'sessions' },
  },
  { id: 'p-flows', group: 'Pages', label: 'Flows', icon: Workflow, to: { name: 'flows' } },
  { id: 'p-jobs', group: 'Pages', label: 'Jobs', icon: SquareTerminal, to: { name: 'jobs' } },
  { id: 'p-settings', group: 'Pages', label: 'Settings', icon: Settings, to: { name: 'settings' } },
]

const actions = computed<Entry[]>(() => {
  const list: Entry[] = []
  if (!readOnly.value) {
    list.push({
      id: 'a-server',
      group: 'Actions',
      label: 'Add a server',
      icon: Plus,
      to: { name: 'server-new' },
    })
    for (const kind of [
      'learn',
      'promote',
      'audit',
      'stage',
      'drift',
      'redact',
      'doctor',
    ] as const) {
      list.push({
        id: `a-job-${kind}`,
        group: 'Actions',
        label: kind === 'doctor' ? 'Run stretto doctor' : `New ${kind} job`,
        icon: SquareTerminal,
        to: { name: 'job-new', query: { kind } },
      })
    }
  }
  list.push({
    id: 'a-theme',
    group: 'Actions',
    label: 'Change the theme',
    detail: 'system, light, dark',
    icon: SunMoon,
    run: () => cycleTheme(),
  })
  return list
})

function matches(entry: Entry, needle: string): boolean {
  if (!needle) return true
  return `${entry.label} ${entry.detail ?? ''}`.toLowerCase().includes(needle)
}

const entries = computed<Entry[]>(() => {
  const needle = q.value.trim().toLowerCase()
  const list: Entry[] = []
  list.push(...pages.filter((e) => matches(e, needle)))
  list.push(...actions.value.filter((e) => matches(e, needle)).slice(0, needle ? 6 : 3))
  list.push(
    ...servers.value
      .map<Entry>((s) => ({
        id: `s-${s.name}`,
        group: 'Servers',
        label: s.name,
        detail: s.description ?? undefined,
        icon: Server,
        mono: true,
        to: { name: 'server', params: { name: s.name } },
      }))
      .filter((e) => matches(e, needle))
      .slice(0, 5),
  )
  list.push(
    ...flows.value
      .map<Entry>((f) => ({
        id: `f-${f.key}`,
        group: 'Flows',
        label: f.key,
        detail: f.error ? 'does not load' : `domain ${f.domain}`,
        icon: Workflow,
        mono: true,
        to: { name: 'flow', params: { key: f.key } },
      }))
      .filter((e) => matches(e, needle))
      .slice(0, 5),
  )
  list.push(
    ...sessions.value.map<Entry>((s) => ({
      id: `x-${s.key}`,
      group: 'Sessions',
      label: s.session_id,
      detail: [s.domain, s.agent, formatRelative(s.started_unix_ms)].filter(Boolean).join(' · '),
      icon: ScrollText,
      mono: true,
      to: { name: 'session', params: { key: s.key } },
    })),
  )
  return list
})

const groups = computed(() => {
  const out: { name: Entry['group']; items: { entry: Entry; index: number }[] }[] = []
  entries.value.forEach((entry, index) => {
    let group = out.find((g) => g.name === entry.group)
    if (!group) {
      group = { name: entry.group, items: [] }
      out.push(group)
    }
    group.items.push({ entry, index })
  })
  return out
})

let loaded = 0
async function loadLists() {
  if (Date.now() - loaded < 30_000) return
  loaded = Date.now()
  const [s, f] = await Promise.allSettled([
    api.servers({ quiet: true }),
    api.flows({ quiet: true }),
  ])
  if (s.status === 'fulfilled') servers.value = s.value.items
  if (f.status === 'fulfilled') flows.value = f.value.items
}

let timer: ReturnType<typeof setTimeout> | null = null
let search: AbortController | null = null
function searchSessions() {
  if (timer) clearTimeout(timer)
  timer = setTimeout(async () => {
    search?.abort()
    const mine = new AbortController()
    search = mine
    searching.value = true
    try {
      const page = await api.sessions(
        { q: q.value.trim() || null, limit: q.value.trim() ? 8 : 4 },
        { quiet: true, signal: mine.signal },
      )
      if (search === mine) sessions.value = page.items
    } catch {
      if (search === mine) sessions.value = []
    } finally {
      if (search === mine) searching.value = false
    }
  }, 140)
}

watch(q, () => {
  active.value = 0
  searchSessions()
})

watch(
  () => props.open,
  async (open) => {
    await nextTick()
    const el = dialog.value
    if (!el) return
    if (open) {
      q.value = ''
      active.value = 0
      if (!el.open) {
        if (typeof el.showModal === 'function') el.showModal()
        else el.setAttribute('open', '')
      }
      input.value?.focus()
      void loadLists()
      searchSessions()
    } else if (el.open) {
      if (typeof el.close === 'function') el.close()
      else el.removeAttribute('open')
    }
  },
  { immediate: true },
)

function close() {
  emit('update:open', false)
}

function go(entry: Entry | undefined) {
  if (!entry) return
  close()
  if (entry.run) entry.run()
  else if (entry.to) void router.push(entry.to)
}

function scrollActive() {
  void nextTick(() => {
    dialog.value
      ?.querySelector(`[data-index="${active.value}"]`)
      ?.scrollIntoView({ block: 'nearest' })
  })
}

function onKey(event: KeyboardEvent) {
  const n = entries.value.length
  if (event.key === 'ArrowDown') {
    event.preventDefault()
    if (n) active.value = (active.value + 1) % n
    scrollActive()
  } else if (event.key === 'ArrowUp') {
    event.preventDefault()
    if (n) active.value = (active.value - 1 + n) % n
    scrollActive()
  } else if (event.key === 'Enter') {
    event.preventDefault()
    go(entries.value[active.value])
  }
}

function onBackdrop(event: MouseEvent) {
  if (event.target === dialog.value) close()
}
</script>

<template>
  <dialog
    ref="dialog"
    class="palette"
    aria-label="Search or jump to"
    @close="props.open && close()"
    @cancel.prevent="close"
    @mousedown="onBackdrop"
  >
    <div v-if="open" class="palette-inner">
      <div class="palette-search">
        <Search :size="18" :stroke-width="1.9" aria-hidden="true" />
        <input
          ref="input"
          v-model="q"
          class="palette-input"
          type="text"
          role="combobox"
          aria-expanded="true"
          aria-controls="palette-list"
          aria-autocomplete="list"
          :aria-activedescendant="entries[active] ? `palette-${entries[active]!.id}` : undefined"
          placeholder="Search sessions, flows, servers and pages"
          autocomplete="off"
          spellcheck="false"
          data-testid="palette-input"
          @keydown="onKey"
        />
        <UiKbd>Esc</UiKbd>
      </div>
      <div id="palette-list" class="palette-list" role="listbox" aria-label="Results">
        <template v-for="group in groups" :key="group.name">
          <div class="palette-group" role="presentation">{{ group.name }}</div>
          <div
            v-for="{ entry, index } in group.items"
            :id="`palette-${entry.id}`"
            :key="entry.id"
            role="option"
            class="palette-item"
            :class="{ active: index === active }"
            :aria-selected="index === active"
            :data-index="index"
            @mousemove="active = index"
            @click="go(entry)"
          >
            <component
              :is="entry.icon"
              class="palette-icon"
              :size="16"
              :stroke-width="1.8"
              aria-hidden="true"
            />
            <span class="palette-label" :class="{ mono: entry.mono }">{{ entry.label }}</span>
            <span v-if="entry.detail" class="palette-detail">{{ entry.detail }}</span>
            <CornerDownLeft
              v-if="index === active"
              class="palette-enter"
              :size="14"
              :stroke-width="2"
              aria-hidden="true"
            />
          </div>
        </template>
        <p v-if="!entries.length" class="palette-empty">
          {{ searching ? 'Searching…' : `Nothing matches “${q}”.` }}
        </p>
      </div>
      <div class="palette-foot" aria-hidden="true">
        <span><UiKbd>↑</UiKbd><UiKbd>↓</UiKbd> move</span>
        <span><UiKbd>↵</UiKbd> open</span>
        <span><UiKbd>Esc</UiKbd> close</span>
      </div>
    </div>
  </dialog>
</template>

<style scoped>
.palette {
  width: min(640px, calc(100vw - 24px));
  max-height: min(560px, calc(100vh - 96px));
  margin: 12vh auto auto;
  padding: 0;
  border: 1px solid var(--stretto-border);
  border-radius: 14px;
  background: var(--stretto-surface);
  color: var(--stretto-text);
  box-shadow: var(--c-shadow-lg);
  overflow: hidden;
}

.palette[open] {
  animation: pop 0.14s var(--c-ease);
}

.palette::backdrop {
  background: var(--c-backdrop);
  backdrop-filter: blur(2px);
}

.palette-inner {
  display: flex;
  flex-direction: column;
  max-height: min(560px, calc(100vh - 96px));
}

.palette-search {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 0 14px;
  border-bottom: 1px solid var(--stretto-border);
  color: var(--stretto-text-subtle);
}

.palette-input {
  flex: 1;
  min-width: 0;
  height: 52px;
  border: 0;
  background: transparent;
  color: var(--stretto-text);
  font-size: 15px;
  outline: none;
}

.palette-input::placeholder {
  color: var(--stretto-text-subtle);
}

.palette-list {
  flex: 1;
  overflow-y: auto;
  padding: 6px;
}

.palette-group {
  padding: 10px 10px 4px;
  font-size: 11.5px;
  font-weight: 600;
  color: var(--stretto-text-subtle);
  letter-spacing: 0.02em;
}

.palette-item {
  display: flex;
  align-items: center;
  gap: 10px;
  min-height: 38px;
  padding: 6px 10px;
  border-radius: 8px;
  cursor: pointer;
  min-width: 0;
}

.palette-item.active {
  background: var(--stretto-accent-soft);
}

.palette-icon {
  flex: none;
  color: var(--stretto-text-subtle);
}

.palette-item.active .palette-icon {
  color: var(--stretto-accent);
}

.palette-label {
  flex: none;
  max-width: 60%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 14px;
}

.palette-label.mono {
  font-size: 13px;
}

.palette-detail {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12.5px;
  color: var(--stretto-text-subtle);
}

.palette-enter {
  flex: none;
  margin-left: auto;
  color: var(--stretto-accent);
}

.palette-empty {
  padding: 24px 12px;
  text-align: center;
  color: var(--stretto-text-subtle);
  font-size: 13.5px;
}

.palette-foot {
  display: flex;
  gap: 16px;
  padding: 9px 14px;
  border-top: 1px solid var(--stretto-border);
  font-size: 12px;
  color: var(--stretto-text-subtle);
}

.palette-foot span {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

@keyframes pop {
  from {
    opacity: 0;
    transform: translateY(-6px) scale(0.99);
  }
}

@media (max-width: 720px) {
  .palette {
    margin-top: 12px;
  }

  .palette-foot {
    display: none;
  }
}
</style>

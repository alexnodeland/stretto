<script setup lang="ts">
/** The console's settings: where the data is and how much of it, the installation, access, the theme, and privacy. */
import { computed } from 'vue'
import {
  CircleCheck,
  ExternalLink,
  Info,
  KeyRound,
  Lock,
  LogOut,
  Monitor,
  Moon,
  ShieldCheck,
  Sun,
} from '@lucide/vue'
import UiPageHeader from '@/components/ui/UiPageHeader.vue'
import UiCard from '@/components/ui/UiCard.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiCopy from '@/components/ui/UiCopy.vue'
import UiBadge from '@/components/ui/UiBadge.vue'
import UiSegmented from '@/components/ui/UiSegmented.vue'
import UiSkeleton from '@/components/ui/UiSkeleton.vue'
import UiError from '@/components/ui/UiError.vue'
import { api } from '@/api/client'
import { useResource } from '@/composables/useResource'
import { useTitle } from '@/composables/useTitle'
import { signOut } from '@/stores/auth'
import { setTheme, theme, type ThemeChoice } from '@/stores/theme'
import { formatBytes, formatCount, formatPercent } from '@/lib/format'

useTitle(() => 'Settings')
const settings = useResource((o) => api.settings(o), { events: ['sessions', 'flows'] })
const s = settings.data

const disk = computed(() => {
  const d = s.value?.disk
  if (!d) return []
  const total = Math.max(1, d.total_bytes)
  return [
    { label: 'Session logs', hint: 'with their flow and confirmation logs', bytes: d.logs_bytes },
    { label: 'Flows', hint: '*.flow.json', bytes: d.flows_bytes },
    { label: 'Answer cache', hint: 'oracle-cache', bytes: d.cache_bytes },
    { label: 'Everything else', hint: 'the registry, jobs, the trash', bytes: d.other_bytes },
  ].map((row) => ({ ...row, share: row.bytes / total }))
})

const themeChoice = computed({
  get: () => theme.value,
  set: (v: ThemeChoice) => setTheme(v),
})

const PRIVACY = 'https://github.com/alexnodeland/stretto/blob/main/docs/privacy.md'
</script>

<template>
  <div class="page">
    <UiPageHeader title="Settings"
      >The console’s data, installation and access. It is set when stretto-console starts: these
      show what it runs with.</UiPageHeader
    >

    <div v-if="settings.error.value" class="card">
      <UiError
        :message="settings.error.value.message"
        :status="settings.error.value.status"
        @retry="settings.refresh()"
      />
    </div>

    <div v-else class="grid layout">
      <div class="stack">
        <UiCard
          title="Data"
          caption="The directory the console reads: --data, else $STRETTO_HOME, else ~/.stretto"
        >
          <template v-if="s">
            <div class="dir">
              <span class="mono">{{ s.data_dir }}</span>
              <UiCopy :text="s.data_dir" what="Path" />
            </div>
            <p class="counts">
              <strong class="num">{{ formatCount(s.sessions) }}</strong> sessions ·
              <strong class="num">{{ formatCount(s.flows) }}</strong> flows ·
              <strong class="num">{{ formatBytes(s.disk.total_bytes) }}</strong> on disk
            </p>
            <ul class="disk" aria-label="Disk use by kind">
              <li v-for="row in disk" :key="row.label">
                <div class="disk-text">
                  <span class="disk-label"
                    >{{ row.label }} <span class="caption">{{ row.hint }}</span></span
                  >
                  <span class="num disk-bytes">{{ formatBytes(row.bytes) }}</span>
                </div>
                <div class="disk-track" aria-hidden="true">
                  <span :style="{ width: `${Math.max(row.bytes ? 0.8 : 0, row.share * 100)}%` }" />
                </div>
                <span class="sr-only">{{ formatPercent(row.share) }} of the total</span>
              </li>
            </ul>
            <p class="notice">
              <Info :size="16" :stroke-width="2" aria-hidden="true" />
              <span>{{ s.retention_note }}</span>
            </p>
          </template>
          <UiSkeleton v-else :lines="6" />
        </UiCard>

        <UiCard title="Privacy">
          <div class="privacy">
            <p>
              Session logs hold every tool call and result verbatim, and what the customer said when
              the host passes the conversation. Treat them like the data the tools touch.
            </p>
            <ul>
              <li>
                The console reads the data dir and serves it to this browser only. It sends nothing
                anywhere else.
              </li>
              <li>
                The registry, <span class="mono">servers.json</span>, holds environment variable
                names and header names, never their values.
              </li>
              <li>
                It never reads or shows a variable’s value, never follows a link out of the data
                dir, and moves what you delete to its trash.
              </li>
              <li>
                To share sessions, redact them first: a job writes a pseudonymized copy that learns
                the same flow.
              </li>
            </ul>
            <a :href="PRIVACY" target="_blank" rel="noopener noreferrer" class="ext">
              What each file holds, and what is sent where
              <ExternalLink :size="13" :stroke-width="2" aria-hidden="true" />
            </a>
          </div>
        </UiCard>
      </div>

      <div class="stack">
        <UiCard title="Installation">
          <template v-if="s">
            <dl class="kv">
              <dt>Console</dt>
              <dd class="mono">{{ s.version }}</dd>
              <template v-for="b in s.binaries" :key="b.name">
                <dt class="mono">{{ b.name }}</dt>
                <dd>
                  <span v-if="b.path" class="binary">
                    <span class="mono">{{ b.version ?? '?' }}</span>
                    <span class="caption mono">{{ b.path }}</span>
                  </span>
                  <span v-else class="text-warn">not found: jobs that run it fail</span>
                </dd>
              </template>
              <dt>Jev key</dt>
              <dd>
                <UiBadge v-if="s.key_set" tone="accent" :icon="KeyRound">set</UiBadge>
                <UiBadge v-else tone="neutral">not set</UiBadge>
                <p class="caption">
                  TYPESAFE_API_KEY or TYPESAFE_API_KEY_FILE, in the console’s environment; its value
                  is never read. Flows served with reach or the habit need none.
                </p>
              </dd>
            </dl>
          </template>
          <UiSkeleton v-else :lines="5" />
        </UiCard>

        <UiCard title="Access">
          <template v-if="s">
            <ul class="access">
              <li>
                <ShieldCheck
                  v-if="s.auth"
                  :size="16"
                  :stroke-width="2"
                  aria-hidden="true"
                  class="text-accent"
                />
                <Info v-else :size="16" :stroke-width="2" aria-hidden="true" />
                <span v-if="s.auth"
                  >A token is required. The browser holds it in an HttpOnly cookie that only this
                  console reads.</span
                >
                <span v-else
                  >Started with <span class="mono">--no-auth</span>: anyone who can reach this
                  loopback address can use it.</span
                >
              </li>
              <li>
                <Lock
                  v-if="s.read_only"
                  :size="16"
                  :stroke-width="2"
                  aria-hidden="true"
                  class="text-warn"
                />
                <CircleCheck
                  v-else
                  :size="16"
                  :stroke-width="2"
                  aria-hidden="true"
                  class="text-accent"
                />
                <span v-if="s.read_only"
                  >Read-only (<span class="mono">--read-only</span>): every write, test and job is
                  refused.</span
                >
                <span v-else>Writes are allowed: servers, deletes, connection tests and jobs.</span>
              </li>
            </ul>
            <UiButton v-if="s.auth" :icon="LogOut" class="signout" @click="signOut()"
              >Sign out</UiButton
            >
          </template>
          <UiSkeleton v-else :lines="3" />
        </UiCard>

        <UiCard title="Appearance" caption="Kept in this browser">
          <UiSegmented
            v-model="themeChoice"
            :options="[
              { value: 'system', label: 'System', icon: Monitor },
              { value: 'light', label: 'Light', icon: Sun },
              { value: 'dark', label: 'Dark', icon: Moon },
            ]"
            label="Theme"
          />
        </UiCard>
      </div>
    </div>
  </div>
</template>

<style scoped>
.layout {
  grid-template-columns: minmax(0, 1.4fr) minmax(0, 1fr);
  align-items: start;
}

.dir {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 8px 8px 12px;
  border-radius: 8px;
  background: var(--stretto-surface-2);
  border: 1px solid var(--stretto-border);
}

.dir .mono {
  flex: 1;
  min-width: 0;
  font-size: 13px;
  overflow-wrap: anywhere;
}

.counts {
  margin: 14px 0 12px;
  font-size: 13.5px;
  color: var(--stretto-text-muted);
}

.counts strong {
  color: var(--stretto-text);
  font-weight: 600;
}

.disk {
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 12px;
  margin-bottom: 16px;
}

.disk-text {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 5px;
  font-size: 13.5px;
}

.disk-label .caption {
  margin-left: 4px;
}

.disk-bytes {
  flex: none;
  color: var(--stretto-text-muted);
}

.disk-track {
  height: 8px;
  border-radius: 4px;
  background: var(--stretto-surface-2);
  box-shadow: inset 0 0 0 1px var(--stretto-border);
  overflow: hidden;
}

.disk-track span {
  display: block;
  height: 100%;
  border-radius: 4px;
  background: var(--stretto-accent-graphic);
}

.binary {
  display: inline-flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 2px 8px;
}

.binary .caption {
  overflow-wrap: anywhere;
}

.privacy {
  display: flex;
  flex-direction: column;
  gap: 10px;
  font-size: 13.5px;
}

.privacy ul {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding-left: 18px;
  color: var(--stretto-text-muted);
}

.privacy .mono {
  font-size: 12.5px;
}

.ext {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-weight: 500;
  width: fit-content;
}

.access {
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 10px;
  font-size: 13.5px;
}

.access li {
  display: flex;
  gap: 10px;
  align-items: flex-start;
}

.access svg {
  flex: none;
  margin-top: 2px;
}

.signout {
  margin-top: 16px;
}

@media (max-width: 1100px) {
  .layout {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>

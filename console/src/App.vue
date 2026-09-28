<script setup lang="ts">
/**
 * The shell: sidebar, top bar, the page, the command palette and toasts;
 * or the sign-in screen after a 401, or a notice when the server is not there.
 */
import { onMounted, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { PlugZap, RotateCcw } from '@lucide/vue'
import SideNav from '@/components/layout/SideNav.vue'
import TopBar from '@/components/layout/TopBar.vue'
import CommandPalette from '@/components/layout/CommandPalette.vue'
import ToastHost from '@/components/layout/ToastHost.vue'
import SignInScreen from '@/components/layout/SignInScreen.vue'
import UiButton from '@/components/ui/UiButton.vue'
import BrandLogo from '@/components/BrandLogo.vue'
import { api, setClientHooks } from '@/api/client'
import { connectEvents, disconnectEvents, onChanged, onJob } from '@/api/events'
import { authState, loadMeta, markSignedOut, metaError } from '@/stores/auth'
import { setDomainNames } from '@/stores/domain'
import { noteJob, noteJobs } from '@/stores/jobs'
import { toast } from '@/stores/toasts'
import { useHotkey } from '@/composables/useHotkey'

setClientHooks({
  onUnauthorized: () => markSignedOut(),
  onError: (error) => toast({ kind: 'error', title: 'The request failed', message: error.message }),
})

const route = useRoute()
const palette = ref(false)
const drawer = ref(false)
const SIDEBAR_KEY = 'stretto-console:sidebar'
const collapsed = ref(false)
try {
  collapsed.value = localStorage.getItem(SIDEBAR_KEY) === 'collapsed'
} catch {
  // Expanded, then.
}

function toggleSidebar() {
  collapsed.value = !collapsed.value
  try {
    localStorage.setItem(SIDEBAR_KEY, collapsed.value ? 'collapsed' : 'expanded')
  } catch {
    // Not kept past this page.
  }
}

watch(
  () => route.fullPath,
  () => {
    drawer.value = false
  },
)

useHotkey({ key: 'k', mod: true, inInputs: true }, (event) => {
  event.preventDefault()
  palette.value = !palette.value
})
useHotkey({ key: '/' }, (event) => {
  event.preventDefault()
  palette.value = true
})
useHotkey({ key: 'Escape', inInputs: true }, () => {
  drawer.value = false
})

async function loadDomains() {
  try {
    const [overview, servers] = await Promise.all([
      api.overview({ quiet: true }),
      api.servers({ quiet: true }),
    ])
    setDomainNames([...overview.domains.map((d) => d.name), ...servers.items.map((s) => s.name)])
  } catch {
    // The filter keeps what it had.
  }
}

async function loadJobs() {
  try {
    noteJobs((await api.jobs({ quiet: true })).items)
  } catch {
    // The count waits for the next event.
  }
}

async function start() {
  await loadMeta()
  if (authState.value !== 'signed-in') return
  connectEvents(() => void loadMeta())
  void loadDomains()
  void loadJobs()
}

onChanged('sessions', () => void loadDomains())
onChanged('servers', () => void loadDomains())
onChanged('jobs', () => void loadJobs())
onJob((job) => {
  const before = noteJob(job)
  if (before !== 'running' && before !== 'queued') return
  if (job.status === 'succeeded') toast({ kind: 'success', title: `${job.title}: done` })
  else if (job.status === 'failed')
    toast({
      kind: 'error',
      title: `${job.title}: failed`,
      message: `Exit code ${job.exit_code ?? 'unknown'}. The job page has its output.`,
    })
  else if (job.status === 'cancelled') toast({ kind: 'info', title: `${job.title}: cancelled` })
})

watch(authState, (state) => {
  if (state !== 'signed-in') disconnectEvents()
})

onMounted(start)
</script>

<template>
  <SignInScreen v-if="authState === 'signed-out'" />

  <main v-else-if="authState === 'unreachable'" class="down">
    <div class="down-card">
      <BrandLogo :height="24" />
      <div class="down-icon" aria-hidden="true"><PlugZap :size="20" :stroke-width="1.8" /></div>
      <h1>The console server did not answer</h1>
      <p class="muted">{{ metaError }}</p>
      <p class="caption">
        Start it with <span class="mono">stretto-console</span> (it listens on 127.0.0.1:7878), then
        try again.
      </p>
      <UiButton variant="primary" :icon="RotateCcw" @click="start">Try again</UiButton>
    </div>
  </main>

  <div v-else class="shell" :class="{ collapsed }">
    <a class="skip" href="#main">Skip to the page</a>
    <aside class="shell-side" :class="{ open: drawer }">
      <SideNav
        :collapsed="collapsed"
        :drawer="drawer"
        @toggle="toggleSidebar"
        @navigate="drawer = false"
      />
    </aside>
    <Transition name="scrim">
      <div v-if="drawer" class="shell-scrim" aria-hidden="true" @click="drawer = false" />
    </Transition>
    <div class="shell-main">
      <TopBar @menu="drawer = true" @search="palette = true" />
      <main id="main" class="shell-page" tabindex="-1">
        <RouterView v-slot="{ Component, route: r }">
          <component :is="Component" :key="r.path" />
        </RouterView>
      </main>
    </div>
    <CommandPalette v-model:open="palette" />
  </div>

  <ToastHost />
</template>

<style scoped>
.shell {
  display: grid;
  grid-template-columns: var(--c-sidebar-w) minmax(0, 1fr);
  min-height: 100vh;
  transition: grid-template-columns 0.18s var(--c-ease);
}

.shell.collapsed {
  grid-template-columns: var(--c-sidebar-w-collapsed) minmax(0, 1fr);
}

.shell-side {
  z-index: 30;
  background: var(--stretto-surface);
  border-right: 1px solid var(--stretto-border);
}

.shell-side > :deep(.side) {
  position: sticky;
  top: 0;
  height: 100vh;
}

.shell-main {
  min-width: 0;
  display: flex;
  flex-direction: column;
}

.shell-page {
  flex: 1;
  min-width: 0;
  outline: none;
}

.skip {
  position: absolute;
  left: 12px;
  top: -48px;
  z-index: 200;
  padding: 8px 12px;
  border-radius: 8px;
  background: var(--stretto-accent-fill);
  color: var(--stretto-on-accent);
  font-weight: 600;
}

.skip:focus {
  top: 12px;
  color: var(--stretto-on-accent);
}

.shell-scrim {
  position: fixed;
  inset: 0;
  z-index: 25;
  background: var(--c-backdrop);
}

.scrim-enter-active,
.scrim-leave-active {
  transition: opacity 0.18s var(--c-ease);
}

.scrim-enter-from,
.scrim-leave-to {
  opacity: 0;
}

@media (max-width: 900px) {
  .shell,
  .shell.collapsed {
    grid-template-columns: minmax(0, 1fr);
  }

  /* Closed, the drawer is out of view and out of the tab order; its shadow shows only when open. */
  .shell-side {
    position: fixed;
    left: 0;
    top: 0;
    bottom: 0;
    width: min(280px, 84vw);
    border-right: 0;
    transform: translateX(-100%);
    visibility: hidden;
    transition:
      transform 0.2s var(--c-ease),
      visibility 0s linear 0.2s;
  }

  .shell-side.open {
    transform: none;
    visibility: visible;
    box-shadow: var(--c-shadow-lg);
    transition:
      transform 0.2s var(--c-ease),
      visibility 0s;
  }
}

.down {
  display: grid;
  place-items: center;
  min-height: 100vh;
  padding: 24px 16px;
}

.down-card {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 12px;
  width: min(460px, 100%);
  padding: 28px;
  border-radius: 16px;
  background: var(--stretto-surface);
  border: 1px solid var(--stretto-border);
  box-shadow: var(--c-shadow-md);
}

.down-icon {
  display: grid;
  place-items: center;
  width: 40px;
  height: 40px;
  margin-top: 8px;
  border-radius: 10px;
  background: var(--c-danger-soft);
  color: var(--c-danger);
}

.down-card h1 {
  font-size: 19px;
}

.down-card .mono {
  font-size: 12px;
}
</style>

<script setup lang="ts">
/** The top bar: the domain filter, the command palette, live status, read-only, and the theme. */
import { computed } from 'vue'
import { Lock, Menu, Monitor, Moon, Search, Sun } from '@lucide/vue'
import UiKbd from '../ui/UiKbd.vue'
import { liveStatus } from '@/api/events'
import { readOnly } from '@/stores/auth'
import { domainFilter, domainNames } from '@/stores/domain'
import { cycleTheme, theme } from '@/stores/theme'
import { modKey } from '@/composables/useHotkey'

defineEmits<{ menu: []; search: [] }>()

const themeLabel = computed(
  () =>
    ({ system: 'Theme: the system’s', light: 'Theme: light', dark: 'Theme: dark' })[theme.value],
)
const live = computed(() => {
  switch (liveStatus.value) {
    case 'live':
      return { label: 'Live', title: 'Lists refresh as stretto writes sessions, flows and jobs' }
    case 'connecting':
      return { label: 'Connecting', title: 'Opening the live updates' }
    case 'reconnecting':
      return { label: 'Reconnecting', title: 'The live updates dropped; trying again' }
    default:
      return { label: 'Offline', title: 'No live updates' }
  }
})
const options = computed(() => {
  const names = [...domainNames.value]
  if (domainFilter.value && !names.includes(domainFilter.value)) names.unshift(domainFilter.value)
  return names
})
</script>

<template>
  <header class="top">
    <button
      type="button"
      class="top-icon top-menu"
      aria-label="Open the menu"
      @click="$emit('menu')"
    >
      <Menu :size="19" :stroke-width="1.8" aria-hidden="true" />
    </button>

    <label class="top-domain">
      <span class="sr-only">Domain</span>
      <select
        v-model="domainFilter"
        class="select top-select"
        :class="{ set: domainFilter }"
        data-testid="domain-filter"
      >
        <option :value="null">All domains</option>
        <option v-for="name in options" :key="name" :value="name">{{ name }}</option>
      </select>
    </label>

    <button type="button" class="top-search" data-testid="palette-open" @click="$emit('search')">
      <Search :size="16" :stroke-width="1.9" aria-hidden="true" />
      <span class="top-search-text">Search or jump to…</span>
      <span class="top-search-keys" aria-hidden="true"
        ><UiKbd>{{ modKey }}</UiKbd
        ><UiKbd>K</UiKbd></span
      >
      <span class="sr-only">({{ modKey }} K)</span>
    </button>

    <span class="spacer" />

    <span
      v-if="readOnly"
      class="top-ro"
      title="Started with --read-only: every write and action is refused"
      data-testid="read-only"
    >
      <Lock :size="13" :stroke-width="2.2" aria-hidden="true" />
      Read-only
    </span>

    <span class="top-live" :class="liveStatus" :title="live.title" data-testid="live">
      <span class="top-live-dot" aria-hidden="true" />
      <span class="top-live-text">{{ live.label }}</span>
    </span>

    <button
      type="button"
      class="top-icon"
      :aria-label="`${themeLabel}. Change it`"
      :title="themeLabel"
      @click="cycleTheme()"
    >
      <Monitor v-if="theme === 'system'" :size="18" :stroke-width="1.8" aria-hidden="true" />
      <Sun v-else-if="theme === 'light'" :size="18" :stroke-width="1.8" aria-hidden="true" />
      <Moon v-else :size="18" :stroke-width="1.8" aria-hidden="true" />
    </button>
  </header>
</template>

<style scoped>
.top {
  position: sticky;
  top: 0;
  z-index: 20;
  display: flex;
  align-items: center;
  gap: 10px;
  height: var(--c-topbar-h);
  padding: 0 var(--c-gutter);
  background: color-mix(in oklab, var(--stretto-bg) 86%, transparent);
  backdrop-filter: saturate(1.4) blur(10px);
  border-bottom: 1px solid var(--stretto-border);
}

.top-icon {
  display: grid;
  place-items: center;
  flex: none;
  width: 36px;
  height: 36px;
  border: 0;
  border-radius: 9px;
  background: none;
  color: var(--stretto-text-muted);
}

.top-icon:hover {
  background: var(--c-hover);
  color: var(--stretto-text);
}

.top-menu {
  display: none;
  margin-left: -6px;
}

.top-domain {
  flex: none;
}

.top-select {
  width: auto;
  min-width: 150px;
  max-width: 220px;
  height: 36px;
  min-height: 36px;
  font-size: 13.5px;
  font-weight: 500;
  border-color: var(--stretto-border);
  background-color: var(--stretto-surface);
}

.top-select.set {
  border-color: color-mix(in oklab, var(--stretto-accent-graphic) 60%, var(--stretto-border));
  background-color: var(--stretto-accent-soft);
  color: var(--stretto-accent);
}

.top-search {
  display: flex;
  align-items: center;
  gap: 9px;
  flex: 0 1 380px;
  min-width: 0;
  height: 36px;
  padding: 0 8px 0 12px;
  border: 1px solid var(--stretto-border);
  border-radius: 9px;
  background: var(--stretto-surface);
  color: var(--stretto-text-subtle);
  font-size: 13.5px;
  text-align: left;
  transition: border-color 0.12s var(--c-ease);
}

.top-search:hover {
  border-color: color-mix(in oklab, var(--stretto-border-strong) 60%, var(--stretto-border));
  color: var(--stretto-text-muted);
}

.top-search-text {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.top-search-keys {
  display: inline-flex;
  gap: 3px;
}

.top-ro {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 26px;
  padding: 0 10px;
  border-radius: 999px;
  background: var(--c-warn-soft);
  border: 1px solid var(--c-warn-border);
  color: var(--c-warn);
  font-size: 12.5px;
  font-weight: 600;
  white-space: nowrap;
}

.top-live {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  font-size: 12.5px;
  font-weight: 500;
  color: var(--stretto-text-subtle);
  white-space: nowrap;
}

.top-live-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--stretto-border-strong);
}

.top-live.live .top-live-dot {
  background: var(--stretto-accent-graphic);
  box-shadow: 0 0 0 3px color-mix(in oklab, var(--stretto-accent-graphic) 22%, transparent);
}

.top-live.reconnecting .top-live-dot,
.top-live.connecting .top-live-dot {
  background: transparent;
  box-shadow: inset 0 0 0 1.5px var(--stretto-accent-graphic);
  animation: blink 1.2s ease-in-out infinite;
}

@keyframes blink {
  50% {
    opacity: 0.35;
  }
}

@media (max-width: 900px) {
  .top-menu {
    display: grid;
  }
}

@media (max-width: 720px) {
  .top {
    gap: 6px;
  }

  .top-select {
    min-width: 0;
    max-width: 150px;
  }

  .top-search {
    flex: 0 0 36px;
    width: 36px;
    padding: 0;
    justify-content: center;
  }

  .top-search-text,
  .top-search-keys {
    display: none;
  }

  .top-live-text {
    display: none;
  }

  .top-ro {
    padding: 0 8px;
    font-size: 0;
    gap: 0;
  }

  .top-ro svg {
    width: 14px;
    height: 14px;
  }
}
</style>

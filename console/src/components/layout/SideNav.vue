<script setup lang="ts">
/** The sidebar: the six pages, the brand, and the console's version. It collapses to icons. */
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import {
  LayoutDashboard,
  PanelLeftClose,
  PanelLeftOpen,
  ScrollText,
  Server,
  Settings,
  SquareTerminal,
  Workflow,
} from '@lucide/vue'
import BrandLogo from '../BrandLogo.vue'
import { meta } from '@/stores/auth'
import { runningJobs } from '@/stores/jobs'

defineProps<{ collapsed: boolean; drawer: boolean }>()
const emit = defineEmits<{ toggle: []; navigate: [] }>()
const route = useRoute()

const items = [
  { name: 'overview', label: 'Overview', icon: LayoutDashboard, match: ['overview'] },
  {
    name: 'servers',
    label: 'Servers',
    icon: Server,
    match: ['servers', 'server', 'server-new', 'server-edit'],
  },
  { name: 'sessions', label: 'Sessions', icon: ScrollText, match: ['sessions', 'session'] },
  { name: 'flows', label: 'Flows', icon: Workflow, match: ['flows', 'flow'] },
  { name: 'jobs', label: 'Jobs', icon: SquareTerminal, match: ['jobs', 'job', 'job-new'] },
]
const settings = { name: 'settings', label: 'Settings', icon: Settings, match: ['settings'] }

const current = computed(() => String(route.name ?? ''))
</script>

<template>
  <nav class="side" :class="{ collapsed: collapsed && !drawer }" aria-label="Pages">
    <div class="side-brand">
      <RouterLink
        :to="{ name: 'overview' }"
        class="side-logo"
        aria-label="stretto console, overview"
        @click="emit('navigate')"
      >
        <BrandLogo v-if="collapsed && !drawer" variant="mark" :height="26" />
        <BrandLogo v-else :height="22" />
      </RouterLink>
      <span v-if="!collapsed || drawer" class="side-product">console</span>
    </div>

    <ul class="side-list">
      <li v-for="item in items" :key="item.name">
        <RouterLink
          :to="{ name: item.name }"
          class="side-link"
          :class="{ active: item.match.includes(current) }"
          :aria-current="item.match.includes(current) ? 'page' : undefined"
          :title="collapsed && !drawer ? item.label : undefined"
          @click="emit('navigate')"
        >
          <component :is="item.icon" :size="18" :stroke-width="1.8" aria-hidden="true" />
          <span class="side-label">{{ item.label }}</span>
          <span
            v-if="item.name === 'jobs' && runningJobs > 0"
            class="side-pill"
            :title="`${runningJobs} running`"
          >
            {{ runningJobs }}<span class="sr-only"> running</span>
          </span>
        </RouterLink>
      </li>
    </ul>

    <div class="side-bottom">
      <RouterLink
        :to="{ name: settings.name }"
        class="side-link"
        :class="{ active: settings.match.includes(current) }"
        :aria-current="settings.match.includes(current) ? 'page' : undefined"
        :title="collapsed && !drawer ? settings.label : undefined"
        @click="emit('navigate')"
      >
        <component :is="settings.icon" :size="18" :stroke-width="1.8" aria-hidden="true" />
        <span class="side-label">{{ settings.label }}</span>
      </RouterLink>
      <div class="side-foot">
        <span v-if="(!collapsed || drawer) && meta" class="side-version" :title="meta.data_dir">
          v{{ meta.version }} · <span class="mono">{{ meta.data_dir }}</span>
        </span>
        <button
          v-if="!drawer"
          type="button"
          class="side-collapse"
          :aria-label="collapsed ? 'Expand the sidebar' : 'Collapse the sidebar'"
          :title="collapsed ? 'Expand the sidebar' : 'Collapse the sidebar'"
          @click="emit('toggle')"
        >
          <PanelLeftOpen v-if="collapsed" :size="17" :stroke-width="1.8" aria-hidden="true" />
          <PanelLeftClose v-else :size="17" :stroke-width="1.8" aria-hidden="true" />
        </button>
      </div>
    </div>
  </nav>
</template>

<style scoped>
.side {
  display: flex;
  flex-direction: column;
  height: 100%;
  padding: 14px 12px 12px;
  gap: 6px;
  overflow: hidden;
}

.side-brand {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 40px;
  padding: 0 8px;
  margin-bottom: 10px;
}

.side-logo {
  display: inline-flex;
  border-radius: 6px;
}

.side-product {
  padding: 2px 7px;
  border-radius: 6px;
  background: var(--stretto-surface-2);
  border: 1px solid var(--stretto-border);
  color: var(--stretto-text-muted);
  font-size: 11.5px;
  font-weight: 500;
  letter-spacing: 0.01em;
  margin-top: 1px;
}

.side-list {
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.side-link {
  position: relative;
  display: flex;
  align-items: center;
  gap: 11px;
  height: 38px;
  padding: 0 10px;
  border-radius: 9px;
  color: var(--stretto-text-muted);
  font-size: 14px;
  font-weight: 500;
  white-space: nowrap;
  transition:
    background-color 0.12s var(--c-ease),
    color 0.12s var(--c-ease);
}

.side-link svg {
  flex: none;
}

.side-link:hover {
  background: var(--c-hover);
  color: var(--stretto-text);
}

.side-link.active {
  background: var(--stretto-accent-soft);
  color: var(--stretto-accent);
}

.side-link.active::before {
  content: '';
  position: absolute;
  left: -12px;
  top: 9px;
  bottom: 9px;
  width: 3px;
  border-radius: 0 3px 3px 0;
  background: var(--stretto-accent-graphic);
}

.side-label {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
}

.side-pill {
  min-width: 20px;
  height: 20px;
  padding: 0 6px;
  border-radius: 999px;
  background: var(--stretto-accent-fill);
  color: var(--stretto-on-accent);
  font-size: 11.5px;
  font-weight: 600;
  line-height: 20px;
  text-align: center;
}

.side-bottom {
  margin-top: auto;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.side-foot {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 4px 0 10px;
  border-top: 1px solid var(--stretto-border);
  min-height: 44px;
}

.side-version {
  flex: 1;
  min-width: 0;
  font-size: 11.5px;
  color: var(--stretto-text-subtle);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.side-version .mono {
  font-size: 11px;
}

.side-collapse {
  display: grid;
  place-items: center;
  flex: none;
  width: 32px;
  height: 32px;
  border: 0;
  border-radius: 8px;
  background: none;
  color: var(--stretto-text-subtle);
}

.side-collapse:hover {
  background: var(--c-hover);
  color: var(--stretto-text);
}

.collapsed .side-brand {
  justify-content: center;
  padding: 0;
}

.collapsed .side-link {
  justify-content: center;
  padding: 0;
}

.collapsed .side-label {
  display: none;
}

.collapsed .side-pill {
  position: absolute;
  top: 2px;
  right: 2px;
  min-width: 16px;
  height: 16px;
  padding: 0 4px;
  font-size: 10px;
  line-height: 16px;
}

.collapsed .side-foot {
  justify-content: center;
  padding: 8px 0 0;
}
</style>

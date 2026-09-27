<script setup lang="ts">
// Tabs whose panels hold Markdown. In a page:
//
//   <Tabs :tabs="[{ key: 'source', label: 'From source' }, { key: 'docker', label: 'Docker', badge: 'Soon' }]">
//   <template #source>
//
//   ...Markdown...
//
//   </template>
//   </Tabs>
//
// Every panel is rendered, and the inactive ones hidden, so search and readers
// without JavaScript see them all.
import { ref, useId } from 'vue'

interface Tab {
  key: string
  label: string
  badge?: string
}

const props = defineProps<{ tabs: Tab[]; initial?: string }>()
const active = ref(props.initial ?? props.tabs[0]?.key)
const id = useId()
const buttons = ref<HTMLButtonElement[]>([])

function select(index: number) {
  const tab = props.tabs[(index + props.tabs.length) % props.tabs.length]
  active.value = tab.key
  buttons.value[props.tabs.indexOf(tab)]?.focus()
}

function onKeydown(event: KeyboardEvent, index: number) {
  const moves: Record<string, number> = { ArrowRight: index + 1, ArrowLeft: index - 1, Home: 0, End: props.tabs.length - 1 }
  if (event.key in moves) {
    event.preventDefault()
    select(moves[event.key])
  }
}
</script>

<template>
  <div class="md-tabs">
    <div class="md-tabs__list" role="tablist">
      <button
        v-for="(tab, index) in tabs"
        :id="`${id}-tab-${tab.key}`"
        ref="buttons"
        :key="tab.key"
        type="button"
        role="tab"
        class="md-tabs__tab"
        :class="{ 'is-active': active === tab.key }"
        :aria-selected="active === tab.key"
        :aria-controls="`${id}-panel-${tab.key}`"
        :tabindex="active === tab.key ? 0 : -1"
        @click="active = tab.key"
        @keydown="onKeydown($event, index)"
      >
        {{ tab.label }}<span v-if="tab.badge" class="md-tabs__badge">{{ tab.badge }}</span>
      </button>
    </div>
    <div
      v-for="tab in tabs"
      v-show="active === tab.key"
      :id="`${id}-panel-${tab.key}`"
      :key="tab.key"
      role="tabpanel"
      class="md-tabs__panel"
      :aria-labelledby="`${id}-tab-${tab.key}`"
    >
      <slot :name="tab.key" />
    </div>
  </div>
</template>

<style scoped>
.md-tabs {
  margin: 16px 0 24px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 12px;
  overflow: hidden;
}

.md-tabs__list {
  display: flex;
  gap: 4px;
  padding: 0 12px;
  overflow-x: auto;
  background: var(--vp-c-bg-soft);
  border-bottom: 1px solid var(--vp-c-divider);
  scrollbar-width: thin;
}

.md-tabs__tab {
  position: relative;
  flex-shrink: 0;
  padding: 12px 10px;
  font-size: 14px;
  font-weight: 500;
  line-height: 20px;
  white-space: nowrap;
  color: var(--vp-c-text-2);
  transition: color 0.25s;
}

.md-tabs__tab:hover {
  color: var(--vp-c-text-1);
}

.md-tabs__tab.is-active {
  color: var(--vp-c-text-1);
}

.md-tabs__tab.is-active::after {
  content: '';
  position: absolute;
  right: 8px;
  bottom: -1px;
  left: 8px;
  height: 2px;
  border-radius: 2px;
  background: var(--vp-c-brand-1);
}

.md-tabs__tab:focus-visible {
  outline: 2px solid var(--vp-c-brand-1);
  outline-offset: -4px;
  border-radius: 6px;
}

.md-tabs__badge {
  margin-left: 6px;
  padding: 1px 6px;
  border-radius: 10px;
  font-size: 11px;
  font-weight: 600;
  vertical-align: 1px;
  color: var(--vp-c-warning-1);
  background: var(--vp-c-warning-soft);
}

.md-tabs__panel {
  padding: 4px 20px 8px;
}

.md-tabs__panel :deep(div[class*='language-']) {
  margin: 16px 0;
}

@media (max-width: 639px) {
  .md-tabs__panel {
    padding: 4px 16px 8px;
  }

  .md-tabs__panel :deep(div[class*='language-']) {
    margin: 16px -16px;
    border-radius: 0;
  }
}
</style>

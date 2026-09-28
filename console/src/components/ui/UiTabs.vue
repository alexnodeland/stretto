<script setup lang="ts">
/**
 * Tabs: arrow keys move between them (and select), Home and End jump. The
 * panels are the parent's. When the tabs are wider than their row, the row
 * scrolls, fades at the edge that hides more, and keeps the selected tab in view.
 */
import { nextTick, onBeforeUnmount, onMounted, ref, watch, type Component } from 'vue'

export interface Tab {
  id: string
  label: string
  count?: number | null
  icon?: Component
}

const props = defineProps<{ tabs: Tab[]; modelValue: string; idPrefix: string; label: string }>()
const emit = defineEmits<{ 'update:modelValue': [id: string] }>()
const buttons = ref<HTMLButtonElement[]>([])
const row = ref<HTMLElement | null>(null)
const more = ref({ before: false, after: false })

function measure() {
  const el = row.value
  if (!el) return
  const max = el.scrollWidth - el.clientWidth
  more.value = { before: el.scrollLeft > 1, after: el.scrollLeft < max - 1 }
}

/** Scrolls the row, and only the row, so the selected tab shows whole. */
function reveal() {
  const el = row.value
  const tab = buttons.value.find((b) => b.dataset.id === props.modelValue)
  if (!el || !tab) return
  const start = tab.offsetLeft - 24
  const end = tab.offsetLeft + tab.offsetWidth + 24 - el.clientWidth
  if (el.scrollLeft > start) el.scrollLeft = Math.max(0, start)
  else if (el.scrollLeft < end) el.scrollLeft = end
  measure()
}

let observer: ResizeObserver | null = null
onMounted(() => {
  reveal()
  if (row.value && typeof ResizeObserver !== 'undefined') {
    observer = new ResizeObserver(measure)
    observer.observe(row.value)
  }
})
onBeforeUnmount(() => observer?.disconnect())
watch(
  () => props.modelValue,
  () => void nextTick(reveal),
)

async function select(index: number) {
  const tab = props.tabs[(index + props.tabs.length) % props.tabs.length]
  if (!tab) return
  emit('update:modelValue', tab.id)
  await nextTick()
  buttons.value.find((b) => b.dataset.id === tab.id)?.focus()
}

function onKey(event: KeyboardEvent, index: number) {
  const moves: Record<string, number> = {
    ArrowRight: index + 1,
    ArrowLeft: index - 1,
    Home: 0,
    End: props.tabs.length - 1,
  }
  const to = moves[event.key]
  if (to === undefined) return
  event.preventDefault()
  void select(to)
}
</script>

<template>
  <div
    ref="row"
    class="tabs"
    :class="{ 'more-before': more.before, 'more-after': more.after }"
    role="tablist"
    :aria-label="label"
    @scroll.passive="measure"
  >
    <button
      v-for="(tab, i) in tabs"
      :id="`${idPrefix}-tab-${tab.id}`"
      :key="tab.id"
      ref="buttons"
      type="button"
      role="tab"
      class="tab"
      :data-id="tab.id"
      :aria-selected="tab.id === modelValue"
      :aria-controls="`${idPrefix}-panel-${tab.id}`"
      :tabindex="tab.id === modelValue ? 0 : -1"
      @click="emit('update:modelValue', tab.id)"
      @keydown="onKey($event, i)"
    >
      <component :is="tab.icon" v-if="tab.icon" :size="15" :stroke-width="1.9" aria-hidden="true" />
      {{ tab.label }}
      <span v-if="tab.count !== undefined && tab.count !== null" class="tab-count">{{
        tab.count
      }}</span>
    </button>
  </div>
</template>

<style scoped>
.tabs {
  position: relative;
  display: flex;
  gap: 4px;
  border-bottom: 1px solid var(--stretto-border);
  overflow-x: auto;
  scrollbar-width: none;
}

.tabs::-webkit-scrollbar {
  display: none;
}

.tabs.more-after {
  mask-image: linear-gradient(to right, #000 calc(100% - 40px), transparent);
}

.tabs.more-before {
  mask-image: linear-gradient(to right, transparent, #000 40px);
}

.tabs.more-before.more-after {
  mask-image: linear-gradient(
    to right,
    transparent,
    #000 40px,
    #000 calc(100% - 40px),
    transparent
  );
}

.tab {
  position: relative;
  flex: none;
  display: inline-flex;
  align-items: center;
  gap: 7px;
  height: 40px;
  padding: 0 12px;
  border: 0;
  background: none;
  color: var(--stretto-text-muted);
  font-size: 13.5px;
  font-weight: 500;
  white-space: nowrap;
  border-radius: 8px 8px 0 0;
  transition: color 0.12s var(--c-ease);
}

.tab:hover {
  color: var(--stretto-text);
}

.tab[aria-selected='true'] {
  color: var(--stretto-text);
}

.tab[aria-selected='true']::after {
  content: '';
  position: absolute;
  left: 8px;
  right: 8px;
  bottom: -1px;
  height: 2px;
  border-radius: 2px;
  background: var(--stretto-accent-graphic);
}

.tab:focus-visible {
  outline-offset: -2px;
}

.tab svg {
  flex: none;
}

/* On a phone the words carry the tabs; the icons would push the last one out of view. */
@media (max-width: 520px) {
  .tabs {
    gap: 2px;
  }

  .tab {
    padding: 0 10px;
  }

  .tab svg {
    display: none;
  }
}

.tab-count {
  min-width: 20px;
  height: 18px;
  padding: 0 6px;
  border-radius: 999px;
  background: var(--stretto-surface-2);
  border: 1px solid var(--stretto-border);
  color: var(--stretto-text-subtle);
  font-size: 11.5px;
  line-height: 16px;
  font-variant-numeric: tabular-nums;
}
</style>

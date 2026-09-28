<script setup lang="ts">
/** A short label: a mode, a kind, a status. Its meaning is in its words; the tone only supports them. */
import type { Component } from 'vue'

withDefaults(
  defineProps<{
    tone?: 'neutral' | 'accent' | 'solid' | 'dashed' | 'outline' | 'ink' | 'danger' | 'warn'
    icon?: Component
    dot?: boolean
    mono?: boolean
    title?: string
  }>(),
  { tone: 'neutral', icon: undefined, title: undefined },
)
</script>

<template>
  <span class="badge" :class="[`badge-${tone}`, { 'badge-mono': mono }]" :title="title">
    <span v-if="dot" class="badge-dot" aria-hidden="true" />
    <component :is="icon" v-if="icon" :size="12" :stroke-width="2.2" aria-hidden="true" />
    <slot />
  </span>
</template>

<style scoped>
.badge {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 22px;
  padding: 0 8px;
  border-radius: 999px;
  border: 1px solid transparent;
  font-family: var(--stretto-font-sans);
  font-size: 12px;
  font-weight: 500;
  letter-spacing: 0;
  line-height: 1;
  white-space: nowrap;
  vertical-align: middle;
}

.badge-mono {
  font-family: var(--stretto-font-mono);
  font-size: 11.5px;
  font-weight: 400;
  border-radius: 6px;
}

.badge-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: currentColor;
}

.badge-neutral {
  background: var(--stretto-surface-2);
  border-color: var(--stretto-border);
  color: var(--stretto-text-muted);
}

.badge-accent {
  background: var(--stretto-accent-soft);
  border-color: color-mix(in oklab, var(--stretto-accent-graphic) 28%, transparent);
  color: var(--stretto-accent);
}

.badge-solid {
  background: var(--stretto-accent-fill);
  color: var(--stretto-on-accent);
}

.badge-dashed {
  background: transparent;
  border: 1px dashed var(--stretto-accent-graphic);
  color: var(--stretto-accent);
}

.badge-outline {
  background: transparent;
  border-color: var(--stretto-border-strong);
  color: var(--stretto-text-muted);
}

.badge-ink {
  background: var(--stretto-text);
  color: var(--stretto-bg);
}

.badge-danger {
  background: var(--c-danger-soft);
  border-color: var(--c-danger-border);
  color: var(--c-danger);
}

.badge-warn {
  background: var(--c-warn-soft);
  border-color: var(--c-warn-border);
  color: var(--c-warn);
}
</style>

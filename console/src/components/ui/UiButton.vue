<script setup lang="ts">
/** A button, or a link that looks like one (`to` for the router, `href` for a download). */
import { computed, type Component } from 'vue'
import { RouterLink, type RouteLocationRaw } from 'vue-router'
import { LoaderCircle } from '@lucide/vue'

const props = withDefaults(
  defineProps<{
    variant?: 'primary' | 'secondary' | 'ghost' | 'danger'
    size?: 'sm' | 'md'
    to?: RouteLocationRaw
    href?: string
    download?: string | boolean
    type?: 'button' | 'submit'
    disabled?: boolean
    loading?: boolean
    icon?: Component
    iconRight?: Component
    /** A square icon button: give it an aria-label. */
    square?: boolean
    block?: boolean
    /** Why it is disabled, shown on hover. */
    reason?: string
  }>(),
  {
    variant: 'secondary',
    size: 'md',
    type: 'button',
    to: undefined,
    href: undefined,
    download: undefined,
    icon: undefined,
    iconRight: undefined,
    reason: undefined,
  },
)

const off = computed(() => props.disabled || props.loading)
const tag = computed(() =>
  off.value ? 'button' : props.to ? RouterLink : props.href ? 'a' : 'button',
)
const attrs = computed(() => {
  if (off.value) return { type: 'button', disabled: true, title: props.reason }
  if (props.to) return { to: props.to }
  if (props.href)
    return { href: props.href, download: props.download === true ? '' : props.download }
  return { type: props.type }
})
</script>

<template>
  <component
    :is="tag"
    v-bind="attrs"
    class="btn"
    :class="[
      `btn-${variant}`,
      `btn-${size}`,
      { 'btn-square': square, 'btn-block': block, 'is-loading': loading },
    ]"
    :aria-busy="loading || undefined"
  >
    <LoaderCircle
      v-if="loading"
      class="btn-icon spin"
      :size="size === 'sm' ? 14 : 16"
      aria-hidden="true"
    />
    <component
      :is="icon"
      v-else-if="icon"
      class="btn-icon"
      :size="size === 'sm' ? 14 : 16"
      :stroke-width="1.9"
      aria-hidden="true"
    />
    <span v-if="$slots.default" class="btn-label"><slot /></span>
    <component
      :is="iconRight"
      v-if="iconRight"
      class="btn-icon"
      :size="size === 'sm' ? 14 : 16"
      :stroke-width="1.9"
      aria-hidden="true"
    />
  </component>
</template>

<style scoped>
.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 7px;
  height: 36px;
  padding: 0 14px;
  border-radius: var(--c-radius-control);
  border: 1px solid transparent;
  font-size: 13.5px;
  font-weight: 500;
  line-height: 1;
  white-space: nowrap;
  text-decoration: none;
  user-select: none;
  transition:
    background-color 0.12s var(--c-ease),
    border-color 0.12s var(--c-ease),
    color 0.12s var(--c-ease),
    box-shadow 0.12s var(--c-ease);
}

.btn-sm {
  height: 30px;
  padding: 0 10px;
  font-size: 13px;
  gap: 6px;
}

.btn-square {
  width: 36px;
  padding: 0;
}

.btn-sm.btn-square {
  width: 30px;
}

.btn-block {
  width: 100%;
}

.btn-label {
  overflow: hidden;
  text-overflow: ellipsis;
}

.btn-icon {
  flex: none;
}

.btn-primary {
  background: var(--stretto-accent-fill);
  color: var(--stretto-on-accent);
  box-shadow: 0 1px 1px rgb(0 0 0 / 0.06);
}

.btn-primary:hover {
  background: var(--stretto-accent-hover);
  color: var(--stretto-on-accent);
}

.btn-secondary {
  background: var(--stretto-surface);
  border-color: var(--stretto-border);
  color: var(--stretto-text);
  box-shadow: var(--c-shadow-sm);
}

.btn-secondary:hover {
  border-color: color-mix(in oklab, var(--stretto-border-strong) 60%, var(--stretto-border));
  background: color-mix(in oklab, var(--stretto-surface) 70%, var(--stretto-surface-2));
  color: var(--stretto-text);
}

.btn-ghost {
  background: transparent;
  color: var(--stretto-text-muted);
}

.btn-ghost:hover {
  background: var(--c-hover);
  color: var(--stretto-text);
}

.btn-danger {
  background: var(--stretto-surface);
  border-color: var(--c-danger-border);
  color: var(--c-danger);
}

.btn-danger:hover {
  background: var(--c-danger-soft);
  color: var(--c-danger);
}

.btn:disabled {
  opacity: 0.55;
  box-shadow: none;
}

.btn-primary:disabled:hover {
  background: var(--stretto-accent-fill);
}

.btn-secondary:disabled:hover {
  border-color: var(--stretto-border);
  background: var(--stretto-surface);
}

.btn-ghost:disabled:hover {
  background: transparent;
  color: var(--stretto-text-muted);
}

.is-loading {
  opacity: 0.85 !important;
  cursor: progress;
}

.spin {
  animation: spin 0.9s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>

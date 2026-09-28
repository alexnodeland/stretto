<script setup lang="ts" generic="T extends string">
/** A choice of a few options, as a radio group: arrow keys move the choice. */
import type { Component } from 'vue'

export interface Option<V> {
  value: V
  label: string
  icon?: Component
}

const props = defineProps<{
  options: Option<T>[]
  modelValue: T
  label: string
  size?: 'sm' | 'md'
}>()
const emit = defineEmits<{ 'update:modelValue': [value: T] }>()

function onKey(event: KeyboardEvent, index: number) {
  const step =
    event.key === 'ArrowRight' || event.key === 'ArrowDown'
      ? 1
      : event.key === 'ArrowLeft' || event.key === 'ArrowUp'
        ? -1
        : 0
  if (!step) return
  event.preventDefault()
  const next = props.options[(index + step + props.options.length) % props.options.length]
  if (!next) return
  emit('update:modelValue', next.value)
  const group = (event.currentTarget as HTMLElement).parentElement
  requestAnimationFrame(() => group?.querySelector<HTMLElement>('[aria-checked="true"]')?.focus())
}
</script>

<template>
  <div class="seg" :class="`seg-${size ?? 'md'}`" role="radiogroup" :aria-label="label">
    <button
      v-for="(option, i) in options"
      :key="option.value"
      type="button"
      role="radio"
      class="seg-option"
      :aria-checked="option.value === modelValue"
      :tabindex="option.value === modelValue ? 0 : -1"
      @click="emit('update:modelValue', option.value)"
      @keydown="onKey($event, i)"
    >
      <component
        :is="option.icon"
        v-if="option.icon"
        :size="14"
        :stroke-width="1.9"
        aria-hidden="true"
      />
      {{ option.label }}
    </button>
  </div>
</template>

<style scoped>
.seg {
  display: inline-flex;
  width: fit-content;
  gap: 2px;
  padding: 3px;
  border-radius: 10px;
  background: var(--stretto-surface-2);
  border: 1px solid var(--stretto-border);
  max-width: 100%;
  overflow-x: auto;
  scrollbar-width: none;
}

.seg-option {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 28px;
  padding: 0 11px;
  border: 0;
  border-radius: 7px;
  background: transparent;
  color: var(--stretto-text-muted);
  font-size: 13px;
  font-weight: 500;
  white-space: nowrap;
  transition:
    background-color 0.12s var(--c-ease),
    color 0.12s var(--c-ease);
}

.seg-sm .seg-option {
  height: 24px;
  padding: 0 9px;
  font-size: 12.5px;
}

.seg-option:hover {
  color: var(--stretto-text);
}

.seg-option[aria-checked='true'] {
  background: var(--stretto-surface);
  color: var(--stretto-text);
  box-shadow:
    0 1px 2px rgb(20 28 30 / 0.08),
    0 0 0 1px var(--stretto-border);
}

/* On a phone an option's words may wrap, rather than push the next option out of view. */
@media (max-width: 520px) {
  .seg-option {
    flex: 0 1 auto;
    min-width: 0;
    height: auto;
    min-height: 28px;
    padding-top: 4px;
    padding-bottom: 4px;
    line-height: 1.25;
    text-align: center;
    white-space: normal;
  }

  .seg-sm .seg-option {
    height: auto;
    min-height: 24px;
  }
}
</style>

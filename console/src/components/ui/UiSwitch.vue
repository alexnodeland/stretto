<script setup lang="ts">
/** An on/off switch with its label. */
defineProps<{ modelValue: boolean; label: string; hint?: string; disabled?: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [value: boolean] }>()
</script>

<template>
  <label class="switch-row" :class="{ disabled }">
    <button
      type="button"
      role="switch"
      class="switch"
      :aria-checked="modelValue"
      :disabled="disabled"
      @click="emit('update:modelValue', !modelValue)"
    >
      <span class="switch-thumb" />
    </button>
    <span class="switch-text">
      <span class="switch-label">{{ label }}</span>
      <span v-if="hint" class="switch-hint">{{ hint }}</span>
    </span>
  </label>
</template>

<style scoped>
.switch-row {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  cursor: pointer;
}

.switch-row.disabled {
  cursor: not-allowed;
  opacity: 0.6;
}

.switch {
  position: relative;
  flex: none;
  width: 36px;
  height: 20px;
  margin-top: 1px;
  padding: 0;
  border-radius: 999px;
  border: 1px solid var(--stretto-border-strong);
  background: var(--stretto-surface-2);
  transition:
    background-color 0.15s var(--c-ease),
    border-color 0.15s var(--c-ease);
}

.switch[aria-checked='true'] {
  background: var(--stretto-accent-fill);
  border-color: var(--stretto-accent-fill);
}

.switch-thumb {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: var(--stretto-surface);
  box-shadow: 0 1px 2px rgb(0 0 0 / 0.25);
  transition: transform 0.15s var(--c-ease);
}

.switch[aria-checked='true'] .switch-thumb {
  transform: translateX(16px);
  background: var(--stretto-on-accent);
}

.switch-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.switch-label {
  font-size: 14px;
  font-weight: 500;
}

.switch-hint {
  font-size: 12.5px;
  color: var(--stretto-text-subtle);
}
</style>

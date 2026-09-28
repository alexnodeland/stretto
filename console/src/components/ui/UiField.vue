<script setup lang="ts">
/** A labelled control, with its hint and its error; the slot gets the ids to wire. */
import { computed, useId } from 'vue'

const props = defineProps<{
  label: string
  hint?: string
  error?: string | null
  optional?: boolean
}>()
const id = useId()
const hintId = computed(() => (props.hint ? `${id}-hint` : undefined))
const errorId = computed(() => (props.error ? `${id}-error` : undefined))
const describedby = computed(
  () => [errorId.value, hintId.value].filter(Boolean).join(' ') || undefined,
)
</script>

<template>
  <div class="field">
    <label class="field-label" :for="id">
      {{ label }}
      <span v-if="optional" class="field-optional">optional</span>
    </label>
    <slot :id="id" :describedby="describedby" :invalid="!!error" />
    <p v-if="error" :id="errorId" class="field-error">{{ error }}</p>
    <p v-if="hint" :id="hintId" class="field-hint">
      <slot name="hint">{{ hint }}</slot>
    </p>
  </div>
</template>

<style scoped>
.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
}

.field-label {
  font-size: 13px;
  font-weight: 500;
  color: var(--stretto-text);
}

.field-optional {
  margin-left: 6px;
  font-weight: 400;
  color: var(--stretto-text-subtle);
}

.field-hint {
  font-size: 12.5px;
  color: var(--stretto-text-subtle);
}

.field-error {
  font-size: 12.5px;
  color: var(--c-danger);
}
</style>

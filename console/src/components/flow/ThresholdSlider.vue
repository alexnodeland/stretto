<script setup lang="ts">
/** The threshold a flow is served at: slide it to preview which lookups act. */
import { computed, useId } from 'vue'
import { RotateCcw } from '@lucide/vue'

const props = withDefaults(
  defineProps<{
    modelValue: number
    defaultValue?: number
    marks?: { value: number; label: string }[]
  }>(),
  {
    defaultValue: 0.3,
    marks: () => [],
  },
)
const emit = defineEmits<{ 'update:modelValue': [value: number] }>()
const id = useId()

function set(value: number) {
  if (!Number.isFinite(value)) return
  emit('update:modelValue', Math.round(Math.max(0, Math.min(1, value)) * 100) / 100)
}

const fill = computed(() => `${props.modelValue * 100}%`)
const allMarks = computed(() => {
  const list = [
    { value: props.defaultValue, label: `${props.defaultValue} default` },
    ...props.marks,
  ]
  return list.filter((m, i) => list.findIndex((x) => Math.abs(x.value - m.value) < 1e-9) === i)
})
</script>

<template>
  <div class="ts">
    <label class="ts-label" :for="id">Threshold</label>
    <div class="ts-track">
      <input
        :id="id"
        type="range"
        min="0"
        max="1"
        step="0.01"
        class="ts-range"
        :style="{ '--fill': fill }"
        :value="modelValue"
        :aria-valuetext="`${modelValue.toFixed(2)}`"
        data-testid="threshold-range"
        @input="set(Number(($event.target as HTMLInputElement).value))"
      />
      <div class="ts-marks" aria-hidden="true">
        <span
          v-for="m in allMarks"
          :key="m.label"
          class="ts-mark"
          :style="{ left: `${m.value * 100}%` }"
        >
          <span class="ts-tick" />
          <span class="ts-mark-label">{{ m.label }}</span>
        </span>
      </div>
    </div>
    <input
      class="input ts-number num"
      type="number"
      min="0"
      max="1"
      step="0.01"
      :value="modelValue.toFixed(2)"
      aria-label="Threshold, as a number"
      data-testid="threshold-number"
      @change="set(Number(($event.target as HTMLInputElement).value))"
    />
    <button
      type="button"
      class="ts-reset"
      :disabled="Math.abs(modelValue - defaultValue) < 1e-9"
      :title="`Back to ${defaultValue}`"
      :aria-label="`Reset the threshold to ${defaultValue}`"
      @click="set(defaultValue)"
    >
      <RotateCcw :size="15" :stroke-width="2" aria-hidden="true" />
    </button>
  </div>
</template>

<style scoped>
.ts {
  display: flex;
  align-items: center;
  gap: 14px;
}

.ts-label {
  flex: none;
  font-size: 13px;
  font-weight: 600;
}

.ts-track {
  position: relative;
  flex: 1 1 auto;
  min-width: 140px;
  padding-bottom: 18px;
}

.ts-range {
  width: 100%;
  height: 22px;
  margin: 0;
  background: transparent;
  appearance: none;
  cursor: pointer;
}

.ts-range::-webkit-slider-runnable-track {
  height: 6px;
  border-radius: 3px;
  background: linear-gradient(
    to right,
    var(--stretto-accent-graphic) var(--fill),
    var(--stretto-surface-2) var(--fill)
  );
  box-shadow: inset 0 0 0 1px var(--stretto-border);
}

.ts-range::-moz-range-track {
  height: 6px;
  border-radius: 3px;
  background: var(--stretto-surface-2);
  box-shadow: inset 0 0 0 1px var(--stretto-border);
}

.ts-range::-moz-range-progress {
  height: 6px;
  border-radius: 3px;
  background: var(--stretto-accent-graphic);
}

.ts-range::-webkit-slider-thumb {
  appearance: none;
  width: 18px;
  height: 18px;
  margin-top: -6px;
  border-radius: 50%;
  background: var(--stretto-surface);
  border: 2px solid var(--stretto-accent-fill);
  box-shadow: 0 1px 3px rgb(0 0 0 / 0.2);
}

.ts-range::-moz-range-thumb {
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: var(--stretto-surface);
  border: 2px solid var(--stretto-accent-fill);
}

.ts-range:focus-visible {
  outline: none;
}

.ts-range:focus-visible::-webkit-slider-thumb {
  box-shadow: 0 0 0 4px color-mix(in oklab, var(--stretto-focus) 30%, transparent);
}

.ts-marks {
  position: absolute;
  left: 9px;
  right: 9px;
  bottom: 0;
  height: 18px;
}

.ts-mark {
  position: absolute;
  top: 0;
  transform: translateX(-50%);
  display: flex;
  flex-direction: column;
  align-items: center;
}

.ts-tick {
  width: 1px;
  height: 5px;
  background: var(--stretto-border-strong);
}

.ts-mark-label {
  font-size: 11px;
  color: var(--stretto-text-subtle);
  white-space: nowrap;
}

.ts-number {
  flex: none;
  width: 84px;
  text-align: right;
}

.ts-reset {
  display: grid;
  place-items: center;
  flex: none;
  width: 34px;
  height: 34px;
  border: 1px solid var(--stretto-border);
  border-radius: 8px;
  background: var(--stretto-surface);
  color: var(--stretto-text-muted);
}

.ts-reset:hover:not(:disabled) {
  color: var(--stretto-text);
  background: var(--stretto-surface-2);
}

.ts-reset:disabled {
  opacity: 0.45;
}

@media (max-width: 520px) {
  .ts {
    flex-wrap: wrap;
  }

  .ts-track {
    order: 3;
    flex-basis: 100%;
  }

  .ts-number {
    margin-left: auto;
  }
}
</style>

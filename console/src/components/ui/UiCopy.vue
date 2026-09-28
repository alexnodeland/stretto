<script setup lang="ts">
/** A copy button: the icon turns to a check for a moment once copied. */
import { ref } from 'vue'
import { Check, Copy } from '@lucide/vue'
import { copyText } from '@/composables/useCopy'

const props = withDefaults(
  defineProps<{ text: string; what?: string; label?: string; size?: 'sm' | 'md' }>(),
  {
    what: 'Text',
    label: undefined,
    size: 'sm',
  },
)
const done = ref(false)
let timer: ReturnType<typeof setTimeout> | null = null

async function copy() {
  if (await copyText(props.text, props.what)) {
    done.value = true
    if (timer) clearTimeout(timer)
    timer = setTimeout(() => (done.value = false), 1600)
  }
}
</script>

<template>
  <button
    type="button"
    class="copy"
    :class="[`copy-${size}`, { 'copy-labelled': label, done }]"
    :aria-label="label ?? `Copy ${what.toLowerCase()}`"
    :title="label ? undefined : `Copy ${what.toLowerCase()}`"
    @click.stop="copy"
  >
    <Check v-if="done" :size="14" :stroke-width="2.2" aria-hidden="true" />
    <Copy v-else :size="14" :stroke-width="1.9" aria-hidden="true" />
    <span v-if="label">{{ done ? 'Copied' : label }}</span>
  </button>
</template>

<style scoped>
.copy {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  height: 28px;
  min-width: 28px;
  padding: 0 7px;
  border-radius: 7px;
  border: 1px solid var(--stretto-border);
  background: var(--stretto-surface);
  color: var(--stretto-text-muted);
  font-size: 12.5px;
  font-weight: 500;
  transition:
    background-color 0.12s var(--c-ease),
    color 0.12s var(--c-ease);
}

.copy-md {
  height: 32px;
  padding: 0 10px;
}

.copy:hover {
  background: var(--stretto-surface-2);
  color: var(--stretto-text);
}

.copy.done {
  color: var(--stretto-accent);
}
</style>

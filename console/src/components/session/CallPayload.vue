<script setup lang="ts">
/** A call's arguments and its result: JSON as a tree, text as text, each with copy. */
import { computed } from 'vue'
import { Scissors } from '@lucide/vue'
import UiJson from '../ui/UiJson.vue'
import UiCopy from '../ui/UiCopy.vue'
import type { CallView } from '@/api/types'

const props = defineProps<{ call: CallView }>()
const hasJsonResult = computed(
  () => props.call.result_json !== null && props.call.result_json !== undefined,
)
</script>

<template>
  <div class="payload">
    <UiJson :value="call.arguments" label="Arguments" what="Arguments" :depth="3" />
    <template v-if="call.ok === null">
      <p class="caption no-answer">The server never answered this call.</p>
    </template>
    <UiJson
      v-else-if="hasJsonResult"
      :value="call.result_json"
      :label="call.ok ? 'Result' : 'Error result'"
      what="Result"
      :depth="2"
      max-height="420px"
    />
    <div v-else class="text-result">
      <div class="text-bar">
        <span class="text-label">{{ call.ok ? 'Result' : 'Error result' }}</span>
        <span class="spacer" />
        <UiCopy :text="call.result_text ?? ''" what="Result" />
      </div>
      <pre class="text-body">{{ call.result_text }}</pre>
    </div>
    <p v-if="call.result_truncated" class="caption truncated">
      <Scissors :size="13" :stroke-width="2" aria-hidden="true" />
      The result was over 64 KiB, so it is cut here. The raw log has all of it.
    </p>
  </div>
</template>

<style scoped>
.payload {
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-width: 0;
}

.text-result {
  border: 1px solid var(--stretto-border);
  border-radius: var(--c-radius-control);
  background: var(--stretto-surface-2);
  overflow: hidden;
}

.text-bar {
  display: flex;
  align-items: center;
  min-height: 38px;
  padding: 4px 6px 4px 12px;
  border-bottom: 1px solid var(--stretto-border);
  background: color-mix(in oklab, var(--stretto-surface-2) 55%, var(--stretto-surface));
}

.text-label {
  font-size: 12px;
  font-weight: 500;
  color: var(--stretto-text-muted);
}

.text-body {
  margin: 0;
  padding: 10px 14px 12px;
  max-height: 420px;
  overflow: auto;
  font-family: var(--stretto-font-mono);
  font-size: 12.5px;
  line-height: 1.6;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.truncated,
.no-answer {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
</style>

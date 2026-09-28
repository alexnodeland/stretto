<script setup lang="ts">
/** A JSON value as a tree that opens and closes, with copy and expand/collapse all. */
import { computed, provide, ref } from 'vue'
import { ChevronsUpDown } from '@lucide/vue'
import JsonNode from './JsonNode.vue'
import UiCopy from './UiCopy.vue'
import { prettyJson } from '@/lib/json'

const props = withDefaults(
  defineProps<{
    value: unknown
    depth?: number
    what?: string
    label?: string
    maxHeight?: string
  }>(),
  { depth: 2, what: 'JSON', label: undefined, maxHeight: undefined },
)

const command = ref({ open: true, tick: 0 })
provide('json-command', command)
provide('json-depth', props.depth)

const text = computed(() => prettyJson(props.value))
const branch = computed(() => props.value !== null && typeof props.value === 'object')

/** Expand everything, then collapse everything, and so on. */
const allOpen = computed(() => command.value.tick > 0 && command.value.open)
function expandAll() {
  command.value = { open: !allOpen.value, tick: command.value.tick + 1 }
}
</script>

<template>
  <div class="json">
    <div class="json-bar">
      <span v-if="label" class="json-label">{{ label }}</span>
      <span class="spacer" />
      <button v-if="branch" type="button" class="json-tool" @click="expandAll">
        <ChevronsUpDown :size="13" :stroke-width="2" aria-hidden="true" />
        {{ allOpen ? 'Collapse all' : 'Expand all' }}
      </button>
      <UiCopy :text="text" :what="what" />
    </div>
    <ul class="json-tree" :style="{ maxHeight }">
      <JsonNode :value="value" :depth="0" root />
    </ul>
  </div>
</template>

<style scoped>
.json {
  border: 1px solid var(--stretto-border);
  border-radius: var(--c-radius-control);
  background: var(--stretto-surface-2);
  min-width: 0;
  overflow: hidden;
}

.json-bar {
  display: flex;
  align-items: center;
  gap: 4px;
  min-height: 38px;
  padding: 4px 6px 4px 12px;
  border-bottom: 1px solid var(--stretto-border);
  background: color-mix(in oklab, var(--stretto-surface-2) 55%, var(--stretto-surface));
}

.json-label {
  font-size: 12px;
  font-weight: 500;
  color: var(--stretto-text-muted);
}

.json-tool {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 28px;
  padding: 0 8px;
  border: 1px solid transparent;
  border-radius: 7px;
  background: transparent;
  color: var(--stretto-text-subtle);
  font-size: 12px;
  font-weight: 500;
}

.json-tool:hover {
  background: var(--stretto-surface);
  border-color: var(--stretto-border);
  color: var(--stretto-text);
}

.json-tree {
  margin: 0;
  padding: 10px 14px 12px 30px;
  font-family: var(--stretto-font-mono);
  font-size: 12.5px;
  overflow: auto;
}
</style>

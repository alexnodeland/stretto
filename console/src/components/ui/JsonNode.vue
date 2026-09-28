<script setup lang="ts">
/** One value of a JSON tree; objects and arrays open and close. */
import { computed, inject, ref, watch, type Ref } from 'vue'
import { ChevronRight } from '@lucide/vue'

const props = defineProps<{
  name?: string | number
  value: unknown
  depth: number
  root?: boolean
}>()

const command = inject<Ref<{ open: boolean; tick: number }>>('json-command')
const expandDepth = inject<number>('json-depth', 2)

const isArray = computed(() => Array.isArray(props.value))
const isObject = computed(() => props.value !== null && typeof props.value === 'object')
const entries = computed<[string | number, unknown][]>(() => {
  if (Array.isArray(props.value)) return props.value.map((v, i) => [i, v])
  if (isObject.value) return Object.entries(props.value as Record<string, unknown>)
  return []
})
// Open to the tree's depth, or as the last expand/collapse-all said, for a node mounted after it.
const expanded = ref(
  props.root ||
    (command && command.value.tick > 0 ? command.value.open : props.depth < expandDepth),
)
if (command)
  watch(
    () => command.value.tick,
    () => (expanded.value = command.value.open),
  )

const summary = computed(() => {
  const n = entries.value.length
  if (isArray.value) return n === 1 ? '1 item' : `${n} items`
  return n === 1 ? '1 key' : `${n} keys`
})

const kind = computed(() => {
  const v = props.value
  if (v === null) return 'null'
  return typeof v
})

const text = computed(() => {
  const v = props.value
  if (typeof v === 'string') return JSON.stringify(v)
  return String(v)
})
</script>

<template>
  <li class="jn" :class="{ 'jn-branch': isObject }">
    <template v-if="isObject">
      <button
        type="button"
        class="jn-toggle"
        :aria-expanded="expanded"
        :disabled="entries.length === 0"
        @click="expanded = !expanded"
      >
        <ChevronRight
          class="jn-caret"
          :class="{ open: expanded }"
          :size="13"
          :stroke-width="2.2"
          aria-hidden="true"
        />
        <span
          v-if="name !== undefined"
          class="jn-key"
          :class="{ index: typeof name === 'number' }"
          >{{ name }}</span
        >
        <span v-if="name !== undefined" class="jn-punct">:</span>
        <span class="jn-punct">{{ isArray ? '[' : '{' }}</span>
        <span v-if="!expanded || entries.length === 0" class="jn-summary">{{
          entries.length ? summary : ''
        }}</span>
        <span v-if="!expanded || entries.length === 0" class="jn-punct">{{
          isArray ? ']' : '}'
        }}</span>
      </button>
      <ul v-if="expanded && entries.length" class="jn-children">
        <JsonNode
          v-for="[key, child] in entries"
          :key="key"
          :name="key"
          :value="child"
          :depth="depth + 1"
        />
      </ul>
      <span v-if="expanded && entries.length" class="jn-punct jn-close">{{
        isArray ? ']' : '}'
      }}</span>
    </template>
    <template v-else>
      <span class="jn-leaf">
        <span
          v-if="name !== undefined"
          class="jn-key"
          :class="{ index: typeof name === 'number' }"
          >{{ name }}</span
        >
        <span v-if="name !== undefined" class="jn-punct">: </span>
        <span class="jn-value" :class="`jn-${kind}`">{{ text }}</span>
      </span>
    </template>
  </li>
</template>

<style scoped>
.jn {
  list-style: none;
  line-height: 1.65;
}

.jn-toggle {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  margin-left: -17px;
  padding: 0 4px 0 2px;
  border: 0;
  border-radius: 4px;
  background: none;
  color: inherit;
  font: inherit;
  text-align: left;
}

.jn-toggle:hover:not(:disabled) {
  background: var(--c-hover);
}

.jn-toggle:disabled {
  cursor: default;
  margin-left: 0;
}

.jn-toggle:disabled .jn-caret {
  display: none;
}

.jn-caret {
  color: var(--stretto-text-subtle);
  transition: transform 0.12s var(--c-ease);
  flex: none;
}

.jn-caret.open {
  transform: rotate(90deg);
}

.jn-children {
  margin: 0;
  padding-left: 20px;
  border-left: 1px solid var(--stretto-border);
  margin-left: -11px;
  padding-left: 28px;
}

.jn-close {
  display: block;
}

.jn-leaf {
  display: inline;
  overflow-wrap: anywhere;
}

.jn-key {
  color: var(--stretto-text-muted);
}

.jn-key.index {
  color: var(--stretto-text-subtle);
}

.jn-punct {
  color: var(--stretto-text-subtle);
}

.jn-summary {
  margin: 0 4px;
  color: var(--stretto-text-subtle);
  font-family: var(--stretto-font-sans);
  font-size: 11.5px;
}

.jn-string {
  color: var(--stretto-accent);
  white-space: pre-wrap;
}

.jn-number,
.jn-bigint {
  color: var(--stretto-text);
}

.jn-boolean,
.jn-null,
.jn-undefined {
  color: var(--stretto-text);
  font-weight: 700;
}
</style>

<script setup lang="ts">
/** A block of code or configuration, with its language and a copy button. */
import UiCopy from './UiCopy.vue'

withDefaults(
  defineProps<{
    code: string
    language?: string
    what?: string
    wrap?: boolean
    maxHeight?: string
  }>(),
  {
    language: undefined,
    what: 'Code',
    maxHeight: undefined,
  },
)
</script>

<template>
  <div class="codeblock">
    <div class="codeblock-bar">
      <span v-if="language" class="codeblock-lang">{{ language }}</span>
      <span class="spacer" />
      <UiCopy :text="code" :what="what" label="Copy" />
    </div>
    <pre class="code" :class="{ wrap }" :style="{ maxHeight }"><code>{{ code }}</code></pre>
  </div>
</template>

<style scoped>
.codeblock {
  position: relative;
  border: 1px solid var(--stretto-border);
  border-radius: var(--c-radius-control);
  background: var(--stretto-surface-2);
  overflow: hidden;
  min-width: 0;
}

.codeblock-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px 6px 14px;
  border-bottom: 1px solid var(--stretto-border);
  background: color-mix(in oklab, var(--stretto-surface-2) 60%, var(--stretto-surface));
}

.codeblock-lang {
  font-size: 11.5px;
  font-weight: 500;
  letter-spacing: 0.02em;
  color: var(--stretto-text-subtle);
  text-transform: uppercase;
}

.codeblock .code {
  border: 0;
  border-radius: 0;
  background: transparent;
  overflow: auto;
}
</style>

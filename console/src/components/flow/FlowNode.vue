<script setup lang="ts">
/** A tool in the flow's graph: a call the flow decides after, a lookup it may make, or both. */
import { computed } from 'vue'
import { Handle, Position } from '@vue-flow/core'
import type { GraphNode } from '@/lib/flow'

const props = defineProps<{ data: GraphNode; vertical?: boolean }>()
const role = computed(() => {
  const n = props.data
  if (n.failed) return 'after a failed call'
  const kind = n.kind === 'generic' ? 'neither' : (n.kind ?? 'unknown')
  if (n.acts) return `${kind} · looked up`
  if (n.lookup) return `${kind} · lookup`
  return `${kind} · call`
})
</script>

<template>
  <div
    class="fnode"
    :class="{ acts: data.acts, failed: data.failed, write: data.kind === 'write' }"
    :style="{ width: `${data.width}px`, height: `${data.height}px` }"
    :title="data.id"
  >
    <Handle
      type="target"
      :position="vertical ? Position.Top : Position.Left"
      :connectable="false"
      class="fnode-handle"
    />
    <span class="fnode-name">{{ data.id }}</span>
    <span class="fnode-role">{{ role }}</span>
    <Handle
      type="source"
      :position="vertical ? Position.Bottom : Position.Right"
      :connectable="false"
      class="fnode-handle"
    />
  </div>
</template>

<style scoped>
.fnode {
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 2px;
  padding: 0 14px;
  border-radius: 10px;
  border: 1px solid var(--stretto-border-strong);
  background: var(--stretto-surface);
  box-shadow: var(--c-shadow-sm);
  cursor: default;
}

.fnode.acts {
  border: 1.5px solid var(--stretto-accent-graphic);
  background: color-mix(in oklab, var(--stretto-accent-soft) 70%, var(--stretto-surface));
}

.fnode.failed {
  border-style: dashed;
}

.fnode-name {
  font-family: var(--stretto-font-mono);
  font-size: 12.5px;
  font-weight: 700;
  color: var(--stretto-text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.fnode-role {
  font-size: 11.5px;
  color: var(--stretto-text-subtle);
}

.acts .fnode-role {
  color: var(--stretto-accent);
}

.fnode-handle {
  opacity: 0;
  width: 1px !important;
  height: 1px !important;
  min-width: 0 !important;
  min-height: 0 !important;
  border: 0 !important;
}
</style>

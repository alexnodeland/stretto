<script setup lang="ts">
/** The domain's tools as the flow holds them: only reads are ever called, writes are flagged. */
import { computed } from 'vue'
import { LockOpen, Lock } from '@lucide/vue'
import KindBadge from '../KindBadge.vue'
import type { FlowTool } from '@/api/types'

const props = defineProps<{ tools: FlowTool[] }>()
const order = { read: 0, write: 1, generic: 2 }
/** Reads first, the tools the flow may call; then writes and the rest. */
const sorted = computed(() =>
  [...props.tools].sort((a, b) => order[a.kind] - order[b.kind] || a.name.localeCompare(b.name)),
)
</script>

<template>
  <div class="table-wrap">
    <table class="table table-stack ft">
      <thead>
        <tr>
          <th scope="col">Tool</th>
          <th scope="col">Kind</th>
          <th scope="col">The flow</th>
          <th scope="col">What it does</th>
          <th scope="col">Pinned input</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="t in sorted" :key="t.name" :class="{ write: t.kind === 'write' }">
          <td class="nowrap inline">
            <span class="tool">{{ t.name }}</span>
          </td>
          <td class="inline"><KindBadge :kind="t.kind" /></td>
          <td class="nowrap inline">
            <span v-if="t.kind === 'read'" class="may"
              ><LockOpen :size="13" :stroke-width="2" aria-hidden="true" />may call it</span
            >
            <span v-else class="never"
              ><Lock :size="13" :stroke-width="2" aria-hidden="true" />never calls it</span
            >
          </td>
          <td class="summary">
            {{ t.summary ?? '—' }}
            <details v-if="Object.keys(t.args).length" class="args">
              <summary>
                {{ Object.keys(t.args).length }}
                {{ Object.keys(t.args).length === 1 ? 'argument' : 'arguments' }}
              </summary>
              <ul>
                <li v-for="(doc, name) in t.args" :key="name">
                  <span class="mono">{{ name }}</span> {{ doc }}
                </li>
              </ul>
            </details>
          </td>
          <td
            class="mono contract"
            :class="{ 'stack-hide': !t.contract }"
            data-label="Pinned input"
          >
            {{ t.contract ?? '—' }}
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<style scoped>
.ft td {
  vertical-align: top;
}

.may,
.never {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 13px;
}

.may {
  color: var(--stretto-accent);
}

.never {
  color: var(--stretto-text-muted);
}

.write .never {
  color: var(--stretto-text);
  font-weight: 500;
}

.summary {
  min-width: 280px;
  max-width: 560px;
  color: var(--stretto-text-muted);
  font-size: 13px;
}

.args {
  margin-top: 6px;
  font-size: 12.5px;
}

.args summary {
  width: fit-content;
  cursor: pointer;
  color: var(--stretto-accent);
  font-weight: 500;
  border-radius: 4px;
}

.args ul {
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 3px;
  margin-top: 6px;
}

.args .mono {
  color: var(--stretto-text);
  font-size: 11.5px;
}

.contract {
  font-size: 11.5px;
  color: var(--stretto-text-muted);
  white-space: nowrap;
}
</style>

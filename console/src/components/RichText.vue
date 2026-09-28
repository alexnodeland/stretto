<script setup lang="ts">
/** Text from the server, with its `backticked` words set as code, as stretto's own messages mark them. */
import { computed } from 'vue'

const props = defineProps<{ text: string }>()
const parts = computed(() =>
  props.text
    .split(/(`[^`]+`)/g)
    .filter(Boolean)
    .map((p) =>
      p.startsWith('`') && p.endsWith('`') && p.length > 2
        ? { code: true, text: p.slice(1, -1) }
        : { code: false, text: p },
    ),
)
</script>

<template>
  <span class="rich"
    ><template v-for="(p, i) in parts" :key="i"
      ><code v-if="p.code" class="rich-code">{{ p.text }}</code
      ><template v-else>{{ p.text }}</template></template
    ></span
  >
</template>

<style scoped>
.rich-code {
  padding: 0 4px;
  border-radius: 4px;
  background: var(--stretto-surface-2);
  border: 1px solid var(--stretto-border);
  font-size: 0.88em;
}
</style>

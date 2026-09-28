<script setup lang="ts">
/** A placeholder while a page loads: lines of text, or a block. */
withDefaults(defineProps<{ lines?: number; height?: string; width?: string }>(), {
  lines: 0,
  height: '14px',
  width: '100%',
})
</script>

<template>
  <div v-if="lines > 0" class="skeleton-lines" aria-hidden="true">
    <span
      v-for="i in lines"
      :key="i"
      class="skeleton"
      :style="{ height, width: i === lines && lines > 1 ? '62%' : width }"
    />
  </div>
  <span v-else class="skeleton" :style="{ height, width }" aria-hidden="true" />
</template>

<style scoped>
.skeleton-lines {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
</style>

<script setup lang="ts">
/** A page or panel that could not load: the server's message, and a retry. */
import { CircleAlert, RotateCcw } from '@lucide/vue'
import UiButton from './UiButton.vue'

defineProps<{ title?: string; message: string; status?: number }>()
defineEmits<{ retry: [] }>()
</script>

<template>
  <div class="error" role="alert">
    <div class="error-icon" aria-hidden="true"><CircleAlert :size="20" :stroke-width="1.8" /></div>
    <p class="error-title">{{ title ?? 'This could not be loaded' }}</p>
    <p class="error-message">
      <span v-if="status" class="mono">{{ status }}</span>
      {{ message }}
    </p>
    <UiButton size="sm" :icon="RotateCcw" @click="$emit('retry')">Try again</UiButton>
  </div>
</template>

<style scoped>
.error {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  gap: 6px;
  padding: 40px 24px;
}

.error-icon {
  display: grid;
  place-items: center;
  width: 40px;
  height: 40px;
  margin-bottom: 6px;
  border-radius: 10px;
  background: var(--c-danger-soft);
  color: var(--c-danger);
}

.error-title {
  font-weight: 600;
  font-size: 15px;
}

.error-message {
  max-width: 560px;
  margin-bottom: 12px;
  color: var(--stretto-text-muted);
  font-size: 13.5px;
  overflow-wrap: anywhere;
}

.error-message .mono {
  margin-right: 4px;
  color: var(--c-danger);
}
</style>

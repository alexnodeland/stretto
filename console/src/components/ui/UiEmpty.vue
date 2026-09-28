<script setup lang="ts">
/** An empty state that says what to do next. */
import type { Component } from 'vue'

withDefaults(defineProps<{ icon?: Component; title: string; compact?: boolean }>(), {
  icon: undefined,
})
</script>

<template>
  <div class="empty" :class="{ compact }">
    <div v-if="icon" class="empty-icon" aria-hidden="true">
      <component :is="icon" :size="compact ? 18 : 22" :stroke-width="1.7" />
    </div>
    <p class="empty-title">{{ title }}</p>
    <div v-if="$slots.default" class="empty-body"><slot /></div>
    <div v-if="$slots.actions" class="empty-actions"><slot name="actions" /></div>
  </div>
</template>

<style scoped>
.empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  padding: 48px 24px;
  gap: 6px;
}

.empty.compact {
  padding: 28px 20px;
}

.empty-icon {
  display: grid;
  place-items: center;
  width: 44px;
  height: 44px;
  margin-bottom: 8px;
  border-radius: 12px;
  background: var(--stretto-accent-soft);
  color: var(--stretto-accent);
}

.compact .empty-icon {
  width: 36px;
  height: 36px;
  border-radius: 10px;
}

.empty-title {
  font-size: 15px;
  font-weight: 600;
  letter-spacing: -0.005em;
}

.compact .empty-title {
  font-size: 14px;
}

.empty-body {
  max-width: 460px;
  font-size: 13.5px;
  color: var(--stretto-text-muted);
}

.empty-actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 8px;
  margin-top: 14px;
}
</style>

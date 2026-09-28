<script setup lang="ts">
/** A page's title, what it shows, the way back, and its actions. */
import { ArrowLeft } from '@lucide/vue'
import { RouterLink, type RouteLocationRaw } from 'vue-router'

defineProps<{ title?: string; back?: RouteLocationRaw; backLabel?: string; mono?: boolean }>()
</script>

<template>
  <header class="ph">
    <RouterLink v-if="back" :to="back" class="ph-back">
      <ArrowLeft :size="14" :stroke-width="2" aria-hidden="true" />
      {{ backLabel ?? 'Back' }}
    </RouterLink>
    <div class="ph-row">
      <div class="ph-titles">
        <h1 class="ph-title" :class="{ mono }">
          <slot name="title">{{ title }}</slot>
        </h1>
        <div v-if="$slots.default" class="ph-sub"><slot /></div>
      </div>
      <div v-if="$slots.actions" class="ph-actions"><slot name="actions" /></div>
    </div>
  </header>
</template>

<style scoped>
.ph {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}

.ph-back {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  align-self: flex-start;
  font-size: 13px;
  font-weight: 500;
  color: var(--stretto-text-muted);
  border-radius: 6px;
}

.ph-back:hover {
  color: var(--stretto-text);
}

.ph-row {
  display: flex;
  align-items: flex-end;
  gap: 16px;
  flex-wrap: wrap;
  min-width: 0;
}

.ph-titles {
  flex: 1 1 320px;
  min-width: 0;
}

.ph-title {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 10px;
  font-size: 24px;
  font-weight: 600;
  letter-spacing: -0.022em;
  line-height: 1.25;
  overflow-wrap: anywhere;
}

.ph-title.mono {
  font-family: var(--stretto-font-mono);
  font-size: 20px;
  font-weight: 700;
  letter-spacing: -0.01em;
}

.ph-sub {
  margin-top: 6px;
  font-size: 14px;
  color: var(--stretto-text-muted);
  /* A path, such as the data dir, may be longer than a phone is wide. */
  overflow-wrap: anywhere;
}

.ph-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

@media (max-width: 720px) {
  .ph-title {
    font-size: 21px;
  }

  .ph-title.mono {
    font-size: 17px;
  }
}
</style>

<script setup lang="ts">
/** A card: a title with what it covers, actions, a body, and an optional footer. */
defineProps<{
  title?: string
  caption?: string
  flush?: boolean
  fill?: boolean
  headingLevel?: 2 | 3
}>()
</script>

<template>
  <section class="card" :class="{ 'card-fill': fill }">
    <header v-if="title || $slots.head || $slots.actions" class="card-head">
      <div class="card-titles">
        <slot name="head">
          <component :is="`h${headingLevel ?? 2}`" class="card-title">{{ title }}</component>
          <p v-if="caption" class="caption">{{ caption }}</p>
        </slot>
      </div>
      <div v-if="$slots.actions" class="card-actions"><slot name="actions" /></div>
    </header>
    <div class="card-body" :class="{ flush }"><slot /></div>
    <footer v-if="$slots.foot" class="card-foot"><slot name="foot" /></footer>
  </section>
</template>

<style scoped>
.card-fill {
  display: flex;
  flex-direction: column;
}

.card-fill > .card-body {
  flex: 1 1 auto;
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.card-titles {
  flex: 1 1 auto;
  min-width: 0;
}

.card-actions {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 6px;
  flex: none;
}
</style>

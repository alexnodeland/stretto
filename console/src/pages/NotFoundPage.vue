<script setup lang="ts">
import { Compass, LayoutDashboard } from '@lucide/vue'
import UiEmpty from '@/components/ui/UiEmpty.vue'
import UiButton from '@/components/ui/UiButton.vue'
import { useTitle } from '@/composables/useTitle'

useTitle(() => 'Not found')
</script>

<template>
  <div class="page">
    <h1 class="sr-only">Page not found</h1>
    <div class="card">
      <UiEmpty :icon="Compass" title="Nothing is at this address">
        The page may have moved, or the session, flow or job it named was deleted.
        <template #actions>
          <UiButton variant="primary" :icon="LayoutDashboard" :to="{ name: 'overview' }"
            >Go to the overview</UiButton
          >
        </template>
      </UiEmpty>
    </div>
  </div>
</template>

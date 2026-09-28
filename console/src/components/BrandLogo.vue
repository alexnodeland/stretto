<script setup lang="ts">
/** The brand's logo files, as they come: ink and petrol on light, paper and light petrol on dark. */
import lockup from '@brand/logo/stretto-lockup.svg'
import lockupDark from '@brand/logo/stretto-lockup-dark.svg'
import mark from '@brand/logo/stretto-mark.svg'
import markDark from '@brand/logo/stretto-mark-dark.svg'

withDefaults(defineProps<{ variant?: 'lockup' | 'mark'; height?: number }>(), {
  variant: 'lockup',
  height: 24,
})
</script>

<template>
  <span class="logo" :style="{ height: `${height}px` }">
    <img
      class="logo-light"
      :src="variant === 'mark' ? mark : lockup"
      alt="stretto"
      :height="height"
    />
    <img
      class="logo-dark"
      :src="variant === 'mark' ? markDark : lockupDark"
      alt="stretto"
      :height="height"
    />
  </span>
</template>

<style scoped>
.logo {
  display: inline-flex;
  align-items: center;
}

.logo img {
  height: 100%;
  width: auto;
}

.logo-dark {
  display: none !important;
}

@media (prefers-color-scheme: dark) {
  :root:not([data-theme='light']) .logo-light {
    display: none !important;
  }

  :root:not([data-theme='light']) .logo-dark {
    display: block !important;
  }
}

:root[data-theme='dark'] .logo-light {
  display: none !important;
}

:root[data-theme='dark'] .logo-dark {
  display: block !important;
}
</style>

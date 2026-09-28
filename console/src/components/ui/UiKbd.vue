<template>
  <kbd class="kbd"><slot /></kbd>
</template>

<style scoped>
.kbd {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 20px;
  height: 20px;
  padding: 0 5px;
  border-radius: 5px;
  border: 1px solid var(--stretto-border);
  border-bottom-width: 2px;
  background: var(--stretto-surface);
  color: var(--stretto-text-subtle);
  font-family: var(--stretto-font-sans);
  font-size: 11px;
  font-weight: 500;
  line-height: 1;
}
</style>

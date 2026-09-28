<script setup lang="ts">
/** A modal dialog on the native <dialog>: focus stays inside, Escape closes it. */
import { nextTick, onBeforeUnmount, ref, watch } from 'vue'
import { X } from '@lucide/vue'

const props = withDefaults(
  defineProps<{ open: boolean; title: string; width?: string; description?: string }>(),
  {
    width: '520px',
    description: undefined,
  },
)
const emit = defineEmits<{ 'update:open': [open: boolean] }>()
const dialog = ref<HTMLDialogElement | null>(null)
let returnFocus: HTMLElement | null = null

watch(
  () => props.open,
  async (open) => {
    await nextTick()
    const el = dialog.value
    if (!el) return
    if (open && !el.open) {
      returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null
      if (typeof el.showModal === 'function') el.showModal()
      else el.setAttribute('open', '')
      el.querySelector<HTMLElement>(
        '[autofocus], input, select, textarea, button.btn-primary',
      )?.focus()
    } else if (!open && el.open) {
      if (typeof el.close === 'function') el.close()
      else el.removeAttribute('open')
      returnFocus?.focus()
    }
  },
  { immediate: true },
)

function onClose() {
  if (props.open) emit('update:open', false)
}

function onBackdrop(event: MouseEvent) {
  if (event.target === dialog.value) emit('update:open', false)
}

onBeforeUnmount(() => {
  if (dialog.value?.open) dialog.value.close()
})
</script>

<template>
  <dialog
    ref="dialog"
    class="dialog"
    :style="{ '--dialog-w': width }"
    :aria-label="title"
    @close="onClose"
    @cancel.prevent="emit('update:open', false)"
    @mousedown="onBackdrop"
  >
    <div v-if="open" class="dialog-inner">
      <header class="dialog-head">
        <div class="dialog-titles">
          <h2 class="dialog-title">{{ title }}</h2>
          <p v-if="description" class="dialog-desc">{{ description }}</p>
        </div>
        <button
          type="button"
          class="dialog-x"
          aria-label="Close"
          @click="emit('update:open', false)"
        >
          <X :size="18" :stroke-width="1.9" aria-hidden="true" />
        </button>
      </header>
      <div class="dialog-body"><slot /></div>
      <footer v-if="$slots.actions" class="dialog-actions"><slot name="actions" /></footer>
    </div>
  </dialog>
</template>

<style scoped>
.dialog {
  width: min(var(--dialog-w), calc(100vw - 32px));
  max-height: calc(100vh - 64px);
  padding: 0;
  border: 1px solid var(--stretto-border);
  border-radius: 14px;
  background: var(--stretto-surface);
  color: var(--stretto-text);
  box-shadow: var(--c-shadow-lg);
}

.dialog[open] {
  animation: pop 0.16s var(--c-ease);
}

.dialog::backdrop {
  background: var(--c-backdrop);
  backdrop-filter: blur(2px);
}

.dialog-inner {
  display: flex;
  flex-direction: column;
  max-height: calc(100vh - 64px);
}

.dialog-head {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 18px 20px 12px;
}

.dialog-titles {
  flex: 1;
  min-width: 0;
}

.dialog-title {
  font-size: 16px;
  font-weight: 600;
}

.dialog-desc {
  margin-top: 4px;
  font-size: 13.5px;
  color: var(--stretto-text-muted);
}

.dialog-x {
  display: grid;
  place-items: center;
  width: 32px;
  height: 32px;
  margin: -4px -6px 0 0;
  border: 0;
  border-radius: 8px;
  background: none;
  color: var(--stretto-text-subtle);
}

.dialog-x:hover {
  background: var(--c-hover);
  color: var(--stretto-text);
}

.dialog-body {
  padding: 4px 20px 16px;
  overflow: auto;
  font-size: 14px;
}

.dialog-actions {
  display: flex;
  justify-content: flex-end;
  flex-wrap: wrap;
  gap: 8px;
  padding: 14px 20px;
  border-top: 1px solid var(--stretto-border);
}

@keyframes pop {
  from {
    opacity: 0;
    transform: translateY(6px) scale(0.985);
  }
}
</style>

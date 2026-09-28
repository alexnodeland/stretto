<script setup lang="ts">
/** Where toasts appear, announced politely to screen readers (errors assertively). */
import { CircleAlert, CircleCheck, Info, X } from '@lucide/vue'
import { dismissToast, toasts } from '@/stores/toasts'

const icons = { info: Info, success: CircleCheck, error: CircleAlert }
</script>

<template>
  <div class="toasts" aria-live="polite" aria-relevant="additions">
    <TransitionGroup name="toast">
      <div
        v-for="t in toasts"
        :key="t.id"
        class="toast"
        :class="`toast-${t.kind}`"
        :role="t.kind === 'error' ? 'alert' : 'status'"
      >
        <component
          :is="icons[t.kind]"
          class="toast-icon"
          :size="17"
          :stroke-width="2"
          aria-hidden="true"
        />
        <div class="toast-text">
          <p class="toast-title">{{ t.title }}</p>
          <p v-if="t.message" class="toast-msg">{{ t.message }}</p>
        </div>
        <button type="button" class="toast-x" aria-label="Dismiss" @click="dismissToast(t.id)">
          <X :size="15" :stroke-width="2" aria-hidden="true" />
        </button>
      </div>
    </TransitionGroup>
  </div>
</template>

<style scoped>
.toasts {
  position: fixed;
  right: 20px;
  bottom: 20px;
  z-index: 100;
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 8px;
  width: min(400px, calc(100vw - 32px));
  pointer-events: none;
}

.toast {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  width: 100%;
  padding: 12px 10px 12px 14px;
  border-radius: 12px;
  background: var(--stretto-surface);
  border: 1px solid var(--stretto-border);
  box-shadow: var(--c-shadow-lg);
  pointer-events: auto;
}

.toast-icon {
  flex: none;
  margin-top: 1px;
}

.toast-success .toast-icon,
.toast-info .toast-icon {
  color: var(--stretto-accent-graphic);
}

.toast-error {
  border-color: var(--c-danger-border);
}

.toast-error .toast-icon {
  color: var(--c-danger);
}

.toast-text {
  flex: 1;
  min-width: 0;
}

.toast-title {
  font-size: 13.5px;
  font-weight: 600;
}

.toast-msg {
  margin-top: 2px;
  font-size: 13px;
  color: var(--stretto-text-muted);
  overflow-wrap: anywhere;
}

.toast-x {
  display: grid;
  place-items: center;
  flex: none;
  width: 26px;
  height: 26px;
  margin: -3px 0 0;
  border: 0;
  border-radius: 7px;
  background: none;
  color: var(--stretto-text-subtle);
}

.toast-x:hover {
  background: var(--c-hover);
  color: var(--stretto-text);
}

.toast-enter-active,
.toast-leave-active {
  transition:
    opacity 0.18s var(--c-ease),
    transform 0.18s var(--c-ease);
}

.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateY(8px);
}

@media (max-width: 720px) {
  .toasts {
    right: 16px;
    left: 16px;
    bottom: 16px;
    width: auto;
  }
}
</style>

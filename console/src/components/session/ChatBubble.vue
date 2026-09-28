<script setup lang="ts">
/** A line of the conversation, as the host wrote it to the context file: the customer's on the left, the agent's on the right. */
import { Bot, UserRound } from '@lucide/vue'
import type { ContextMessage } from '@/api/types'
import { formatOffset } from '@/lib/format'

defineProps<{ message: ContextMessage }>()
</script>

<template>
  <div class="bubble-row" :class="message.role">
    <div class="bubble-meta">
      <component
        :is="message.role === 'user' ? UserRound : Bot"
        :size="13"
        :stroke-width="2"
        aria-hidden="true"
      />
      <span>{{ message.role === 'user' ? 'Customer' : 'Agent' }}</span>
      <span class="bubble-t num">{{ formatOffset(message.t_ms) }}</span>
    </div>
    <p class="bubble">{{ message.content }}</p>
  </div>
</template>

<style scoped>
.bubble-row {
  display: flex;
  flex-direction: column;
  gap: 5px;
  max-width: min(560px, 88%);
}

.bubble-row.assistant {
  align-self: flex-end;
  align-items: flex-end;
}

.bubble-meta {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  font-weight: 500;
  color: var(--stretto-text-muted);
}

.bubble-t {
  font-weight: 400;
  color: var(--stretto-text-subtle);
}

.bubble {
  padding: 10px 14px;
  border-radius: 14px;
  font-size: 14px;
  line-height: 1.5;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.user .bubble {
  background: var(--stretto-surface);
  border: 1px solid var(--stretto-border);
  border-top-left-radius: 4px;
}

.assistant .bubble {
  background: var(--stretto-surface-2);
  border: 1px solid var(--stretto-border);
  border-top-right-radius: 4px;
}
</style>

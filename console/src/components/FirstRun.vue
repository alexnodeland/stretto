<script setup lang="ts">
/** The first steps, until they are done: add a server, use the agent through it, learn a flow. */
import { computed } from 'vue'
import { Check, Plus, Sparkles } from '@lucide/vue'
import UiButton from './ui/UiButton.vue'
import type { Totals } from '@/api/types'

const props = defineProps<{ totals: Totals; readOnly: boolean }>()

const steps = computed(() => [
  {
    title: 'Add a server',
    body: 'Register the MCP server your agent uses. The console prints the configuration for your host, with stretto-proxy in front.',
    done: props.totals.servers > 0,
  },
  {
    title: 'Use your agent through it',
    body: 'Work as usual. The proxy records each session, and they appear here as they are written.',
    done: props.totals.sessions > 0,
  },
  {
    title: 'Learn a flow',
    body: 'From the recorded sessions, with no key: which reads follow which calls, and where their arguments come from.',
    done: props.totals.flows > 0,
  },
])
const next = computed(() => steps.value.findIndex((s) => !s.done))
</script>

<template>
  <section class="fr card" aria-labelledby="first-run-title">
    <div class="fr-head">
      <span class="fr-icon" aria-hidden="true"><Sparkles :size="18" :stroke-width="1.8" /></span>
      <div>
        <h2 id="first-run-title" class="fr-title">Get started</h2>
        <p class="caption">Three steps from a server to a flow that reads ahead of your agent.</p>
      </div>
    </div>
    <ol class="fr-steps">
      <li
        v-for="(step, i) in steps"
        :key="step.title"
        class="fr-step"
        :class="{ done: step.done, current: i === next }"
      >
        <span class="fr-num" aria-hidden="true">
          <Check v-if="step.done" :size="14" :stroke-width="2.6" />
          <template v-else>{{ i + 1 }}</template>
        </span>
        <div class="fr-text">
          <p class="fr-step-title">
            {{ step.title }}
            <span v-if="step.done" class="sr-only">(done)</span>
          </p>
          <p class="fr-body">{{ step.body }}</p>
          <div v-if="i === next" class="fr-action">
            <UiButton
              v-if="i === 0"
              variant="primary"
              size="sm"
              :icon="Plus"
              :to="{ name: 'server-new' }"
              :disabled="readOnly"
              reason="The console is read-only"
            >
              Add a server
            </UiButton>
            <UiButton v-else-if="i === 1" size="sm" :to="{ name: 'servers' }"
              >Show the host configuration</UiButton
            >
            <UiButton
              v-else
              variant="primary"
              size="sm"
              :to="{ name: 'job-new', query: { kind: 'learn' } }"
              :disabled="readOnly"
              reason="The console is read-only"
            >
              Learn a flow
            </UiButton>
          </div>
        </div>
      </li>
    </ol>
  </section>
</template>

<style scoped>
.fr {
  padding: 20px;
  background:
    radial-gradient(
      120% 140% at 100% 0%,
      color-mix(in oklab, var(--stretto-accent-soft) 80%, transparent) 0%,
      transparent 55%
    ),
    var(--stretto-surface);
}

.fr-head {
  display: flex;
  gap: 12px;
  align-items: flex-start;
  margin-bottom: 18px;
}

.fr-icon {
  display: grid;
  place-items: center;
  width: 36px;
  height: 36px;
  border-radius: 10px;
  background: var(--stretto-accent-soft);
  color: var(--stretto-accent);
  flex: none;
}

.fr-title {
  font-size: 16px;
}

.fr-steps {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 12px;
  list-style: none;
}

.fr-step {
  display: flex;
  gap: 12px;
  padding: 14px;
  border-radius: 10px;
  border: 1px solid var(--stretto-border);
  background: var(--stretto-surface);
}

.fr-step.current {
  border-color: color-mix(in oklab, var(--stretto-accent-graphic) 55%, var(--stretto-border));
  box-shadow: 0 0 0 3px color-mix(in oklab, var(--stretto-accent-graphic) 12%, transparent);
}

.fr-num {
  display: grid;
  place-items: center;
  flex: none;
  width: 24px;
  height: 24px;
  border-radius: 50%;
  border: 1px solid var(--stretto-border-strong);
  font-size: 12px;
  font-weight: 600;
  color: var(--stretto-text-muted);
}

.done .fr-num {
  border-color: var(--stretto-accent-fill);
  background: var(--stretto-accent-fill);
  color: var(--stretto-on-accent);
}

.current .fr-num {
  border-color: var(--stretto-accent-graphic);
  color: var(--stretto-accent);
}

.fr-text {
  min-width: 0;
}

.fr-step-title {
  font-weight: 600;
  font-size: 14px;
}

.done .fr-step-title {
  color: var(--stretto-text-muted);
}

.fr-body {
  margin-top: 4px;
  font-size: 13px;
  color: var(--stretto-text-muted);
}

.fr-action {
  margin-top: 12px;
}

@media (max-width: 900px) {
  .fr-steps {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>

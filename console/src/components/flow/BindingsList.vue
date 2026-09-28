<script setup lang="ts">
/** Where each lookup's arguments come from: argument ← tool $.path (values found there). */
import { ArrowLeft, Ban } from '@lucide/vue'
import type { BindingView } from '@/api/types'
import { formatPercent, formatProb, plural } from '@/lib/format'
import { compactJson } from '@/lib/json'

defineProps<{ bindings: BindingView[] }>()
const bindable = (b: BindingView) =>
  b.args.every((a) => !a.required || a.sources.length > 0 || a.constant !== null)
const unsourced = (b: BindingView) =>
  b.args.filter((a) => a.required && !a.sources.length && a.constant === null).map((a) => a.name)
</script>

<template>
  <div class="bl">
    <article
      v-for="b in bindings"
      :key="b.tool"
      class="binding"
      :class="{ unbindable: !bindable(b) }"
      :data-testid="`binding-${b.tool}`"
    >
      <header class="b-head">
        <span class="tool b-tool">{{ b.tool }}</span>
        <span class="caption">{{ plural(b.calls, 'call') }} in training</span>
        <span class="spacer" />
        <span
          v-if="b.chance && bindable(b)"
          class="b-chance caption num"
          title="How often binding the arguments this way gave the agent's own, in training"
        >
          chance it is the agent’s: <strong>{{ formatProb(b.chance[0]) }}</strong> not mentioned ·
          <strong>{{ formatProb(b.chance[1]) }}</strong> mentioned
        </span>
      </header>
      <ul
        v-if="b.args.some((a) => a.sources.length || a.constant !== null || !a.required)"
        class="b-args"
      >
        <template v-for="a in b.args" :key="a.name">
          <li v-if="a.sources.length || a.constant !== null || !a.required" class="b-arg">
            <span class="b-name mono">{{ a.name }}</span>
            <span v-if="!a.required" class="caption">optional, not passed</span>
            <template v-else-if="a.constant !== null">
              <ArrowLeft class="b-arrow" :size="14" :stroke-width="2" aria-hidden="true" />
              <span class="mono b-const">{{ compactJson(a.constant, 40) }}</span>
              <span class="caption">a constant the agent always passed</span>
            </template>
            <template v-else>
              <span v-for="(s, i) in a.sources" :key="`${s.tool}${s.path}`" class="b-src">
                <span v-if="i > 0" class="caption">or</span>
                <ArrowLeft v-else class="b-arrow" :size="14" :stroke-width="2" aria-hidden="true" />
                <span class="tool">{{ s.tool }}</span>
                <code class="b-path">{{ s.path }}</code>
                <span class="caption tnum">({{ s.count }} · {{ formatPercent(s.share) }})</span>
              </span>
            </template>
          </li>
        </template>
      </ul>
      <p v-if="unsourced(b).length" class="b-none caption">
        <Ban :size="13" :stroke-width="2" aria-hidden="true" />
        <span>
          <span v-for="a in unsourced(b)" :key="a" class="b-name mono">{{ a }}</span>
          {{ unsourced(b).length === 1 ? 'has' : 'have' }} no source: only the customer knows
          {{ unsourced(b).length === 1 ? 'it' : 'them' }}, so the flow never makes this lookup
          itself.
        </span>
      </p>
    </article>
  </div>
</template>

<style scoped>
.bl {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.binding {
  padding: 12px 14px;
  border-radius: 10px;
  border: 1px solid var(--stretto-border);
  background: var(--stretto-surface);
}

.binding.unbindable {
  background: transparent;
}

.b-head {
  display: flex;
  align-items: baseline;
  flex-wrap: wrap;
  gap: 4px 10px;
}

.b-tool {
  font-weight: 700;
}

.b-chance strong {
  color: var(--stretto-text);
  font-weight: 600;
}

.b-args {
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 8px;
}

.b-arg {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 4px 8px;
  font-size: 13px;
}

.b-name {
  padding: 1px 7px;
  border-radius: 6px;
  background: var(--stretto-surface-2);
  border: 1px solid var(--stretto-border);
  font-size: 12px;
}

.b-arrow {
  color: var(--stretto-accent-graphic);
}

.b-src {
  display: inline-flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 6px;
}

.b-path {
  padding: 1px 6px;
  border-radius: 5px;
  background: var(--stretto-accent-soft);
  color: var(--stretto-accent);
  font-size: 12px;
}

.b-const {
  font-size: 12px;
}

.b-none {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  margin-top: 8px;
}

.b-none > svg {
  flex: none;
  margin-top: 3px;
}

.b-none .b-name {
  margin-right: 4px;
}
</style>

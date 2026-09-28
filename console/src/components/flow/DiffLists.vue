<script setup lang="ts">
/** A flow-diff's changes: those a reviewer must look at first, then every change under its heading. */
import { CircleCheck, TriangleAlert } from '@lucide/vue'
import RichText from '../RichText.vue'
import type { FlowDiffView } from '@/api/types'

defineProps<{ diff: FlowDiffView }>()
</script>

<template>
  <div class="cmp-lists">
    <section>
      <h3 class="cmp-h">
        <TriangleAlert
          v-if="diff.review.length"
          class="text-warn"
          :size="16"
          :stroke-width="2"
          aria-hidden="true"
        />
        <CircleCheck v-else class="text-accent" :size="16" :stroke-width="2" aria-hidden="true" />
        {{ diff.review.length ? `Needs review: ${diff.review.length}` : 'Nothing needs review' }}
      </h3>
      <p v-if="!diff.review.length" class="caption">
        The flow calls no tool, makes no lookup, binds no argument and asks no model or question it
        did not before.
      </p>
      <ul v-else class="cmp-list review" data-testid="diff-review">
        <li v-for="(r, i) in diff.review" :key="i"><RichText :text="r" /></li>
      </ul>
    </section>
    <section>
      <h3 class="cmp-h">Every change: {{ diff.changes.length }}</h3>
      <template v-if="diff.sections.length">
        <div v-for="sec in diff.sections" :key="sec.title" class="cmp-sec">
          <h4 class="cmp-sub">{{ sec.title }}</h4>
          <ul class="cmp-list">
            <li v-for="(c, i) in sec.changes" :key="i"><RichText :text="c" /></li>
          </ul>
        </div>
      </template>
      <ul v-else-if="diff.changes.length" class="cmp-list">
        <li v-for="(c, i) in diff.changes" :key="i"><RichText :text="c" /></li>
      </ul>
      <p v-else class="caption">None.</p>
    </section>
  </div>
</template>

<style scoped>
.cmp-lists {
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.cmp-h {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 14px;
  margin-bottom: 8px;
}

.cmp-sec + .cmp-sec {
  margin-top: 12px;
}

.cmp-sub {
  margin-bottom: 6px;
  font-size: 12.5px;
  font-weight: 600;
  color: var(--stretto-text-muted);
}

.cmp-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding-left: 20px;
  font-size: 13.5px;
}

.cmp-list.review li::marker {
  color: var(--c-warn);
}
</style>

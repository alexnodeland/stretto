<script setup lang="ts">
/** After each call: the lookups the flow may make, what the agent did next in training, and what the flow does at this threshold. */
import { computed } from 'vue'
import ProbBar from '../ui/ProbBar.vue'
import UiBadge from '../ui/UiBadge.vue'
import type { SitePreview } from '@/lib/flow'
import { formatPercent, plural } from '@/lib/format'

const props = defineProps<{ previews: SitePreview[]; promoted: boolean }>()
const rows = computed(() => props.previews)
</script>

<template>
  <div class="table-wrap">
    <table class="table stack sites">
      <thead>
        <tr>
          <th scope="col">After</th>
          <th scope="col">What the agent did next in training</th>
          <th scope="col">At this threshold</th>
          <th v-if="promoted" scope="col">Promotion record</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="p in rows" :key="p.site.name" :data-testid="`site-${p.site.name}`">
          <td class="after">
            <span class="tool">{{ p.site.tool }}</span>
            <UiBadge v-if="p.site.failed" tone="warn">after a failure</UiBadge>
            <p class="caption">
              {{ plural(p.site.steps, 'step') }} in training · weighed by
              {{ p.site.weighed_by === 'reach' ? 'reach' : 'the habit' }}
            </p>
          </td>
          <td class="next" data-label="What the agent did next in training">
            <ul class="shares">
              <li
                v-for="l in p.site.lookups"
                :key="l.tool"
                :class="{ chosen: p.choice?.tool === l.tool }"
              >
                <span class="bar" aria-hidden="true"
                  ><span
                    :class="{ acts: p.acts && p.choice?.tool === l.tool }"
                    :style="{ width: `${l.share * 100}%` }"
                /></span>
                <span class="num share">{{ formatPercent(l.share) }}</span>
                <span class="tool">{{ l.tool }}</span>
                <span class="caption num">({{ l.count }})</span>
              </li>
              <li v-if="p.site.hand_back_share > 0" class="back">
                <span class="bar" aria-hidden="true"
                  ><span class="hb" :style="{ width: `${p.site.hand_back_share * 100}%` }"
                /></span>
                <span class="num share">{{ formatPercent(p.site.hand_back_share) }}</span>
                <span class="subtle">handed back to the model</span>
              </li>
            </ul>
          </td>
          <td class="verdict" data-label="At this threshold">
            <ProbBar
              v-if="p.choice"
              :value="p.prob"
              :threshold="p.threshold"
              :acts="p.acts"
              :width="88"
            />
            <p class="verdict-text" :class="{ acts: p.acts }">
              <template v-if="p.acts"
                >looks up <span class="tool">{{ p.choice?.tool }}</span></template
              >
              <template v-else>hands back: {{ p.reason }}</template>
            </p>
            <p v-if="p.choice?.binding_chance !== null && p.choice" class="caption tnum">
              {{ p.choice.weighed_share.toFixed(2) }} ×
              {{ (p.choice.binding_chance ?? 0).toFixed(2) }} binding
            </p>
            <p v-if="p.site.threshold !== null" class="caption">
              the site’s own threshold: {{ p.site.threshold }}
            </p>
          </td>
          <td v-if="promoted" class="promo">
            <template v-if="p.site.promoted">
              <UiBadge :tone="p.site.promoted.promoted ? 'accent' : 'neutral'">{{
                p.site.promoted.promoted ? 'promoted' : 'not promoted'
              }}</UiBadge>
              <p class="caption tnum">
                {{ p.site.promoted.used }} of {{ p.site.promoted.lookups }} used · lower
                {{ p.site.promoted.lower.toFixed(2) }} · {{ p.site.promoted.tasks }} tasks
              </p>
            </template>
            <span v-else class="subtle">not scored</span>
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<style scoped>
.sites td {
  vertical-align: top;
  padding-top: 14px;
  padding-bottom: 14px;
}

.after {
  min-width: 190px;
}

.after :deep(.badge) {
  margin-left: 8px;
}

.after .caption {
  margin-top: 3px;
}

.shares {
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 300px;
}

.shares li {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
}

.bar {
  position: relative;
  flex: none;
  width: 64px;
  height: 6px;
  border-radius: 3px;
  background: var(--stretto-surface-2);
  box-shadow: inset 0 0 0 1px var(--stretto-border);
}

.bar > span {
  position: absolute;
  inset: 0 auto 0 0;
  border-radius: 3px;
  background: var(--stretto-chart-baseline);
}

.bar > span.acts {
  background: var(--stretto-chart-accent);
}

.bar > span.hb {
  background: var(--stretto-border-strong);
  opacity: 0.55;
}

.share {
  width: 36px;
  text-align: right;
  color: var(--stretto-text-muted);
}

.verdict {
  min-width: 250px;
}

.verdict-text {
  margin-top: 6px;
  font-size: 13px;
  color: var(--stretto-text-muted);
}

.verdict-text.acts {
  color: var(--stretto-accent);
  font-weight: 500;
}

.promo {
  min-width: 180px;
}

.promo .caption {
  margin-top: 4px;
}

@media (max-width: 640px) {
  .sites tr {
    gap: 12px;
    padding-top: 14px;
    padding-bottom: 14px;
  }

  .sites td {
    padding: 0;
  }

  .shares,
  .after,
  .verdict,
  .promo {
    min-width: 0;
  }
}
</style>

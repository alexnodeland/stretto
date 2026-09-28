<script setup lang="ts">
/**
 * How the staged flow did against the committed one, site by site, on the last
 * sessions both were scored on: the lookups each would have made, how many the
 * agent made later (with a 90% interval on the share), and the detours.
 */
import { computed } from 'vue'
import type { ComparisonView, Counts, SiteCounts } from '@/api/types'
import { formatPercent, plural } from '@/lib/format'

const props = defineProps<{ last: ComparisonView }>()

const rows = computed<(SiteCounts & { total?: boolean })[]>(() => [
  ...props.last.sites,
  { ...props.last.total, site: 'Every site', total: true },
])
const served = computed(() => [props.last.total.committed.served, props.last.total.staged.served])

/** The share's interval, as the report writes it. */
function interval(c: Counts): string {
  const s = c.used_share
  return s ? `${formatPercent(s.share)}, ${formatPercent(s.lower)}–${formatPercent(s.upper)}` : ''
}

/** A signed difference, staged minus committed; empty when there is none. */
function delta(a: number, b: number): string {
  const d = b - a
  return d === 0 ? '' : d > 0 ? `+${d}` : `−${-d}`
}
</script>

<template>
  <div class="table-wrap">
    <table class="table table-stack stage-cmp" data-testid="stage-comparison">
      <thead>
        <tr>
          <th scope="col">After</th>
          <th v-if="last.committed" scope="col">Committed flow</th>
          <th scope="col">Staged flow</th>
        </tr>
      </thead>
      <tbody>
        <tr
          v-for="r in rows"
          :key="r.site"
          :class="{ total: r.total }"
          :data-testid="r.total ? 'stage-total' : `stage-site-${r.site}`"
        >
          <th scope="row" :class="{ tool: !r.total }">{{ r.site }}</th>
          <td v-if="last.committed" data-label="Committed flow">
            <p class="counts num">
              <span
                ><strong>{{ r.committed.lookups }}</strong>
                {{ r.committed.lookups === 1 ? 'lookup' : 'lookups' }}</span
              >
              <span
                ><strong>{{ r.committed.used }}</strong> used<span
                  v-if="r.committed.used_share"
                  class="caption"
                >
                  ({{ interval(r.committed) }})</span
                ></span
              >
              <span
                ><strong>{{ r.committed.detours }}</strong>
                {{ r.committed.detours === 1 ? 'detour' : 'detours' }}</span
              >
            </p>
            <span v-if="r.committed.used_share" class="share" aria-hidden="true">
              <span
                class="range"
                :style="{
                  left: `${r.committed.used_share.lower * 100}%`,
                  width: `${(r.committed.used_share.upper - r.committed.used_share.lower) * 100}%`,
                }"
              />
              <span class="point" :style="{ left: `${r.committed.used_share.share * 100}%` }" />
            </span>
          </td>
          <td data-label="Staged flow">
            <p class="counts num">
              <span
                ><strong>{{ r.staged.lookups }}</strong>
                {{ r.staged.lookups === 1 ? 'lookup' : 'lookups' }}</span
              >
              <span
                ><strong>{{ r.staged.used }}</strong> used<span
                  v-if="r.staged.used_share"
                  class="caption"
                >
                  ({{ interval(r.staged) }})</span
                ></span
              >
              <span
                ><strong>{{ r.staged.detours }}</strong>
                {{ r.staged.detours === 1 ? 'detour' : 'detours' }}</span
              >
            </p>
            <span v-if="r.staged.used_share" class="share" aria-hidden="true">
              <span
                class="range"
                :style="{
                  left: `${r.staged.used_share.lower * 100}%`,
                  width: `${(r.staged.used_share.upper - r.staged.used_share.lower) * 100}%`,
                }"
              />
              <span class="point" :style="{ left: `${r.staged.used_share.share * 100}%` }" />
            </span>
            <p v-if="last.committed && r.total" class="caption delta" data-testid="stage-delta">
              <template v-if="delta(r.committed.used, r.staged.used)"
                >{{ delta(r.committed.used, r.staged.used) }} used</template
              >
              <template
                v-if="
                  delta(r.committed.used, r.staged.used) &&
                  delta(r.committed.detours, r.staged.detours)
                "
                >,
              </template>
              <template v-if="delta(r.committed.detours, r.staged.detours)"
                >{{ delta(r.committed.detours, r.staged.detours) }}
                {{
                  Math.abs(r.staged.detours - r.committed.detours) === 1 ? 'detour' : 'detours'
                }}</template
              >
              <template
                v-if="
                  !delta(r.committed.used, r.staged.used) &&
                  !delta(r.committed.detours, r.staged.detours)
                "
                >as many used and detours</template
              >
            </p>
          </td>
        </tr>
      </tbody>
    </table>
  </div>
  <ul class="notes caption">
    <li v-if="served[0] || served[1]">
      The proxy had already made {{ served[0] }} of the committed flow’s lookups and
      {{ served[1] }} of the staged flow’s, serving a flow: they count as neither used nor detours,
      since the agent had no reason to make them again.
    </li>
    <li v-if="last.unanswered[0] || last.unanswered[1]">
      Left out, as the System-One model gave no answer:
      {{ plural(last.unanswered[0], 'decision') }} of the committed flow’s and
      {{ last.unanswered[1] }} of the staged flow’s.
    </li>
    <li>
      A lookup counts once after each of the agent’s calls, as
      <span class="mono">stretto promote</span>
      counts it: one the flow would make after several calls counts after each.
    </li>
  </ul>
</template>

<style scoped>
.stage-cmp th[scope='row'] {
  font-weight: 500;
  font-size: 13.5px;
  color: var(--stretto-text);
  vertical-align: middle;
}

.stage-cmp tbody tr:last-child th {
  border-bottom: 0;
}

.stage-cmp .tool {
  font-family: var(--stretto-font-mono);
  font-size: 12.5px;
}

.stage-cmp tr.total th,
.stage-cmp tr.total td {
  border-top: 1px solid var(--stretto-border-strong);
  font-weight: 600;
}

.counts {
  display: flex;
  flex-wrap: wrap;
  gap: 2px 12px;
  font-size: 13px;
}

.counts strong {
  font-weight: 600;
}

.share {
  position: relative;
  display: block;
  width: 140px;
  max-width: 100%;
  height: 6px;
  margin-top: 6px;
  border-radius: 3px;
  background: var(--stretto-surface-2);
}

.share .range {
  position: absolute;
  top: 0;
  bottom: 0;
  border-radius: 3px;
  background: var(--stretto-accent-soft);
}

.share .point {
  position: absolute;
  top: -2px;
  width: 2px;
  height: 10px;
  margin-left: -1px;
  background: var(--stretto-accent-graphic);
}

.delta {
  margin-top: 4px;
}

.notes {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 12px 20px 16px 36px;
  list-style: disc;
}
</style>

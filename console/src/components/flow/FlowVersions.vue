<script setup lang="ts">
/** Every committed version of a flow, the latest first, as `stretto flow-log` lists them. */
import { History, RotateCcw } from '@lucide/vue'
import UiBadge from '../ui/UiBadge.vue'
import UiButton from '../ui/UiButton.vue'
import RichText from '../RichText.vue'
import type { ComparisonView, VersionView } from '@/api/types'
import { formatDateTime, plural } from '@/lib/format'

defineProps<{ versions: VersionView[]; readOnly: boolean }>()
const emit = defineEmits<{ rollback: [version: VersionView] }>()

function what(v: VersionView): string {
  if (v.kind === 'found') return 'found in place'
  if (v.kind === 'rollback') return `rolled back to version ${v.restored ?? '?'}`
  return 'committed'
}

/** The evidence a commit rested on, in a line. */
function evidence(e: ComparisonView): string {
  const s = e.total.staged
  const c = e.total.committed
  const staged = `the staged flow’s lookups: ${s.lookups}, ${s.used} used, ${s.detours} ${s.detours === 1 ? 'detour' : 'detours'}`
  const committed = e.committed
    ? `; the committed flow’s: ${c.lookups}, ${c.used} used, ${c.detours} ${c.detours === 1 ? 'detour' : 'detours'}`
    : ''
  return `On the last ${plural(e.compared, 'session')} before it, ${staged}${committed}.`
}
</script>

<template>
  <ol class="versions" data-testid="versions">
    <li
      v-for="v in versions"
      :key="v.version"
      class="version"
      :class="{ current: v.current }"
      :data-testid="`version-${v.version}`"
    >
      <div class="v-head">
        <span class="v-n num">v{{ v.version }}</span>
        <UiBadge v-if="v.current" tone="accent">committed now</UiBadge>
        <span class="v-what">{{ what(v) }}</span>
        <span class="subtle v-when">{{ formatDateTime(v.unix_ms) }}</span>
        <span class="spacer" />
        <UiButton
          v-if="!v.current"
          size="sm"
          :icon="RotateCcw"
          :disabled="readOnly"
          reason="The console is read-only"
          :data-testid="`rollback-${v.version}`"
          @click="emit('rollback', v)"
          >Roll back to this</UiButton
        >
      </div>
      <blockquote v-if="v.note" class="v-note">{{ v.note }}</blockquote>
      <p v-if="v.evidence" class="caption">{{ evidence(v.evidence) }}</p>
      <details v-if="v.changes.length" class="v-changes">
        <summary>{{ plural(v.changes.length, 'change') }} from the version before</summary>
        <ul>
          <li v-for="(c, i) in v.changes" :key="i"><RichText :text="c" /></li>
        </ul>
      </details>
    </li>
  </ol>
  <p v-if="!versions.length" class="empty-note caption">
    <History :size="15" :stroke-width="2" aria-hidden="true" />
    No version has been committed. The first commit keeps the flow as it is now as a version of its
    own, so it can be rolled back to.
  </p>
</template>

<style scoped>
.versions {
  display: flex;
  flex-direction: column;
  list-style: none;
}

.version {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 12px 20px;
  border-top: 1px solid var(--stretto-border);
}

.version:first-child {
  border-top: 0;
}

.version.current {
  background: var(--stretto-accent-soft);
}

.v-head {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 6px 10px;
}

.v-n {
  font-weight: 600;
}

.v-what {
  font-size: 13.5px;
}

.v-when {
  font-size: 12.5px;
}

.v-note {
  margin: 0;
  padding-left: 10px;
  border-left: 2px solid var(--stretto-border-strong);
  color: var(--stretto-text-muted);
  font-size: 13.5px;
}

.v-changes summary {
  cursor: pointer;
  font-size: 12.5px;
  color: var(--stretto-text-muted);
}

.v-changes ul {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-top: 6px;
  padding-left: 20px;
  font-size: 13px;
}

.empty-note {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 16px 20px;
}
</style>

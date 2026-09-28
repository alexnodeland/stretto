<script setup lang="ts">
/**
 * A flow's staged learning: the next version `stretto stage` learns beside the
 * committed flow as sessions arrive, how the two did on those sessions before
 * either learned from them, what committing it would change, and every
 * committed version, to commit and to roll back.
 */
import { computed, ref } from 'vue'
import {
  BadgeCheck,
  GitBranch,
  GitCommitHorizontal,
  Info,
  RotateCcw,
  TriangleAlert,
} from '@lucide/vue'
import UiButton from '../ui/UiButton.vue'
import UiCard from '../ui/UiCard.vue'
import UiCode from '../ui/UiCode.vue'
import UiDialog from '../ui/UiDialog.vue'
import UiEmpty from '../ui/UiEmpty.vue'
import UiError from '../ui/UiError.vue'
import UiSkeleton from '../ui/UiSkeleton.vue'
import MarkdownView from '../MarkdownView.vue'
import DiffLists from './DiffLists.vue'
import StageComparison from './StageComparison.vue'
import FlowVersions from './FlowVersions.vue'
import { api, ApiError } from '@/api/client'
import type { VersionView } from '@/api/types'
import { useResource } from '@/composables/useResource'
import { readOnly } from '@/stores/auth'
import { toast } from '@/stores/toasts'
import { commandText, formatDateTime, plural } from '@/lib/format'

const props = defineProps<{ flowKey: string }>()

const stage = useResource((o) => api.flowStage(props.flowKey, o), {
  watch: [() => props.flowKey],
  events: ['flows'],
})
const view = stage.data
const staged = computed(() => view.value?.staged ?? null)
const last = computed(() => view.value?.last ?? null)
/** Why a commit would be refused, when it is not simply that nothing is staged. */
const refusal = computed(() => (staged.value ? (view.value?.refused ?? null) : null))
const nothingToCommit = computed(() => !!refusal.value?.includes('nothing to commit'))
const latest = computed(() => view.value?.versions[0]?.version ?? 0)
const stageCommand = computed(() =>
  commandText([
    'stretto',
    'stage',
    '--flow',
    `~/.stretto/${view.value?.committed_path ?? `${props.flowKey}.flow.json`}`,
    '--sessions',
    '<dir>',
  ]),
)

const commitOpen = ref(false)
const rollbackTo = ref<VersionView | null>(null)
const note = ref('')
const busy = ref(false)
const actionError = ref<string | null>(null)

function openCommit() {
  note.value = ''
  actionError.value = null
  commitOpen.value = true
}

function openRollback(v: VersionView) {
  note.value = ''
  actionError.value = null
  rollbackTo.value = v
}

async function commit() {
  busy.value = true
  actionError.value = null
  try {
    const v = await api.commitFlow(
      props.flowKey,
      { note: note.value.trim() || null },
      { quiet: true },
    )
    commitOpen.value = false
    toast({
      kind: 'success',
      title: `Committed as version ${v.version}`,
      message: 'A proxy that starts now reads it: the next session is served the new flow.',
    })
    await stage.refresh()
  } catch (e) {
    actionError.value = e instanceof ApiError ? e.message : String(e)
  } finally {
    busy.value = false
  }
}

async function rollback() {
  const to = rollbackTo.value
  if (!to) return
  busy.value = true
  actionError.value = null
  try {
    const v = await api.rollbackFlow(
      props.flowKey,
      { to: to.version, note: note.value.trim() || null },
      { quiet: true },
    )
    rollbackTo.value = null
    toast({
      kind: 'success',
      title: `Rolled back to version ${to.version}`,
      message: `As version ${v.version}, which can be rolled back too.`,
    })
    await stage.refresh()
  } catch (e) {
    actionError.value = e instanceof ApiError ? e.message : String(e)
  } finally {
    busy.value = false
  }
}

const rollbackOpen = computed({
  get: () => rollbackTo.value !== null,
  set: (open: boolean) => {
    if (!open) rollbackTo.value = null
  },
})
</script>

<template>
  <div class="stack">
    <div v-if="stage.error.value" class="card">
      <UiError
        :message="stage.error.value.message"
        :status="stage.error.value.status"
        @retry="stage.refresh()"
      />
    </div>
    <div v-else-if="!view" class="card pad"><UiSkeleton :lines="6" /></div>

    <template v-else>
      <UiCard v-if="!staged" data-testid="stage-empty">
        <UiEmpty :icon="GitBranch" title="Nothing is staged">
          <code class="mono">stretto stage</code> learns the next version of this flow from the
          sessions as they arrive, beside it as <span class="mono">{{ view.staged_path }}</span
          >, and scores each new session with both flows before it learns from it. Nothing reaches
          the proxy until you commit.
          <template #actions>
            <UiButton
              variant="primary"
              :icon="GitBranch"
              :to="{ name: 'job-new', query: { kind: 'stage', flow: flowKey } }"
              :disabled="readOnly"
              reason="The console is read-only"
              >Stage this flow</UiButton
            >
          </template>
        </UiEmpty>
        <div class="pad cli">
          <UiCode :code="stageCommand" language="sh" what="Command" />
        </div>
      </UiCard>

      <UiCard v-else title="The staged flow" data-testid="stage-status">
        <template #actions>
          <UiButton
            :icon="GitBranch"
            :to="{ name: 'job-new', query: { kind: 'stage', flow: flowKey } }"
            :disabled="readOnly"
            reason="The console is read-only"
            >Stage again</UiButton
          >
          <UiButton
            variant="primary"
            :icon="GitCommitHorizontal"
            :disabled="readOnly || !!refusal"
            :reason="readOnly ? 'The console is read-only' : (refusal ?? undefined)"
            data-testid="stage-commit"
            @click="openCommit"
            >Commit</UiButton
          >
        </template>
        <div class="stack">
          <p>
            <RouterLink :to="{ name: 'flow', params: { key: staged.key } }" class="mono">{{
              view.staged_path
            }}</RouterLink>
            learned from {{ plural(staged.habit_episodes, 'session') }} on
            {{ formatDateTime(staged.compiled_unix_ms)
            }}<template v-if="last">; its last run took in {{ last.new }} new</template>.
            <template v-if="!view.committed">
              There is no committed flow yet: the first commit makes
              <span class="mono">{{ view.committed_path }}</span
              >.
            </template>
          </p>
          <p v-if="last?.carried.length" class="caption">
            It keeps what the committed flow has that it does not learn:
            {{ last.carried.join(', ') }}.
          </p>
          <p v-if="nothingToCommit" class="notice accent" data-testid="stage-same">
            <BadgeCheck :size="16" :stroke-width="2" aria-hidden="true" />
            <span
              >The staged flow is the committed flow: nothing to commit. Stage again as sessions
              arrive.</span
            >
          </p>
          <p v-else-if="refusal" class="notice danger" role="status" data-testid="stage-refused">
            <TriangleAlert :size="16" :stroke-width="2" aria-hidden="true" />
            <span>It cannot be committed: {{ refusal }}</span>
          </p>
          <p v-if="last && !view.evidence" class="notice warn">
            <Info :size="16" :stroke-width="2" aria-hidden="true" />
            <span
              >The last comparison is of an earlier staged flow, so a commit would record no
              evidence. Stage again to compare this one.</span
            >
          </p>
        </div>
      </UiCard>

      <UiCard
        v-if="last"
        title="How the two flows did"
        :caption="
          last.compared
            ? `On the last ${plural(last.compared, 'session')} both were scored on, each as it arrived, before either flow learned from it.`
            : 'No session has been scored by both flows yet: the staged flow is scored on the sessions that arrive after it.'
        "
        flush
      >
        <StageComparison v-if="last.compared" :last="last" />
        <details v-if="view.report_markdown" class="report">
          <summary>The report, as stretto stage writes it</summary>
          <MarkdownView :text="view.report_markdown" />
        </details>
      </UiCard>

      <UiCard
        v-if="view.diff && !nothingToCommit"
        title="What committing it would change"
        caption="As stretto flow-diff lists it, and as the commit records it"
      >
        <DiffLists :diff="view.diff" />
      </UiCard>

      <UiCard
        title="Versions"
        :caption="`Every committed version of ${view.committed_path}, the latest first: stretto flow-log`"
        flush
      >
        <p v-if="view.unrecorded && view.versions.length" class="notice warn versions-note">
          <TriangleAlert :size="16" :stroke-width="2" aria-hidden="true" />
          <span
            >The committed flow is not the latest version: it changed since, by other means than a
            commit. A commit or a rollback keeps it as a version first.</span
          >
        </p>
        <FlowVersions :versions="view.versions" :read-only="readOnly" @rollback="openRollback" />
      </UiCard>
    </template>

    <UiDialog v-model:open="commitOpen" title="Commit the staged flow?">
      <div class="stack dialog-body">
        <p>
          <span class="mono">{{ view?.staged_path }}</span> replaces
          <span class="mono">{{ view?.committed_path }}</span> whole, as version
          {{ latest + (view?.unrecorded && view?.committed ? 2 : 1) }}. A proxy reads its flow when
          it starts, and an MCP host starts it for each session, so the next session is served the
          new flow.
        </p>
        <p v-if="view?.unrecorded && view?.committed" class="caption">
          The committed flow as it is now is kept first, as a version of its own.
        </p>
        <label class="field">
          <span class="label">Why <span class="subtle">(optional)</span></span>
          <textarea
            v-model="note"
            class="input"
            rows="2"
            placeholder="reads the order after the account"
            data-testid="stage-note"
          />
        </label>
        <p v-if="actionError" class="notice danger" role="alert">
          <TriangleAlert :size="16" :stroke-width="2" aria-hidden="true" />
          <span>{{ actionError }}</span>
        </p>
      </div>
      <template #actions>
        <UiButton @click="commitOpen = false">Cancel</UiButton>
        <UiButton
          variant="primary"
          :icon="GitCommitHorizontal"
          :loading="busy"
          data-testid="stage-commit-confirm"
          @click="commit"
          >Commit</UiButton
        >
      </template>
    </UiDialog>

    <UiDialog v-model:open="rollbackOpen" :title="`Roll back to version ${rollbackTo?.version}?`">
      <div class="stack dialog-body">
        <p>
          Version {{ rollbackTo?.version }} becomes
          <span class="mono">{{ view?.committed_path }}</span> again, as a version of its own, so
          the rollback can be rolled back too. The staged flow stays as it is.
        </p>
        <label class="field">
          <span class="label">Why <span class="subtle">(optional)</span></span>
          <textarea
            v-model="note"
            class="input"
            rows="2"
            placeholder="detours after get_order_details"
            data-testid="rollback-note"
          />
        </label>
        <p v-if="actionError" class="notice danger" role="alert">
          <TriangleAlert :size="16" :stroke-width="2" aria-hidden="true" />
          <span>{{ actionError }}</span>
        </p>
      </div>
      <template #actions>
        <UiButton @click="rollbackTo = null">Cancel</UiButton>
        <UiButton
          variant="primary"
          :icon="RotateCcw"
          :loading="busy"
          data-testid="rollback-confirm"
          @click="rollback"
          >Roll back</UiButton
        >
      </template>
    </UiDialog>
  </div>
</template>

<style scoped>
.pad {
  padding: 18px 20px;
}

.cli {
  padding-top: 0;
}

.report {
  padding: 4px 20px 16px;
}

.report summary {
  cursor: pointer;
  padding: 8px 0;
  font-size: 13px;
  color: var(--stretto-text-muted);
}

.versions-note {
  margin: 12px 20px 0;
}

.dialog-body {
  gap: 12px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 5px;
}

textarea.input {
  resize: vertical;
  min-height: 56px;
  padding: 8px 10px;
}
</style>

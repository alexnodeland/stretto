<script setup lang="ts">
/** The stretto CLI, run by the console: each job, newest first. */
import { useRouter } from 'vue-router'
import { FileText, FolderOpen, Plus, SquareTerminal, Workflow } from '@lucide/vue'
import UiPageHeader from '@/components/ui/UiPageHeader.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiEmpty from '@/components/ui/UiEmpty.vue'
import UiError from '@/components/ui/UiError.vue'
import UiSkeleton from '@/components/ui/UiSkeleton.vue'
import UiRelTime from '@/components/ui/UiRelTime.vue'
import UiBadge from '@/components/ui/UiBadge.vue'
import JobStatus from '@/components/JobStatus.vue'
import { api } from '@/api/client'
import type { Job } from '@/api/types'
import { useResource } from '@/composables/useResource'
import { useTitle } from '@/composables/useTitle'
import { readOnly } from '@/stores/auth'
import { formatDuration } from '@/lib/format'

useTitle(() => 'Jobs')
const router = useRouter()
const jobs = useResource((o) => api.jobs(o), { events: ['jobs'] })

function duration(j: Job): string {
  if (!j.started_unix_ms) return '—'
  return formatDuration((j.finished_unix_ms ?? Date.now()) - j.started_unix_ms)
}

function open(j: Job, event: MouseEvent) {
  if ((event.target as HTMLElement).closest('a, button')) return
  void router.push({ name: 'job', params: { id: j.id } })
}

const icons = { flow: Workflow, report: FileText, dir: FolderOpen }
</script>

<template>
  <div class="page">
    <UiPageHeader title="Jobs">
      The stretto CLI, run for you: learn, promote, audit, redact and doctor. Jobs run one at a
      time, in the order they were started.
      <template #actions>
        <UiButton
          variant="primary"
          :icon="Plus"
          :to="{ name: 'job-new' }"
          :disabled="readOnly"
          reason="The console is read-only"
          >New job</UiButton
        >
      </template>
    </UiPageHeader>

    <section class="card" :class="{ refreshing: jobs.refreshing.value }" aria-label="Jobs">
      <UiError
        v-if="jobs.error.value"
        :message="jobs.error.value.message"
        :status="jobs.error.value.status"
        @retry="jobs.refresh()"
      />
      <div v-else-if="!jobs.data.value" class="pad"><UiSkeleton :lines="6" height="18px" /></div>
      <UiEmpty v-else-if="!jobs.data.value.items.length" :icon="SquareTerminal" title="No jobs yet">
        Once a server has recorded some sessions, learn a flow from them here, with no key. Then
        promote it on shadow sessions, and audit it on new ones.
        <template #actions>
          <UiButton
            variant="primary"
            :icon="Plus"
            :to="{ name: 'job-new', query: { kind: 'learn' } }"
            :disabled="readOnly"
            >Learn a flow</UiButton
          >
          <UiButton :to="{ name: 'job-new', query: { kind: 'doctor' } }" :disabled="readOnly"
            >Run stretto doctor</UiButton
          >
        </template>
      </UiEmpty>
      <div v-else class="table-wrap">
        <table class="table table-stack">
          <thead>
            <tr>
              <th scope="col">Job</th>
              <th scope="col">Kind</th>
              <th scope="col">Status</th>
              <th scope="col">What it wrote</th>
              <th scope="col" class="num">Took</th>
              <th scope="col" class="num">Started</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="j in jobs.data.value.items"
              :key="j.id"
              class="clickable"
              :data-testid="`job-${j.id}`"
              @click="open(j, $event)"
            >
              <td class="title">
                <RouterLink :to="{ name: 'job', params: { id: j.id } }">{{ j.title }}</RouterLink>
                <span class="caption mono job-id">{{ j.id }}</span>
              </td>
              <td class="inline">
                <UiBadge mono>{{ j.kind }}</UiBadge>
              </td>
              <td class="inline"><JobStatus :status="j.status" /></td>
              <td class="arts" :class="{ 'stack-hide': !j.artifacts.length }">
                <template v-if="j.artifacts.length">
                  <span v-for="a in j.artifacts" :key="a.path" class="art">
                    <component
                      :is="icons[a.kind]"
                      :size="13"
                      :stroke-width="2"
                      aria-hidden="true"
                    />
                    <RouterLink
                      v-if="a.kind === 'flow' && a.key"
                      :to="{ name: 'flow', params: { key: a.key } }"
                      class="mono"
                      >{{ a.path }}</RouterLink
                    >
                    <span v-else class="mono">{{ a.path }}</span>
                  </span>
                </template>
                <span v-else class="subtle">—</span>
              </td>
              <td class="num inline" data-label="Took">{{ duration(j) }}</td>
              <td class="num subtle inline" data-label="Started">
                <UiRelTime :ms="j.created_unix_ms" />
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </section>
  </div>
</template>

<style scoped>
.pad {
  padding: 20px;
}

.title a {
  color: var(--stretto-text);
  font-weight: 500;
}

.title a:hover {
  color: var(--stretto-accent);
}

.job-id {
  display: block;
  margin-top: 2px;
  font-size: 11px;
  white-space: nowrap;
}

.arts {
  font-size: 12.5px;
}

/* One artifact a line. */
.art {
  display: flex;
  align-items: center;
  gap: 5px;
  color: var(--stretto-text-subtle);
  white-space: nowrap;
}

.art + .art {
  margin-top: 3px;
}

.art .mono {
  font-size: 12px;
}
</style>

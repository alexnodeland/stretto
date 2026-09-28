/** Which jobs are queued or running, for the sidebar's count; kept current by job events. */
import { computed, reactive } from 'vue'
import type { Job } from '@/api/types'

const statuses = reactive(new Map<string, Job['status']>())

export function noteJobs(jobs: Job[]): void {
  statuses.clear()
  for (const job of jobs) statuses.set(job.id, job.status)
}

/** Record a job's status; returns the one it had. */
export function noteJob(job: Job): Job['status'] | undefined {
  const before = statuses.get(job.id)
  statuses.set(job.id, job.status)
  return before
}

export const runningJobs = computed(
  () => [...statuses.values()].filter((s) => s === 'running' || s === 'queued').length,
)

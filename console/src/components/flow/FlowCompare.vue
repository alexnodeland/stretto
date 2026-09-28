<script setup lang="ts">
/** What changed from another flow to this one, as `stretto flow-diff` lists it for a pull request. */
import { computed, ref, watch } from 'vue'
import { ArrowRightLeft, CircleCheck, GitCompareArrows, TriangleAlert } from '@lucide/vue'
import UiSegmented from '../ui/UiSegmented.vue'
import UiSkeleton from '../ui/UiSkeleton.vue'
import UiError from '../ui/UiError.vue'
import UiEmpty from '../ui/UiEmpty.vue'
import MarkdownView from '../MarkdownView.vue'
import RichText from '../RichText.vue'
import { api, ApiError } from '@/api/client'
import type { FlowDiffView, FlowSummary } from '@/api/types'

const props = defineProps<{
  flow: FlowSummary
  others: FlowSummary[]
  threshold: number
  initial?: string | null
}>()
const emit = defineEmits<{ other: [key: string | null] }>()

const other = ref<string | null>(
  props.initial ??
    props.others.find((f) => f.domain === props.flow.domain)?.key ??
    props.others[0]?.key ??
    null,
)
/** Whether this flow is the newer of the two. */
const thisIsNew = ref(true)
const view = ref<'list' | 'markdown'>('list')
const diff = ref<FlowDiffView | null>(null)
const error = ref<ApiError | null>(null)
const loading = ref(false)

async function load() {
  if (!other.value) return
  loading.value = true
  error.value = null
  const [from, to] = thisIsNew.value ? [other.value, props.flow.key] : [props.flow.key, other.value]
  try {
    diff.value = await api.flowDiff(from, to, props.threshold, { quiet: true })
  } catch (e) {
    error.value = e instanceof ApiError ? e : new ApiError(0, String(e))
    diff.value = null
  } finally {
    loading.value = false
  }
}

watch([other, thisIsNew, () => props.threshold], () => void load(), { immediate: true })
watch(other, (k) => emit('other', k))

const names = computed(() => {
  const o = other.value ?? '?'
  return thisIsNew.value ? { from: o, to: props.flow.key } : { from: props.flow.key, to: o }
})
</script>

<template>
  <div class="cmp">
    <div v-if="!others.length" class="card">
      <UiEmpty :icon="GitCompareArrows" title="No other flow to compare with" compact>
        Learn the domain again from newer sessions, or promote this flow, and compare the two here.
      </UiEmpty>
    </div>
    <template v-else>
      <div class="cmp-bar">
        <label class="cmp-pick">
          <span class="label">{{
            thisIsNew ? 'Compare with the older flow' : 'Compare with the newer flow'
          }}</span>
          <select v-model="other" class="select" data-testid="compare-select">
            <option v-for="f in others" :key="f.key" :value="f.key">
              {{ f.key }} · {{ f.domain }}
            </option>
          </select>
        </label>
        <button
          type="button"
          class="cmp-swap"
          :title="'Swap which flow is the newer'"
          @click="thisIsNew = !thisIsNew"
        >
          <ArrowRightLeft :size="15" :stroke-width="2" aria-hidden="true" />
          <span class="mono">{{ names.from }}</span> → <span class="mono">{{ names.to }}</span>
        </button>
        <span class="spacer" />
        <UiSegmented
          v-model="view"
          :options="[
            { value: 'list', label: 'Changes' },
            { value: 'markdown', label: 'As Markdown' },
          ]"
          label="View"
          size="sm"
        />
      </div>

      <div class="card" :class="{ refreshing: loading && diff }">
        <UiError v-if="error" :message="error.message" :status="error.status" @retry="load" />
        <div v-else-if="!diff" class="pad"><UiSkeleton :lines="6" /></div>
        <div v-else-if="view === 'markdown'" class="pad">
          <MarkdownView :text="diff.markdown" />
        </div>
        <div v-else class="pad cmp-lists">
          <section>
            <h3 class="cmp-h">
              <TriangleAlert
                v-if="diff.review.length"
                class="text-warn"
                :size="16"
                :stroke-width="2"
                aria-hidden="true"
              />
              <CircleCheck
                v-else
                class="text-accent"
                :size="16"
                :stroke-width="2"
                aria-hidden="true"
              />
              {{
                diff.review.length ? `Needs review: ${diff.review.length}` : 'Nothing needs review'
              }}
            </h3>
            <p v-if="!diff.review.length" class="caption">
              The flow calls no tool, makes no lookup, binds no argument and asks no model or
              question it did not before.
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
      </div>
    </template>
  </div>
</template>

<style scoped>
.cmp {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.cmp-bar {
  display: flex;
  align-items: flex-end;
  flex-wrap: wrap;
  gap: 10px 12px;
}

.cmp-pick {
  display: flex;
  flex-direction: column;
  gap: 5px;
  min-width: 240px;
}

.cmp-swap {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 36px;
  padding: 0 12px;
  border: 1px solid var(--stretto-border);
  border-radius: 8px;
  background: var(--stretto-surface);
  color: var(--stretto-text-muted);
  font-size: 13px;
}

.cmp-swap .mono {
  color: var(--stretto-text);
  font-size: 12px;
}

.cmp-swap:hover {
  background: var(--stretto-surface-2);
}

.pad {
  padding: 18px 20px;
}

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

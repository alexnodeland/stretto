/**
 * A session, turn by turn: the conversation's messages where they fell, each
 * of the agent's turns with its calls, and under each call the flow's
 * decisions after it, with the lookup each one made ("read ahead by stretto").
 */
import type {
  CallView,
  ContextMessage,
  FlowDecision,
  FlowRun,
  SessionDetail,
  Turn,
} from '@/api/types'

export interface DecisionItem {
  index: number
  decision: FlowDecision
  /** The flow's call for this decision, when it made one (not in shadow). */
  lookup: CallView | null
}

export interface CallItem {
  call: CallView
  decisions: DecisionItem[]
  /** Lookups after this call that no decision names. */
  extraLookups: CallView[]
  run: FlowRun | null
}

export type TimelineEntry =
  | { type: 'message'; t: number; message: ContextMessage }
  | { type: 'turn'; t: number; turn: Turn; calls: CallItem[] }

/** The value a decision is weighed on against the threshold: the lookup's probability times its binding's chance. */
export function decisionValue(d: Pick<FlowDecision, 'prob' | 'binding'>): number | null {
  if (d.prob === null) return null
  return d.prob * (d.binding ?? 1)
}

export function buildTimeline(detail: SessionDetail): TimelineEntry[] {
  const callsById = new Map(detail.calls.map((c) => [c.id, c]))
  const lookupsByDecision = new Map<number, CallView>()
  const lookupsByAfter = new Map<string, CallView[]>()
  for (const call of detail.calls) {
    if (call.by !== 'flow') continue
    if (call.decision !== null) lookupsByDecision.set(call.decision, call)
    else if (call.after !== null) {
      const list = lookupsByAfter.get(call.after) ?? []
      list.push(call)
      lookupsByAfter.set(call.after, list)
    }
  }
  const decisionsByAfter = new Map<string, DecisionItem[]>()
  detail.decisions.forEach((decision, index) => {
    const list = decisionsByAfter.get(decision.after) ?? []
    list.push({ index, decision, lookup: lookupsByDecision.get(index) ?? null })
    decisionsByAfter.set(decision.after, list)
  })
  const runsByAfter = new Map(detail.runs.map((r) => [r.after, r]))

  const item = (call: CallView): CallItem => ({
    call,
    decisions: decisionsByAfter.get(call.id) ?? [],
    extraLookups: lookupsByAfter.get(call.id) ?? [],
    run: runsByAfter.get(call.id) ?? null,
  })

  const entries: TimelineEntry[] = []
  const placed = new Set<string>()
  for (const turn of [...detail.turns].sort((a, b) => a.index - b.index)) {
    const calls = turn.calls
      .map((id) => callsById.get(id))
      .filter((c): c is CallView => !!c)
      .map((c) => {
        placed.add(c.id)
        return item(c)
      })
    entries.push({ type: 'turn', t: turn.start_ms, turn, calls })
  }
  // The agent's calls no turn holds (never answered, say) get a turn of their own.
  let extra = detail.turns.length
  for (const call of detail.calls) {
    if (call.by !== 'agent' || placed.has(call.id)) continue
    const turn: Turn = {
      index: extra++,
      start_ms: call.t_ms,
      end_ms: call.result_t_ms ?? call.t_ms,
      calls: [call.id],
    }
    entries.push({ type: 'turn', t: call.t_ms, turn, calls: [item(call)] })
  }
  for (const message of detail.context) entries.push({ type: 'message', t: message.t_ms, message })
  // In time order; a message logged at the same moment as a call came before it.
  return entries
    .map((entry, order) => ({ entry, order }))
    .sort(
      (a, b) =>
        a.entry.t - b.entry.t ||
        (a.entry.type === b.entry.type ? a.order - b.order : a.entry.type === 'message' ? -1 : 1),
    )
    .map(({ entry }) => entry)
}

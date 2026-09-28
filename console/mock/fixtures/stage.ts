/**
 * The mock's staged learning, for the shop flow: `stretto stage` learned
 * shop.staged.flow.json from the six recorded sessions and the five that
 * came after them, and scored each of those five with both flows before it
 * learned from it. Nothing has been committed yet, so the committed flow has
 * no version: the first commit keeps it, as it is, as version 1.
 */
import type {
  ComparisonView,
  Counts,
  SiteCounts,
  StageLink,
  StageView,
  VersionView,
} from '../../src/api/types.ts'
import type { FlowRecord } from './flows.ts'

/** One deployment's staged learning. */
export interface StageRecord {
  /** The committed flow's key, and the staged flow's. */
  committed: string
  staged: string
  /** Whether the staged flow differs from the committed one. */
  pending: boolean
  /** The last run of stretto stage. */
  last: ComparisonView | null
  /** Every committed version, the oldest first, with the flow it was. */
  versions: { view: VersionView; flow: FlowRecord }[]
}

/** The lower end of Wilson's interval: `promote`'s bound, at 90% two-sided. */
function wilsonLower(k: number, n: number): number {
  const z = 1.6448536
  const p = k / n
  const d = 1 + (z * z) / n
  const centre = p + (z * z) / (2 * n)
  const spread = z * Math.sqrt((p * (1 - p)) / n + (z * z) / (4 * n * n))
  return Math.max(0, (centre - spread) / d)
}

/** A flow's counts, with the share `stretto stage` reports. */
export function counts(decisions: number, lookups: number, used: number, served = 0): Counts {
  const known = lookups - served
  return {
    decisions,
    lookups,
    used,
    served,
    detours: lookups - used - served,
    used_share:
      known > 0
        ? {
            share: used / known,
            lower: wilsonLower(used, known),
            upper: 1 - wilsonLower(known - used, known),
          }
        : null,
  }
}

function add(a: Counts, b: Counts): Counts {
  return counts(
    a.decisions + b.decisions,
    a.lookups + b.lookups,
    a.used + b.used,
    a.served + b.served,
  )
}

/** A comparison over `sites`, with its total. */
export function comparison(
  learned: number,
  sessions: number,
  fresh: number,
  compared: number,
  sites: SiteCounts[],
): ComparisonView {
  const zero = counts(0, 0, 0)
  return {
    learned_unix_ms: learned,
    sessions,
    new: fresh,
    committed: true,
    compared,
    sites,
    total: {
      site: '',
      committed: sites.reduce((t, s) => add(t, s.committed), zero),
      staged: sites.reduce((t, s) => add(t, s.staged), zero),
    },
    unanswered: [0, 0],
    carried: [],
  }
}

/** The shop flow's staged learning, the staged flow learned at `learned`. */
export function shopStage(learned: number): StageRecord {
  return {
    committed: 'shop',
    staged: 'shop.staged',
    pending: true,
    last: comparison(learned, 11, 5, 5, [
      {
        site: 'find_user_id_by_email',
        committed: counts(5, 5, 5),
        staged: counts(5, 5, 5),
      },
      {
        site: 'get_order_details',
        committed: counts(4, 3, 1),
        staged: counts(4, 1, 1),
      },
      {
        site: 'get_user_details',
        committed: counts(5, 4, 3),
        staged: counts(5, 5, 4),
      },
    ]),
    versions: [],
  }
}

/** A deployment's link, as a flow summary carries it. */
export function stageLink(
  stages: StageRecord[],
  key: string,
  exists: (key: string) => boolean,
): StageLink | null {
  for (const s of stages) {
    if (s.staged === key)
      return { staged: true, other: exists(s.committed) ? s.committed : null, pending: s.pending }
    if (s.committed === key && exists(s.staged))
      return { staged: false, other: s.staged, pending: s.pending }
  }
  return null
}

/** `stretto stage`'s report of `last`, as the CLI writes it. */
export function stageReport(path: string, last: ComparisonView): string {
  const cell = (c: Counts) =>
    c.used_share
      ? `${c.used} (${Math.round(100 * c.used_share.share)}%, ${Math.round(100 * c.used_share.lower)}–${Math.round(100 * c.used_share.upper)}%)`
      : '—'
  const row = (s: SiteCounts) =>
    `| ${s.site ? `\`${s.site}\`` : '**Total**'} | ${s.committed.lookups} | ${cell(s.committed)} | ${s.committed.detours} | ${s.staged.lookups} | ${cell(s.staged)} | ${s.staged.detours} |`
  return [
    `# Staged flow: ${path}`,
    '',
    `The staged flow learned from ${last.sessions} sessions, ${last.new} of them new since the last run. Each new session was scored by the committed flow, and by the staged flow as it was before it learned from the session.`,
    '',
    `On the last ${last.compared} sessions both flows were scored on:`,
    '',
    '| Site | Committed: lookups | used | detours | Staged: lookups | used | detours |',
    '|---|---|---|---|---|---|---|',
    ...last.sites.map(row),
    row(last.total),
    '',
    `Commit it with \`stretto flow-commit --flow ${path}\`; the committed flow's versions are kept for \`stretto flow-rollback\`.`,
    '',
  ].join('\n')
}

/** The view `GET /api/flows/:key/stage` answers. */
export function stageView(
  record: StageRecord | null,
  committedKey: string | null,
  paths: { committed: string; staged: string; dataDir: string },
  staged: StageView['staged'],
  diff: StageView['diff'],
): StageView {
  const versions = [...(record?.versions ?? [])].reverse().map((v) => v.view)
  const refused = !staged
    ? `there is no staged flow at ${paths.dataDir}/${paths.staged}: stretto stage learns it`
    : record?.pending
      ? null
      : 'the staged flow is the committed one: nothing to commit'
  return {
    committed: committedKey,
    committed_path: paths.committed,
    staged,
    staged_path: paths.staged,
    last: record?.last ?? null,
    report_markdown: record?.last
      ? stageReport(`${paths.dataDir}/${paths.committed}`, record.last)
      : null,
    evidence: !!record?.last && !!staged,
    refused,
    diff: staged && committedKey ? diff : null,
    versions,
    unrecorded: !!committedKey && versions.length === 0,
  }
}

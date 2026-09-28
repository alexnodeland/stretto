/**
 * The console API's types.
 *
 * The server's DTOs are generated into `./generated/` by ts-rs, from
 * `crates/stretto-console` (`make types`), and re-exported here as they are:
 * snake_case fields, timestamps in unix milliseconds (`*_unix_ms`), sizes in
 * bytes, probabilities from 0 to 1. What follows the re-export is the UI's
 * own: names for parts of the generated unions, and the shape of a query.
 */

export type * from './generated'

import type { JobRequest, SessionMode } from './generated'

/** The hosts `GET /api/servers/:name/config?host=` knows, in the order the tabs show them. */
export type HostName = 'claude-code' | 'claude-desktop' | 'cursor' | 'vscode'

/** One kind of job request, such as `JobRequestOf<'promote'>`. */
export type JobRequestOf<K extends JobRequest['kind']> = Extract<JobRequest, { kind: K }>

/** The filters and the page of `GET /api/sessions`. */
export interface SessionQuery {
  domain?: string | null
  mode?: SessionMode | null
  q?: string | null
  limit?: number
  offset?: number
}

/* What Settings reads, and the things it may write: the pause and exclusions
 * of the project in front of the person (write addresses 26 and 27, through
 * data/holds.js), and the person's preferences (write address 15,
 * data/preferences.js, `PREFERENCES`).
 *
 * PURE apart from the `fetch` it is handed, so a node test can stub it and
 * prove the write without ever reaching a server.
 *
 * C46 §1.2(1): the pause switch used to reach `POST /api/observation` (row 12),
 * which pauses only HQ's in-memory watch on Codex. It now records the pause
 * (row 26): the six capture callers honour a recorded pause, and the same route
 * also pauses the Codex watch when it watches that folder. Row 12 is reached by
 * no control any more. */
import { holdAnswerOf, recordHold } from '../../data/holds.js'

export const READS = Object.freeze({
  storage: '/api/storage',
  lanes: '/api/lanes',
  observation: '/api/observation',
  mind: '/api/mind/status',
  licence: '/api/licence',
  projects: '/api/hq/projects',
})

/** The write addresses this screen reaches (CLAUDE.md write-address table rows 26, 27 and 15). */
export { HOLDS_PAUSE as PAUSE_WRITE } from '../../data/holds.js'
export { HOLDS_EXCLUSIONS as EXCLUSION_WRITE } from '../../data/holds.js'
export { PREFERENCES as PREFERENCES_WRITE } from '../../data/preferences.js'

/**
 * Pause (`paused`) or resume (`resumed`) recording in one project. Never throws:
 * resolves to what the answer means (holds.js `holdAnswerOf`), a refusal in
 * words that say nothing was saved.
 */
export async function setPause(project, act, fetchImpl = fetch) {
  return holdAnswerOf(await recordHold({ project, kind: 'pause', act }, fetchImpl))
}

/** Leave a built-in or typed path out of shown and exported views, or include
 * it again. The sealed transcript is not changed. Never throws. */
export async function setExclusion(project, act, pattern, fetchImpl = fetch) {
  return holdAnswerOf(await recordHold({ project, kind: 'exclusion', act, pattern }, fetchImpl))
}

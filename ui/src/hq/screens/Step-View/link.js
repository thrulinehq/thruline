/* The address of one step of the open Change — and the one place that decides
 * whether a chip may link to it (C30 R-ROUTES "Step view").
 *
 * PURE. A step is a row's place in the thread's record (data/change.js). A
 * check ThruLine itself ran is not in the agent's record, so it has no step
 * (`step: null`, or a half step between two records) and no page: its chip
 * does not link. A chip that names a step, or a version recorded at a step,
 * links to `href('step', …)` and nothing else does.
 */
import { href } from '../../routes.js'

/** A step number as the record numbers it: a whole number from 1. */
export const isStep = (n) => Number.isInteger(n) && n > 0

/**
 * The address of step `step` of the Change `at.change`, with its project, or
 * undefined when there is nothing real to open.
 * @param {{change?: string, project?: string}} at
 * @param {number|null|undefined} step
 */
export function stepHref(at, step) {
  if (!isStep(step) || !at || !at.change) return undefined
  return href('step', { change: at.change, step, project: at.project })
}

/**
 * `(step) => address|undefined` for the Change in front of the person. The
 * project is the address's, else the record's own (the address may omit it).
 * @param {{change?: string, project?: string}|undefined} at
 * @param {string|null|undefined} project  the open Change's project id
 */
export function stepLinker(at, project) {
  const where = { change: at && at.change, project: (at && at.project) || (project ? String(project) : undefined) }
  return (step) => stepHref(where, step)
}

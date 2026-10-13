/* "What should I tell the agent?" — the check sheet's own message, so ⌘K and
 * Check it again · Tell the agent copy one draft, not two (census §3.08 row 6).
 *
 * The message is composed in exactly one place: Repair-Loop's logic
 * (`renderVals().draft`), from Repair-Loop's own selection of the snapshot
 * (`selectRepairLoop`). This module runs that composer on the same snapshot
 * and hands ⌘K what it wrote; it writes no sentence of the message itself.
 * "Open it to edit" goes to that sheet (`#/changes/<id>/check`), where the
 * same draft can be edited.
 *
 * PURE. No fetch, no clipboard, no network: nothing is sent anywhere.
 */
import RepairLogic from '../Repair-Loop/logic.js'
import { selectRepairLoop } from '../Repair-Loop/from-change.js'
import { trackedIdOf } from '../Repair-Loop/live.js'

/**
 * The message for the Change in front of the person, or why there is none.
 *
 * @param {object|null} snap  the one Change snapshot (data/change.js)
 * @param {{now?: Date}} [ctx]
 * @returns {{draft: string|null, reason: string|null, command: string|null,
 *            statementStep: number|null, heldStep: number|null,
 *            movedStep: number|null, movedFile: string|null}}
 */
export function tellOf(snap, ctx = {}) {
  const none = (reason) => ({ draft: null, reason, command: null, statementStep: null, heldStep: null, movedStep: null, movedFile: null })
  if (!snap) return none(null)
  const id = trackedIdOf(snap)
  const tracked = id ? { id, runSheet: snap.runSheet || null } : null
  const data = selectRepairLoop(snap, tracked, { now: ctx.now instanceof Date ? ctx.now : new Date() })
  /* the check sheet's own words for this case (Repair-Loop/index.jsx) */
  if (!data || !data.statement) return none('Nothing the agent said in this work claims it is finished, so there is nothing to check again or tell the agent yet.')
  const inst = new RepairLogic({ data })
  inst.props = { data }
  const v = inst.renderVals()
  if (!v.hasDraft) return none('No message to prepare. The proof holds on ' + (data.version.id || 'this version') + '.')
  return {
    draft: v.draft,
    reason: null,
    command: data.check.command || null,
    statementStep: data.statement.step,
    heldStep: data.heldRun ? data.heldRun.step : null,
    movedStep: data.movedEdit ? data.movedEdit.step : null,
    movedFile: data.movedEdit ? data.movedEdit.file : null,
  }
}

/* Check it again on the Brief (C30 R-RUN): in place, as the canvas draws it.
 *
 * PURE. Given the act the run sheet allows (data/run.js `runActOf`, through
 * `useCheckRun`) and the presses, it returns the actions the Brief's logic
 * reads. Nothing here writes: `start` and `stop` are the one run client's.
 *
 * - The server would start the check: the press starts it here (permission
 *   once, then the run; the press is the consent), and the Brief draws
 *   "Checking now" from the snapshot the run sheet is folded into.
 * - The server says it will not start one: the control is off, with the
 *   server's own reason and what would let it run.
 * - Work not tracked as a Change, or a sheet that could not be read: the press
 *   opens the check sheet, which offers "Track this work as a Change" or says
 *   what it could not read. It writes nothing.
 */
export function briefCheckActions({ act, start, stop, open }) {
  const a = act || {}
  const inPlace = Boolean(a.changeId && a.canStart)
  return {
    check: () => {
      if (a.busy) return undefined
      if (inPlace) return start()
      if (a.blocked) return undefined
      return open()
    },
    stopCheck: a.runKey && !a.busy ? stop : undefined,
    inPlace,
    checkBlocked: !inPlace && a.blocked ? a.blocked : null,
    needsPermission: inPlace && a.needsPermission,
    refusal: a.refusal || null,
    // the Change the terminal line names (Repair-Loop/terminal.js), or null for untracked work
    changeId: a.changeId || null,
    // a check still starting has nothing to stop yet: say so rather than let Stop do nothing
    stopWhy: a.stopBlocked ? a.stopBlocked.reason + ' ' + a.stopBlocked.enabler : a.running === 'starting' ? 'It is still starting. Stop works once it has started.' : null,
  }
}

/* What the check sheet says and offers about tracking (C26 b). PURE, so every
 * sentence can be asserted on its own.
 *
 * - Work not tracked as a Change: the one primary act is "Track this work as a
 *   Change", with what it records and what it does not.
 * - Tracked, with no check that can run yet: the next honest step, and — when
 *   some of the work's sessions could not be linked — that, in plain words.
 */
export const TRACK_WHY = 'This work is not tracked as a Change yet, so a check has nothing to belong to. Tracking it records a Change named after your request, with this work linked to it as yours. It approves nothing and sends nothing to the agent.'
export const TRACK_NEXT = 'Then say what counts as done, and Check it again runs here.'
export const TRACKED_NEXT_WHY = 'This work is tracked as a Change. Before a check can run here, say what counts as done for it.'
export const TRACKED_NEXT = 'Then Check it again runs here, with the check you accept.'
export const TRACK_BUSY = 'Tracking this work as a Change. Nothing is approved and nothing is sent to the agent.'

export function trackOf({ untracked, busy, said, canStart, tracked, note }) {
  if (untracked) {
    if (said) return { offered: true, busy: false, why: said, next: null }
    return { offered: true, busy: !!busy, why: busy ? TRACK_BUSY : TRACK_WHY, next: TRACK_NEXT }
  }
  if (tracked && !canStart) return { offered: false, busy: false, why: note ? TRACKED_NEXT_WHY + ' ' + note : TRACKED_NEXT_WHY, next: TRACKED_NEXT }
  return { offered: false, busy: false, why: null, next: null }
}

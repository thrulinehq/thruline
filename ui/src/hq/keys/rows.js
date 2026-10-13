/* The rows Settings › Keyboard and the `?` map draw, generated from the key
 * table (table.js): one row per entry, in the table's order, in the row
 * shape Settings' keyboard map already draws ({keyText, label, state,
 * keyColor, kbdBd, labelColor}). PURE.
 *
 * The state cell, in plain words (STATE; spaces that never break, so a word
 * never wraps inside the canvas's 36 px row — a long LABEL wraps instead, and the row grows to fit it; a label is never cut off with an ellipsis or clipped, C30 R-KEYS sign-off):
 *   off          a single key, with single keys off
 *   not yet      drawn, and it does nothing yet (on this screen, for the map)
 *   on a Change  it works when a Change is open, and none is
 *   in lists     Settings only: it works on the Inbox, Changes and Projects
 *                and not yet on the other screens that draw it
 */
import { KEY_TABLE, entriesFor, wiredOn } from './table.js'

const T1 = '#EDEDEF', T2 = '#A1A1AA', T3 = '#82828C', LINE = '#2A2A30', CTRL = '#66666F'
const LISTS = ['Inbox', 'Changes', 'Projects']

export const STATE = Object.freeze({ off: 'off', notYet: 'not\u00a0yet', change: 'on\u00a0a\u00a0Change', lists: 'in\u00a0lists', some: 'on\u00a0some\u00a0screens' })

/** Every mode Settings › Keyboard offers, as `/api/preferences` spells it. */
export const MODES = Object.freeze(['on', 'mod', 'off'])

/** What the keys show: with ⌥ in front of a single key when single keys need ⌥ (as the canvas drew it). */
export function keyText(entry, mode) {
  const alt = mode === 'mod' && entry.kind === 'single'
  return entry.keys.map((k) => (alt ? '⌥' + k : k)).join(' / ')
}

/** The mode in a few words: "Single keys on" / "need ⌥" / "off". */
export const MODE_WORDS = Object.freeze({ on: 'on', mod: 'need ⌥', off: 'off' })

function row(entry, mode, state) {
  const dim = state === STATE.off || state === STATE.notYet
  return {
    id: entry.id,
    keyText: keyText(entry, mode),
    label: entry.label,
    state,
    keyColor: dim ? T3 : T1,
    kbdBd: dim ? LINE : CTRL,
    labelColor: dim ? T2 : T1,
  }
}

/** One entry's state on one screen. */
export function stateOn(entry, { mode, screen, change }) {
  if (entry.kind === 'single' && mode === 'off') return STATE.off
  if (!wiredOn(entry, screen)) return STATE.notYet
  if (entry.change && !change) return STATE.change
  return ''
}

/** One entry's state across the app (Settings, where no Change is open). */
export function stateAnywhere(entry, mode) {
  if (entry.kind === 'single' && mode === 'off') return STATE.off
  if (entry.global) return entry.waiting ? STATE.notYet : entry.change ? STATE.change : ''
  const wired = entry.wired || []
  if (!wired.length) return STATE.notYet
  if (wired.length < entry.screens.length) return wired.every((s) => LISTS.includes(s)) ? STATE.lists : STATE.some
  return ''
}

/** Settings › Keyboard: the whole table. */
export function settingsRows(mode) {
  return KEY_TABLE.map((e) => row(e, mode, stateAnywhere(e, mode)))
}

/** The map on `screen`: the keys drawn there, then the keys that work everywhere. */
export function mapRows({ screen, change, mode }) {
  const { here, everywhere } = entriesFor(screen)
  const at = { mode, screen, change }
  return {
    here: here.map((e) => row(e, mode, stateOn(e, at))),
    everywhere: everywhere.map((e) => row(e, mode, stateOn(e, at))),
  }
}

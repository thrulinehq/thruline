/* Settings › Keyboard on the record (C30 R-KEYS): the choice the person keeps
 * in preferences.json (data/preferences.js), and the key map generated from
 * the one key table (keys/rows.js `settingsRows`). PURE.
 *
 * The choice is drawn as the server last kept it. While it is being read no
 * segment is pressed; a read that failed presses the default and says it is
 * the default; a write in flight, or refused, is said in the pane and the
 * pressed segment stays where the server has it. */
import { DEFAULTS, fileWords } from '../../data/preferences.js'
import { MODES, settingsRows } from '../../keys/rows.js'

export const KEY_SEGMENTS = Object.freeze([{ key: 'on', label: 'On' }, { key: 'mod', label: 'Only with ⌥' }, { key: 'off', label: 'Off' }])

const NAME = { on: 'On', mod: 'Only with ⌥', off: 'Off' }
const SENTENCE = {
  on: 'On: one key per action. Turn off or add ⌥ if you use speech input.',
  mod: 'Each single key now needs ⌥ held, so typing can’t trigger one.',
  off: 'Off: no single key acts. ⌘K, ⌘↵, ↵ and Esc still work.',
}
const STILL = { on: 'Single keys are still on.', mod: 'Single keys still need ⌥.', off: 'Single keys are still off.' }

export const KEYS_NOTE = 'No single key acts while you type in a field. “not yet” marks a key that does nothing yet, “on a Change” one that works while a Change is open, and “in lists” one that works on the Inbox, Changes and Projects.'

/**
 * @param {{prefs: object|null, mkSeg: Function, setPreference?: Function}} ctx
 *   prefs  the preferences store's state (data/preferences.js), or null before it is read
 * @returns {{segKeys, keysText, keymap}}
 */
export function keyboardPane({ prefs, mkSeg, setPreference }) {
  const P = prefs || { status: 'idle', prefs: DEFAULTS, pending: {}, writeError: {} }
  const reading = P.status === 'idle' || P.status === 'reading'
  const mode = MODES.includes(P.prefs && P.prefs.keys) ? P.prefs.keys : 'on'
  const pending = P.pending && P.pending.keys
  const refused = P.writeError && P.writeError.keys
  const pick = (k) => {
    if (reading || pending || k === mode || typeof setPreference !== 'function') return
    setPreference('keys', k)
  }
  let keysText
  if (reading) keysText = 'Reading your choice from this machine…'
  else if (pending) keysText = 'Keeping “' + NAME[pending] + '” on this machine…'
  else if (refused) keysText = 'ThruLine could not keep “' + (NAME[refused.value] || refused.value) + '”: ' + refused.why + '. ' + STILL[mode]
  else if (P.status === 'failed') keysText = 'ThruLine could not read your saved choice just now, so single keys are on, the default.'
  else keysText = [SENTENCE[mode], fileWords(P.file)].filter(Boolean).join(' ')
  return {
    segKeys: mkSeg(KEY_SEGMENTS, reading ? null : mode, pick),
    keysText,
    keymap: settingsRows(mode),
  }
}

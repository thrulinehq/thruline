/* What a stored choice is called, in a person's words (C30 R-PREFS): the one
 * list the Settings rows and the notice on every other screen both read, so a
 * choice that was not kept is said the same way wherever it is said.
 *
 * PURE. A refusal's reason is the store's own (`writeError[name].why`).
 */

const WORD = {
  face: { plain: 'Plain', technical: 'Technical' },
  density: { comfortable: 'Comfortable', compact: 'Compact' },
  motion: { system: 'Follow this Mac', reduce: 'Always reduce' },
  colour_vision: { standard: 'Proven in green', proven_teal: 'Proven is teal' },
  keys: { on: 'On', mod: 'Only with ⌥', off: 'Off' },
}

const SUBJECT = {
  face: 'Face',
  density: 'Density',
  motion: 'Motion',
  colour_vision: 'Colour',
  keys: 'Single keys',
  masked_copy: 'The masked copy',
  weekly_summary: 'The weekly summary',
  'notifications.ready': 'Your move notifications',
  'notifications.not_backed': 'Not backed notifications',
  'notifications.agent_waiting': 'Agent waiting notifications',
  'notifications.recording_stopped': 'Recording stopped notifications',
}

/** The words for one value of one choice: “Technical”, “Always reduce”, “On”. */
export function valueWords(name, value) {
  if (typeof value === 'boolean') return value ? 'On' : 'Off'
  return (WORD[name] && WORD[name][value]) || String(value)
}

/** "Face is still Plain." */
export function stillLine(name, current) {
  return (SUBJECT[name] || 'That choice') + ' is still ' + valueWords(name, current) + '.'
}

/** While the server is asked to keep a choice. */
export function keepingLine(name, value) {
  return 'Keeping “' + valueWords(name, value) + '” on this machine…'
}

/**
 * A choice the server did not keep: what was asked, why not, and what stands.
 * @param {string} name   'face', 'notifications.moved', …
 * @param {{value: any, why: string}} refusal   the store's `writeError[name]`
 * @param {any} current   the value that is still stored
 */
export function refusedLine(name, refusal, current) {
  return 'ThruLine could not keep “' + valueWords(name, refusal.value) + '”: ' + refusal.why + '. ' + stillLine(name, current)
}

/** Said once on the Settings pane when the choices could not be read, or the file was not usable. */
export const NOT_READ = 'ThruLine could not read your saved choices just now, so these show the defaults.'
export const READING = 'Reading your choices from this machine…'

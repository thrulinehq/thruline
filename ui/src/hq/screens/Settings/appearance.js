/* Settings on the person's stored choices (C30 R-PREFS): face, density,
 * motion, colour vision, the masked copy, the four notification switches and
 * the weekly summary. PURE.
 *
 * Two jobs, both beside keyboard.js (which does the same for single keys):
 *  - `storedSettings(props)` reads the choices as the screen's own keys
 *    (settingsOf in logic.js), so every control is drawn as the server last
 *    kept it, and a choice never kept is not drawn as kept;
 *  - `appearanceText(v, ctx)` says, beside each control, what it does now and
 *    what a write that was not kept left standing: a write in flight, a
 *    refusal, a read that failed, a file that could not be used.
 *
 * What each choice does is applied elsewhere and shown here only in words:
 *  face (data/face.js, top bar, `t`) · density (Inbox and Changes rows) ·
 *  motion and colour vision (appearance.css, on the app root) · masked copy
 *  (the export sheet starts with its redacted copy on) · notifications from
 *  the open HQ page · a weekly card with an explicit Markdown export.
 */
import { allowed, fileWords, valueOf } from '../../data/preferences.js'
import { NOT_READ, READING, keepingLine, refusedLine } from '../../data/preference-words.js'

const NOTIF_KEYS = ['ready', 'not_backed', 'agent_waiting', 'recording_stopped']

/** The stored choices, under Settings' own keys; a key the store does not hold is left out. */
export function storedSettings(props, base = {}) {
  const st = props && props.prefs
  const p = st && st.prefs
  if (!p) return {}
  const out = {}
  if (allowed('face', p.face)) out.face = p.face
  if (allowed('density', p.density)) out.density = p.density
  if (allowed('motion', p.motion)) out.motion = p.motion
  if (allowed('keys', p.keys)) out.keys = p.keys
  if (allowed('colour_vision', p.colour_vision)) out.cvd = p.colour_vision === 'proven_teal'
  if (typeof p.masked_copy === 'boolean') out.redactCopy = p.masked_copy
  if (typeof p.weekly_summary === 'boolean') out.digest = p.weekly_summary
  if (p.notifications && typeof p.notifications === 'object') {
    out.notif = Object.assign({}, base.notif)
    for (const k of NOTIF_KEYS) if (typeof p.notifications[k] === 'boolean') out.notif[k] = p.notifications[k]
  }
  return out
}

/** Whether the choices are still being read, and whether the read failed. */
export function readState(prefs) {
  const status = prefs ? prefs.status : 'idle'
  return { reading: status === 'idle' || status === 'reading', failed: status === 'failed' }
}

/** What stands after a write that is in flight or was refused, else `normal`. */
function lineFor(P, name, normal) {
  const stored = valueOf(P.prefs, name)
  if (P.pending && name in P.pending) return keepingLine(name, P.pending[name])
  const refused = P.writeError && P.writeError[name]
  if (refused) return refusedLine(name, refused, stored)
  return normal
}

const noop = () => {}

/**
 * Say the live words beside each control, and lock the controls while the choices are being read.
 * @param {object} v     the vals logic.js built (mutated, as liveOverrides does)
 * @param {{prefs: object|null, s: object, mkSwitch: Function}} ctx
 */
export function appearanceText(v, { prefs, s, mkSwitch, notificationPermission = null }) {
  const P = prefs || { status: 'idle', prefs: {}, pending: {}, writeError: {} }
  const { reading, failed } = readState(prefs)
  const line = (name, normal) => lineFor(P, name, normal)
  const lock = (segs) => (Array.isArray(segs) ? segs.map((o, i) => Object.assign({}, o, { dis: 'true', bl: i === 0 ? '0' : '1px dashed #66666F', pick: noop })) : segs)

  v.content = Object.assign({}, v.content, {
    faceTail: [' switches it anywhere.', lineFor(P, 'face', '')].filter(Boolean).join(' '),
    motionSub: line('motion', v.content && v.content.motionSub),
    lookSub: ['Yours alone, remembered on this machine. States never depend on colour alone.', reading ? READING : failed ? NOT_READ : fileWords(P.file)].filter(Boolean).join(' '),
    redactSub: line('masked_copy', 'A second, labelled file that names the file it came from. The original is never edited. '
      + (s.redactCopy ? 'The export sheet starts with its redacted copy on.' : 'The export sheet starts with its redacted copy off; switch it on there each time.')),
    digestSub: line('weekly_summary', 'On the first HQ open each week, show a dismissible last-7-day card. Save as Markdown downloads it to this machine.'),
    notifChip: 'Open HQ page only · browser permission required',
  })
  v.notes = Object.assign({}, v.notes, { notif: 'These four events can notify from this open HQ page only. Each starts Off. Turning one on first asks your browser for permission. Nothing leaves this machine.' + (notificationPermission ? ' ' + notificationPermission : '') })
  v.densityText = line('density', v.densityText)
  v.cvd = Object.assign({}, v.cvd, { sentence: line('colour_vision', v.cvd && v.cvd.sentence) })
  v.notifs = (v.notifs || []).map((n, i) => (i < NOTIF_KEYS.length
    ? Object.assign({}, n, { sub: line('notifications.' + NOTIF_KEYS[i], n.sub) })
    : n))

  if (reading) {
    v.segFace = lock(v.segFace)
    v.segDensity = lock(v.segDensity)
    v.segMotion = lock(v.segMotion)
    v.sw = Object.assign({}, v.sw, {
      redact: mkSwitch(s.redactCopy, noop, true),
      cvd: mkSwitch(s.cvd, noop, true),
      digest: mkSwitch(s.digest, noop, true),
    })
    v.notifs = v.notifs.map((n, i) => (i < NOTIF_KEYS.length ? Object.assign({}, n, { sw: mkSwitch(s.notif && s.notif[NOTIF_KEYS[i]], noop, true) }) : n))
  }
  return v
}

/* What exactly will be examined — the deterministic model behind the Prepare
 * view (T-3-2, `B4.6`, `B6.2`, `B6.3`).
 *
 * PURE. No React, no fetch, no clock, no locale, no randomness. One set of
 * functions over the document `GET /api/changes/<id>/inputs` serves, and
 * nothing else. `change-workspace.jsx` lays out what this decided; every word
 * it lays out comes from `HQ_COPY`.
 *
 * WHAT THIS FILE IS FOR. Sam's first question's second clause — "one version".
 * Until this card the Prepare view could name a repository and a starting point
 * and could not say whether the unsaved edits on Sam's disk were part of what
 * was looked at, which tools would run, or which running copy of the app the
 * answer came from.
 *
 * THE RULES IT HOLDS:
 *
 *  - NOTHING IS INFERRED HERE. The engine names the slug for every line; this
 *    file looks the sentence up and fills its placeholders. A line whose slug
 *    has no sentence renders nothing rather than a half-sentence.
 *  - UNKNOWN IS NOT A VALUE. A field the engine served as unknown renders "not
 *    written down". It never renders as a blank, a dash, or the word itself.
 *  - THE PLAIN LAYER NEVER SAYS the snapshot id, the whole tree hash, the pid
 *    or the word worktree. They are all in `technicalRows`, one click deeper.
 *  - A REVISION IS SHORTENED HERE, not in the engine. The engine serves the
 *    fact; a surface that wants twelve characters cuts twelve characters.
 */

import { HQ_COPY } from './hq-copy.js'
import { fill } from './hq-narrate.js'

const C = HQ_COPY.change.prepare

const list = (v) => (Array.isArray(v) ? v : [])
const str = (v) => (typeof v === 'string' && v.trim() ? v.trim() : null)

/** How many characters of a revision a person reads. */
export const REVISION_CHARACTERS = 12

/** A revision, shortened for reading. Anything else is left alone. */
export function shortRevision(value) {
  const text = str(value)
  if (!text) return null
  return text.length > REVISION_CHARACTERS ? text.slice(0, REVISION_CHARACTERS) : text
}

/** `{state: 'known', value}` → the value; anything else → null. */
export function inputValue(field) {
  const f = field || {}
  return f.state === 'known' ? str(f.value) : null
}

/** A count of files with its noun attached. A bare number is a number with no sentence. */
export function fileCount(n) {
  const count = Number.isFinite(n) ? Math.max(0, Math.trunc(n)) : 0
  return count === 1 ? C.one_file : fill(C.many_files, { n: count })
}

/**
 * One line of the Prepare view, as a person reads it.
 *
 * @param {object} line one entry of `prepare.lines`
 * @param {object} doc the whole `prepare` object, for the app's program name
 * @returns {string|null} null when the slug has no sentence here
 */
export function lineSentence(line, doc) {
  const l = line || {}
  const slug = str(l.slug)
  if (!slug) return null
  const template = C.lines[slug]
  if (!template) return null
  const raw = str(l.value)
  const values = {
    value: slug === 'prepare_base' ? shortRevision(raw) || C.not_written_down : raw || C.not_written_down,
    port: raw || '',
    app: str(doc && doc.app_program) || '',
    tool: str(l.tool) || '',
    count: fileCount(Number(raw)),
  }
  return fill(template, values)
}

/**
 * Every line of the Prepare view, in the order the engine served them.
 *
 * @param {object} doc the `prepare` object of the served document
 * @returns {Array<{slug: string, text: string}>}
 */
export function prepareLines(doc) {
  return list(doc && doc.lines)
    .map((line) => ({ slug: str(line && line.slug) || '', text: lineSentence(line, doc) }))
    .filter((line) => line.text)
}

/**
 * The technical layer: every field the record holds, named, with the ones the
 * probes could not answer said rather than hidden.
 *
 * This is the one place the engine's own words appear on this screen, and it
 * is one click deeper than the sentences above.
 *
 * @param {object} doc the `prepare` object
 * @returns {Array<{label: string, value: string}>}
 */
export function technicalRows(doc) {
  const inputs = (doc && doc.inputs) || null
  if (!inputs) return []
  const missing = C.not_written_down
  const rows = [
    ['snapshot_id', inputValue(inputs.snapshot_id)],
    ['source_identity', inputValue(inputs.source_identity)],
    ['base_revision', inputValue(inputs.base_revision)],
    ['worktree_path', inputValue(inputs.worktree_path)],
    ['lockfiles_sha256', inputValue(inputs.lockfiles_sha256)],
    ['config_sha256', inputValue(inputs.config_sha256)],
  ]
  const target = inputs.target || {}
  if (target.kind === 'running_app') {
    rows.push(['port', target.port == null ? null : String(target.port)])
    rows.push(['port_owner_pid', inputValue(target.owner_pid)])
    rows.push(['port_owner_exe', inputValue(target.owner_exe)])
    rows.push(['database_path', inputValue(target.database_path)])
  }
  return rows.map(([label, value]) => ({ label, value: value || missing }))
}

/**
 * The note under the facts.
 *
 * It changes when a running application is named, because the standing sentence
 * — "nothing about databases, ports, secrets or outside services is claimed" —
 * is false the moment a port and a database are on the screen above it.
 *
 * @param {object} doc the `prepare` object
 * @returns {string}
 */
export function isolationNote(doc) {
  const target = (doc && doc.inputs && doc.inputs.target) || {}
  return target.kind === 'running_app' ? C.isolation_note_app : C.isolation_note
}

/** How many records of what will be examined this Change has. */
export function recordsLine(doc) {
  const n = doc && typeof doc.record_count === 'number' ? doc.record_count : 0
  if (n <= 0) return null
  return n === 1 ? C.record_count_one : fill(C.record_count_many, { n })
}

/**
 * Everything the Prepare view renders, decided in one place.
 *
 * @param {object} served the whole document `GET /api/changes/<id>/inputs`
 *   serves, or null when it has not been read
 * @returns {{recorded: boolean, lines: object[], technical: object[],
 *   records: ?string, note: string}}
 */
export function buildPrepareView(served) {
  const doc = (served && served.prepare) || null
  return {
    recorded: Boolean(doc && doc.inputs),
    lines: prepareLines(doc),
    technical: technicalRows(doc),
    records: recordsLine(doc),
    note: isolationNote(doc),
  }
}

export default buildPrepareView

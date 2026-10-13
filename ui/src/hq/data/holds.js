/* The pause and exclusions writes (C30 R-LEDGER capture holds), and the read of
 * `holds` they change. Settings' pause switch and exclusions pane reach these
 * through `recordHold` (Settings/writes.js, C46 §1.2(1)).
 *
 *   POST /api/hq/projects/<project>/capture/pause        (write address 26, CLAUDE.md)
 *   POST /api/hq/projects/<project>/capture/exclusions   (write address 27)
 *   GET  /api/hq/projects/<project>/capture              (`holds`, read by `readCapture`, capture.js)
 *
 * The six capture callers honour a recorded pause; served views and exports
 * honour an active path exclusion. `holdsOf` keeps both server claims and its
 * sentence, and never infers either claim from a recorded row alone.
 *
 * Declared here, and `holds.test.js` asserts this is the only file under
 * `ui/src/hq` that reaches either address. The body is `{act}` (a pause) or
 * `{act, pattern}` (an exclusion) and nothing else: the folder, the project and
 * the name are the SERVER's to say. The acts are free — approving a version is
 * the one paid act.
 *
 * `crates/cli/src/uiserve/capture_holds.rs` answers one of:
 *   200 {schema_version, act, hold, holds, codex_observer, message}      pause
 *   200 {schema_version, act, pattern, hold, holds, message}             exclusion
 *   400 {error: "malformed_request" | "unknown_act"}
 *   400 {error: "bad_pattern", message}
 *   403 {error: "same_origin_required"}
 *   404 {error: "no_such_project"}
 *   409 {error: "already_paused" | "not_paused" | "already_excluded" | "not_excluded", message}
 *   409 {error: "too_long", field: "pattern", limit}
 *   413 {error: "too_long"}
 *   503 {error: "upgrade_pending" | "store_busy", message}
 * and `send` turns a request that got no answer into status 0,
 * `{error: "unreachable"}`. `holdAnswerOf` reads every one; none is dropped.
 */

/** The pause switch: write address 26. */
export const HOLDS_PAUSE = (project) => '/api/hq/projects/' + encodeURIComponent(project) + '/capture/pause'

/** The exclusions list: write address 27. */
export const HOLDS_EXCLUSIONS = (project) => '/api/hq/projects/' + encodeURIComponent(project) + '/capture/exclusions'

async function send(url, body, f) {
  try {
    const res = await f(url, {
      method: 'POST',
      cache: 'no-store',
      headers: { Accept: 'application/json', 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    })
    const doc = await res.json().catch(() => null)
    return { ok: res.ok, status: res.status, doc }
  } catch (_e) {
    return { ok: false, status: 0, doc: { error: 'unreachable' } }
  }
}

const NOT_AVAILABLE = { ok: false, status: 0, doc: { error: 'not_available' } }

/**
 * Keep one hold. `kind` is `pause` (`act` is `paused` or `resumed`) or
 * `exclusion` (`act` is `excluded` or `included`, and `pattern` is a built-in
 * name — env, secrets, generated — or one path inside the project). Nothing is
 * sent without a project, a known act and, for an exclusion, words in the
 * pattern.
 */
export function recordHold({ project, kind, act, pattern }, f = fetch) {
  if (!project) return Promise.resolve(NOT_AVAILABLE)
  if (kind === 'pause') {
    if (act !== 'paused' && act !== 'resumed') return Promise.resolve(NOT_AVAILABLE)
    return send(HOLDS_PAUSE(project), { act }, f)
  }
  if (kind === 'exclusion') {
    if (act !== 'excluded' && act !== 'included') return Promise.resolve(NOT_AVAILABLE)
    if (typeof pattern !== 'string' || !pattern.trim()) return Promise.resolve({ ok: false, status: 0, doc: { error: 'pattern_required' } })
    return send(HOLDS_EXCLUSIONS(project), { act, pattern }, f)
  }
  return Promise.resolve(NOT_AVAILABLE)
}

/** What was NOT done, in words, whatever the refusal: said after every one. */
const AND_NOTHING = ' Nothing was saved.'

/** Plain words for a refusal the server named. Each says nothing was saved. */
const REFUSAL_WORDS = {
  not_available: 'ThruLine cannot tell which project is on screen.',
  pattern_required: 'There is no path to leave out: the box is empty.',
  malformed_request: 'ThruLine could not read the request this page sent. Reload this page and try again.',
  unknown_act: 'ThruLine could not read the request this page sent. Reload this page and try again.',
  same_origin_required: 'ThruLine keeps this only from its own page. Reload this page and try again.',
  no_such_project: 'This project is no longer in the record ThruLine is reading. Reload this page.',
  bad_pattern: 'That cannot be left out. Use a built-in name or a path inside the project.',
  too_long: 'A path to leave out can be at most 256 characters. Shorten it and try again.',
  already_paused: 'This project is already paused. Reload this page.',
  not_paused: 'This project is not paused. Reload this page.',
  already_excluded: 'That path is already excluded. Reload this page.',
  not_excluded: 'That path is not excluded. Reload this page.',
  upgrade_pending: 'ThruLine needs an upgrade before it can record anything. Run thruline upgrade in a terminal, then try again.',
  unreachable: 'ThruLine did not answer, so it may not have kept this. Reload this page to see what the record holds.',
}

/** The server's own sentence for these, which already says nothing was saved. */
const SERVER_SAYS = new Set(['already_paused', 'not_paused', 'already_excluded', 'not_excluded', 'bad_pattern', 'store_busy'])

/**
 * What one answer means for the screen. PURE.
 * @param {{ok: boolean, status: number, doc: any}} res what `recordHold` resolved to
 * @returns {{kind: 'kept', act: string, hold: object|null, holds: object|null, codexObserver: string|null} | {kind: 'refused', detail: string, code: string}}
 *   `holds` is where the folder stands after the act, as `holdsOf` reads it.
 */
export function holdAnswerOf(res) {
  const r = res && typeof res === 'object' ? res : {}
  if (typeof r.status !== 'number') return { kind: 'refused', code: 'unreachable', detail: REFUSAL_WORDS.unreachable }
  const doc = r.doc && typeof r.doc === 'object' ? r.doc : {}
  if (r.status === 200 && typeof doc.act === 'string' && doc.holds && typeof doc.holds === 'object') {
    return {
      kind: 'kept',
      act: doc.act,
      hold: doc.hold && typeof doc.hold === 'object' ? doc.hold : null,
      holds: holdsOf({ holds: doc.holds }),
      codexObserver: typeof doc.codex_observer === 'string' ? doc.codex_observer : null,
    }
  }
  const code = typeof doc.error === 'string' ? doc.error : ''
  if (SERVER_SAYS.has(code) && typeof doc.message === 'string' && doc.message) return { kind: 'refused', code, detail: doc.message }
  if (code === 'store_busy') return { kind: 'refused', code, detail: REFUSAL_WORDS.unreachable }
  if (REFUSAL_WORDS[code]) return { kind: 'refused', code, detail: REFUSAL_WORDS[code] + AND_NOTHING }
  if (r.status === 413) return { kind: 'refused', code: 'too_long', detail: REFUSAL_WORDS.too_long + AND_NOTHING }
  if (r.status === 200) return { kind: 'refused', code: 'unreadable', detail: 'ThruLine answered in a form this page cannot read. Reload this page to see what the record holds.' }
  return { kind: 'refused', code: 'other', detail: 'ThruLine could not keep this (it answered ' + r.status + '). Reload this page to see what the record holds.' }
}

/**
 * `holds` of a capture document (or of an answer), read for a screen. PURE.
 * `null` when the document has none — the record could not be read, or this is
 * an older server — never a guess. `honoured` is `false` unless the server
 * said otherwise, and the sentence beside it is kept.
 *
 * @param {any} doc a `GET …/capture` document, or `{holds}`
 * @returns {null | {paused: boolean, pausedAt: string|null, pausedBy: string|null, honoured: boolean, exclusionsHonoured: boolean, note: string,
 *   exclusions: Array<{key: string, covers: string, note: string|null, builtIn: boolean, on: boolean,
 *   source: 'default'|'recorded', basis: string|null, since: string|null, by: string|null}>}}
 */
export function holdsOf(doc) {
  const h = doc && typeof doc === 'object' ? doc.holds : null
  if (!h || typeof h !== 'object' || !Array.isArray(h.exclusions)) return null
  return {
    paused: h.paused === true,
    pausedAt: typeof h.paused_at === 'string' ? h.paused_at : null,
    pausedBy: typeof h.paused_by === 'string' ? h.paused_by : null,
    honoured: h.honoured === true,
    exclusionsHonoured: h.exclusions_honoured === true,
    note: typeof h.note === 'string' ? h.note : '',
    exclusions: h.exclusions
      .filter((e) => e && typeof e === 'object' && typeof e.key === 'string')
      .map((e) => ({
        key: e.key,
        covers: typeof e.covers === 'string' ? e.covers : e.key,
        note: typeof e.note === 'string' ? e.note : null,
        builtIn: e.built_in === true,
        on: e.on === true,
        source: e.source === 'recorded' ? 'recorded' : 'default',
        basis: typeof e.basis === 'string' ? e.basis : null,
        since: typeof e.since === 'string' ? e.since : null,
        by: typeof e.by === 'string' ? e.by : null,
      })),
  }
}

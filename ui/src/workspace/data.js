/* ThruLine workspace — the read layer. The ONLY file in `src/workspace/` that
 * calls `fetch`.
 *
 * `adapter.js` is pure and stays pure; this file does the I/O and hands it the
 * documents. Splitting them is what makes the adapter unit-testable without a
 * server, and it is what keeps the read discipline in one auditable place.
 *
 * THE DISCIPLINE, carried over verbatim from the shipped app (spec-target §B):
 *  1. The index is a PROP, never a fetch. `main.jsx` reads
 *     `/fixtures/index.json` exactly once and hands it down; `main.jsx` must
 *     keep exactly one `fetch(` and it must be `fetch(INDEX` (pinned at
 *     `lib/arcade.test.js:228-229`). Nothing here re-reads it.
 *  2. A project fixture is read only when a project is OPENED. "A prop is not a
 *     load" (`mac/shell.jsx:26-33`).
 *  3. Every read is `{ cache: 'no-store' }`. No exception.
 *  4. Every effect carries a `live` flag and every load carries an epoch
 *     counter re-checked before each setState, so a slow response for an
 *     abandoned project cannot overwrite a newer one
 *     (`mac/shell.jsx:204,222,233,236,239`).
 *  5. Silent degradation is allowed ONLY for an optional local server
 *     capability (`/api/lanes`, `/api/state`). A missing DOCUMENT is spoken,
 *     never swallowed.
 *  6. Foreground polling only; every interval is cleared on unmount and nothing
 *     outlives the page.
 *  7. A source digest is taken over the RAW response text, not a
 *     re-serialization (`mac/shell.jsx:220-221,230`).
 *  8. Nothing here executes work. Remedies are copyable strings.
 */

/** The two §9 addresses. The index is not fetched here — see rule 1. */
export const PROJECT = (fixture) => `/fixtures/projects/${fixture}.json`

/**
 * Endpoints this build may call, and the ONE place their paths are written.
 * `/api/receipt`, `/api/verify`, `/api/asks` and `/api/narrate` are absent from
 * this map because they do not exist on the server
 * (crates/cli/src/uiserve.rs:607-680) — do not add a path here hoping one does.
 *
 * `sessions` is the list a session is CHOSEN from
 * (`crates/cli/src/uiserve.rs:656-670`, serving the bytes of
 * `thruline sessions --json`). It is added here because the route now exists,
 * and it is what makes the ONE reading addressable: `/api/explain` with no
 * `?session=` opens whichever id sorts last, and on a real store that is an
 * arbitrary session with nothing in it. IT TAKES NO QUERY PARAMETER — the
 * server refuses any (`uiserve.rs:661-668`), so nothing may append one.
 */
export const API = Object.freeze({
  sessions: '/api/sessions',
  explain: '/api/explain',
  claims: '/api/claims',
  state: '/api/state',
  refresh: '/api/refresh',
  lanes: '/api/lanes',
  acceptance: '/api/acceptance',
  /* What this product is keeping on the machine (T-2-6, `L1.1`). A GET that
   * measures the store the server already has open and answers with a byte
   * count and the folder — no record text, nothing from a transcript. It is an
   * OPTIONAL local capability in the sense of rule 5: a server that does not
   * answer it leaves the sentence saying the size could not be measured, and
   * never a guessed number. */
  storage: '/api/storage',
  /* Whether this machine has a licence that works (T-DIST-4). A GET, and the
   * only licence address: there is no upload and no licence write anywhere on
   * this surface — a licence is installed from a terminal. The body carries no
   * path and no fingerprint. */
  licence: '/api/licence',
  /* The Change routes (T-2-1/T-2-2/T-2-3). The app's read contract is widened
   * by these three and by nothing else; the widening is recorded in CLAUDE.md
   * the way A90 recorded the last one, rather than happening quietly.
   *
   * All three are GET and all three are masked before they are served
   * (`change::read_changes`, `change_propose::propose`), so this surface can
   * only render text the engine has already put through the masker. None of
   * these three writes. The `POST` addresses under `/api/changes/` are not in
   * this map: the ones a screen sends are each declared as a named constant
   * beside the control that sends it (`CHANGE_DECIDE` in `change-workspace.jsx`
   * is one), CLAUDE.md's write-address table lists the ones the Change screen
   * reaches, and `change-workspace.test.js` pins them. */
  changes: '/api/changes',
  /* The first-use path (T-2-7). Two reads — what this machine already has, and
   * the Change its most recent recorded work would make — and two writes, each
   * of which is one click by a person: `firstUseRegister` writes the picked
   * project's container, `firstUseChange` writes the Change. Neither carries a
   * path, a command or a title from the browser: the server re-derives the
   * whole candidate list on every request and matches an opaque key against
   * what IT found (`crates/cli/src/uiserve/first_use.rs`). */
  firstUse: '/api/first-use',
  firstUseRegister: '/api/first-use/register',
  firstUseChange: '/api/first-use/change',
})

/** The Change this project's most recent recorded work would make (read-only). */
export const FIRST_USE_PROPOSAL = (project) =>
  '/api/first-use/proposal?project=' + encodeURIComponent(project)

/* There is deliberately NO Change WRITE address in this file. T-2-4 pins that
 * the read layer holds none (`change-workspace.test.js`, "navigating writes
 * nothing"), and the one Change write this build makes — accepting what counts
 * as done — is written down beside its only caller, in `first-use.jsx`. The
 * fetch still happens here, in `writeAction`: the discipline is that this file
 * is the only one that calls `fetch`, not that it owns every address. */

/** The Read (P6-2): the steps of opening one piece of recorded work, from the
 * engine's own phase log (`GET /api/read?session=<id>`,
 * `crates/cli/src/uiserve/read_route.rs`). A read, and only a read: the route
 * writes nothing to any store or project. It is not in `API` because it always
 * names one session; the rail (`read-rail.jsx`) is its one caller, through
 * `readProjection` below. */
export const READ = (session) => '/api/read?session=' + encodeURIComponent(session)

/** One Change, by id (`GET /api/changes/<id>`). */
export const CHANGE = (id) => '/api/changes/' + encodeURIComponent(id)

/** What that Change could be judged against (`GET /api/changes/<id>/proposals`). */
export const CHANGE_PROPOSALS = (id) => '/api/changes/' + encodeURIComponent(id) + '/proposals'

/** What the record holds for each accepted line (`GET /api/changes/<id>/coverage`).
 *
 * A read. The route brings the engine's derived link rows up to date before it
 * answers — it says so in its own doc comment, and the rows it writes are a
 * deterministic, idempotent, append-only function of the record — but nothing
 * on this side sends anything. The only Change addresses this module exports
 * are GETs, and `change-workspace.test.js` asserts it. */
export const CHANGE_COVERAGE = (id) => '/api/changes/' + encodeURIComponent(id) + '/coverage'

/** What exactly will be examined for one Change (T-3-2). A read, and only a
 * read: unlike `/coverage` this route derives nothing. */
export const CHANGE_INPUTS = (id) => '/api/changes/' + encodeURIComponent(id) + '/inputs'

/** Whether each piece of linked evidence still applies now that the tree has
 * moved (T-S-2/T-S-3). A read, and only a read: this route derives nothing and
 * writes nothing, and a Change with nothing assessed serves an empty list —
 * which the model reads as "not assessed", never as "still applies". */
export const CHANGE_APPLICABILITY = (id) => '/api/changes/' + encodeURIComponent(id) + '/applicability'

/** One review's raw report, or its parsed record, as the masked inspection
 * copy the server builds (T-4-1, `B9.6`). Reads, and only reads: the served
 * body is never the stored bytes, and a review whose stored parts no longer
 * match is refused with a code (`review_integrity_altered`,
 * `review_integrity_missing`, `review_not_text`, `no_such_review`). */
export const CHANGE_REVIEW_RAW = (id, review) =>
  '/api/changes/' + encodeURIComponent(id) + '/reviews/' + encodeURIComponent(review) + '/raw'
export const CHANGE_REVIEW_PARSED = (id, review) =>
  '/api/changes/' + encodeURIComponent(id) + '/reviews/' + encodeURIComponent(review) + '/parsed'

/** Every finding of this Change's reviews and where each one stands (T-4-2,
 * `GET /api/changes/<id>/findings`). A read, and only a read. The one write
 * that changes a standing is declared beside its only control, in
 * `change-review.jsx`, not here: this file holds no Change write address. */
export const CHANGE_FINDINGS = (id) => '/api/changes/' + encodeURIComponent(id) + '/findings'

/** The review drill-down's refusal codes, exactly as the server names them. */
export const REVIEW_REFUSAL_CODES = Object.freeze([
  'review_integrity_altered',
  'review_integrity_missing',
  'review_not_text',
  'no_such_review',
])

/** The code a refused read carries (`<url> refused: <code>`), or null. */
export function refusalCode(read) {
  const r = read && typeof read.refusal === 'string' ? read.refusal : ''
  const m = / refused: ([a-z_]+)$/.exec(r)
  return m ? m[1] : null
}

/**
 * @typedef {object} Read
 * @property {'idle'|'loading'|'ready'|'refused'} status
 * @property {any} [doc] the parsed body, when `ready`
 * @property {string} [raw] the response text, for a source digest
 * @property {number} [exit] the `X-ThruLine-Exit` header — exit 3 still serves
 *   HTTP 200 and is NOT a failure (docs/UI-SERVER-CONTRACT.md §2.3)
 * @property {string} [projection] the `X-ThruLine-Projection` header, which is
 *   the only honest source for a session's `origin`
 * @property {string} [refusal] why it refused, in the reader's words, when
 *   `status === 'refused'`
 */

/**
 * Read one projection endpoint.
 *
 * Exit **0 or 3 → HTTP 200** and both are successful reads; a 502 carries
 * `{error:'projection_failed', command, exit_code, stderr}` with the stderr
 * already masked then bounded to 2000 chars, and that body's `command` is what
 * the refusal names.
 *
 * @param {string} url one of `API.*`, optionally with `?session=`
 * @param {AbortSignal} [signal]
 * @returns {Promise<Read>} never throws for a server error — it returns a
 *   `refused` Read carrying the named cause, so the caller renders `<Refusal>`
 *   instead of a blank surface
 */
export async function readProjection(url, signal, options) {
  /* `/api/refresh` is the one POST on this surface
   * (docs/UI-SERVER-CONTRACT.md:183); everything else is GET. The method is
   * derived from the address rather than passed at every call site, so a
   * caller cannot get it wrong. */
  const method = (options && options.method) || (url === API.refresh ? 'POST' : 'GET')
  let res
  let raw
  try {
    res = await fetch(url, {
      method,
      cache: 'no-store',
      signal,
      headers: { Accept: 'application/json' },
    })
    raw = await res.text()
  } catch (e) {
    if (e && e.name === 'AbortError') return { status: 'idle' }
    return {
      status: 'refused',
      refusal: url + ' did not answer — is `thruline ui` serving this page?',
    }
  }

  const exitHeader = res.headers && res.headers.get ? res.headers.get('X-ThruLine-Exit') : null
  const exit = exitHeader == null || exitHeader === '' ? undefined : Number(exitHeader)
  const projection =
    res.headers && res.headers.get ? res.headers.get('X-ThruLine-Projection') || undefined : undefined

  if (!res.ok) {
    /* A 502 carries the named cause. Anything else is named by its status —
     * never swallowed, never rendered as an empty surface (Rule 12). */
    let refusal = url + ' answered HTTP ' + res.status
    try {
      const body = JSON.parse(raw)
      if (body && body.error === 'projection_failed') {
        refusal =
          '`' +
          (body.command || url) +
          '` exited ' +
          body.exit_code +
          (body.stderr ? ' — ' + String(body.stderr).slice(0, 2000) : '')
      } else if (body && body.error) {
        refusal = url + ' refused: ' + body.error
      } else if (body && body.reason) {
        refusal = url + ' refused: ' + body.reason
      }
    } catch (_parseError) {
      /* a non-JSON error body adds nothing the status has not already said */
    }
    return { status: 'refused', refusal, exit, projection }
  }

  /* Exit 3 is a POLICY answer served as HTTP 200 and is a successful read
   * (docs/UI-SERVER-CONTRACT.md §2.3). It must never become a refusal. */
  let doc = null
  if (raw && raw.trim()) {
    try {
      doc = JSON.parse(raw)
    } catch (_jsonError) {
      return {
        status: 'refused',
        refusal: url + ' answered with something that is not JSON',
        exit,
        projection,
      }
    }
  }
  return { status: 'ready', doc, raw, exit, projection }
}

/**
 * Write one thing, because a person clicked once (T-2-7).
 *
 * The two first-use writes and the acceptance of what counts as done are the
 * only calls on this surface that are not reads, and each of them is one click
 * by a person on a control whose label says exactly what it will write. This
 * helper exists so they all take the same shape and the same refusal handling:
 * a failure NEVER reads as a success, and the sentence the caller shows carries
 * the server's own named reason.
 *
 * `Origin` is not set here — the browser sets it on a same-origin POST, and the
 * server requires it (`uiserve.rs`); a caller cannot forge or forget it.
 *
 * @param {string} url one of `API.firstUse*` or `SCOPE_ACCEPT(id)`
 * @param {object} body the JSON body, serialised here
 * @returns {Promise<Read>}
 */
export async function writeAction(url, body) {
  let res
  let raw
  try {
    res = await fetch(url, {
      method: 'POST',
      cache: 'no-store',
      headers: { Accept: 'application/json', 'Content-Type': 'application/json' },
      body: JSON.stringify(body || {}),
    })
    raw = await res.text()
  } catch (_networkError) {
    return { status: 'refused', refusal: url + ' did not answer — is `thruline ui` serving this page?' }
  }
  let doc = null
  if (raw && raw.trim()) {
    try {
      doc = JSON.parse(raw)
    } catch (_jsonError) {
      doc = null
    }
  }
  if (!res.ok) {
    let refusal = url + ' answered HTTP ' + res.status
    if (doc && doc.error) refusal = url + ' refused: ' + doc.error + (doc.detail ? ' — ' + String(doc.detail).slice(0, 2000) : '')
    return { status: 'refused', refusal }
  }
  if (doc == null) return { status: 'refused', refusal: url + ' answered with something that is not JSON' }
  return { status: 'ready', doc, raw }
}

/**
 * Read one §9 project fixture. Refuses a `diagnostics` export
 * (`mac/shell.jsx:221`) and returns the raw text alongside the parsed doc so
 * the caller can digest the source, not a re-serialization.
 *
 * @param {string} fixture the `projects[].fixture` id from the index
 * @param {AbortSignal} [signal]
 * @returns {Promise<Read>}
 */
export async function readProject(fixture, signal) {
  let res
  let raw
  try {
    res = await fetch(PROJECT(fixture), { cache: 'no-store', signal })
    raw = await res.text()
  } catch (e) {
    if (e && e.name === 'AbortError') return { status: 'idle' }
    return { status: 'refused', refusal: PROJECT(fixture) + ' could not be read' }
  }
  if (!res.ok) {
    return {
      status: 'refused',
      refusal: PROJECT(fixture) + ' answered HTTP ' + res.status + ' ' + (res.statusText || ''),
    }
  }
  let doc
  try {
    doc = JSON.parse(raw)
  } catch (_jsonError) {
    return { status: 'refused', refusal: PROJECT(fixture) + ' is not readable JSON' }
  }
  /* mac/shell.jsx:221 — a diagnostics export is not a product surface. */
  if (doc && doc.diagnostics) {
    return {
      status: 'refused',
      refusal:
        'diagnostics export — the product surface renders graduated classes only',
    }
  }
  return { status: 'ready', doc, raw }
}

/* THERE IS NO DATA HOOK HERE, deliberately.
 *
 * An earlier draft carried a `useWorkspaceData()` that returned
 * `{ status: 'idle' }` for everything. Nothing called it, and while it stood
 * it read as though the read layer existed. The workspace root owns its reads
 * in ONE place — `shell.jsx` `loadData()` — with the live flag and the epoch
 * counter this file's rules 4 and 6 require, and it calls the two functions
 * above. A second reader would be a second place for the same fact to differ.
 */

/**
 * What backs each of the fifteen rail destinations, and what it says when the
 * source is absent. This table IS the answer to spec-target Decision 4, and a
 * destination not listed here does not ship.
 *
 * `owner: 'os-shell'` means the destination renders the already-shipped
 * `os/shell.jsx` and the port MUST NOT rebuild that surface — two views over
 * one derivation are two places for the same fact to disagree, and the Inbox is
 * the app's ONE attention queue (doctrine 6).
 *
 * TODO(port): keep this table and `chrome.jsx`'s `<Rail>` in lockstep; a key in
 * `logic.RAIL` with no row here must render `<Refusal>` naming itself.
 */
export const DESTINATION_SOURCE = Object.freeze({
  inbox: { owner: 'os-shell', source: 'the §9 index, via inboxQueue', absent: null },
  /* The Sessions drawer reads the LISTING, not the adopted corpus. It used to
   * say `index.projects + /api/explain`, which was neither what it read
   * (`allSessions()`) nor what it could ever list. `absent` is the listing
   * coming back EMPTY — a store with nothing captured in it. A listing that was
   * REFUSED is a different fact and says so in its own words
   * (`state.SESSIONS_LEAD_REFUSED`); the two must not share a sentence. */
  sessions: { owner: 'workspace', source: '/api/sessions', absent: 'The session list came back empty — nothing has been captured into this store yet.' },
  trace: { owner: 'workspace', source: 'the adapted session + autoLayout', absent: 'This session has no trace to draw.' },
  claims: { owner: 'workspace', source: '/api/claims', absent: 'No claims projection was returned.' },
  asks: { owner: 'workspace', source: '/api/explain .promises (+ asks fixture)', absent: 'Asks are not exported over the server in this build.' },
  agreed: { owner: 'workspace', source: '/api/acceptance', absent: 'Agreed checks need `thruline ui --acceptance`.' },
  receipts: { owner: 'workspace', source: 'a pinned receipt fixture', absent: 'Receipts are not served over HTTP in this build.' },
  bindings: { owner: 'workspace', source: '/api/claims .claims[].binding', absent: 'No claims projection was returned.' },
  ledger: { owner: 'workspace', source: '§9 projects/<id>.json .ledger', absent: 'No project is open.' },
  explorer: { owner: 'workspace', source: 'the §9 index', absent: 'No records have been read on this machine yet.' },
  compare: { owner: 'workspace', source: 'two adapted sessions', absent: 'Comparing needs two readings.' },
  capture: { owner: 'workspace', source: '/api/lanes + /api/state', absent: 'Capture status needs the ThruLine app serving this page.' },
  policy: { owner: 'workspace', source: 'receipt .predicate.verifier.policy_hash', absent: 'Policy is not served over HTTP in this build.' },
  handoff: { owner: 'workspace', source: 'none — a copyable command only', absent: null },
  diag: { owner: 'workspace', source: '/api/state + X-ThruLine-Exit headers', absent: 'Diagnostics need the ThruLine app serving this page.' },
})

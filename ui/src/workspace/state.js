/* ThruLine workspace — the reading's state, as pure decisions.
 *
 * PURE. No JSX, no React, no `window`, no fetch. Everything here is a function
 * of the values `shell.jsx` already holds, so the two decisions that were
 * previously made inline — WHICH SURFACE the reader is looking at, and WHICH
 * SESSION opens — can be pinned by `./state.test.js` without a browser.
 *
 * WHY THIS FILE EXISTS. `shell.jsx` used to compute, in `render()`:
 *
 *     const ready = st.dataStatus === 'ready'
 *     const refusal = st.dataRefusal || (ready ? null : { title: 'This reading
 *       is not open yet.', missing: 'no session projection has been read',
 *       remedy: 'thruline export --all' })
 *
 * — one refusal object for EVERY non-ready state. While the app was reading the
 * reader's own data it told them the data was missing and to run an export.
 * Loading and failure are never the same branch anywhere else in this product
 * (`os/shell.jsx:522-525`, `mac/shell.jsx:628-631`, `acceptance.jsx:39` all test
 * the error before the in-flight case), and this file is where that rule now
 * lives for the workspace.
 *
 * THE FOUR FACTS, kept four (Rule 12):
 *   a read is in flight · the read succeeded and the record holds nothing ·
 *   the read failed or the shape is unknown · the record holds something.
 * An honest emptiness is never dressed as a failure, and a failure is never
 * dressed as an emptiness.
 */

/** The product's word for in-flight is "Reading" — `os/shell.jsx:525`,
 *  `mac/shell.jsx:631`, `acceptance.jsx:41`. Never "Loading", never "Working on
 *  it", never "Please wait": those strings appear nowhere in `ui/src` and this
 *  file does not add them. */
import { HQ_COPY } from './hq-copy.js'

export const READING_LINE = 'Reading the record…'

/**
 * The second waiting line, shown once a read has been in flight long enough for
 * the reader to wonder. It names the address being read, and it says the thing
 * a spinner cannot: that nothing has failed.
 *
 * No percentage, no ETA, no elapsed count — `lib/refresh-presentation.test.js:75`
 * forbids all three in progress copy and this inherits the ban. Progress belongs
 * to the terminal (`refresh-presentation.js:10`).
 *
 * @param {string} endpoint the address actually being read
 * @returns {string}
 */
export function stillReading(endpoint) {
  return `Still reading ${endpoint} — nothing has failed.`
}

/**
 * The third waiting line. Says the page has not given up and will speak if the
 * read fails, which is the only promise it can honestly make.
 *
 * @param {string} endpoint
 * @returns {string}
 */
export function nothingBackYet(endpoint) {
  return `Nothing has come back from ${endpoint} yet. This page keeps waiting; it has not given up, and it will say so if the read fails.`
}

/** The two moments the waiting line escalates, in ms. They are page timings,
 *  not measurements of the read, and they carry no number into the copy. */
export const WAITING_STEPS = Object.freeze({ named: 2500, patient: 10000 })

/**
 * Does this session's record hold anything at all?
 *
 * The emptiness test is computed ONLY from what was read — never from an id,
 * never from a project row. Over HTTP "this session does not exist" and "this
 * session is empty" are the same answer (`/api/explain?session=<anything>`
 * returns HTTP 200 with an all-empty document), so the app may state what the
 * returned document holds and nothing beyond it.
 *
 * The fields are the adapted counterparts of the explain document's
 * `what_happened` / `claims_vs_proof` / `promises` / `what_it_forgot.compactions`
 * / `why_it_stopped.last_events`. `risk_flags` are folded onto their event by
 * `adapter.js:401-409`, so an event-bearing session is caught by `events`.
 *
 * NOT TESTED HERE, and said out loud rather than quietly skipped:
 * `what_it_forgot.lost` is not carried onto the adapted session by
 * `adapter.js` (only `compactions` is, at :419), so a record whose ONLY content
 * were a `lost` entry would read as empty. That is an adapter gap, not a
 * decision this function may fake.
 *
 * `nodes` is deliberately NOT part of the test: the adapter synthesises a single
 * `ending` node for a session with zero events, so `nodes.length` is 1 for the
 * emptiest record in the store and would answer "yes" to every session.
 *
 * @param {object|null|undefined} S an adapted session
 * @returns {boolean}
 */
export function recordHoldsSomething(S) {
  if (!S) return false
  const n = (v) => (Array.isArray(v) ? v.length : 0)
  return (
    n(S.events) > 0 ||
    n(S.claims) > 0 ||
    n(S.promises) > 0 ||
    n(S.checks) > 0 ||
    n(S.asks) > 0 ||
    n(S.testChanges) > 0 ||
    (Number(S.compactions) || 0) > 0 ||
    n(S.stop && S.stop.lastEvents) > 0
  )
}

/**
 * THE DEFAULT-SESSION RULE.
 *
 * The opening session is chosen by what the record holds, in this order: the
 * first session with recorded events, else the first with a completion claim,
 * else the first the reading returned. Before this, the workspace opened
 * `corpus.order[0]` unconditionally, and on a live store whose default session
 * is chosen by `ORDER BY id` that is the one session in 44,075 with zero events.
 *
 * Empty sessions are NOT skipped out of existence — the chosen key is still a
 * key in `order`, every reading stays reachable, and when nothing holds
 * anything this says so through `reason` rather than opening one and hoping.
 * A session that holds nothing is a fact the reader may need.
 *
 * @param {{order?: string[], sessions?: object}|null|undefined} corpus
 * @returns {{key: string, reason: 'events'|'claims'|'nothing-held'|'no-readings'}}
 *   `reason` is the branch that actually ran, so a caller can never report a
 *   choice this function did not make (the discipline of
 *   `refresh-presentation.js:89-92`).
 */
export function chooseSession(corpus) {
  const order = corpus && Array.isArray(corpus.order) ? corpus.order : []
  const sessions = (corpus && corpus.sessions) || {}
  if (!order.length) return { key: '', reason: 'no-readings' }

  const withEvents = order.find((k) => sessions[k] && Array.isArray(sessions[k].events) && sessions[k].events.length > 0)
  if (withEvents) return { key: withEvents, reason: 'events' }

  const withClaims = order.find((k) => sessions[k] && Array.isArray(sessions[k].claims) && sessions[k].claims.length > 0)
  if (withClaims) return { key: withClaims, reason: 'claims' }

  return { key: order[0], reason: 'nothing-held' }
}

/**
 * Does ANY reading open in this workspace hold something?
 *
 * @param {{order?: string[], sessions?: object}|null|undefined} corpus
 * @returns {boolean}
 */
export function anyReadingHolds(corpus) {
  const order = corpus && Array.isArray(corpus.order) ? corpus.order : []
  const sessions = (corpus && corpus.sessions) || {}
  return order.some((k) => recordHoldsSomething(sessions[k]))
}

/* ── The session listing ─────────────────────────────────────────────────── */

/* THE LISTING IS ADDRESSES, NOT READINGS, and everything below keeps that line.
 *
 * `GET /api/sessions` serves the bytes of `thruline sessions --json`
 * (`crates/cli/src/uiserve.rs:656-670`), whose document is
 * `crates/cli/src/sessions_cmd.rs:52-63` inside the registry envelope
 * (`crates/cli/src/json_out.rs:136`, `:216-222`):
 *
 *   { schema_version: 1, shape: 'thruline.sessions', store, total, returned,
 *     limit, sessions: [ { id, repo_key, repo_root, harness, event_count,
 *       user_prompts, stop_events, file_change_events, final_messages,
 *       first_event_at, last_event_at, head_hash, capture_degraded } ] }
 *
 * A row carries counts and an address. It carries NO state word, and this file
 * never derives one for it: a session's word comes from the adapter reading
 * that session's claims, asks and promises (`logic.sessionWordFrom`), so a word
 * on an unopened row would be invented — and a negative word with no clause is
 * a doctrine-3 violation on top of that. Rows show what the listing proves.
 */

/** The shape id and version this build knows how to read. Anything else is
 *  refused by name rather than rendered as a guess (Rule 12). */
export const SESSIONS_SHAPE = 'thruline.sessions'
export const SESSIONS_VERSION = 2

/**
 * The server's own `session` parameter grammar, mirrored so the app never
 * sends a request it already knows will be refused.
 *
 * `crates/cli/src/uiserve.rs:1387-1394`: non-empty, at most 128 characters,
 * `[A-Za-z0-9._-]` only, not `.`, and never leading `-`. A listing row whose id
 * fails this is UNOPENABLE — it is still listed, because hiding a row the store
 * holds would be a quieter lie than showing one that cannot be opened.
 *
 * @param {*} id
 * @returns {boolean}
 */
export function isOpenableSessionId(id) {
  if (typeof id !== 'string') return false
  if (!id || id.length > 128 || id === '.' || id.startsWith('-')) return false
  return /^[A-Za-z0-9._-]+$/.test(id)
}

/**
 * The session id an address names, or null.
 *
 * Takes the query string rather than reading `window`, so the parse is pinnable
 * without a browser. Returns null for anything the server would refuse, which
 * is the app declining to send a 400 rather than a duplicate of the server's
 * rule.
 *
 * @param {string} search `window.location.search`, with or without its `?`
 * @returns {string|null}
 */
export function sessionIdFromSearch(search) {
  const raw = String(search == null ? '' : search)
  const q = raw.charAt(0) === '?' ? raw.slice(1) : raw
  let value = null
  try {
    value = new URLSearchParams(q).get('session')
  } catch (_addressError) {
    return null
  }
  return isOpenableSessionId(value) ? value : null
}

/** A count the listing actually carried, or null. `0` is a count; `undefined`,
 *  `null` and a non-number are the absence of one, and the two must never
 *  render the same way. */
function countOf(v) {
  return typeof v === 'number' && Number.isFinite(v) && v >= 0 ? v : null
}

/**
 * One listing row, in this app's words.
 *
 * `repo` prefers `repo_root` (the real path, present on 2 of 43 repos) over
 * `repo_key` (the lossy dash form). Neither is un-mangled here — a guessed path
 * is a fabricated receipt (`crates/ledger/src/schema.rs:45-53`).
 *
 * @param {object} raw
 * @returns {object|null} null when the row carries no id, because a row with no
 *   address is not an address
 */
function listingRow(raw) {
  if (!raw || typeof raw !== 'object') return null
  const id = typeof raw.id === 'string' ? raw.id : ''
  if (!id) return null
  const root = typeof raw.repo_root === 'string' && raw.repo_root ? raw.repo_root : ''
  const key = typeof raw.repo_key === 'string' && raw.repo_key ? raw.repo_key : ''
  return {
    id,
    repo: root || key,
    harness: typeof raw.harness === 'string' ? raw.harness : '',
    events: countOf(raw.event_count),
    prompts: countOf(raw.user_prompts),
    stops: countOf(raw.stop_events),
    fileChanges: countOf(raw.file_change_events),
    finalMessages: countOf(raw.final_messages),
    lastAt: typeof raw.last_event_at === 'string' && raw.last_event_at ? raw.last_event_at : null,
    headHash: typeof raw.head_hash === 'string' && raw.head_hash ? raw.head_hash : null,
    degraded: raw.capture_degraded === true,
    openable: isOpenableSessionId(id),
  }
}

/**
 * Read the listing document.
 *
 * REFUSES LOUDLY on an unknown shape or version, the same discipline every
 * other read on this surface keeps: a document this build does not know how to
 * read is named, never partially rendered. The refusal is NOT fatal to the
 * workspace — the caller keeps the reading it opened and the picker says why it
 * is empty — but it is never silent.
 *
 * Rows are kept in the order the engine returned them. The engine ranks by what
 * the store carries (`crates/ledger/src/schema.rs:1294-1297`: a `stop` first,
 * then real recency, then fold order); re-sorting that here would replace a
 * measured ranking with a guess.
 *
 * @param {any} doc the parsed `/api/sessions` body
 * @returns {{rows: object[], total: number|null, returned: number|null,
 *   limit: number|null, store: string|null, dropped: number,
 *   refusal: {title: string, missing: string, remedy: string}|null}}
 */
export function listingFromDocument(doc) {
  const no = (missing) => ({
    rows: [],
    total: null,
    returned: null,
    limit: null,
    store: null,
    dropped: 0,
    refusal: {
      title: 'The session list could not be read.',
      missing,
      remedy: 'thruline sessions',
    },
  })
  if (!doc || typeof doc !== 'object' || Array.isArray(doc)) {
    return no('the session listing answered with something that is not a document')
  }
  if (doc.shape !== SESSIONS_SHAPE) {
    return no(
      `the session listing named its shape ${JSON.stringify(String(doc.shape == null ? 'nothing' : doc.shape))}, and this build reads ${JSON.stringify(SESSIONS_SHAPE)}`,
    )
  }
  if (doc.schema_version !== 1 && doc.schema_version !== SESSIONS_VERSION) {
    return no(
      `the session listing is version ${JSON.stringify(String(doc.schema_version == null ? 'nothing' : doc.schema_version))}, and this build reads versions 1 and ${SESSIONS_VERSION}`,
    )
  }
  if (doc.schema_version === 2 && !['store', 'project'].includes(doc.limit_scope)) {
    return no('version 2 of the session listing must name its limit_scope as store or project')
  }
  if (!Array.isArray(doc.sessions)) {
    return no('the session listing carries no `sessions` array')
  }
  const rows = []
  let dropped = 0
  doc.sessions.forEach((raw) => {
    const row = listingRow(raw)
    if (row) rows.push(row)
    else dropped += 1
  })
  return {
    rows,
    total: countOf(doc.total),
    returned: countOf(doc.returned),
    limit: countOf(doc.limit),
    store: typeof doc.store === 'string' && doc.store ? doc.store : null,
    dropped,
    refusal: null,
  }
}

/**
 * Is this id one the listing actually returned?
 *
 * THE ONLY PLACE AN ADDRESS CAN BE CHECKED. `/api/explain?session=<anything>`
 * answers HTTP 200 with an all-empty document for any syntactically valid id
 * (`crates/cli/src/explain_cmd.rs:63`, `crates/cli/src/uiserve.rs:1324-1330`),
 * so over HTTP "no such session" and "an empty session" are the same answer.
 * The app may only say the listing does not name it — which is what
 * `DEEP_LINK_ABSENT` says, and no more.
 *
 * @param {object[]|null|undefined} rows
 * @param {string|null|undefined} id
 * @returns {boolean}
 */
export function listingHasSession(rows, id) {
  if (!id || !Array.isArray(rows)) return false
  return rows.some((r) => r && r.id === id)
}

/**
 * Narrow the listing by id or repository, keeping the engine's order.
 *
 * Matching is case-insensitive substring over exactly the two things a row
 * shows as an address. An empty query is not a filter.
 *
 * @param {object[]|null|undefined} rows
 * @param {string} query
 * @returns {object[]}
 */
export function filterListing(rows, query) {
  const list = Array.isArray(rows) ? rows.filter(Boolean) : []
  const q = String(query == null ? '' : query).trim().toLowerCase()
  if (!q) return list
  return list.filter((r) => String(r.id).toLowerCase().includes(q) || String(r.repo || '').toLowerCase().includes(q))
}

/** Project identities come from the session index, not just opened traces.
 * Cached sessions keep the current project reachable if the index is absent.
 * Keep full identities: equal folder names do not establish equal projects. */
export function projectListing(rows, corpus, query = '') {
  const groups = new Map()
  const add = row => {
    if (!row?.repo || !row.id) return
    if (!groups.has(row.repo)) groups.set(row.repo, new Map())
    if (!groups.get(row.repo).has(row.id)) groups.get(row.repo).set(row.id, row)
  }
  for (const row of rows || []) add(row)
  for (const key of corpus?.order || []) {
    const session = corpus.sessions[key]
    if (session) add({ id: key, repo: session.project, openable: true, events: session.eventCount })
  }
  const normalize = value => String(value).toLowerCase().replace(/[^\p{L}\p{N}]/gu, '')
  const search = normalize(query.trim())
  return [...groups].filter(([repo]) => !search || normalize(repo).includes(search)).map(([repo, sessions]) => ({
    repo, count: sessions.size, session: chooseListedSession([...sessions.values()]).id,
  })).sort((a, b) => a.repo.localeCompare(b.repo))
}

/**
 * THE OPENING SESSION, decided from the LISTING.
 *
 * `chooseSession` decides from ADAPTED sessions — it reads `events.length` and
 * `claims.length` — and cannot be used here: a listing row has counts, not
 * arrays, and adapting forty-four thousand sessions to pick one is not a thing
 * this app may do. So the two functions stay separate and each says which
 * branch it took.
 *
 * The tiers are what the listing can actually prove, worst-checkable-first:
 *
 *  1. a `stop` was recorded AND the tree was touched — a completion claim is
 *     extractable and there is work to check it against;
 *  2. a `stop` was recorded — a completion claim is extractable;
 *  3. events were recorded;
 *  4. the first row the engine returned.
 *
 * WITHIN A TIER THE ENGINE'S ORDER DECIDES, and that is a deliberate departure
 * from the draft spec's "more events first": the engine now ranks by real
 * recency (`crates/ledger/src/schema.rs:1294-1297`), and re-sorting a measured
 * ranking by a proxy would be this app inventing an order it did not measure.
 *
 * Only openable ids are candidates — offering one the server's parameter
 * grammar refuses would be a row that 400s on click.
 *
 * @param {object[]|null|undefined} rows normalised listing rows
 * @returns {{id: string, reason: 'stopped-with-changes'|'stopped'|'events'|'first-listed'|'none-openable'|'no-listing'}}
 */
export function chooseListedSession(rows) {
  const list = Array.isArray(rows) ? rows.filter(Boolean) : []
  if (!list.length) return { id: '', reason: 'no-listing' }
  const open = list.filter((r) => r.openable)
  if (!open.length) return { id: '', reason: 'none-openable' }

  const stopped = open.filter((r) => (r.stops || 0) > 0)
  const withChanges = stopped.find((r) => (r.fileChanges || 0) > 0)
  if (withChanges) return { id: withChanges.id, reason: 'stopped-with-changes' }
  if (stopped.length) return { id: stopped[0].id, reason: 'stopped' }

  const withEvents = open.find((r) => (r.events || 0) > 0)
  if (withEvents) return { id: withEvents.id, reason: 'events' }

  return { id: open[0].id, reason: 'first-listed' }
}

/* ── The centre's state ──────────────────────────────────────────────────── */

/**
 * @typedef {object} CentreState
 * @property {'reading'|'empty'|'refused'|'ready'} kind which surface to draw
 * @property {string} phase the `dataStatus` this decision was made from, echoed
 *   back so the view can never claim a branch it did not take
 * @property {object|null} refusal `{title, missing, remedy?, detail?}` when
 *   `kind === 'refused'`, else null
 */

/**
 * Which of the four surfaces the centre column is showing.
 *
 * PRECEDENCE, and it is the shipped one: the refusal is tested FIRST, exactly as
 * `os/shell.jsx:522-525` tests `error` before `!doc`. A read that failed says so
 * even if a later status would have said something calmer.
 *
 * `'idle'` folds into `'reading'` deliberately, and this is the reason rather
 * than an oversight: `dataStatus` is `'idle'` only between the constructor and
 * `componentDidMount`, which React runs in the same tick as the first commit,
 * and `loadData()`'s first act is to set `'loading'`. There is no frame in which
 * "a read is being started" is on screen while no read is being started.
 * `readProjection` also returns `{status:'idle'}` on an abort, and an abort is
 * always followed by the newer read that caused it (the epoch discipline in
 * `data.js` rule 4), so it too is a read in flight.
 *
 * An UNRECOGNISED status is not silent (Rule 12): it becomes a refusal that
 * names itself, because a status this build has no rendering for is a defect in
 * this build and the reader is entitled to know which one.
 *
 * @param {object} args
 * @param {string} args.dataStatus `'idle'|'loading'|'ready'|'refused'`
 * @param {object|null} [args.dataRefusal] the NAMED refusal composed at the read
 *   site. There is no manufactured fallback here on purpose — a refusal with no
 *   named cause is the thing this file was written to delete.
 * @param {object|null} [args.session] the adapted session, when one is open
 * @returns {CentreState}
 */
export function centreState({ dataStatus, dataRefusal, session }) {
  const phase = String(dataStatus)
  if (dataRefusal) return { kind: 'refused', phase, refusal: dataRefusal }
  if (phase === 'loading' || phase === 'idle') return { kind: 'reading', phase, refusal: null }
  if (phase === 'ready') {
    return recordHoldsSomething(session)
      ? { kind: 'ready', phase, refusal: null }
      : { kind: 'empty', phase, refusal: null }
  }
  return {
    kind: 'refused',
    phase,
    refusal: {
      title: 'This reading is in a state this app has no screen for.',
      missing: `the read reported status "${phase}", which this build does not render`,
      remedy: 'thruline ui',
    },
  }
}

/* ── What an emptiness says ──────────────────────────────────────────────── */

/**
 * The absent noun, named once. It is a list of what the record does not hold,
 * not a claim that the data is missing: the read succeeded, the index loaded,
 * and this session simply holds none of these things.
 *
 * IT NAMES ONLY WHAT WAS READ, and that boundary is the whole point. This
 * sentence used to assert that "no claims … and no agreed checks are in this
 * session's record" — two absences about documents this app never opened.
 * `shell.jsx`'s `loadData` reads `/api/explain` and nothing else; it hands the
 * adapter `claims: null` (`shell.jsx:665`), and `adapter.js:491` fills
 * `S.claims` from that null document, so `S.claims` is `[]` because it was NOT
 * READ. Agreed checks are worse: `adapter.js:555` fills `S.checks` from
 * `payload.predicate.checks`, and the explain document has no `predicate` block
 * at all, so they are not part of this reading by construction. Only recorded
 * events (`what_happened`) and promises (`explain.promises`, `adapter.js:534`)
 * are genuinely read-and-empty. "Nothing was recorded" and "nothing was read"
 * are the two facts this file exists to keep apart, and this sentence is inside
 * that rule, not outside it (`canvas.jsx:22-32`, `ui-kit.jsx:482-485`).
 */
const SESSION_ABSENT =
  'no recorded events and no promises are in the explain document this reading returned — and claims and agreed checks are not in that document at all, so this reading says nothing about them'

/** The same sentence when nothing else open holds anything either. */
const SESSION_ABSENT_ALL =
  'no recorded events and no promises are in the explain document this reading returned, and no other reading open here holds any either — claims and agreed checks are not in that document at all, so this reading says nothing about them'

/**
 * What would change it. A remedy is a route a human can actually take, and both
 * halves of the previous sentence failed that test:
 *
 *  · "open a different session from the Inbox" — the Inbox is `os/shell.jsx`,
 *    mounted as `<OsShell index={…} />` with no callback into this workspace,
 *    so opening a row there switches that shell's own tab and never reaches
 *    Trace. The sentence also conceded its own category error mid-clause: open
 *    a SESSION from a list of PROJECTS. Both faults stand; the Inbox is still
 *    not the door, and this sentence still does not send anyone there.
 *  · "what the recorder saw for this one" — `thruline doctor` takes no
 *    `--session` (`crates/cli/src/main.rs:797-807`) and reports a store-wide
 *    tally (`crates/cli/src/doctor.rs:83-100`). The command is real and
 *    read-only; the per-session scope was not.
 *
 * AND ONE CLAUSE OF THIS SENTENCE HAS NOW BEEN REPLACED IN TURN. It read
 * "…nothing on this screen can open another", which was the honest record of a
 * real defect: the live path adopted a one-session corpus, so the switcher held
 * the session already open and nothing else. The workspace now reads
 * `/api/sessions`, the header button lists what came back, and any listed
 * session opens here — so that clause became false the moment the picker
 * landed, and a stale honest sentence is still a false one. It is replaced in
 * the same change that made it false, and `state.test.js` was re-pinned with
 * it: the test that forbade "open a different session" was a tripwire on
 * exactly this, and it now asserts the sentence names the door that exists.
 *
 * It still deliberately does NOT offer `thruline export --all` or
 * `thruline ingest`. An export already exists on a machine that got this far —
 * the index loaded, or this screen would be a refusal — and telling a reader to
 * rebuild it cannot make an empty session non-empty. A remedy that cannot work
 * is a false claim about the machine.
 */
/* T-6-6: the first half is `HQ_COPY.empty.trace.fill` — the one thing that
 * fills an empty Trace — and the second the pointer to the command below it.
 * Both now live in the copy file, and "events" became "steps": the word the
 * Trace's own Plain face uses (design record §5). */
const SESSION_CHANGE = HQ_COPY.empty.trace.fill + ' ' + HQ_COPY.empty.trace.doctor
const SESSION_REMEDY = 'thruline doctor'

/* ── What the listing says about itself ──────────────────────────────────── */

/**
 * The Sessions drawer's lead.
 *
 * It replaces "Every session this reading has opened, newest first.", which was
 * true of a drawer listing `allSessions()` and is false of one listing the
 * store: it is not what this reading opened, and nothing here is ordered by
 * time — `sessions.started_at` and `ended_at` are NULL on every row of a real
 * store (`crates/ledger/src/schema.rs:36-41`), which is why the engine ranks by
 * a recorded `stop`, then by the newest recorded EVENT, and why this sentence
 * says "recorded" rather than "newest".
 *
 * It also says the list is bounded, because it is: the server spawns
 * `thruline sessions --json` with no arguments and the default limit is fifty
 * rows (`crates/cli/src/main.rs:277-278`, `crates/cli/src/uiserve.rs:669`),
 * over a store holding many times that. A bounded list that does not say so
 * implies it showed everything.
 */
export const SESSIONS_LEAD =
  'The sessions this machine returned, ranked as the record ranks them: the ones whose work can be checked come first. This list is bounded, so it is not everything the store holds.'

/** The same drawer when the listing itself was refused — the workspace kept the
 *  session it opened, so this is an absence in one panel, not a dead app. */
export const SESSIONS_LEAD_REFUSED =
  'The session list could not be read, so this panel cannot say what the store holds. The session already open is unaffected.'

/**
 * A deep link naming a session the listing does not contain.
 *
 * THE LIMIT OF WHAT THIS APP KNOWS IS PART OF THE COPY. It cannot say the
 * session does not exist: the server answers HTTP 200 with an all-empty
 * document for any syntactically valid id, so "no such session" and "an empty
 * session" arrive identically (`recordHoldsSomething`'s docblock, above). All
 * it knows is that the listing it read does not name the id — and because the
 * listing is bounded, not naming it is not the same as not holding it. The
 * sentence says exactly that and stops.
 *
 * The app does NOT open the id anyway. Opening it would draw an empty screen
 * indistinguishable from a real empty session, which is the defect this whole
 * change exists against, with an extra step.
 */
export const DEEP_LINK_ABSENT = Object.freeze({
  title: 'The session in this link is not on the list this machine returned.',
  absent:
    'the address named a session id, and the session list this page read does not contain it — the list is bounded, so this page cannot say whether the record holds that session or not',
  change:
    'What would change it: an address naming a session the list returned, or the header’s session button, which opens that list. This page opened the session the list ranked first instead. To see what the recorder holds on this machine, run',
  remedy: 'thruline doctor',
})

/** One title per surface, because each names what IT cannot draw. Every one is
 *  a full sentence with a verb and a stop (F-6), and every one names the fact
 *  that the reading SUCCEEDED — `tree` was the one surface that did not, which
 *  made the left dock the only place a reader could not tell an empty record
 *  from an unread one.
 *
 *  There is no `inspector` or `dock` key. Both were written and neither was
 *  ever rendered: `Inspector` composes its own `<Absence>` for the different
 *  fact that nothing is SELECTED (`docks.jsx:848-852`), and `BottomDock` is
 *  passed no `absence` at all, its empty states being the shipped per-pane
 *  notes. Tests pinned both as if they shipped; a string no surface renders is
 *  not copy, it is a claim about a screen that does not exist (Rule 12). */
const SESSION_TITLE = Object.freeze({
  /* T-6-6: `HQ_COPY.empty.trace.why`, read from the copy file. */
  graph: HQ_COPY.empty.trace.why,
  timeline: 'This session was read, and its record holds no recorded events to place on a line.',
  table: 'This session was read, and its record holds no objects to list.',
  tree: 'This session was read, and its record holds no objects to list.',
})

/**
 * The `<Absence>` props for a session that was read and holds nothing.
 *
 * This is the operator's cell. It must say three things and no more: the
 * reading HAPPENED, what it does not hold, and what would change it. It must
 * not say the data is missing (it is not), must not show a zero (doctrine 4),
 * and must not be painted as a fault (an emptiness is not a failure).
 *
 * @param {'graph'|'timeline'|'table'|'tree'} surface
 * @param {{readings?: number, anyHeld?: boolean}} [corpus] what else is open
 * @returns {{title: string, absent: string, change: string, remedy: string}}
 */
export function sessionAbsence(surface, corpus) {
  const readings = (corpus && Number(corpus.readings)) || 0
  const noneHold = readings > 1 && corpus && corpus.anyHeld === false
  return {
    title: SESSION_TITLE[surface] || SESSION_TITLE.graph,
    absent: noneHold ? SESSION_ABSENT_ALL : SESSION_ABSENT,
    change: SESSION_CHANGE,
    remedy: SESSION_REMEDY,
  }
}

/* ── Where this reading came from ────────────────────────────────────────── */

/**
 * The connection line, as a THREE-state read and never a boolean.
 *
 * `live` is set only once a read has RESOLVED, so the boolean form
 * (`live ? 'served by the local app' : 'no local server in this reading'`)
 * stated a negative fact for the whole duration of every read: the footer said
 * "no local server in this reading" while the app was at that moment reading
 * from the local server, beside a status bar reading "reading /api/explain…".
 * That is the same conflation of "not ready" with "a negative finding" that
 * `centreState` exists to end, and it is ended here for the two surfaces that
 * render this line — the status strip and the Operator diagnostics drawer.
 *
 * "No local server" is said ONLY once a read has actually finished without one.
 *
 * @param {object} args
 * @param {boolean} args.live whether a local server answered this reading
 * @param {string} args.dataStatus `'idle'|'loading'|'ready'|'refused'`
 * @param {boolean} [args.isDemo] whether the bundled corpus is what rendered
 * @param {boolean} [args.long] append the egress guarantee
 * @returns {string}
 */
export function connectionLine({ live, dataStatus, isDemo, long }) {
  const phase = String(dataStatus)
  const inFlight = phase === 'loading' || phase === 'idle'
  const where = live
    ? 'served by the local app'
    : inFlight
      ? 'reading from the local app…'
      : isDemo
        ? 'fixture mode — no local server in this reading'
        : 'no local server in this reading'
  return long ? where + ' · loopback only' : where
}

/**
 * The status strip's four slots for a state that is not `ready`.
 *
 * The strip is the one surface that renders in every state, so it is where the
 * reader learns which state they are in. `scope` is BLANK while reading and
 * while empty: `0 of 0 events` is a naked zero and a fabricated denominator,
 * and the emptiness is stated once, by the pane (doctrine 4).
 *
 * @param {CentreState} centre
 * @param {string} endpoint the address being read
 * @returns {{origin: string, scope: string, read: string}|null} null when the
 *   root's own composed strings should stand
 */
export function statusFor(centre, endpoint) {
  if (!centre) return null
  if (centre.kind === 'reading') return { origin: `reading ${endpoint}…`, scope: '', read: READING_LINE }
  if (centre.kind === 'empty') return { origin: null, scope: '', read: null }
  /* The SAME words the centre's refusal uses ("This reading could not be
   * opened.", `shell.jsx`'s `loadData`). It read "could not be read" here — a
   * second phrasing of one fact, on one screen, three inches apart. */
  if (centre.kind === 'refused') return { origin: 'this reading could not be opened', scope: '', read: null }
  return null
}

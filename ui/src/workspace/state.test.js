import test from 'node:test'
import assert from 'node:assert/strict'
import { bannedIn } from '../lib/voice.js'
import {
  DEEP_LINK_ABSENT,
  READING_LINE,
  SESSIONS_LEAD,
  SESSIONS_LEAD_REFUSED,
  anyReadingHolds,
  centreState,
  chooseListedSession,
  chooseSession,
  connectionLine,
  filterListing,
  isOpenableSessionId,
  listingFromDocument,
  listingHasSession,
  projectListing,
  nothingBackYet,
  recordHoldsSomething,
  sessionAbsence,
  sessionIdFromSearch,
  statusFor,
  stillReading,
} from './state.js'

/* The workspace's two pure decisions, pinned: WHICH SURFACE the reader is
 * looking at, and WHICH SESSION opens.
 *
 * Both were previously made inline in `shell.jsx`'s `render()` and `adopt()`,
 * and both were wrong in a way no test could see: every non-ready state
 * produced one identical refusal telling the reader their data was missing
 * while it was being read, and the opening session was `order[0]` regardless of
 * whether it held anything. Tests verify intent, not implementation trivia
 * (Rule 9) — every assertion below is a doctrine rule.
 */

/* ── helpers ─────────────────────────────────────────────────────────────── */

const empty = (key) => ({ key, events: [], claims: [], promises: [], checks: [], asks: [], testChanges: [], compactions: 0, stop: {} })
const withEvents = (key, n) => ({ ...empty(key), events: Array.from({ length: n }, (_, i) => ({ seq: i })) })
const withClaims = (key, n) => ({ ...empty(key), claims: Array.from({ length: n }, (_, i) => ({ id: `c${i}` })) })

const corpus = (list) => ({ order: list.map((S) => S.key), sessions: Object.fromEntries(list.map((S) => [S.key, S])) })

/* ── the state selection ─────────────────────────────────────────────────── */

test('a read in flight is never a refusal', () => {
  // The bug, stated as a test. `shell.jsx:4210-4215` gave every non-ready
  // status the same refusal object — title "This reading is not open yet.",
  // remedy `thruline export --all` — so the app told a reader their data was
  // missing, and to rebuild it, while it was fetching it.
  const c = centreState({ dataStatus: 'loading', dataRefusal: null, session: null })
  assert.equal(c.kind, 'reading')
  assert.equal(c.refusal, null)
})

test('a real refusal is still a refusal, and it wins over every other status', () => {
  // Loading and failure are never the same branch, and the failure is tested
  // FIRST — `os/shell.jsx:522-525`, `mac/shell.jsx:628-631`, `acceptance.jsx:39`.
  const named = { title: 'This reading could not be opened.', missing: '/api/explain did not answer', remedy: 'thruline ui' }
  for (const dataStatus of ['idle', 'loading', 'ready', 'refused']) {
    const c = centreState({ dataStatus, dataRefusal: named, session: withEvents('a', 3) })
    assert.equal(c.kind, 'refused')
    assert.equal(c.refusal, named)
  }
})

test('nothing manufactures a refusal that the read site did not name', () => {
  // The deleted fallback. A refusal must name a document, an endpoint or a
  // shape (`ui-kit.jsx:298-300`); a refusal invented by the view names nothing.
  const c = centreState({ dataStatus: 'ready', dataRefusal: null, session: empty('a') })
  assert.notEqual(c.kind, 'refused')
  assert.equal(c.refusal, null)
})

test('a record that was read and holds nothing is EMPTY, not refused and not loading', () => {
  const c = centreState({ dataStatus: 'ready', dataRefusal: null, session: empty('a') })
  assert.equal(c.kind, 'empty')
})

test('a record that holds something is ready', () => {
  assert.equal(centreState({ dataStatus: 'ready', session: withEvents('a', 1) }).kind, 'ready')
  assert.equal(centreState({ dataStatus: 'ready', session: withClaims('a', 1) }).kind, 'ready')
  assert.equal(centreState({ dataStatus: 'ready', session: { ...empty('a'), compactions: 2 } }).kind, 'ready')
  assert.equal(centreState({ dataStatus: 'ready', session: { ...empty('a'), stop: { lastEvents: [{ seq: 1 }] } } }).kind, 'ready')
})

test('idle is folded into the working state deliberately, and never into a refusal', () => {
  assert.equal(centreState({ dataStatus: 'idle', session: null }).kind, 'reading')
})

test('a status this build has no screen for says so, and names itself', () => {
  // Rule 12: silence is not an option. The refusal that results must name the
  // actual cause, not a generic failure.
  const c = centreState({ dataStatus: 'wat', session: null })
  assert.equal(c.kind, 'refused')
  assert.match(c.refusal.missing, /"wat"/)
})

test('every branch reports the phase it actually rendered', () => {
  // `refresh-presentation.js:89-92`'s discipline: the view can never claim a
  // branch it did not take.
  for (const dataStatus of ['idle', 'loading', 'ready', 'refused', 'wat']) {
    assert.equal(centreState({ dataStatus, session: empty('a') }).phase, dataStatus)
  }
})

/* ── the emptiness test ──────────────────────────────────────────────────── */

test('an adapted empty session is empty even though the adapter gives it one node', () => {
  // `adaptSession` synthesises a single `ending` node for a session with zero
  // events, so a `nodes.length` test would answer "holds something" for the
  // emptiest record in the store. Measured on the real all-empty document.
  const S = { ...empty('950682d3'), nodes: [{ id: 'ending' }], frames: [{ id: 'f' }] }
  assert.equal(recordHoldsSomething(S), false)
})

test('emptiness is decided from what was read, and an absent session is not "holds something"', () => {
  assert.equal(recordHoldsSomething(null), false)
  assert.equal(recordHoldsSomething(undefined), false)
  assert.equal(recordHoldsSomething({}), false)
})

/* ── the default-session rule ────────────────────────────────────────────── */

test('the opening session is the first that holds recorded events, not the first returned', () => {
  // The bug: `corpus.order[0]`. On the live store the default session is the
  // lexicographically greatest id, which is the ONE session of 44,075 with no
  // events at all — the app opened the emptiest row every time.
  const c = corpus([empty('a'), withEvents('b', 3), withEvents('c', 9)])
  assert.deepEqual(chooseSession(c), { key: 'b', reason: 'events' })
})

test('with no events anywhere, a session carrying a claim is preferred', () => {
  const c = corpus([empty('a'), withClaims('b', 1)])
  assert.deepEqual(chooseSession(c), { key: 'b', reason: 'claims' })
})

test('when nothing holds anything the first is opened and the rule SAYS so', () => {
  // Empty sessions are not skipped out of existence: a session that holds
  // nothing is a fact the reader may need, so the key is still a real key and
  // the reason is the branch that ran.
  const c = corpus([empty('a'), empty('b')])
  assert.deepEqual(chooseSession(c), { key: 'a', reason: 'nothing-held' })
})

test('no readings at all is its own answer, never a fabricated key', () => {
  assert.deepEqual(chooseSession({ order: [], sessions: {} }), { key: '', reason: 'no-readings' })
  assert.deepEqual(chooseSession(null), { key: '', reason: 'no-readings' })
})

test('the chosen key is always a key the reading returned', () => {
  for (const c of [corpus([empty('a'), withEvents('b', 2)]), corpus([empty('a')]), corpus([withClaims('z', 1)])]) {
    const { key } = chooseSession(c)
    assert.ok(c.order.includes(key), `${key} is not in the reading`)
  }
})

test('anyReadingHolds separates "this one is empty" from "all of them are"', () => {
  assert.equal(anyReadingHolds(corpus([empty('a'), empty('b')])), false)
  assert.equal(anyReadingHolds(corpus([empty('a'), withEvents('b', 1)])), true)
  assert.equal(anyReadingHolds(null), false)
})

/* ── the copy those decisions carry ──────────────────────────────────────── */

test('no in-flight or empty sentence carries a banned term', () => {
  const copy = [
    READING_LINE,
    stillReading('/api/explain'),
    nothingBackYet('/api/explain'),
    ...Object.values(sessionAbsence('graph')),
    ...Object.values(sessionAbsence('timeline', { readings: 2, anyHeld: false })),
  ]
  for (const line of copy) assert.equal(bannedIn(line), null, `banned term in: ${line}`)
})

test('no waiting line carries a percentage, an ETA or a remaining time', () => {
  // `lib/refresh-presentation.test.js:75` forbids all three in progress copy;
  // new progress copy inherits the ban. Progress belongs to the terminal.
  const copy = [READING_LINE, stillReading('/api/explain'), nothingBackYet('/api/explain')].join(' ')
  assert.doesNotMatch(copy, /\b\d+(?:\.\d+)?%|\bpercent\b|\bETA\b|remaining time/i)
})

test('the waiting line names the address being read', () => {
  assert.match(stillReading('/api/explain'), /\/api\/explain/)
  assert.match(nothingBackYet('/api/explain'), /\/api\/explain/)
})

test('an emptiness names the absent noun, says what would change it, and never offers an export', () => {
  const a = sessionAbsence('graph')
  assert.ok(a.absent && a.absent.length > 0)
  assert.match(a.change, /What would change it:/)
  // D6: `thruline export --all` / `thruline ingest` write the §9 ledger; the
  // live projection reads a different store, so neither can make an empty
  // session non-empty. A remedy that cannot work is a false claim.
  for (const line of Object.values(a)) {
    assert.doesNotMatch(line, /export --all|thruline ingest/)
  }
})

test('an emptiness asserts an absence ONLY about the document that was read', () => {
  // The rule this whole file exists for, applied to its own copy. `loadData`
  // reads `/api/explain` and nothing else: it hands the adapter `claims: null`,
  // and the explain document carries no `predicate` block, so `S.claims` and
  // `S.checks` are `[]` because they were NOT READ. The sentence used to say
  // "no claims … and no agreed checks are in this session's record" — two
  // absences about documents this app never opened.
  const a = sessionAbsence('graph')
  assert.match(a.absent, /claims and agreed checks are not in that document/)
  assert.doesNotMatch(a.absent, /no claims/)
  assert.doesNotMatch(a.absent, /no agreed checks/)
})

test('the remedy names the door that exists, and no route this app cannot take', () => {
  // Doctrine: a remedy is a route a human can actually take.
  //
  // THIS TEST WAS A TRIPWIRE AND IT HAS BEEN TRIPPED DELIBERATELY. It used to
  // assert `doesNotMatch(a.change, /open a different session/)`, because at the
  // time the live corpus WAS one session and there was no different session to
  // open — the sentence said so out loud ("nothing on this screen can open
  // another"). The workspace now reads `/api/sessions` and the header button
  // opens what came back, so the ban became a ban on the truth. The assertion
  // is inverted rather than deleted: the sentence must now NAME that door, so
  // this cannot quietly regress into a remedy that points nowhere.
  const a = sessionAbsence('graph')
  assert.match(a.change, /session button in the header/)
  assert.doesNotMatch(a.change, /nothing on this screen can open another/)
  // The Inbox is still `os/shell.jsx` with no callback into this workspace, so
  // it is still not the door and this sentence still does not send anyone there.
  assert.doesNotMatch(a.change, /from the Inbox/)
  // `thruline doctor` is real and read-only, but it takes no `--session`
  // (crates/cli/src/main.rs:797-807), so nothing may claim it is scoped to one.
  assert.doesNotMatch(a.change, /for this one/)
})

test('every emptiness title says the reading SUCCEEDED, on every surface it renders on', () => {
  // `tree` said only "This session's record holds no objects to list." — the
  // one surface where a reader could not tell an empty record from an unread
  // one, which is the exact distinction this file draws.
  for (const surface of ['graph', 'timeline', 'table', 'tree']) {
    assert.match(sessionAbsence(surface).title, /^This session was read,/, `${surface} does not say the read happened`)
  }
})

test('an emptiness sentence is a full sentence with a verb and a stop, on every surface', () => {
  for (const surface of ['graph', 'timeline', 'table', 'tree']) {
    const { title } = sessionAbsence(surface)
    assert.match(title, /\.$/, `${surface} title is a fragment`)
    assert.ok(title.split(' ').length > 3, `${surface} title is not a sentence`)
  }
})

test('an emptiness carries no digit — a zero never renders as a number', () => {
  // Doctrine 4: every count carries a noun, and a zero count renders nothing.
  for (const surface of ['graph', 'timeline', 'table', 'tree']) {
    for (const line of Object.values(sessionAbsence(surface, { readings: 3, anyHeld: false }))) {
      assert.doesNotMatch(line, /\d/, `a digit reached the empty state: ${line}`)
    }
  }
})

test('an emptiness says whether the whole reading is empty or only this session', () => {
  const one = sessionAbsence('graph', { readings: 1, anyHeld: false })
  const all = sessionAbsence('graph', { readings: 3, anyHeld: false })
  const some = sessionAbsence('graph', { readings: 3, anyHeld: true })
  assert.notEqual(all.absent, one.absent)
  assert.equal(some.absent, one.absent)
})

/* ── the status strip ────────────────────────────────────────────────────── */

test('the status strip blanks its scope rather than printing a fabricated denominator', () => {
  const reading = statusFor(centreState({ dataStatus: 'loading' }), '/api/explain')
  assert.equal(reading.scope, '')
  assert.match(reading.origin, /\/api\/explain/)
  assert.equal(reading.read, READING_LINE)

  const emptyStrip = statusFor(centreState({ dataStatus: 'ready', session: empty('a') }), '/api/explain')
  assert.equal(emptyStrip.scope, '')
})

test('a ready reading leaves the strip to the root, which composes it from the record', () => {
  assert.equal(statusFor(centreState({ dataStatus: 'ready', session: withEvents('a', 2) }), '/api/explain'), null)
})

/* ── the connection line ─────────────────────────────────────────────────── */

test('a read in flight is never reported as "no local server"', () => {
  // The same bug as the refusal, on the footer: `live` is set only after the
  // read resolves, so the boolean form asserted a NEGATIVE FACT — "no local
  // server in this reading" — for the whole duration of every read, beside a
  // status bar that read "reading /api/explain…".
  for (const dataStatus of ['loading', 'idle']) {
    const line = connectionLine({ live: false, dataStatus })
    assert.doesNotMatch(line, /no local server/)
    assert.match(line, /reading from the local app/)
  }
})

test('"no local server" is said only once a read has finished without one', () => {
  assert.equal(connectionLine({ live: false, dataStatus: 'ready' }), 'no local server in this reading')
  assert.equal(connectionLine({ live: false, dataStatus: 'refused' }), 'no local server in this reading')
  assert.equal(connectionLine({ live: true, dataStatus: 'ready' }), 'served by the local app')
})

test('the bundled corpus keeps saying it is the bundled corpus', () => {
  // Doctrine 9: demo data is bannered as demo, permanently. The diagnostics
  // row said "fixture mode" and it still does.
  assert.match(connectionLine({ live: false, dataStatus: 'ready', isDemo: true }), /^fixture mode/)
})

test('the egress guarantee rides on the long form, in every state', () => {
  for (const dataStatus of ['idle', 'loading', 'ready', 'refused']) {
    for (const live of [true, false]) {
      assert.match(connectionLine({ live, dataStatus, long: true }), /loopback only$/)
    }
  }
})

/* ── the session listing ─────────────────────────────────────────────────── */

/* THE DEFECT THESE PIN. `/api/explain` with no `?session=` opens whichever id
 * sorts last (`crates/cli/src/claims_cmd.rs:275-282`), which on the operator's
 * store is the ONE session of 44,107 with zero events — so the product opened
 * on an empty screen while the thirteen sessions carrying a `stop` sat
 * unreachable, because nothing enumerated them. `/api/sessions` enumerates
 * them; everything below is the surface's half of that, kept pure so it can be
 * pinned without a browser (there is no DOM harness in this repo). */

/** One listing row as the engine actually serves it, measured on the store. */
const listed = (id, over) => ({
  id,
  repo_key: '/Users/a/thruline',
  repo_root: '/Users/a/thruline',
  harness: 'claude-code',
  event_count: 0,
  user_prompts: 0,
  stop_events: 0,
  file_change_events: 0,
  final_messages: 0,
  first_event_at: null,
  last_event_at: null,
  head_hash: 'sha256:abc',
  capture_degraded: false,
  ...over,
})

const listing = (rows, over) => ({
  schema_version: 1,
  shape: 'thruline.sessions',
  store: '/Users/a/.thruline/exec.db',
  total: 44107,
  returned: rows.length,
  limit: 50,
  sessions: rows,
  ...over,
})

test('the real row shape the engine serves is read, field for field', () => {
  const read = listingFromDocument(
    listing([
      listed('5e5a0022-0000-4000-8000-000000000022', {
        event_count: 3252,
        user_prompts: 16,
        stop_events: 15,
        file_change_events: 55,
        final_messages: 15,
        last_event_at: '2026-09-05T13:52:53Z',
      }),
    ]),
  )
  assert.equal(read.refusal, null)
  assert.equal(read.total, 44107)
  assert.deepEqual(read.rows[0], {
    id: '5e5a0022-0000-4000-8000-000000000022',
    repo: '/Users/a/thruline',
    harness: 'claude-code',
    events: 3252,
    prompts: 16,
    stops: 15,
    fileChanges: 55,
    finalMessages: 15,
    lastAt: '2026-09-05T13:52:53Z',
    headHash: 'sha256:abc',
    degraded: false,
    openable: true,
  })
})

test('an unknown shape refuses LOUDLY and names what came back', () => {
  // Rule 12, and the reason `readProjection` alone is not enough: a 200 whose
  // body this build does not know how to read must never be mined for the
  // fields it happens to carry.
  const read = listingFromDocument(listing([listed('a')], { shape: 'thruline.claims' }))
  assert.notEqual(read.refusal, null)
  assert.match(read.refusal.missing, /thruline\.claims/)
  assert.match(read.refusal.missing, /thruline\.sessions/)
  assert.deepEqual(read.rows, [])
})

test('an unknown version refuses rather than reading a shape it does not know', () => {
  const read = listingFromDocument(listing([listed('a')], { schema_version: 3 }))
  assert.notEqual(read.refusal, null)
  assert.match(read.refusal.missing, /version/)
})

test('version 2 retains addressed historical sessions and validates the limit scope', () => {
  for (const limit_scope of ['store', 'project']) {
    const read = listingFromDocument(listing([listed('historical-claim'), listed('later-check')], { schema_version: 2, limit_scope }))
    assert.equal(read.refusal, null)
    assert.equal(listingHasSession(read.rows, 'historical-claim'), true)
    assert.deepEqual(read.rows.map(row => row.id), ['historical-claim', 'later-check'])
  }
  for (const limit_scope of [undefined, null, 'global', 1]) {
    const read = listingFromDocument(listing([listed('historical-claim')], { schema_version: 2, limit_scope }))
    assert.match(read.refusal.missing, /limit_scope/)
    assert.deepEqual(read.rows, [])
  }
})

test('a body that is not a document, and one with no sessions array, each refuse by name', () => {
  for (const doc of [null, 'nope', [], 42, {}]) {
    assert.notEqual(listingFromDocument(doc).refusal, null, `${JSON.stringify(doc)} was not refused`)
  }
  const read = listingFromDocument(listing([], { sessions: 'no' }))
  assert.match(read.refusal.missing, /sessions/)
})

test('a row with no id is counted and reported, never silently dropped', () => {
  const read = listingFromDocument(listing([listed('a'), { repo_key: 'x' }, listed('b')]))
  assert.equal(read.refusal, null)
  assert.deepEqual(read.rows.map((r) => r.id), ['a', 'b'])
  assert.equal(read.dropped, 1)
})

test('a count the listing did not carry is not the same fact as a count of zero', () => {
  // Doctrine 4: a zero renders as a sentence and an absent count renders as a
  // different sentence. If they collapsed to one value the surface could not
  // tell them apart, and it must.
  const [zero, absent] = listingFromDocument(
    listing([listed('a', { event_count: 0 }), listed('b', { event_count: null })]),
  ).rows
  assert.equal(zero.events, 0)
  assert.equal(absent.events, null)
})

test('an id the server would refuse is listed but marked unopenable, never hidden', () => {
  // `uiserve.rs:1387-1394`. Hiding a row the store holds is a quieter lie than
  // showing one that cannot be opened; the surface must be able to say which.
  const read = listingFromDocument(listing([listed('ok-1'), listed('-lead'), listed('.'), listed('a b')]))
  assert.deepEqual(read.rows.map((r) => [r.id, r.openable]), [['ok-1', true], ['-lead', false], ['.', false], ['a b', false]])
})

test('the parameter grammar mirrors the server, including its two named refusals', () => {
  assert.equal(isOpenableSessionId('5e5a0021-0000-4000-8000-000000000021'), true)
  assert.equal(isOpenableSessionId('a.b_c-1'), true)
  assert.equal(isOpenableSessionId(''), false)
  assert.equal(isOpenableSessionId('.'), false)
  assert.equal(isOpenableSessionId('-x'), false)
  assert.equal(isOpenableSessionId('a/b'), false)
  assert.equal(isOpenableSessionId('x'.repeat(129)), false)
  assert.equal(isOpenableSessionId(null), false)
})

/* ── the deep link ───────────────────────────────────────────────────────── */

test('the address is parsed once, and a malformed one becomes null rather than a 400', () => {
  assert.equal(sessionIdFromSearch('?session=5e5a0021-0000-4000-8000-000000000021'), '5e5a0021-0000-4000-8000-000000000021')
  assert.equal(sessionIdFromSearch('session=abc'), 'abc', 'the leading ? is optional')
  assert.equal(sessionIdFromSearch('?project=x&session=abc&node=y'), 'abc', 'it composes with the addresses already in use')
  assert.equal(sessionIdFromSearch(''), null)
  assert.equal(sessionIdFromSearch('?session='), null)
  assert.equal(sessionIdFromSearch('?session=a/b'), null)
  assert.equal(sessionIdFromSearch('?session=-x'), null)
  assert.equal(sessionIdFromSearch(null), null)
})

test('a deep-linked id is checked against the LISTING, because the server cannot refuse one', () => {
  // `/api/explain?session=<anything>` answers HTTP 200 with an all-empty
  // document, so "no such session" and "an empty session" arrive identically.
  // The listing is the only place the app can learn the difference.
  const rows = listingFromDocument(listing([listed('a'), listed('b')])).rows
  assert.equal(listingHasSession(rows, 'a'), true)
  assert.equal(listingHasSession(rows, 'zzz'), false)
  assert.equal(listingHasSession(rows, null), false)
  assert.equal(listingHasSession(null, 'a'), false)
})

test('the missing-deep-link sentence says only what this page can know', () => {
  // It may NOT say the session does not exist — it knows only that the list it
  // read does not name it, and that the list is bounded. Doctrine 5: advisory,
  // never an accusation.
  assert.match(DEEP_LINK_ABSENT.absent, /does not contain it/)
  assert.match(DEEP_LINK_ABSENT.absent, /bounded/)
  assert.doesNotMatch(DEEP_LINK_ABSENT.absent, /does not exist|no such session/)
  assert.match(DEEP_LINK_ABSENT.change, /What would change it:/)
  assert.equal(DEEP_LINK_ABSENT.remedy, 'thruline doctor')
})

test('every new listing sentence obeys the copy rules the rest of this file obeys', () => {
  // The same three rules the shipped absence copy is held to: no banned term,
  // no digit (doctrine 4 — a zero renders as a sentence, never as a number),
  // and a title that is a full sentence with a verb and a stop. `absent` and
  // `change` are deliberately NOT stop-terminated, exactly as `SESSION_ABSENT`
  // and `SESSION_CHANGE` are not: one is a noun phrase under "What is not
  // here", the other runs straight into the command that follows it.
  const copy = [...Object.values(DEEP_LINK_ABSENT), SESSIONS_LEAD, SESSIONS_LEAD_REFUSED]
  for (const line of copy) {
    assert.equal(bannedIn(line), null, `banned term in: ${line}`)
    assert.doesNotMatch(line, /\d/, `a digit reached the copy: ${line}`)
  }
  for (const line of [DEEP_LINK_ABSENT.title, SESSIONS_LEAD, SESSIONS_LEAD_REFUSED]) {
    assert.match(line, /\.$/, `not a full sentence: ${line}`)
    assert.ok(line.split(' ').length > 3, `not a sentence: ${line}`)
  }
})

test('the Sessions lead stops claiming an order and a completeness it does not have', () => {
  // It read "Every session this reading has opened, newest first." Neither half
  // survives a store view: it is not what this reading opened, and NOTHING here
  // is ordered by time — `started_at`/`ended_at` are NULL on every row.
  assert.doesNotMatch(SESSIONS_LEAD, /newest first/)
  assert.doesNotMatch(SESSIONS_LEAD, /this reading has opened/)
  assert.match(SESSIONS_LEAD, /bounded/)
})

/* ── which session opens, decided from the listing ───────────────────────── */

const rowsOf = (docRows) => listingFromDocument(listing(docRows)).rows

test('the opening session is the first that can actually be checked, not the first returned', () => {
  // The whole defect: the store's default is a lexicographic maximum with no
  // events in it. A completion claim is extractable only from a session that
  // recorded a `stop`, and it is checkable against something only when the tree
  // was touched, so that pair wins.
  const rows = rowsOf([
    listed('empty'),
    listed('busy', { event_count: 900 }),
    listed('stopped', { event_count: 10, stop_events: 1 }),
    listed('stopped-and-changed', { event_count: 5, stop_events: 1, file_change_events: 3 }),
  ])
  assert.deepEqual(chooseListedSession(rows), { id: 'stopped-and-changed', reason: 'stopped-with-changes' })
})

test('each tier is reached only when the one above it is empty, and each says which ran', () => {
  assert.deepEqual(
    chooseListedSession(rowsOf([listed('a'), listed('b', { event_count: 4, stop_events: 2 })])),
    { id: 'b', reason: 'stopped' },
  )
  assert.deepEqual(chooseListedSession(rowsOf([listed('a'), listed('b', { event_count: 4 })])), { id: 'b', reason: 'events' })
  assert.deepEqual(chooseListedSession(rowsOf([listed('a'), listed('b')])), { id: 'a', reason: 'first-listed' })
})

test('within a tier the ENGINE order decides, and this function never re-sorts it', () => {
  // The engine ranks by a recorded ending, then by the newest recorded event,
  // then by fold order (`crates/ledger/src/schema.rs:1294-1297`). That is a
  // measurement; re-sorting it here by a proxy such as "more events first"
  // would replace it with a guess this app never made.
  const rows = rowsOf([
    listed('first', { event_count: 5, stop_events: 1, file_change_events: 1 }),
    listed('bigger', { event_count: 5000, stop_events: 1, file_change_events: 1 }),
  ])
  assert.equal(chooseListedSession(rows).id, 'first')
})

test('an unopenable id is never the one opened, and an all-unopenable list says so', () => {
  const rows = rowsOf([listed('-bad', { event_count: 900, stop_events: 1 }), listed('good', { event_count: 1 })])
  assert.deepEqual(chooseListedSession(rows), { id: 'good', reason: 'events' })
  assert.deepEqual(chooseListedSession(rowsOf([listed('-bad'), listed('.')])), { id: '', reason: 'none-openable' })
})

test('no listing is its own answer, never a fabricated id', () => {
  assert.deepEqual(chooseListedSession([]), { id: '', reason: 'no-listing' })
  assert.deepEqual(chooseListedSession(null), { id: '', reason: 'no-listing' })
})

test('the chosen id is always an id the listing returned', () => {
  // The same invariant `chooseSession` carries: a chooser may not invent a key.
  for (const rows of [
    rowsOf([listed('a'), listed('b', { stop_events: 1 })]),
    rowsOf([listed('a')]),
    rowsOf([listed('z', { event_count: 3 })]),
  ]) {
    const { id } = chooseListedSession(rows)
    assert.ok(rows.some((r) => r.id === id), `${id} is not on the listing`)
  }
})

/* ── narrowing the list ──────────────────────────────────────────────────── */

test('the filter narrows by the two things a row shows as an address, and never re-orders', () => {
  const rows = rowsOf([
    listed('aaa-1', { repo_key: '/Users/a/thruline', repo_root: '/Users/a/thruline' }),
    listed('bbb-2', { repo_key: '/Users/a/other', repo_root: '/Users/a/other' }),
    listed('ccc-3', { repo_key: '/Users/a/thruline', repo_root: '/Users/a/thruline' }),
  ])
  assert.deepEqual(filterListing(rows, 'bbb').map((r) => r.id), ['bbb-2'])
  assert.deepEqual(filterListing(rows, 'THRULINE').map((r) => r.id), ['aaa-1', 'ccc-3'], 'case-insensitive, and in the engine order')
  assert.deepEqual(filterListing(rows, '').map((r) => r.id), ['aaa-1', 'bbb-2', 'ccc-3'], 'an empty query is not a filter')
  assert.deepEqual(filterListing(rows, '   ').map((r) => r.id), ['aaa-1', 'bbb-2', 'ccc-3'])
  assert.deepEqual(filterListing(rows, 'nothing-matches'), [])
  assert.deepEqual(filterListing(null, 'x'), [])
})

test('project picker includes unopened projects and keeps equal folder names at distinct paths', () => {
  const rows = rowsOf([
    listed('opened', { repo_root: '/work/thruline' }),
    listed('other', { repo_root: '/Developer/sample-app', event_count: 4 }),
    listed('other-two', { repo_root: '/Developer/sample-app', stop_events: 1 }),
    listed('same-name', { repo_root: '/archive/sample-app' }),
    listed('old-key', { repo_key: '-Users-a-Developer-demo-build', repo_root: null }),
  ])
  const cached = { order: ['opened'], sessions: { opened: { project: '/work/thruline' } } }
  const projects = projectListing(rows, cached)
  assert.equal(projects.length, 4)
  assert.equal(projects.find(p => p.repo === '/work/thruline').count, 1)
  assert.deepEqual(projects.find(p => p.repo === '/Developer/sample-app'), { repo: '/Developer/sample-app', count: 2, session: 'other-two' })
  assert.equal(projectListing(rows, cached, 'SAMPLE-APP').length, 2)
  assert.equal(projectListing(rows, cached, 'demo.build')[0].repo, '-Users-a-Developer-demo-build')
  assert.deepEqual(projectListing(rows, cached, 'no-such-project'), [])
  assert.equal(projectListing([], cached)[0].session, 'opened')
})

test('project picker retains a project with an unopenable id without inventing an address', () => {
  const projects = projectListing(rowsOf([listed('-invalid', { repo_root: '/work/project' })]), null)
  assert.equal(projects.length, 1)
  assert.equal(projects[0].session, '')
})

/* ThruLine workspace — the bundled demo corpus.
 *
 * The port of `scratchpad/design/demo-records.js` (`window.THRULINE_DEMO`):
 * three hand-authored sessions in the Session shape that `./adapter.js`
 * produces from real documents.
 *
 * TWO HARD RULES ON THIS FILE, both enforceable and both non-negotiable:
 *
 * 1. NOTHING FROM HERE MAY RENDER WITHOUT THE PERMANENT DEMO BANNER.
 *    `chrome.jsx`'s `<DemoBanner>` is sticky, top:0, undismissable and NOT
 *    conditionally rendered while a demo session is active (PORT-CONTEXT §Data,
 *    spec-doctrine F-13). If this file ships, `demo-data.js` and every file
 *    that imports it must be added to BOTH `DEMO_SURFACES` and `ALL_SURFACES`
 *    in `src/lib/inbox.test.js:304-319` — the `deepEqual` there passes silently
 *    otherwise and the banner ships unguarded (spec-target §F.4 edit 5).
 *
 * 2. THE ACCUSATORY REGISTER DOES NOT SURVIVE THE PORT.
 *    The design's `commentary` map ships ten strings — `bullshit`, `vibes`,
 *    `Slop detected`, `"Trust me, bro"`, `Declared victory. Classic.` — that
 *    assert deceit, attribute intent, and put fabricated quotations in the
 *    agent's mouth. NS4:156-170 makes every negative finding advisory:
 *    "claimed, and no proof found in the record", never proof of falsehood.
 *    The replacements below are record-descriptive and are quoted verbatim from
 *    spec-doctrine F-1's table. Do not restore the originals.
 *
 * Whether this file ships AT ALL is an operator decision, not a porter's
 * (spec-target Decision 3; recommendation there is that it does not, and the 11
 * new destinations render `<Refusal>` instead). The module exists so the
 * decision is a one-import change either way.
 *
 * EVERY OTHER VALUE IS BYTE-IDENTICAL to `demo-records.js`. They are pinned
 * fixture facts (inv-data.md §1-§8, brief §10) and are not a porter's to
 * re-author. The corrections applied on the way in, each a doctrine fix with
 * its citation, are exactly these and no others:
 *
 *   · `commentary`     — spec-doctrine F-1's replacement table; `session` deleted
 *   · `sentence`       — DELETED (F-11); the contextual strip renders
 *                        `narr.session[reg][0]` then `[1]` instead
 *   · node `lines[]`   — record enums stripped per F-10's replacement table
 *   · `capture.fidelity` → `capture.recorderLine` (F-9); the identifier
 *                        `capture_fidelity` survives only as a `verify.rows[]`
 *                        name, which is the level-3 receipt-check table
 *   · `verify.rows[]`  — the degraded row's `reason` rewritten per F-14
 *   · `Receipt verified` → `Receipt intact` on the honest receipt node (F-8)
 *   · `real-fog-redacted.capture.sources` — the fixture's two sources, not the
 *                        demo's one (spec-data §0: the demo is wrong here)
 *   · node `clause`    — added to every node carrying a negative word, verbatim
 *                        from F-2's table, because `<StateWord>` throws without it
 *
 * The demo's elisions (`"(text withheld)"`, `"sha256:…"`, the `demo-honest` and
 * `real-fog` `null`s) are KEPT here, because here they are the record: this is
 * a hand-authored corpus, not a reading of a fixture. The rule against carrying
 * them forward (spec-data §D.4) binds `./adapter.js`, which reads the fixtures
 * themselves and must not inherit an authoring gap.
 */

/** @typedef {import('./adapter.js').Session} Session */

const TREE = 'c324df66d87ae811b42f2f3e395988361be76959'
const COMMIT = '0d81d696595ef43bf7e8745be523f4993c24d353'
const CHAIN = 'sha256:afb960f9eb04728dbbff8706449e3856d855db284bdf35d5676ebd8e1bee2861'
const TRAILER = 'sha256:c8d23871c72e8090bd4416a1f5c029d89b747c8a97833407f21ea75db1c53237'
const POLICY_HASH = 'sha256:cfbdf195a01ee22251ddcbc3c0bd23c88bff89a265080934cb00099d329d62ad'
const SID = 'd3m0d3m0-0000-4000-8000-000000000001'
const CLM = 'clm_7ADJ9XM19VJR59T0FC4VQ0Z7Q2'
const CHK = 'chk_4KJW396K69ZJ4KX5J4EY44QT08'
const PRM = 'prm_5942T1HFCY0652ZV5Z535DFYG9'
const ASK = 'ask_4PX0M9RMTKDCVBYPCE3XZE4M66'
const EVT_STOP = 'evt_4D14PF5YQTFHDQWB1DE83KS5XN'
const EVT_PROMPT = 'evt_6QC1RDFZP2KVH64BKZMCRWY7KZ'

/* The clause strings, verbatim from spec-doctrine F-2's table. Named once so a
 * reader can see that the same record sentence reaches every surface. */
const CLAUSE_ASK = 'What would prove it: a claim, a promise, or a change to something it named.'
const CLAUSE_CLAIM = 'What would prove it: a full-suite run on the final code, with skipped tests accounted for.'
const CLAUSE_PROMISE = 'What would prove it: a later change to a path this names.'
const CLAUSE_ENDING = 'What would prove it: a stop event for this session, from a hook that is installed.'

const verifyRowsNarrowed = [
  ['signature', 'pass', 'signature 0 verifies over the DSSE PAE'],
  ['payload_type', 'pass', 'application/vnd.in-toto+json'],
  ['statement_schema', 'pass', 'in-toto Statement v1, ThruLine predicate'],
  ['subject_tree', 'pass', TREE + ' is a tree in this repository'],
  ['subject_commit', 'pass', COMMIT + ' is present locally'],
  ['chain_head', 'pass', CHAIN + ' recomputed from 16 ledger rows'],
  ['reexecution', 'pass', 'no check claims to have been re-executed'],
  ['policy_hash', 'pass', 'the policy this receipt was judged under is the policy here (' + POLICY_HASH + ')'],
  ['capture_fidelity', 'pass', 'capture was not degraded, and no content transform is in scope here'],
  ['verdicts', 'pass', "1 verdict(s), none left ‘unverifiable‘ (1 ‘unsupported‘, which is the gate's business and not this command's)"],
  ['verdict_derivation', 'pass', '0 ‘supported‘ verdict(s) re-derived from the checks cited beside them, and each ran on a tree this receipt is about'],
  ['check_provenance', 'pass', "1 check(s) in this receipt are the ledger's own rows"],
  [
    'head_binding',
    'pass',
    'the tree in this working directory IS the tree this receipt is about, so what is being reviewed here is what the checks ran on — no commit needed',
  ],
].map((r) => ({ name: r[0], outcome: r[1], detail: r[2] }))

const verifyRowsHonest = verifyRowsNarrowed.map((r) => {
  let d = 'confirmed here (hash-specific detail not quoted in this reading)'
  if (
    r.name === 'payload_type' ||
    r.name === 'statement_schema' ||
    r.name === 'reexecution' ||
    r.name === 'capture_fidelity' ||
    r.name === 'check_provenance'
  ) {
    d = r.detail
  }
  if (r.name === 'subject_tree') d = '0c380073fc4b… is a tree in this repository'
  if (r.name === 'verdicts') d = '1 verdict(s), none left ‘unverifiable‘'
  return { name: r.name, outcome: 'pass', detail: d }
})

const verifyRowsFog = verifyRowsNarrowed.map((r) => {
  if (r.name === 'capture_fidelity') {
    return {
      name: r.name,
      outcome: 'skipped',
      detail: '',
      /* spec-doctrine F-14: the internal cross-reference and the shouted enum
       * are producer vocabulary, not a reader's. */
      reason:
        'the receipt records capture with gaps, so the record cannot settle claims in this session. This is not a pass.',
    }
  }
  if (r.name === 'head_binding') {
    return {
      name: r.name,
      outcome: 'skipped',
      detail: '',
      reason:
        "the head under review IS the receipt's commit 592321f77c43d6cddbc9c9a30e8368eed445cccc, and the tree being reviewed differs from the tree the checks ran on at 42 path(s): INTERFACE-REQUESTS.md, … Nothing in a receipt records which paths its checks covered, so this build cannot say those changes are harmless — only that the evidence does not reach them. This is NOT a pass.",
    }
  }
  if (r.name === 'chain_head') return { name: r.name, outcome: 'pass', detail: 'sha256:… recomputed from 582 ledger rows' }
  return { name: r.name, outcome: 'pass', detail: 'confirmed here (hash-specific detail not quoted in this reading)' }
})

/* ---------- demo-narrowed ---------- */

const narrowedEvents = [
  [4, 'session_start', 'session started'],
  [8, 'user_prompt', 'make the shipping rate include a handling fee'],
  [12, 'tool_result', 'Edit ok'],
  [13, 'fs_change', 'modify shipping.py'],
  [14, 'snapshot', 'tree c324df66d87a (taken at fold)'],
  [16, 'tool_result', 'Edit ok'],
  [17, 'fs_change', 'modify test_shipping.py'],
  [18, 'snapshot', 'tree c324df66d87a (taken at fold)'],
  [20, 'tool_call', 'Bash {"command":"pytest -q -k test_rate_small"}'],
  [24, 'note', 'exec_begin f8c40ed31b74…'],
  [28, 'snapshot', 'tree c324df66d87a'],
  [32, 'exec', 'pytest -q -k test_rate_small → exit 0'],
  [36, 'snapshot', 'tree c324df66d87a'],
  [40, 'tool_result', 'Bash ok'],
  [41, 'snapshot', 'tree c324df66d87a (taken at fold)'],
  [44, 'stop', 'stopped: Added the handling fee to the shipping rate. All tests pass.…'],
].map((e) => ({ seq: e[0], kind: e[1], text: e[2] }))

const narrowedFindings = [
  {
    key: 'narrowed_run',
    name: 'Narrowed run',
    severity: 'fail',
    detail: '`pytest -q -k test_rate_small` narrows the run with `-k`, so it exercised a subset of the suite',
    ref: 'command seq 20 · check ' + CHK,
    node: 'f_narrow',
  },
  {
    key: 'skipped_tests',
    name: 'Skipped tests',
    severity: 'warn',
    detail: '1 skipped and 0 xfailed of 2 — they were not run',
    ref: 'check ' + CHK,
    node: 'f_skip',
  },
  {
    key: 'assertion_removed',
    name: 'Assertion removed',
    severity: 'fail',
    detail: '`test_rate_large` had 1 assertion(s) at the base tree and 0 at the final tree',
    path: 'test_shipping.py',
    ref: 'test_shipping.py · seq 17',
    node: 'f_removed',
  },
  {
    key: 'assertion_weakened',
    name: 'Assertion weakened',
    severity: 'warn',
    detail: "`test_rate_small`'s strongest assertion went from Equality to Containment",
    path: 'test_shipping.py',
    ref: 'test_shipping.py · seq 17',
    node: 'f_weak',
  },
  {
    key: 'coverage_not_measured',
    name: 'Coverage not measured',
    severity: 'info',
    detail: '',
    derivedDetail:
      'Informational. The record holds no coverage measurement for this run (detector detail not quoted in this reading).',
    ref: 'check ' + CHK,
    node: 'f_cov',
  },
]

const narrowed = {
  key: 'demo-narrowed',
  label: 'demo-narrowed',
  project: 'demo',
  id: SID,
  origin: 'Demo records · fixture demo-narrowed · pinned core 29f83b6 / p3 cc69822',
  harness: 'Claude Code',
  harnessRaw: 'claude-code',
  started: '2026-09-05T15:19:59Z',
  ended: '2026-09-05T15:20:11Z',
  eventCount: 16,
  eventsComplete: true,
  finalTree: TREE,
  commit: COMMIT,
  chainHead: CHAIN,
  capture: {
    sources: ['claude-code-hook', 'exec-recorder'],
    redactions: 0,
    degraded: false,
    condition: 'intact',
    /* F-9 · BRIEF:1521's instrument sentence, in place of the identifier. */
    recorderLine: 'The recorder was working for this session.',
  },
  exits: { explain: 3, claims: 3, narrate: 3, verifyReceipt: 0 },
  sessionWord: 'not_proven',
  countLine: '1 not proven claim · 2 loose threads',
  events: narrowedEvents,
  stop: {
    kind: 'agent_finished',
    summary: 'the agent finished its turn',
    seq: 44,
    lastEvents: ['tree c324df66d87a', 'Bash ok', 'tree c324df66d87a (taken at fold)'],
    text: 'Added the handling fee to the shipping rate. All tests pass.…',
  },
  compactions: 0,
  asks: [
    {
      id: ASK,
      kind: 'feature',
      status: 'untouched',
      word: 'loose',
      text: 'make the shipping rate include a handling fee',
      eventId: EVT_PROMPT,
      offset: [0, 45],
      extractor: 'rules',
      node: 'ask',
    },
  ],
  claims: [
    {
      id: CLM,
      kind: 'tests_pass',
      text: 'All tests pass.',
      verdict: 'unsupported',
      word: 'not_proven',
      verifier: 'tests_pass@1',
      scope: 'all',
      extractor: 'rules',
      eventId: EVT_STOP,
      offset: [45, 59],
      seq: 44,
      evidence: [CHK],
      binding: 'ran_on_final_tree',
      reasons: ['ran on the final tree', '1/2 passed'],
      warnings: [
        '1 skipped and 0 xfailed of 2 — they were not run',
        "`test_rate_small`'s strongest assertion went from Equality to Containment",
      ],
      findings: narrowedFindings,
      node: 'clm',
      plain: 'The recorded command ran a subset of the suite. One test passed and one was skipped.',
      prove: CLAUSE_CLAIM,
      nextCheck: 'thruline check -- pytest -q',
    },
  ],
  promises: [
    {
      id: PRM,
      kind: 'follow_up',
      status: 'open',
      word: 'loose',
      text: "I'll add a regression test for the large-weight case in a follow-up.",
      eventId: EVT_STOP,
      seq: 44,
      node: 'prm',
      followUp: 'no later session in this repository has changed anything matching it',
      prove: CLAUSE_PROMISE,
    },
  ],
  checks: [
    {
      id: CHK,
      category: 'test',
      runner: 'pytest',
      command: 'pytest -q -k test_rate_small',
      scopeNote: 'narrowed by -k test_rate_small',
      exit: 0,
      argvHash: 'sha256:568669b0…',
      inputTree: TREE,
      bound: true,
      bindingLabel: 'ran on the final tree',
      reexecuted: false,
      parsed: { passed: 1, failed: 0, skipped: 1, tests_total: 2, xfail: 0, duration_ms: 0, test_ids: [] },
      node: 'chk',
      seqHint: 'exec seq 32 (join by id not exported)',
    },
  ],
  testChanges: [
    {
      test: 'test_rate_large',
      file: 'test_shipping.py',
      change: '1 assertion at the base tree → 0 at the final tree',
      detector: 'assertion_removed',
    },
    {
      test: 'test_rate_small',
      file: 'test_shipping.py',
      change: 'strongest assertion went from Equality to Containment',
      detector: 'assertion_weakened',
    },
  ],
  receipt: {
    trailer: TRAILER,
    keyid: 'sha256:f37e9ee5…',
    payloadType: 'application/vnd.in-toto+json',
    predicateType: 'https://thrulinehq.com/attestation/execution-check/v0.1',
    subjectName: 'git+/Users/demo/demo-workspace/fixtures/raw/demo/…',
    verifierVersion: '0.1.0',
    mode: 'local',
    policyHash: POLICY_HASH,
    policyInForce: false,
    promisesOpen: [],
    promisesFulfilled: [],
    node: 'rcpt',
  },
  verify: {
    rows: verifyRowsNarrowed,
    conclusion: 'neutral',
    verified: false,
    status: 'partial',
    exit: 0,
    integrity: 'verified',
    integrityExit: 0,
    claims: { total: 1, supported: 0, unsupported: 1, unverifiable: 0 },
    policyEvaluated: false,
    policyNote: 'no policy was evaluated, so no `failure` can come from one',
    integrityNote:
      'the axis `verify` itself decides, and the axis `exit` reports. `unsupported` claims never move it (ruling 1) — gate on `conclusion`, never on `exit`.',
  },
  narr: {
    session: {
      mgr: [
        'The agent reported 1 thing done.',
        'Not proven: the record backs 0 of them.',
        'What would prove it: evidence for the ones nothing backs.',
      ],
      op: [
        'The agent made 1 completion claim in this session.',
        'Not proven: the record backs 0 of them.',
        'What would prove it: evidence for the claims nothing backs. Read those first.',
      ],
    },
    stop: {
      mgr: [
        'The agent finished on its own.',
        'Nothing interrupted it.',
        'Judge the work by the claims, not by the clean ending.',
      ],
      op: [
        'The agent stopped on its own, saying: "Added the handling fee to the shipping rate. All tests pass. I\'ll add a …"',
        'The record shows a normal finish: the agent finished its turn',
        'Read the claims below; a normal finish is not a proof of anything.',
      ],
    },
    verdict: {
      mgr: [
        'The agent reported the tests passing.',
        'Not proven: the record holds no test run that covers the final code.',
        'What would prove it: a test run on the final code.',
      ],
      op: [
        'The agent wrote: "All tests pass."',
        'Not proven: the record carries no passing run bound to the final tree — ran on the final tree.',
        'What would prove it: a suite run on this exact tree, after the final edit.',
      ],
    },
    promise: {
      mgr: [
        'The agent said it would come back to something.',
        'Loose thread: nothing since has been done about it.',
        'What would prove it: later work that covers it.',
      ],
      op: [
        'The agent wrote: "I\'ll add a regression test for the large-weight case in a follow-up."',
        'Loose thread: no later session in this repository has changed anything matching it.',
        'What would prove it: a later change to a path this names.',
      ],
    },
    ask: {
      mgr: [
        'One request: "make the shipping rate include a handling fee"',
        'Loose thread: nothing was done about it.',
        'What would prove it: any work in the record about it.',
      ],
      op: [
        'You asked: "make the shipping rate include a handling fee"',
        'Loose thread: nothing in the record refers to it — no claim, no promise, no file it named.',
        'What would prove it: a claim, a promise, or a change to something it named.',
      ],
    },
  },
  /* spec-doctrine F-1 · record-descriptive, one per key. `session` is deleted:
   * the session line already speaks. */
  commentary: {
    findings: 'Five findings attach to this claim. Each one names what the record shows, not what the agent meant.',
    clm: 'The claim is about the whole suite; the run named one test.',
    f_removed: 'The assertion is present at the base tree and absent at the final tree.',
    f_weak: 'The strongest assertion in this test changed from equality to containment.',
    f_narrow: 'The command carries -k, so it ran a subset of the suite.',
    f_skip: 'One of two tests was skipped rather than run.',
    prm: 'Nothing later in this repository has touched what this names.',
    chk: 'One check ran, on the final tree, at its stated scope.',
  },
  frames: [
    { id: 'request', title: 'Request', x: 0, y: 0, w: 250, h: 400 },
    { id: 'changes', title: 'Changes', x: 290, y: 0, w: 820, h: 400, grow: 260 },
    { id: 'execution', title: 'Execution', x: 290, y: 430, w: 820, h: 420, shift: 260 },
    { id: 'claims', title: 'Claims & evidence', x: 1150, y: 0, w: 280, h: 850, grow: 260 },
  ],
  nodes: [
    {
      id: 'ev4',
      frame: 'request',
      x: 20,
      y: 36,
      w: 210,
      fam: 'grey',
      type: 'EVENT',
      sub: 'Session start',
      seq: 4,
      title: 'session started',
      lines: [],
      pinsL: [],
      pinsR: [{ id: 'o', label: 'order', t: 'order' }],
      foot: 'seq 4 · session_start',
    },
    {
      id: 'ev8',
      frame: 'request',
      x: 20,
      y: 150,
      w: 210,
      fam: 'blue',
      type: 'EVENT',
      sub: 'Prompt',
      seq: 8,
      title: '“make the shipping rate include a handling fee”',
      lines: [],
      pinsL: [{ id: 'i', label: 'order', t: 'order' }],
      pinsR: [
        { id: 'o', label: 'order', t: 'order' },
        { id: 'span', label: 'span', t: 'ref', c: 'blue' },
      ],
      foot: 'seq 8 · user_prompt',
    },
    {
      id: 'ask',
      frame: 'request',
      x: 20,
      y: 282,
      w: 210,
      fam: 'blue',
      type: 'DERIVED',
      sub: 'Ask · feature',
      word: 'loose',
      clause: CLAUSE_ASK,
      title: '“make the shipping rate include a handling fee”',
      /* F-10 · the record enum `status untouched` is gone; the sentence the
       * design already carried says the same thing in the reader's words. */
      lines: ['nothing in the record refers to it'],
      pinsL: [{ id: 'span', label: 'from prompt 1', t: 'inf', c: 'blue' }],
      pinsR: [],
      foot: ASK,
      objId: ASK,
    },
    {
      id: 'tr',
      frame: 'changes',
      x: 580,
      y: 36,
      w: 200,
      fam: 'grey',
      type: 'EVENT GROUP',
      sub: 'Tool results ×3',
      seq: 12,
      group: true,
      members: ['ev12', 'ev16', 'ev40'],
      title: 'Edit ok · Edit ok · Bash ok',
      lines: ['seq 12 · 16 · 40'],
      pinsL: [{ id: 'i', label: 'order', t: 'order' }],
      pinsR: [{ id: 'o', label: 'order', t: 'order' }],
      foot: '3 tool_result events · collapsed',
    },
    {
      id: 'ev12',
      frame: 'changes',
      x: 580,
      y: 36,
      w: 200,
      fam: 'grey',
      type: 'EVENT',
      sub: 'Tool result',
      seq: 12,
      member: 'tr',
      title: 'Edit ok',
      lines: [],
      pinsL: [{ id: 'i', label: 'order', t: 'order' }],
      pinsR: [{ id: 'o', label: 'order', t: 'order' }],
      foot: 'seq 12 · tool_result',
    },
    {
      id: 'ev16',
      frame: 'changes',
      x: 580,
      y: 118,
      w: 200,
      fam: 'grey',
      type: 'EVENT',
      sub: 'Tool result',
      seq: 16,
      member: 'tr',
      title: 'Edit ok',
      lines: [],
      pinsL: [{ id: 'i', label: 'order', t: 'order' }],
      pinsR: [{ id: 'o', label: 'order', t: 'order' }],
      foot: 'seq 16 · tool_result',
    },
    {
      id: 'ev40',
      frame: 'changes',
      x: 580,
      y: 200,
      w: 200,
      fam: 'grey',
      type: 'EVENT',
      sub: 'Tool result',
      seq: 40,
      member: 'tr',
      title: 'Bash ok',
      lines: [],
      pinsL: [{ id: 'i', label: 'order', t: 'order' }],
      pinsR: [{ id: 'o', label: 'order', t: 'order' }],
      foot: 'seq 40 · tool_result',
    },
    {
      id: 'ev13',
      frame: 'changes',
      x: 310,
      y: 36,
      w: 210,
      fam: 'teal',
      type: 'EVENT',
      sub: 'File change',
      seq: 13,
      title: 'modify shipping.py',
      lines: [],
      pinsL: [{ id: 'i', label: 'order', t: 'order' }],
      pinsR: [
        { id: 'o', label: 'order', t: 'order' },
        { id: 'path', label: 'path', t: 'path', c: 'teal' },
      ],
      foot: 'seq 13 · fs_change',
      file: 'shipping.py',
    },
    {
      id: 'ev17',
      frame: 'changes',
      x: 310,
      y: 150,
      w: 210,
      fam: 'teal',
      type: 'EVENT',
      sub: 'File change',
      seq: 17,
      title: 'modify test_shipping.py',
      lines: [],
      pinsL: [{ id: 'i', label: 'order', t: 'order' }],
      pinsR: [
        { id: 'o', label: 'order', t: 'order' },
        { id: 'path', label: 'path', t: 'path', c: 'teal' },
      ],
      foot: 'seq 17 · fs_change',
      file: 'test_shipping.py',
    },
    {
      id: 'snap',
      frame: 'changes',
      x: 310,
      y: 268,
      w: 210,
      fam: 'teal',
      type: 'EVENT GROUP',
      sub: 'Snapshots ×5',
      seq: 14,
      group: true,
      members: ['ev14', 'ev18', 'ev28', 'ev36', 'ev41'],
      rep: 'ev41',
      title: 'tree c324df66d87a',
      lines: ['seq 14 · 18 · 28 · 36 · 41', '3 taken at fold · same tree throughout'],
      pinsL: [{ id: 'i', label: 'order', t: 'order' }],
      pinsR: [
        { id: 'o', label: 'order', t: 'order' },
        { id: 'tree', label: 'tree', t: 'path', c: 'teal' },
      ],
      foot: '5 snapshot events · collapsed',
    },
    {
      id: 'ev14',
      frame: 'changes',
      x: 310,
      y: 268,
      w: 210,
      fam: 'teal',
      type: 'EVENT',
      sub: 'Snapshot',
      seq: 14,
      member: 'snap',
      title: 'tree c324df66d87a (taken at fold)',
      lines: [],
      pinsL: [{ id: 'i', label: 'order', t: 'order' }],
      pinsR: [{ id: 'o', label: 'order', t: 'order' }],
      foot: 'seq 14 · snapshot',
    },
    {
      id: 'ev18',
      frame: 'changes',
      x: 310,
      y: 350,
      w: 210,
      fam: 'teal',
      type: 'EVENT',
      sub: 'Snapshot',
      seq: 18,
      member: 'snap',
      title: 'tree c324df66d87a (taken at fold)',
      lines: [],
      pinsL: [{ id: 'i', label: 'order', t: 'order' }],
      pinsR: [{ id: 'o', label: 'order', t: 'order' }],
      foot: 'seq 18 · snapshot',
    },
    {
      id: 'ev28',
      frame: 'changes',
      x: 310,
      y: 432,
      w: 210,
      fam: 'teal',
      type: 'EVENT',
      sub: 'Snapshot',
      seq: 28,
      member: 'snap',
      title: 'tree c324df66d87a',
      lines: [],
      pinsL: [{ id: 'i', label: 'order', t: 'order' }],
      pinsR: [{ id: 'o', label: 'order', t: 'order' }],
      foot: 'seq 28 · snapshot',
    },
    {
      id: 'ev36',
      frame: 'changes',
      x: 310,
      y: 514,
      w: 210,
      fam: 'teal',
      type: 'EVENT',
      sub: 'Snapshot',
      seq: 36,
      member: 'snap',
      title: 'tree c324df66d87a',
      lines: [],
      pinsL: [{ id: 'i', label: 'order', t: 'order' }],
      pinsR: [{ id: 'o', label: 'order', t: 'order' }],
      foot: 'seq 36 · snapshot',
    },
    {
      id: 'ev41',
      frame: 'changes',
      x: 310,
      y: 596,
      w: 210,
      fam: 'teal',
      type: 'EVENT',
      sub: 'Snapshot',
      seq: 41,
      member: 'snap',
      title: 'tree c324df66d87a (taken at fold)',
      lines: ['final tree · bound to the receipt'],
      pinsL: [{ id: 'i', label: 'order', t: 'order' }],
      pinsR: [
        { id: 'o', label: 'order', t: 'order' },
        { id: 'tree', label: 'tree', t: 'path', c: 'teal' },
      ],
      foot: 'seq 41 · snapshot',
    },
    {
      id: 'f_removed',
      frame: 'changes',
      x: 850,
      y: 150,
      w: 230,
      fam: 'red',
      type: 'DERIVED',
      sub: 'Finding · fail',
      title: 'Assertion removed',
      lines: ['`test_rate_large` had 1 assertion(s) at the base tree and 0 at the final tree', 'Why it is not proven'],
      pinsL: [{ id: 'path', label: 'path', t: 'path', c: 'teal' }],
      pinsR: [{ id: 'claim', label: 'on claim', t: 'ref', c: 'red' }],
      foot: 'assertion_removed · test_shipping.py',
      file: 'test_shipping.py',
      finding: 'assertion_removed',
    },
    {
      id: 'f_weak',
      frame: 'changes',
      x: 850,
      y: 282,
      w: 230,
      fam: 'ochre',
      type: 'DERIVED',
      sub: 'Finding · warn',
      title: 'Assertion weakened',
      lines: ["`test_rate_small`'s strongest assertion went from Equality to Containment", 'Worth knowing'],
      pinsL: [{ id: 'path', label: 'path', t: 'path', c: 'teal' }],
      pinsR: [{ id: 'claim', label: 'on claim', t: 'ref', c: 'ochre' }],
      foot: 'assertion_weakened · test_shipping.py',
      file: 'test_shipping.py',
      finding: 'assertion_weakened',
    },
    {
      id: 'ev20',
      frame: 'execution',
      x: 310,
      y: 466,
      w: 220,
      fam: 'ochre',
      type: 'EVENT',
      sub: 'Tool call',
      seq: 20,
      title: 'Bash {"command":"pytest -q -k test_rate_small"}',
      lines: [],
      pinsL: [{ id: 'i', label: 'order', t: 'order' }],
      pinsR: [
        { id: 'o', label: 'order', t: 'order' },
        { id: 'cmd', label: 'command', t: 'ref', c: 'ochre' },
      ],
      foot: 'seq 20 · tool_call',
      mono: true,
    },
    {
      id: 'ev24',
      frame: 'execution',
      x: 310,
      y: 596,
      w: 220,
      fam: 'grey',
      type: 'EVENT',
      sub: 'Note',
      seq: 24,
      title: 'exec_begin f8c40ed31b74…',
      lines: [],
      pinsL: [{ id: 'i', label: 'order', t: 'order' }],
      pinsR: [{ id: 'o', label: 'order', t: 'order' }],
      foot: 'seq 24 · note',
      mono: true,
    },
    {
      id: 'ev32',
      frame: 'execution',
      x: 570,
      y: 466,
      w: 220,
      fam: 'ochre',
      type: 'EVENT',
      sub: 'Command ran',
      seq: 32,
      title: 'pytest -q -k test_rate_small → exit 0',
      lines: [],
      pinsL: [
        { id: 'i', label: 'order', t: 'order' },
        { id: 'cmd', label: 'command', t: 'ref', c: 'ochre' },
      ],
      pinsR: [
        { id: 'o', label: 'order', t: 'order' },
        { id: 'res', label: 'captured result', t: 'ref', c: 'ochre' },
      ],
      foot: 'seq 32 · exec',
      mono: true,
    },
    {
      id: 'chk',
      frame: 'execution',
      x: 840,
      y: 466,
      w: 240,
      fam: 'ochre',
      type: 'RECEIPT',
      sub: 'Check · test',
      title: 'pytest · exit 0',
      lines: ['1 passed · 0 failed · 1 skipped · 2 total', 'tree c324df66… = the final tree', 'not re-executed'],
      pinsL: [
        { id: 'res', label: 'result', t: 'ref', c: 'ochre' },
        { id: 'tree', label: 'tree', t: 'path', c: 'teal' },
      ],
      pinsR: [
        { id: 'ev', label: 'evidence', t: 'ref', c: 'red' },
        { id: 'counts', label: 'counts', t: 'ref', c: 'ochre' },
        { id: 'rcpt', label: 'in receipt', t: 'ref', c: 'green' },
      ],
      foot: CHK,
      objId: CHK,
    },
    {
      id: 'f_narrow',
      frame: 'execution',
      x: 570,
      y: 610,
      w: 230,
      fam: 'red',
      type: 'DERIVED',
      sub: 'Finding · fail',
      title: 'Narrowed run',
      lines: [
        '`pytest -q -k test_rate_small` narrows the run with `-k`, so it exercised a subset of the suite',
        'Why it is not proven',
      ],
      pinsL: [{ id: 'cmd', label: 'names command', t: 'inf', c: 'ochre' }],
      pinsR: [{ id: 'claim', label: 'on claim', t: 'ref', c: 'red' }],
      foot: 'narrowed_run',
      finding: 'narrowed_run',
    },
    {
      id: 'f_skip',
      frame: 'execution',
      x: 840,
      y: 610,
      w: 240,
      fam: 'ochre',
      type: 'DERIVED',
      sub: 'Finding · warn',
      title: 'Skipped tests',
      lines: ['1 skipped and 0 xfailed of 2 — they were not run', 'Worth knowing'],
      pinsL: [{ id: 'counts', label: 'counts', t: 'inf', c: 'ochre' }],
      pinsR: [{ id: 'claim', label: 'on claim', t: 'ref', c: 'ochre' }],
      foot: 'skipped_tests',
      finding: 'skipped_tests',
    },
    {
      id: 'f_cov',
      frame: 'execution',
      x: 310,
      y: 716,
      w: 220,
      fam: 'grey',
      type: 'DERIVED',
      sub: 'Finding · info',
      title: 'Coverage not measured',
      lines: ['For the record — informational, not a failed check'],
      pinsL: [],
      pinsR: [{ id: 'claim', label: 'on claim', t: 'ref', c: 'grey' }],
      foot: 'coverage_not_measured',
      finding: 'coverage_not_measured',
    },
    {
      id: 'ev44',
      frame: 'claims',
      x: 1170,
      y: 36,
      w: 240,
      fam: 'violet',
      type: 'EVENT',
      sub: 'Stopped',
      seq: 44,
      title: 'stopped: Added the handling fee to the shipping rate. All tests pass.…',
      lines: ['agent_finished — the agent finished its turn'],
      pinsL: [{ id: 'i', label: 'order', t: 'order' }],
      pinsR: [
        { id: 'span', label: 'claim span', t: 'ref', c: 'violet' },
        { id: 'span2', label: 'promise', t: 'ref', c: 'violet' },
      ],
      foot: 'seq 44 · stop · ' + EVT_STOP,
    },
    {
      id: 'clm',
      frame: 'claims',
      x: 1170,
      y: 200,
      w: 240,
      fam: 'red',
      type: 'DERIVED',
      sub: 'Claim · tests_pass',
      word: 'not_proven',
      clause: CLAUSE_CLAIM,
      title: '“All tests pass.”',
      lines: ['scope all · extracted by rules', 'ran on the final tree · 1/2 passed'],
      pinsL: [
        { id: 'ev', label: 'evidence', t: 'ref', c: 'red' },
        { id: 'fnd', label: 'findings ×5', t: 'ref', c: 'red' },
        { id: 'span', label: 'span', t: 'inf', c: 'violet' },
      ],
      pinsR: [{ id: 'rcpt', label: 'in receipt', t: 'ref', c: 'green' }],
      foot: CLM,
      objId: CLM,
    },
    {
      id: 'prm',
      frame: 'claims',
      x: 1170,
      y: 410,
      w: 240,
      fam: 'violet',
      type: 'DERIVED',
      sub: 'Promise · follow_up',
      word: 'loose',
      clause: CLAUSE_PROMISE,
      title: '“I\'ll add a regression test for the large-weight case in a follow-up.”',
      /* F-10 · `status open` was the record's enum; this is the sentence the
       * promise's own `followUp` already carries. */
      lines: ['no later session in this repository has changed anything matching it'],
      pinsL: [{ id: 'span', label: 'opened at', t: 'inf', c: 'violet' }],
      pinsR: [],
      foot: PRM,
      objId: PRM,
    },
    {
      id: 'rcpt',
      frame: 'claims',
      x: 1170,
      y: 580,
      w: 240,
      fam: 'green',
      type: 'RECEIPT',
      sub: 'DSSE · intact',
      title: 'ThruLine-Receipt: sha256:c8d23871…',
      lines: ['13 of 13 checks confirmed here', 'conclusion neutral · exit 0', 'signed · key sha256:f37e9ee5… (a hint)'],
      pinsL: [
        { id: 'claim', label: 'verdict', t: 'ref', c: 'green' },
        { id: 'check', label: 'check', t: 'ref', c: 'green' },
        { id: 'tree', label: 'subject tree', t: 'path', c: 'teal' },
      ],
      pinsR: [],
      foot: 'execution-check/v0.1 · verifier 0.1.0 · local',
      objId: 'receipt',
    },
  ],
  wires: [
    {
      id: 'w_ask',
      from: ['ev8', 'span'],
      to: ['ask', 'span'],
      kind: 'inf',
      c: 'blue',
      label: 'placed at prompt 1 by order · span evt_6QC1… chars 0–45',
    },
    {
      id: 'w_clmspan',
      from: ['ev44', 'span'],
      to: ['clm', 'span'],
      kind: 'inf',
      c: 'violet',
      label: 'placed at the stop event · span evt_4D14… chars 45–59',
    },
    { id: 'w_prm', from: ['ev44', 'span2'], to: ['prm', 'span'], kind: 'inf', c: 'violet', label: 'opened at evt_4D14… (the stop event)' },
    { id: 'w_cmdres', from: ['ev20', 'cmd'], to: ['ev32', 'cmd'], kind: 'ref', c: 'ochre', label: 'command → captured result' },
    {
      id: 'w_execchk',
      from: ['ev32', 'res'],
      to: ['chk', 'res'],
      kind: 'inf',
      c: 'ochre',
      label: 'same runner and exit 0 · the check is a ledger row (check_provenance) · join by id not exported',
    },
    { id: 'w_tree', from: ['snap', 'tree'], to: ['chk', 'tree'], kind: 'ref', c: 'teal', label: 'input_tree_hash c324df66… = the final tree' },
    { id: 'w_ev', from: ['chk', 'ev'], to: ['clm', 'ev'], kind: 'ref', c: 'red', label: 'claims[].evidence → chk_4KJW…' },
    { id: 'w_f1', from: ['f_removed', 'claim'], to: ['clm', 'fnd'], kind: 'ref', c: 'red', label: 'finding assertion_removed · fail' },
    { id: 'w_f2', from: ['f_weak', 'claim'], to: ['clm', 'fnd'], kind: 'ref', c: 'ochre', label: 'finding assertion_weakened · warn' },
    { id: 'w_f3', from: ['f_narrow', 'claim'], to: ['clm', 'fnd'], kind: 'ref', c: 'red', label: 'finding narrowed_run · fail' },
    { id: 'w_f4', from: ['f_skip', 'claim'], to: ['clm', 'fnd'], kind: 'ref', c: 'ochre', label: 'finding skipped_tests · warn' },
    { id: 'w_f5', from: ['f_cov', 'claim'], to: ['clm', 'fnd'], kind: 'ref', c: 'grey', label: 'finding coverage_not_measured · info' },
    { id: 'w_p1', from: ['ev17', 'path'], to: ['f_removed', 'path'], kind: 'ref', c: 'teal', label: 'path test_shipping.py' },
    { id: 'w_p2', from: ['ev17', 'path'], to: ['f_weak', 'path'], kind: 'ref', c: 'teal', label: 'path test_shipping.py' },
    { id: 'w_ncmd', from: ['ev20', 'cmd'], to: ['f_narrow', 'cmd'], kind: 'inf', c: 'ochre', label: 'names `pytest -q -k test_rate_small` (by text)' },
    { id: 'w_skc', from: ['chk', 'counts'], to: ['f_skip', 'counts'], kind: 'inf', c: 'ochre', label: '1 skipped of 2 (by text)' },
    { id: 'w_r1', from: ['clm', 'rcpt'], to: ['rcpt', 'claim'], kind: 'ref', c: 'green', label: 'predicate.verdicts[] · clm_7ADJ…' },
    { id: 'w_r2', from: ['chk', 'rcpt'], to: ['rcpt', 'check'], kind: 'ref', c: 'green', label: 'predicate.checks[] · chk_4KJW…' },
    { id: 'w_r3', from: ['snap', 'tree'], to: ['rcpt', 'tree'], kind: 'ref', c: 'green', label: 'subject.digest.gitTree c324df66…' },
  ],
  evidencePath: ['clm', 'chk', 'ev32', 'ev20', 'snap', 'ev41', 'rcpt', 'f_narrow', 'f_skip', 'f_removed', 'f_weak', 'f_cov', 'ev17', 'ev44'],
}

/* ---------- demo-honest ---------- */

const HCLM = 'clm_4JN7B0R7XF00XEMYVFKDQ66MHZ'
const HCHK = 'chk_2R971YDQX05RQF9WA23MTRSE7Y'

const honest = {
  key: 'demo-honest',
  label: 'demo-honest',
  project: 'demo',
  id: null,
  idNote: 'session id not exported in this reading',
  origin: 'Demo records · fixture demo-honest · pinned core 29f83b6 / p3 cc69822',
  harness: 'Claude Code',
  harnessRaw: 'claude-code',
  started: null,
  ended: null,
  eventCount: null,
  eventsComplete: false,
  eventsNote:
    'Recorded event list for this fixture is not in this reading (explain excerpt only). No events were invented.',
  finalTree: '0c380073fc4b…',
  commit: null,
  chainHead: null,
  capture: {
    sources: ['claude-code-hook', 'exec-recorder'],
    redactions: 0,
    degraded: false,
    condition: 'intact',
    recorderLine: 'The recorder was working for this session.',
  },
  exits: { explain: 0, claims: 0, narrate: 0, verifyReceipt: 0 },
  sessionWord: 'proven',
  countLine: '1 loose thread · 1 proven claim',
  events: [],
  stop: {
    kind: 'agent_finished',
    summary: 'the agent finished its turn',
    seq: null,
    lastEvents: [],
    text: 'Added the handling fee to the shipping rate and updated both…',
  },
  compactions: 0,
  asks: [
    {
      id: null,
      idNote: 'ask id not exported in this reading',
      kind: 'feature',
      status: 'untouched',
      word: 'loose',
      text: 'make the shipping rate include a handling fee and update both tests to match',
      eventId: null,
      offset: null,
      extractor: 'rules',
      node: 'ask',
      limitation:
        'Resolver limitation (documented): the ask stays untouched although the same session\'s claim is Proven with a full run — the resolver could not link the ask to the claim or the edits. Not the whole project cleared.',
    },
  ],
  claims: [
    {
      id: HCLM,
      kind: 'tests_pass',
      text: 'All tests pass.',
      verdict: 'supported',
      word: 'proven',
      verifier: 'tests_pass@1',
      scope: 'all',
      extractor: 'rules',
      eventId: null,
      offset: null,
      seq: null,
      evidence: [HCHK],
      binding: 'ran_on_final_tree',
      reasons: ['ran on the final tree', '2/2 passed'],
      warnings: [],
      findings: [
        {
          key: 'coverage_not_measured',
          name: 'Coverage not measured',
          severity: 'info',
          detail: '',
          derivedDetail:
            'Informational. The record holds no coverage measurement for this run (detector detail not quoted in this reading).',
          ref: 'check ' + HCHK,
          node: 'f_cov',
        },
      ],
      node: 'clm',
      plain: 'The recorded command ran the whole suite on the final tree. Two of two tests passed.',
      prove: 'Nothing. The run is bound to the tree you are reading. This proves the declared check, not the whole project.',
      nextCheck: null,
    },
  ],
  promises: [],
  checks: [
    {
      id: HCHK,
      category: 'test',
      runner: 'pytest',
      command: 'pytest -q',
      scopeNote: 'whole suite',
      exit: 0,
      argvHash: null,
      inputTree: '0c380073fc4b…',
      bound: true,
      bindingLabel: 'ran on the final tree',
      reexecuted: false,
      parsed: { passed: 2, failed: 0, skipped: 0, tests_total: 2, xfail: 0 },
      node: 'chk',
      seqHint: 'exec seq not in this reading',
    },
  ],
  testChanges: [],
  receipt: {
    trailer: null,
    keyid: 'sha256:f37e9ee5…',
    payloadType: 'application/vnd.in-toto+json',
    predicateType: 'https://thrulinehq.com/attestation/execution-check/v0.1',
    subjectName: 'git+/Users/demo/demo-workspace/fixtures/raw/demo/…',
    verifierVersion: '0.1.0',
    mode: 'local',
    policyHash: POLICY_HASH,
    policyInForce: false,
    promisesOpen: [],
    promisesFulfilled: [],
    node: 'rcpt',
  },
  verify: {
    rows: verifyRowsHonest,
    conclusion: 'success',
    verified: true,
    status: 'verified',
    exit: 0,
    integrity: 'verified',
    integrityExit: 0,
    claims: { total: 1, supported: 1, unsupported: 0, unverifiable: 0 },
    policyEvaluated: false,
    policyNote: 'no policy was evaluated, so no `failure` can come from one',
    integrityNote: 'gate on `conclusion`, never on `exit`.',
  },
  narr: {
    session: {
      mgr: ['The agent reported 1 thing done.', 'Proven: the record backs all of them.', 'Nothing is outstanding here.'],
      op: [
        'The agent made 1 completion claim in this session.',
        'Proven: the record backs every one of them.',
        'Nothing. This session is covered.',
      ],
    },
    stop: {
      mgr: ['The agent finished on its own.', 'Nothing interrupted it.', 'Judge the work by the claims, not by the clean ending.'],
      op: [
        'The agent stopped on its own, saying: "Added the handling fee to the shipping rate and updated both…"',
        'The record shows a normal finish: the agent finished its turn',
        'Read the claims below; a normal finish is not a proof of anything.',
      ],
    },
    verdict: {
      mgr: [
        'The agent reported the tests passing.',
        'Proven: a test run on the final code backs that up.',
        'Nothing is outstanding here.',
      ],
      op: [
        'The agent wrote: "All tests pass."',
        'Proven: the record shows 2 of 2 tests passing on the final tree, at 15:20.',
        'Nothing. The run is bound to the tree you are reading.',
      ],
    },
    promise: null,
    ask: {
      mgr: [
        'One request: "make the shipping rate include a handling fee and update both tests to match"',
        'Loose thread: nothing was done about it.',
        'What would prove it: any work in the record about it.',
      ],
      op: [
        'You asked: "make the shipping rate include a handling fee and update both tests to match"',
        'Loose thread: nothing in the record refers to it — no claim, no promise, no file it named.',
        'What would prove it: a claim, a promise, or a change to something it named.',
      ],
    },
  },
  /* spec-doctrine F-1 · the `demo-honest clm` / `chk` row. */
  commentary: {
    clm: 'The run is bound to the tree you are reading.',
    chk: 'The run is bound to the tree you are reading.',
  },
  frames: [
    { id: 'request', title: 'Request', x: 0, y: 0, w: 250, h: 300 },
    { id: 'execution', title: 'Execution', x: 290, y: 0, w: 560, h: 300 },
    { id: 'claims', title: 'Claims & evidence', x: 890, y: 0, w: 280, h: 560 },
  ],
  nodes: [
    {
      id: 'ev_prompt',
      frame: 'request',
      x: 20,
      y: 36,
      w: 210,
      fam: 'blue',
      type: 'EVENT',
      sub: 'Prompt',
      title: '“make the shipping rate include a handling fee and update both tests to match”',
      lines: [],
      pinsL: [],
      pinsR: [{ id: 'span', label: 'span', t: 'ref', c: 'blue' }],
      foot: 'user_prompt · seq not in this reading',
    },
    {
      id: 'ask',
      frame: 'request',
      x: 20,
      y: 170,
      w: 210,
      fam: 'blue',
      type: 'DERIVED',
      sub: 'Ask · feature',
      word: 'loose',
      clause: CLAUSE_ASK,
      title: '“make the shipping rate include a handling fee and update both tests to match”',
      /* F-10 · `status untouched — resolver could not link` was two record
       * enums and an internal component name. */
      lines: ['nothing in the record refers to it'],
      pinsL: [{ id: 'span', label: 'from prompt 1', t: 'inf', c: 'blue' }],
      pinsR: [],
      foot: 'ask id not exported',
    },
    {
      id: 'chk',
      frame: 'execution',
      x: 310,
      y: 36,
      w: 240,
      fam: 'ochre',
      type: 'RECEIPT',
      sub: 'Check · test',
      title: 'pytest -q · exit 0',
      lines: ['2 passed · 0 failed · 0 skipped · 2 total', 'tree 0c380073… = the final tree', 'not re-executed'],
      pinsL: [],
      pinsR: [
        { id: 'ev', label: 'evidence', t: 'ref', c: 'green' },
        { id: 'rcpt', label: 'in receipt', t: 'ref', c: 'green' },
      ],
      foot: HCHK,
      objId: HCHK,
    },
    {
      id: 'f_cov',
      frame: 'execution',
      x: 590,
      y: 36,
      w: 230,
      fam: 'grey',
      type: 'DERIVED',
      sub: 'Finding · info',
      title: 'Coverage not measured',
      lines: ['For the record — informational, not a failed check'],
      pinsL: [],
      pinsR: [{ id: 'claim', label: 'on claim', t: 'ref', c: 'grey' }],
      foot: 'coverage_not_measured',
      finding: 'coverage_not_measured',
    },
    {
      id: 'ev_stop',
      frame: 'claims',
      x: 910,
      y: 36,
      w: 240,
      fam: 'violet',
      type: 'EVENT',
      sub: 'Stopped',
      title: 'stopped: Added the handling fee to the shipping rate and updated both…',
      lines: ['agent_finished — the agent finished its turn'],
      pinsL: [],
      pinsR: [{ id: 'span', label: 'claim span', t: 'ref', c: 'violet' }],
      foot: 'stop · seq not in this reading',
    },
    {
      id: 'clm',
      frame: 'claims',
      x: 910,
      y: 200,
      w: 240,
      fam: 'green',
      type: 'DERIVED',
      sub: 'Claim · tests_pass',
      word: 'proven',
      title: '“All tests pass.”',
      lines: ['scope all · extracted by rules', 'ran on the final tree · 2/2 passed'],
      pinsL: [
        { id: 'ev', label: 'evidence', t: 'ref', c: 'green' },
        { id: 'fnd', label: 'findings ×1', t: 'ref', c: 'grey' },
        { id: 'span', label: 'span', t: 'inf', c: 'violet' },
      ],
      pinsR: [{ id: 'rcpt', label: 'in receipt', t: 'ref', c: 'green' }],
      foot: HCLM,
      objId: HCLM,
    },
    {
      id: 'rcpt',
      frame: 'claims',
      x: 910,
      y: 400,
      w: 240,
      fam: 'green',
      type: 'RECEIPT',
      sub: 'DSSE · intact',
      /* F-8 · integrity is not a state word, and `Receipt intact` is the one
       * wording everywhere. */
      title: 'Receipt intact',
      lines: ['13 of 13 checks confirmed here', 'conclusion success · exit 0'],
      pinsL: [
        { id: 'claim', label: 'verdict', t: 'ref', c: 'green' },
        { id: 'check', label: 'check', t: 'ref', c: 'green' },
      ],
      pinsR: [],
      foot: 'execution-check/v0.1 · verifier 0.1.0 · local',
      objId: 'receipt',
    },
  ],
  wires: [
    { id: 'w_ask', from: ['ev_prompt', 'span'], to: ['ask', 'span'], kind: 'inf', c: 'blue', label: 'placed at prompt 1 by order' },
    { id: 'w_clmspan', from: ['ev_stop', 'span'], to: ['clm', 'span'], kind: 'inf', c: 'violet', label: 'placed at the stop event' },
    { id: 'w_ev', from: ['chk', 'ev'], to: ['clm', 'ev'], kind: 'ref', c: 'green', label: 'claims[].evidence → chk_2R97…' },
    { id: 'w_f5', from: ['f_cov', 'claim'], to: ['clm', 'fnd'], kind: 'ref', c: 'grey', label: 'finding coverage_not_measured · info' },
    { id: 'w_r1', from: ['clm', 'rcpt'], to: ['rcpt', 'claim'], kind: 'ref', c: 'green', label: 'predicate.verdicts[] · clm_4JN7…' },
    { id: 'w_r2', from: ['chk', 'rcpt'], to: ['rcpt', 'check'], kind: 'ref', c: 'green', label: 'predicate.checks[] · chk_2R97…' },
  ],
  evidencePath: ['clm', 'chk', 'rcpt', 'f_cov'],
}

/* ---------- real-fog-redacted ---------- */

const fog = {
  key: 'real-fog-redacted',
  label: 'real-fog-redacted',
  project: 'demo-workspace',
  id: 'cccccccc-cccc-4ccc-8ccc-000000000552',
  origin: 'Local fixture — text withheld · derived from real-fog · imported after the fact',
  harness: 'Claude Code',
  harnessRaw: 'claude-code',
  started: '2026-01-15T09:00:00.000Z',
  ended: null,
  endedNote: 'no ending recorded (receipt carries the epoch sentinel)',
  eventCount: 582,
  eventsComplete: false,
  eventsNote:
    '582 events in the record · 4 enumerated in this reading (the flagged ones) · 578 not enumerated — open the full record with `thruline explain --full`.',
  finalTree: null,
  commit: null,
  chainHead: 'sha256:…',
  capture: {
    /* spec-data §0 · the demo says one source; `real-fog/receipt.json`'s decoded
     * predicate says two. The fixture wins. */
    sources: ['claude-code-hook', 'exec-recorder'],
    redactions: 0,
    degraded: true,
    condition: 'degraded — cause not recorded',
    recorderLine: 'The recorder is not vouched for this session — cause not recorded.',
  },
  exits: { explain: 0, claims: 0, narrate: 0, verifyReceipt: 2 },
  sessionWord: null,
  countLine: 'recorded with gaps · 4 loose threads',
  events: [
    { seq: 553, kind: 'tool_call', text: '(text withheld)', risk: 'destructive command: `rm -rf`', tool: 'Bash' },
    { seq: 686, kind: 'tool_call', text: '(text withheld)', risk: 'destructive command: `rm -rf`', tool: 'Bash' },
    { seq: 727, kind: 'tool_call', text: '(text withheld)', risk: 'destructive command: `rm -rf`', tool: 'Bash' },
    { seq: 743, kind: 'tool_call', text: '(text withheld)', risk: 'destructive command: `rm -rf`', tool: 'Edit' },
  ],
  stop: {
    kind: 'no_stop_recorded',
    summary: 'no `stop` event is in the record: the session is still open, or the Stop hook never fired',
    seq: null,
    lastEvents: [
      'unknown ok',
      'note',
      'legacy `meta` rows record JSONL field names the parser did not recognise, not session content — folded to one census rather than imported as events',
    ],
    text: null,
    word: 'fog',
  },
  compactions: 0,
  asks: [1, 2, 3, 4].map((n) => ({
    id: null,
    idNote: 'ask id withheld',
    kind: 'not attributed in this reading (3 of 4 are questions)',
    status: 'untouched',
    word: 'loose',
    text: '(text withheld)',
    eventId: null,
    offset: null,
    extractor: 'rules',
    node: 'asks',
    n,
  })),
  unitsDropped: { total: 19, harness: 7, code: 0, unmatched: 11, wrapped: 1, fragment: 0 },
  claims: [],
  promises: [],
  checks: [],
  testChanges: [],
  receipt: {
    trailer: null,
    keyid: 'sha256:f37e9ee5…',
    payloadType: 'application/vnd.in-toto+json',
    predicateType: 'https://thrulinehq.com/attestation/execution-check/v0.1',
    subjectName: 'git+/Users/demo/demo-workspace',
    verifierVersion: '0.1.0',
    mode: 'local',
    policyHash: POLICY_HASH,
    policyInForce: false,
    promisesOpen: [],
    promisesFulfilled: [],
    node: 'rcpt',
    commitUnderReview: '592321f77c43d6cddbc9c9a30e8368eed445cccc',
  },
  verify: {
    rows: verifyRowsFog,
    conclusion: 'neutral',
    verified: false,
    status: 'partial',
    exit: 2,
    integrity: 'partial',
    integrityExit: 2,
    claims: { total: 0, supported: 0, unsupported: 0, unverifiable: 0 },
    policyEvaluated: false,
    policyNote: 'no policy was evaluated, so no `failure` can come from one',
    integrityNote:
      'PARTIALLY verified: nothing contradicts the receipt, and 2 check(s) could not be made on this machine. That is not the same as verified.',
  },
  narr: {
    session: {
      mgr: ['The agent did not report anything as done.', 'There is nothing to check.', 'Nothing is outstanding here.'],
      op: [
        'The agent made no completion claims in this session.',
        'The record holds 582 events and no claim to check against them.',
        'Nothing to verify. Read the timeline if you want to know what happened.',
      ],
    },
    stop: {
      mgr: [
        'There is no recorded ending for this session.',
        'Fog: it is either still running or was not being watched.',
        'What would prove it: a recording that covers how the session ended.',
      ],
      op: [
        'The record runs to the end with no stop event.',
        'Fog: either this session is still open, or the Stop hook never fired.',
        'What would prove it: a stop event for this session, from a hook that is installed.',
      ],
    },
    verdict: null,
    promise: null,
    ask: {
      mgr: [
        'One request: "(text withheld)"',
        'Loose thread: nothing was done about it.',
        'What would prove it: any work in the record about it.',
      ],
      op: [
        'You asked: "(text withheld)"',
        'Loose thread: nothing in the record refers to it — no claim, no promise, no file it named.',
        'What would prove it: a claim, a promise, or a change to something it named.',
      ],
    },
  },
  commentary: {},
  frames: [
    { id: 'request', title: 'Requests', x: 0, y: 0, w: 250, h: 220 },
    /* Was `Risk flags · recorded`. `recorded` filling a whole label slot is a
     * record enum used as a name (`lib/voice.js` ENUMS, `bannedIn`), banned by
     * CONTRACT §7 correction 10 — the correction this module is named as
     * carrying. The auto-layout emits the same corrected title
     * (`adapter.js:670-674`), so the demo frame and a real one now agree. */
    { id: 'risk', title: 'Risk flags', x: 290, y: 0, w: 1060, h: 220 },
    { id: 'claims', title: 'Ending & receipt', x: 290, y: 250, w: 560, h: 260 },
  ],
  nodes: [
    {
      id: 'asks',
      frame: 'request',
      x: 20,
      y: 36,
      w: 210,
      fam: 'blue',
      type: 'DERIVED GROUP',
      sub: 'Asks ×4',
      group: true,
      members: ['ask1', 'ask2', 'ask3', 'ask4'],
      word: 'loose',
      clause: CLAUSE_ASK,
      title: '(text withheld) ×4',
      /* F-10 · `all untouched → 4 loose threads` led with the record enum. */
      lines: ['4 loose threads', '3 of 4 are questions'],
      pinsL: [],
      pinsR: [],
      foot: 'asks.json · 19 units not read as asks',
    },
    {
      id: 'ask1',
      frame: 'request',
      x: 20,
      y: 36,
      w: 210,
      fam: 'blue',
      type: 'DERIVED',
      sub: 'Ask 1',
      member: 'asks',
      word: 'loose',
      clause: CLAUSE_ASK,
      title: '(text withheld)',
      lines: ['placed at prompt 1'],
      pinsL: [],
      pinsR: [],
      foot: 'ask id withheld',
    },
    {
      id: 'ask2',
      frame: 'request',
      x: 20,
      y: 130,
      w: 210,
      fam: 'blue',
      type: 'DERIVED',
      sub: 'Ask 2',
      member: 'asks',
      word: 'loose',
      clause: CLAUSE_ASK,
      title: '(text withheld)',
      lines: ['placed at prompt 2'],
      pinsL: [],
      pinsR: [],
      foot: 'ask id withheld',
    },
    {
      id: 'ask3',
      frame: 'request',
      x: 20,
      y: 224,
      w: 210,
      fam: 'blue',
      type: 'DERIVED',
      sub: 'Ask 3',
      member: 'asks',
      word: 'loose',
      clause: CLAUSE_ASK,
      title: '(text withheld)',
      lines: ['placed at prompt 3'],
      pinsL: [],
      pinsR: [],
      foot: 'ask id withheld',
    },
    {
      id: 'ask4',
      frame: 'request',
      x: 20,
      y: 318,
      w: 210,
      fam: 'blue',
      type: 'DERIVED',
      sub: 'Ask 4',
      member: 'asks',
      word: 'loose',
      clause: CLAUSE_ASK,
      title: '(text withheld)',
      lines: ['placed at prompt 4'],
      pinsL: [],
      pinsR: [],
      foot: 'ask id withheld',
    },
    {
      id: 'ev553',
      frame: 'risk',
      x: 310,
      y: 36,
      w: 240,
      fam: 'ochre',
      type: 'EVENT',
      sub: 'Tool call · Bash',
      seq: 553,
      risk: true,
      title: '(text withheld)',
      lines: ['destructive command: `rm -rf`'],
      pinsL: [],
      pinsR: [{ id: 'o', label: 'order', t: 'order' }],
      foot: 'seq 553 · tool_call · risk flag',
    },
    {
      id: 'ev686',
      frame: 'risk',
      x: 570,
      y: 36,
      w: 240,
      fam: 'ochre',
      type: 'EVENT',
      sub: 'Tool call · Bash',
      seq: 686,
      risk: true,
      title: '(text withheld)',
      lines: ['destructive command: `rm -rf`'],
      pinsL: [{ id: 'i', label: 'order', t: 'order' }],
      pinsR: [{ id: 'o', label: 'order', t: 'order' }],
      foot: 'seq 686 · tool_call · risk flag',
    },
    {
      id: 'ev727',
      frame: 'risk',
      x: 830,
      y: 36,
      w: 240,
      fam: 'ochre',
      type: 'EVENT',
      sub: 'Tool call · Bash',
      seq: 727,
      risk: true,
      title: '(text withheld)',
      lines: ['destructive command: `rm -rf`'],
      pinsL: [{ id: 'i', label: 'order', t: 'order' }],
      pinsR: [{ id: 'o', label: 'order', t: 'order' }],
      foot: 'seq 727 · tool_call · risk flag',
    },
    {
      id: 'ev743',
      frame: 'risk',
      x: 1090,
      y: 36,
      w: 240,
      fam: 'ochre',
      type: 'EVENT',
      sub: 'Tool call · Edit',
      seq: 743,
      risk: true,
      title: '(text withheld)',
      lines: ['destructive command: `rm -rf`'],
      pinsL: [{ id: 'i', label: 'order', t: 'order' }],
      pinsR: [],
      foot: 'seq 743 · tool_call · risk flag',
    },
    {
      id: 'ending',
      frame: 'claims',
      x: 310,
      y: 286,
      w: 240,
      fam: 'grey',
      type: 'DERIVED',
      sub: 'Ending',
      word: 'fog',
      clause: CLAUSE_ENDING,
      title: 'No ending recorded',
      /* F-10 · `no_stop_recorded — …` led with the record enum. */
      lines: ['either this session is still open, or the Stop hook never fired', '582 events · line dissolves'],
      pinsL: [],
      pinsR: [],
      foot: 'ses_cccccccc… · seq null',
      objId: 'ending',
    },
    {
      id: 'rcpt',
      frame: 'claims',
      x: 590,
      y: 286,
      w: 240,
      fam: 'grey',
      type: 'RECEIPT',
      sub: 'DSSE · partly checked',
      title: 'Receipt partly checked here',
      lines: [
        '11 confirmed · 2 could not be made here — not a pass',
        'no final tree — nothing to bind to',
        'conclusion neutral · exit 2',
      ],
      pinsL: [],
      pinsR: [],
      foot: 'capture degraded · cause not recorded',
      objId: 'receipt',
    },
  ],
  wires: [],
  evidencePath: ['ending', 'rcpt'],
}

/** Session keys, in the order the switcher lists them. */
export const DEMO_ORDER = Object.freeze(['demo-narrowed', 'demo-honest', 'real-fog-redacted'])

/**
 * The three bundled sessions, keyed by `DEMO_ORDER`.
 *
 * @type {Readonly<Record<string, Session>>}
 */
export const DEMO_SESSIONS = Object.freeze({
  'demo-narrowed': narrowed,
  'demo-honest': honest,
  'real-fog-redacted': fog,
})

/**
 * The banner text, verbatim from BRIEF:1599 (spec-doctrine F-13). Two strings,
 * and the design's own `demoLabel` ternary picks between them by whether the
 * origin names a local fixture.
 */
export const DEMO_LABEL = Object.freeze({
  /* R14: the design's fixture banner was `Local fixture — text withheld.`,
   * which says nothing about whose project the reader is looking at. Switching
   * to that record replaced the "not your project" banner with one that reads
   * like a note about redaction, on a surface that was still bundled data.
   * Doctrine 9 wants demo data bannered AS demo, permanently, so both labels
   * now carry that half of the sentence and only the second half differs. */
  fixture: 'Bundled fixture — text withheld, not your project',
  bundled: 'Demo session — a bundled run, not your project',
})

/**
 * The three corpus-level strings `window.THRULINE_DEMO` carries beside its
 * sessions, kept because the shell reads them: `autopsyHeader` is the Compare
 * pane's `autopsy` line, `pins` is the status strip's provenance line, and
 * `shapes` backs the shapes readout. Verbatim.
 *
 * NOT in CONTRACT.md §1's export column — recorded as an amendment rather than
 * dropped, because dropping them would leave `<ComparePane autopsy>` with no
 * source and the porter would invent one.
 */
export const DEMO_META = Object.freeze({
  autopsyHeader:
    '2 sessions read from /Users/demo/demo-workspace/fixtures/raw/demo/home/exec.db. Times are UTC. Nothing left this machine.',
  pins: 'thruline @ 29f83b60 (codex/build-spec-v1) · thruline-x @ cc698224 (codex/p3-narration-asks) · fixtures/MANIFEST.txt',
  shapes: Object.freeze({
    fixture: 35,
    head: 44,
    known: Object.freeze(['thruline.explain@1', 'thruline.claims@1', 'thruline.verify-receipt@1']),
  }),
})

/**
 * Look one up.
 *
 * @param {string} key a member of `DEMO_ORDER`
 * @returns {Session|null} null for an unknown key — never a fabricated session,
 *   and never a quiet fallback to the first one: showing a different session
 *   than the one asked for is a lie the reader cannot see
 */
export function demoSession(key) {
  return Object.prototype.hasOwnProperty.call(DEMO_SESSIONS, key) ? DEMO_SESSIONS[key] : null
}

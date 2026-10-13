/* A80 — the rendered-lexicon scan, made repeatable.
 *
 * M6.R1.1 found four leaks that no table test could see, because they were
 * composed at render time rather than stored in the voice table. They were
 * caught by reading the DOM by hand. Hand-reading is not a gate, so each one
 * becomes a named regression case here, and the general sweep runs `bannedIn`
 * over every string the shell composes plus every literal it hardcodes.
 *
 * This is not a DOM test — the UI has no DOM harness and adding one for four
 * strings would be more machinery than the rule is worth. It is the next
 * strongest thing: the actual composition functions, called with real fixture
 * shapes, plus a source scan of the copy the shell writes inline. */

import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

import {
  RECEIPT_STATE,
  STATES,
  claimMeta,
  fmt,
  ledgerLine,
  rendersWhenZero,
  sessionLine,
  verdictDetail,
  verdictLine,
} from './design.js'
import { bannedIn, chipLabel, composition, countPhrase, headline, word } from './voice.js'

const src = (name) =>
  readFileSync(fileURLToPath(new URL(`../${name}`, import.meta.url)), 'utf8')

/* M6.R2.4: the shell these four regressions guard is no longer one file.
 * `App.jsx` — the pre-R2 graph-hero shell they were written against — left
 * navigation at R2.1 and left disk at R2.4. The regressions did not go with it:
 * each one is re-pointed at the altitudes that replaced it, so a leak that once
 * shipped cannot ship again through a new door. */
/* RA5: the altitude shell is superseded by the macOS graph-hero shell; the
 * regressions re-point again, at the files that replaced it. */
/* The Advanced Trace workspace is the shell `main.jsx` boots into; its five
 * .jsx files carry visible copy, so the engine-lexicon sweep must reach them.
 * A new screen file not listed here is not lexicon-checked (CONTRACT §5 edit 4). */
const SHELL_FILES = ['mac/shell.jsx', 'mac/work-map.jsx', 'mac/decision-flow.jsx', 'mac/graph.jsx', 'mac/inspector.jsx', 'mac/sprites.jsx', 'mac/distilled.jsx', 'os/shell.jsx', 'cast.jsx', 'gallery.jsx', 'main.jsx', 'refresh.jsx', 'workspace/shell.jsx', 'workspace/chrome.jsx', 'workspace/canvas.jsx', 'workspace/docks.jsx', 'workspace/ui-kit.jsx']
const SHELL = SHELL_FILES.map((f) => [f, src(f)])
const DESIGN = src('lib/design.js')
const RECEIPT = src('lib/receipt.js')

/* Every string the shell composes, at the counts that matter: zero, one, many. */
function composed() {
  const out = []
  const add = (where, text) => out.push([where, String(text)])

  for (const events of [0, 1, 2, 777404]) {
    add(
      `ledger line (${events})`,
      countPhrase(fmt(events), 'event on the ledger', 'events on the ledger'),
    )
    add(
      `chain line (${events})`,
      countPhrase(fmt(events), 'event on the ledger', 'events on the ledger') + ' · chain checked',
    )
    add(`headline (${events})`, headline('thruline', events))
  }
  for (const s of STATES) {
    add(`verdict line ${s}`, verdictLine(s))
    add(`chip ${s}`, chipLabel(word(s)))
  }
  add('ledgerLine', ledgerLine({ event_no: 4, total: 777404, verified: true }))
  add('ledgerLine not confirmed', ledgerLine({ event_no: 4, total: 9, verified: false }))
  add('sessionLine', sessionLine({ label: 'Session 1', date: 'Aug 4', msgs: 12 }))
  add(
    'claimMeta',
    claimMeta({ label: 'Session 1' }, { claim: { message_no: 3, ts: '10:02' } }),
  )
  add('composition', composition(STATES.map((s, i) => [i, word(s)])))
  return out
}

/* JSX text nodes and quoted copy the shell writes inline, minus the things that
 * are plainly not prose: CSS declarations, single tokens, identifiers. */
function inlineCopy() {
  const out = []
  const looksLikeProse = (t) =>
    /[A-Za-z]/.test(t) &&
    t.includes(' ') &&
    !/[{}<>]/.test(t) &&
    // CSS declarations, units, custom properties, selectors: not copy.
    !/(px|rem|%|var\(|:\s*\d|--[a-z]|;|\/\*)/.test(t)
  for (const [file, text] of SHELL) {
    for (const [, t0] of text.matchAll(/>([^<>{}\n]{4,120})</g)) {
      const t = t0.trim()
      if (looksLikeProse(t)) out.push([`${file} text node`, t])
    }
    for (const [, t0] of text.matchAll(/'([^'\n]{4,120})'/g)) {
      const t = t0.trim()
      if (looksLikeProse(t)) out.push([`${file} literal`, t])
    }
  }
  return out
}

test('no string the shell composes carries engine lexicon', () => {
  const strings = composed()
  assert.ok(strings.length >= 30, `the sweep covered only ${strings.length} strings`)
  for (const [where, text] of strings) {
    const term = bannedIn(text)
    assert.equal(term, null, `engine lexicon ${term} in ${where}: ${JSON.stringify(text)}`)
  }
})

test('no copy the shell hardcodes carries engine lexicon', () => {
  const strings = inlineCopy()
  assert.ok(strings.length >= 5, `the sweep covered only ${strings.length} literals`)
  for (const [where, text] of strings) {
    const term = bannedIn(text)
    assert.equal(term, null, `engine lexicon ${term} in ${where}: ${JSON.stringify(text)}`)
  }
})

/* ── the four M6.R1.1 catches, as named cases ──────────────────────────── */

test('regression 1: the ledger line names no provider, model, or glyph state', () => {
  /* Was: `Ollama · <model> · Ledger ✓ 777,404 events`. Three leaks in one
   * string — the provider, a bare model tag, and a check mark standing in for a
   * word. Note the shape is asserted rather than routed through `bannedIn`:
   * "ollama" is deliberately NOT in the jargon list, because the settings
   * surface legitimately names the provider the user chose. The ban is about
   * first-level status copy, so first-level status copy is what this asserts. */
  const line =
    countPhrase(fmt(777404), 'event on the ledger', 'events on the ledger') +
    ' · chain intact'
  assert.match(line, /^777,404 events on the ledger · chain intact$/)
  assert.doesNotMatch(line, /Ollama|Ledger [✓✗]|\bmodel\b/i)
  assert.equal(bannedIn(line), null)
  /* And the old shape is still recognisably wrong, so the case has teeth. */
  assert.match('Ollama · llama3.2 · Ledger ✓ 777,404 events', /Ollama|Ledger ✓/)
})

test('regression 2: a zero count renders nothing at all', () => {
  /* Was: `NEEDS ATTENTION 0` — a naked number, and a zero one. Every `count:`
   * the shell hands a section header must be guarded by a positive test; the
   * guard is the rule, not each call site remembering it. */
  assert.equal(rendersWhenZero(), false)
  assert.equal(composition([[0, 'no proof found']]), '')
  assert.equal(countPhrase(0, 'thing', 'things'), '0 things')

  /* Re-pointed at the R2 altitudes. The old shell passed section counts as a
     `count:` prop; the new ones render a section only when it has contents, so
     the mechanical form of the rule is: every section that could be empty is
     behind a `> 0` test, and every number that reaches the DOM either carries
     its noun through the authority or is a ring/window index. */
  let guarded = 0
  for (const [file, text] of SHELL) {
    for (const [, expr] of text.matchAll(/\{(\w[\w.?[\]]*)\.length > 0 \?/g)) {
      assert.ok(expr, `${file}: empty guard`)
      guarded += 1
    }
    // No section header may print a bare interpolated number beside a label.
    assert.ok(
      !/<h2[^>]*>[^<{]+\{[^}]*\}\s*<\/h2>/.test(text),
      `${file}: a naked number reached a section header`,
    )
  }
  assert.ok(guarded >= 2, `expected zero-count guards across the shell, saw ${guarded}`)
})

test('regression 3: no first-level line states a zero as a finding', () => {
  /* Was: `Chain verified · 777,404 events · 0 conflicts` — a count of nothing,
   * presented as news. The replacement states the positive fact only. */
  const line =
    'Chain intact · ' +
    countPhrase(fmt(777404), 'event on the ledger', 'events on the ledger')
  assert.doesNotMatch(line, /\b0 \w+/)
  assert.doesNotMatch(line, /conflicts/)
  assert.equal(bannedIn(line), null)
  /* The shape being banned is a zero appended as an extra fact — `· 0 conflicts`.
   * A line whose whole subject is a count may legitimately read "0 events on the
   * ledger": that is the count carrying its noun (§4), not a finding of nothing. */
  for (const [where, text] of composed()) {
    // The zero fixtures exist to prove `countPhrase` handles an empty ledger,
    // where the count IS the subject ("0 events on the ledger") rather than an
    // extra fact appended to one. That empty state is honest; skip it.
    if (/\(0\)$/.test(where)) continue
    assert.doesNotMatch(text, /· 0 \w/, `a zero stated as a finding in ${where}: ${text}`)
  }
  // And no line may ever pair a zero with a problem noun, which is the exact
  // shape `· 0 conflicts` had: reassurance dressed as a measurement.
  for (const [where, text] of composed()) {
    assert.doesNotMatch(
      text,
      /\b0 (conflicts?|errors?|failures?|warnings?|problems?|issues?)\b/,
      `a zero problem-count reaches first level in ${where}: ${text}`,
    )
  }
})

test('regression 4: engine reasoning is one disclosure behind, never at first level', () => {
  /* Was: the deterministic pass printing its own wording — score, model tag,
   * and `deterministic · local` — inside the first-level Verdict block. The
   * detail is kept, exactly one tap deeper: `verdictLine` is the sentence,
   * `verdictDetail` is the engine's wording, and they are different strings. */
  const engine = { text: 'deterministic · local', score: 0.97 }
  const detail = verdictDetail(engine)
  assert.match(detail, /deterministic · local · 0\.97/)
  assert.ok(bannedIn(detail), 'the engine detail is engine vocabulary, by design')
  for (const s of STATES) {
    const first = verdictLine(s)
    assert.equal(bannedIn(first), null, `first-level verdict line leaked: ${first}`)
    assert.notEqual(first, detail)
    assert.doesNotMatch(first, /0\.\d\d|deterministic/)
    assert.equal(first, RECEIPT_STATE[s] || RECEIPT_STATE.unknown)
  }
  /* The shell must open it behind a disclosure, not print it inline. Re-pointed
     at L2, which is where a receipt now lives: `judgedDetail` is the same
     composition under a name that says what it is. */
  assert.match(
    src('mac/inspector.jsx'),
    /Show how this was judged[\s\S]{0,700}v\.judged/,
    'the engine detail must sit behind a disclosure in the inspector',
  )
  assert.match(RECEIPT, /judgedDetail/)
  assert.match(DESIGN, /verdictDetail/)
  // And no first-level surface prints a score inline.
  for (const [file, text] of SHELL) {
    const stripped = text.replace(/\/\*[\s\S]*?\*\//g, '')
    assert.ok(!/toFixed\(2\)/.test(stripped), `${file}: an engine score is composed at first level`)
  }
})

test('the pre-R2 graph-hero shell is gone from disk', () => {
  /* R2.4's second half. It left navigation at R2.1 and leaves disk here; what
     survived is the receipt content model, carried into lib/receipt.js with a
     provenance header naming exactly which of its sections came across. */
  for (const dead of ['App.jsx', 'lib/style.js']) {
    assert.throws(() => src(dead), /ENOENT/, `${dead} is still on disk`)
  }
  assert.match(RECEIPT, /Provenance: the receipt content model is carried from/)
  assert.match(RECEIPT, /App\.jsx/, 'the survivors must name where they came from')
  // Nothing imports it any more, by any spelling.
  for (const [file, text] of SHELL) {
    assert.ok(!/App\.jsx/.test(text), `${file} still imports the retired shell`)
    assert.ok(!/lib\/style\.js/.test(text), `${file} still imports the retired style helper`)
  }
  assert.ok(!/App\.jsx/.test(src('main.jsx')))
})

test('the embedded Trace drawer has an in-panel close and a non-overlapping inert backdrop', () => {
  const shell = src('mac/shell.jsx')
  const css = src('macos.css')
  const close = shell.match(/const closeSidebar = React\.useCallback\(\(\) => \{([\s\S]*?)\}, \[\]\)/)
  assert.ok(close, 'the drawer needs one shared close action')
  assert.equal(close[1].replace(/\s+/g, ' ').trim(), 'setSidebar(false)')
  assert.doesNotMatch(close[1], /setRow|setSelected|openProject|setFixture/)

  const aside = shell.match(/<aside[\s\S]*?<\/aside>/)?.[0] || ''
  assert.match(aside, /className="tl-mobile-project-close"[^>]*aria-label="Close navigation"[^>]*onClick=\{closeSidebar\}/)
  assert.match(aside, />Close<\/span>/, 'the in-drawer close action must be visible, not icon-only')
  assert.match(shell, /if \(e\.key === 'Escape'\)[\s\S]{0,260}closeSidebar\(\)/)

  const backdrop = shell.match(/<div className="tl-project-backdrop"[^>]*\/>/)?.[0] || ''
  assert.match(backdrop, /aria-hidden="true"/)
  assert.match(backdrop, /onClick=\{closeSidebar\}/)
  assert.doesNotMatch(backdrop, /aria-label|role=|<button/)
  assert.match(css, /\.tl-project-sidebar, \.tl-project-sidebar\.compact \{[^}]*width: min\(300px, 86vw\);[^}]*box-sizing: border-box/)
  assert.match(css, /\.tl-project-backdrop \{[^}]*left: min\(300px, 86vw\)/)
})

test('the outer Recovery OS drawer closes without mutating the explicit Trace route or project selection', () => {
  const shell = src('os/shell.jsx')
  const css = src('os.css')
  const representative = '0123456789abcdef'
  const route = new URL(`http://127.0.0.1:4179/?project=${representative}`)
  assert.equal(route.searchParams.get('project'), representative)
  assert.match(shell, /return \[q\.get\('project'\), q\.get\('node'\)\]/)
  assert.match(shell, /const \[traceFixture, setTraceFixture\] = React\.useState\(deepLink\)/)
  assert.match(shell, /<MacShell index=\{index\} embedded initialFixture=\{traceFixture\} initialNode=\{traceNode\}/)

  const close = shell.match(/const closeSidebar = React\.useCallback\(\(\) => \{([\s\S]*?)\}, \[\]\)/)
  assert.ok(close, 'the outer drawer needs one shared close action')
  assert.equal(close[1].replace(/\s+/g, ' ').trim(), 'setSidebarOpen(false)')
  assert.doesNotMatch(close[1], /setTraceFixture|setTraceNode|setTab|openTraceProject|setProjectSearch/)

  const aside = shell.match(/<aside className=\{`os-sidebar[\s\S]*?<\/aside>/)?.[0] || ''
  assert.match(aside, /className="os-sidebar-close"[^>]*aria-label="Close navigation"[^>]*onClick=\{closeSidebar\}/)
  assert.match(aside, />Close<\/span>/)
  assert.match(shell, /if \(event\.key === 'Escape'\) closeSidebar\(\)/)

  const backdrop = shell.match(/<div className="os-sidebar-backdrop"[^>]*\/>/)?.[0] || ''
  assert.match(backdrop, /aria-hidden="true"/)
  assert.match(backdrop, /onClick=\{closeSidebar\}/)
  assert.doesNotMatch(backdrop, /aria-label|role=|tabIndex|<button/)
  assert.doesNotMatch(shell, /<button[^>]*className="os-sidebar-backdrop"/)
  assert.match(css, /--os-sidebar-drawer-width: min\(320px, 88vw\)/)
  assert.match(css, /\.os-sidebar \{[^}]*width: var\(--os-sidebar-drawer-width\);[^}]*box-sizing: border-box/)
  assert.match(css, /\.os-sidebar-backdrop \{[^}]*left: var\(--os-sidebar-drawer-width\)/)
})

test('local readiness has one reusable GET-only status path with overlap and stale-epoch guards', () => {
  const shell = src('mac/shell.jsx')
  const component = src('mac/work-map.jsx')
  const callbackStart = shell.indexOf('const checkMindStatus = React.useCallback')
  const callbackEnd = shell.indexOf('const recheckMindStatus = React.useCallback', callbackStart)
  const callback = shell.slice(callbackStart, callbackEnd)

  assert.ok(callbackStart >= 0 && callbackEnd > callbackStart)
  assert.equal([...shell.matchAll(/fetch\(MIND_ENDPOINTS\.status/g)].length, 1)
  assert.match(callback, /method: 'GET'/)
  assert.match(callback, /cache: 'no-store'/)
  assert.match(callback, /headers: \{ Accept: 'application\/json' \}/)
  assert.doesNotMatch(callback, /\bbody\s*:/)
  assert.match(callback, /mindCheckMayStart\(ready, mindCheckEpoch\.current, epoch\)/)
  assert.match(callback, /mindCheckIsCurrent\(mindCheckEpoch\.current, projectLoadEpoch\.current, epoch\)/)
  assert.match(shell, /void checkMindStatus\(false\)/)
  assert.equal([...shell.matchAll(/void checkMindStatus\(false\)/g)].length, 1)
  assert.match(shell, /const recheckMindStatus = React\.useCallback\(\(\) => \{\s*void checkMindStatus\(true\)/)
  assert.match(shell, /onCheckMind=\{recheckMindStatus\}/)

  assert.match(component, /const analysisReady = mindStatus\.state === 'available'/)
  assert.match(component, /disabled=\{!analysisReady \|\| analyzing\}/)
  assert.match(component, /disabled=\{availability\.checking\}/)
  assert.match(component, /Analyze unavailable — check local model/)
  assert.match(component, /<MindRecovery status=\{mindStatus\} statusId=\{statusId\} onCheckMind=\{onCheckMind\} \/>/)

  const analyzeStart = shell.indexOf('const analyzeRun = React.useCallback')
  const analyzeEnd = shell.indexOf('const select = React.useCallback', analyzeStart)
  const analyze = shell.slice(analyzeStart, analyzeEnd)
  assert.ok(analyzeStart >= 0 && analyzeEnd > analyzeStart)
  assert.ok(analyze.indexOf("if (mindStatus.state !== 'available')") < analyze.indexOf('fetch(MIND_ENDPOINTS.interpret'))
  assert.match(analyze, /Local analysis was not started because the fixed local model is not ready\./)
  assert.match(analyze, /\[fixture, mindStatus\.state, row\?\.fixture, sourceDigest\]/)
  assert.match(analyze, /if \(projectLoadEpoch\.current !== epoch\) return[\s\S]*failure\.state === 'unavailable'[\s\S]*setMindStatus\(\{ state: 'unavailable' \}\)/)
})

test('an exported-count mismatch is attached, reported once, and shown without blocking the graph', () => {
  const shell = src('mac/shell.jsx')
  const fixture = src('lib/fixture.js')
  const css = src('macos.css')
  const openStart = shell.indexOf('const openProject = React.useCallback')
  const openEnd = shell.indexOf('const checkMindStatus = React.useCallback', openStart)
  const open = shell.slice(openStart, openEnd)

  assert.ok(openStart >= 0 && openEnd > openStart)
  assert.ok(open.indexOf('setFixture(null)') < open.indexOf('fetch(PROJECT(r.fixture)'))
  assert.match(open, /const loadedFixture = attachExportedCountDiscrepancy\(doc\)/)
  assert.match(open, /if \(loadedFixture\._count_discrepancy\) \{\s*console\.error\(boundedCountDiscrepancy\(loadedFixture\._count_discrepancy\)\)\s*\}/)
  assert.equal([...open.matchAll(/console\.error\(/g)].length, 1)
  assert.ok(open.indexOf('console.error(') < open.indexOf('setFixture(loadedFixture)'))
  assert.doesNotMatch(open, /throw new Error\([^)]*count/i)

  assert.match(fixture, /export function exportedCountDiscrepancy\(raw\)/)
  assert.match(fixture, /export function attachExportedCountDiscrepancy\(raw\)/)
  assert.match(fixture, /export function boundedCountDiscrepancy\(discrepancy\)/)
  assert.match(fixture, /const COUNT_REPORT_LIMIT = 12/)
  assert.match(fixture, /const COUNT_KEY_REPORT_LIMIT = 64/)
  assert.match(shell, /<CountMismatchBanner discrepancy=\{fixture\._count_discrepancy\} \/>/)
  assert.match(shell, /className="tl-count-mismatch" role="alert"/)
  assert.match(shell, /The graph remains available/)
  assert.match(shell, /<div className="tl-main-surface">[\s\S]*<WorkMap/)

  assert.match(css, /\.tl-main \{[^}]*display: flex;[^}]*flex-direction: column/)
  assert.match(css, /\.tl-main-surface \{[^}]*position: relative;[^}]*min-width: 0;[^}]*min-height: 0/)
  assert.match(css, /\.tl-count-mismatch \{[^}]*flex-wrap: wrap;[^}]*box-sizing: border-box/)
  assert.match(css, /\.tl-count-mismatch code \{[^}]*white-space: normal;[^}]*overflow-wrap: anywhere;[^}]*word-break: break-word/)
})

/* ── The readable Trace's render fixes (DESIGN-readable-trace §4, §5) ──────
 *
 * There is no DOM harness in this project and adding one for four style
 * objects would be more machinery than the rule is worth, so these are style
 * CONTRACTS asserted on the source — the same technique the four regressions
 * above use, and the same technique `workspace.css`'s own `.ws-head-cell`
 * comment was written after. Each one is a collision the operator photographed.
 */

const DOCKS = src('workspace/docks.jsx')
const UIKIT = src('workspace/ui-kit.jsx')
const CHROME = src('workspace/chrome.jsx')
const WORKSPACE = src('workspace/shell.jsx')
const CANVAS = src('workspace/canvas.jsx')

test('T15 · the left dock tree can be clipped, so a state word can never paint over a claim', () => {
  const glyph = DOCKS.match(/export const TREE_GLYPH = Object\.freeze\(\{([^}]*)\}\)/)
  assert.ok(glyph, 'TREE_GLYPH must be exported so this contract has something to hold')
  /* `minWidth: '10px'` REPLACED the flex item's default `min-width: auto`,
     which is the floor that normally prevents the overlap. It must not come
     back: a 10px slot holding a 110px `<StateWord variant="inline">` is the
     screenshot. */
  assert.doesNotMatch(glyph[1], /minWidth:\s*'10px'/)
  assert.match(glyph[1], /minWidth:\s*0/)
  assert.match(glyph[1], /overflow:\s*'hidden'/)
  assert.match(glyph[1], /maxWidth:/)

  const label = DOCKS.match(/export const TREE_LABEL = Object\.freeze\(\{([^}]*)\}\)/)[1]
  assert.match(label, /minWidth:\s*0/, 'a flex:1 label with min-width:auto cannot give back space it never claimed')
  assert.match(label, /textOverflow:\s*'ellipsis'/)

  const head = UIKIT.match(/export const WORD_HEAD = Object\.freeze\(\{([^}]*)\}\)/)[1]
  assert.match(head, /minWidth:\s*0/, 'a nowrap head with no minWidth:0 cannot be clipped at all')
  assert.match(head, /overflow:\s*'hidden'/)
})

test('T15 · a dock group heading and its count cannot be painted on top of each other', () => {
  const title = DOCKS.match(/export const TREE_TITLE = Object\.freeze\(\{([^}]*)\}\)/)[1]
  assert.match(title, /minWidth:\s*0/)
  assert.match(title, /overflow:\s*'hidden'/)
  assert.match(title, /textOverflow:\s*'ellipsis'/)
  assert.match(title, /whiteSpace:\s*'nowrap'/)

  const count = DOCKS.match(/export const TREE_COUNT = Object\.freeze\(\{([^}]*)\}\)/)[1]
  assert.match(count, /flexShrink:\s*0/, 'without this the count is the only shrinkable item and wraps inside a 28px row')
  assert.match(count, /whiteSpace:\s*'nowrap'/)

  const group = DOCKS.match(/export const TREE_GROUP = Object\.freeze\(\{[\s\S]*?\}\)/)[0]
  assert.match(group, /overflow:\s*'hidden'/, 'nothing clipped the wrapped lines, so they painted over the rows either side')

  /* The copy half of the same fix: the count noun is ~45px narrower, so the
     CSS is not left holding the whole problem on a dragged-narrow dock. */
  assert.match(WORKSPACE, /\['command', 'commands'\]/)
  assert.doesNotMatch(WORKSPACE, /\['command event', 'command events'\]/)
})

test('T16 · no heading is rendered over a void, and the absence is stated once instead', () => {
  /* `insp.narr` is empty for essentially every object of every real session,
     and the kicker rendered unconditionally. Both instances get the guard. */
  assert.match(DOCKS, /\{\(insp\.narr \|\| \[\]\)\.length \? \(/)
  assert.match(CANVAS, /\{narr\.length \? \(/)
  const docksCode = DOCKS.replace(/\/\*[\s\S]*?\*\//g, '').replace(/\{\/\*[\s\S]*?\*\/\}/g, '')
  assert.equal([...docksCode.matchAll(/As the CLI narrates it/g)].length, 1, 'one narration kicker in the inspector, and it is the guarded one')
  assert.match(DOCKS, /\{\(insp\.narr \|\| \[\]\)\.length \? \(\s*<React\.Fragment>\s*<Kicker style=\{CAP_MARGIN\}>As the CLI narrates it/)
  assert.match(WORKSPACE, /base\.narrShow = \(base\.narr \|\| \[\]\)\.length > 0/)
  assert.match(WORKSPACE, /No narration is exported over this server/)
  assert.equal([...WORKSPACE.matchAll(/No narration is exported over this server/g)].length, 1, 'stated once per inspector, not once per section')
  assert.match(DOCKS, /insp\.narrCommand/, 'the absence carries the command that would end it')

  /* And the second, smaller instance of the same bug: a fixed-height band
     above the canvas fed by the same null narration. */
  assert.match(CHROME, /if \(!lines\.length && !notice\) return <div style=\{sx\(SENTENCE_STRIP, SENTENCE_STRIP_EMPTY\)\} \/>/)
  assert.match(CHROME, /const SENTENCE_STRIP_EMPTY = Object\.freeze\(\{ height: 0/)
})

test('T17 · the destructive commands are stated on the opening surface, advisory, with a clause and no state word', () => {
  const fn = WORKSPACE.match(/riskNotice\(\) \{[\s\S]*?\n  \}/)
  assert.ok(fn, 'the promoted finding must be composed in one place')
  const body = fn[0]

  /* Doctrine 4: every count carries its noun, and the noun comes from the one
     locale-free helper. */
  assert.match(body, /countPhrase\(flagged\.length, 'command', 'commands'\)/)
  assert.doesNotMatch(body, /toLocaleString|fmt\(/)
  /* Doctrine 3: a negative finding carries what would prove it. */
  assert.match(body, /What would prove it: open them and read the paths they name\./)
  /* Doctrine 1: no state word is asserted, because the ruling does not rule
     one for "matched a destructive pattern". Deliberately under-claiming. */
  assert.doesNotMatch(body, /wordSlug|StateWord|'broken'|'fog'|'not_proven'/)
  /* Doctrine 5: advisory, never an accusation, never proof of falsehood. */
  for (const forbidden of ['unverified', 'confidence', 'entailment', 'fidelity', 'nli', 'lie', 'deleted', 'destroyed', 'caught in a']) {
    assert.ok(!body.toLowerCase().includes(forbidden), `the promoted finding must not say "${forbidden}"`)
  }
  for (const [, text] of body.matchAll(/'([^'\n]{6,160})'/g)) {
    if (!/[A-Za-z]/.test(text) || !text.includes(' ')) continue
    assert.equal(bannedIn(text), null, `engine lexicon in the promoted finding: ${JSON.stringify(text)}`)
  }

  /* The right analysis dock retains the advisory and its graph filter action. */
  assert.match(WORKSPACE, /riskNotice=\{v\.riskNotice\}/, 'the right analysis dock retains the advisory while the graph stays first')
  assert.match(WORKSPACE, /onShowRisk=\{\(\) => this\.applyView\(\{ name: 'Destructive commands', tokens: \['risk:destructive'\] \}\)\}/)
  assert.match(WORKSPACE, /\{ name: 'Destructive commands', tokens: \['risk:destructive'\] \}/)
})

test('the graph, the table and the Explorer count one population, and the footer says which', () => {
  /* `!n.synthetic`, not `!n.group`: a step row is a real recorded tool_call
     that absorbed its result, and dropping it from the denominator would
     understate the record by 1,367 objects on the target session. */
  assert.equal([...WORKSPACE.matchAll(/filter\(\(n\) => !n\.synthetic/g)].length, 3, 'the three population reads must agree')
  /* They filtered different sets — `!n.group` here, `!n.group && !n.member`
     there — so the day grouping shipped they would have shown different
     populations of the same record. */
  const code = WORKSPACE.replace(/\/\*[\s\S]*?\*\//g, '')
  assert.doesNotMatch(code, /!n\.group && !n\.member/)
  assert.match(WORKSPACE, /the same objects the graph holds, folded or not/)
  assert.doesNotMatch(code, /same scope, selection and filters as the graph/)
})

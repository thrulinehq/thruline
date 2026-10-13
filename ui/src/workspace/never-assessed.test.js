import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

/* NEVER-ASSESSED-NOT-READY (R-2026-09-28-25) — a criterion whose
 * applicability was never assessed does not count toward Ready, and the
 * Change says plainly what is missing.
 *
 * The rule lives in the ENGINE (`change_readiness.rs`). What this file holds:
 *
 *  1. `neverAssessed()` — the one rule the packet's "What was not recorded"
 *     and rests-on lines read — is the engine's answer, served, and nothing
 *     the client works out from the items. It cannot disagree with the
 *     engine because it does not decide.
 *  2. `never-assessed.readiness.json` is the engine's own served answer for
 *     the fixture in `crates/core/tests/never_assessed_readiness.rs`, and a
 *     Rust test holds it equal to what `readiness()` serves. Fed to the
 *     screen's derivation here, it reads NOT ready on the Change screen, the
 *     packet page and the exported files, and each names the line and the one
 *     act that would assess it.
 *
 * Every string below is synthetic.
 */

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-never-assessed-'))
after(() => rm(temporary, { recursive: true, force: true }))

const ENTRY = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { PacketPage } from './packet.jsx';
  import { buildPacketView } from './packet-view.js';
  import { deriveChangeModel, neverAssessed } from './change-model.js';
  import { renderExport } from './packet-export.js';
  import { HQ_COPY } from './hq-copy.js';
  import { fill } from './hq-narrate.js';
  export { buildPacketView, deriveChangeModel, neverAssessed, renderExport, HQ_COPY, fill };
  export const renderPacket = props => renderToStaticMarkup(React.createElement(PacketPage, props));
`

const M = await (async () => {
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: here },
  })
  const file = join(temporary, 'never-assessed.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
})()

const { buildPacketView, deriveChangeModel, neverAssessed, renderExport, HQ_COPY, fill, renderPacket } = M
const MD = HQ_COPY.change.packet.markdown
const REASON = HQ_COPY.change.model.readiness.reasons.readiness_never_assessed
/* NEVER-ASSESSED-ON-SCREEN (D-050) — what the screen says in place of the terminal's command, where it offers Check it again
 * for this Change. `REASON` above is the terminal's and is unchanged (voice.rs); the screen's tests below expect this. */
const SCREEN_REASON = 'Whether this still applies was never assessed. Check it again on this Change, and it will be.'

/* The engine's served answer, committed, and pinned to the engine by
 * `the_served_answer_is_the_document_the_screen_is_pinned_against`. */
const ENGINE = JSON.parse(await readFile(join(here, 'never-assessed.readiness.json'), 'utf8'))
const [LINE_1, LINE_2] = ENGINE.obligations.filter((o) => o.criterion_index).map((o) => o.text)
const VERSION = ENGINE.target_version
const CHECK = 'bash:synthetic-repo:cargo clippy --workspace'

const change = () => ({
  id: 'cccccccc-0000-4000-8000-00000000c928',
  project_id: 'cccccccc-0000-4000-8000-00000000p928',
  opened_at: '2026-01-02T09:00:00Z',
  request: { title: 'Keep the build clean', purpose: 'Keep the build clean', requester: 'sam', source: 'person' },
  accepted_scope: {
    version: 1, criteria: [LINE_1, LINE_2], exclusions: [], allowed_scope: ['crates/**'],
    constraints: [], check_profile: ['clippy'],
    content_sha256: '2f66b941756eaa221b796f446617f89701d62b6a514bafac62947e5661425b1c',
    acceptance: { actor: 'sam', accepted_at: '2026-01-02T09:40:00Z', rationale: 'this is what counts' },
    proposed_at: '2026-01-02T09:30:00Z',
  },
  implementation: {
    version: 1, bound_at: '2026-01-02T09:45:00Z', repository_identity: 'repo-synthetic-0928',
    base_revision: VERSION, snapshot_id: null, workspace_identity: 'workspace-synthetic-0928',
  },
  associations: [], examination: 'none_yet', decision: 'none_yet',
  decisions: { standing: null, history: [], target_revision: VERSION, scope_version: 1 },
  masking: { state: 'ran', detector: 'v2', masked: 0, patterns: [], statement: 'detector v2 · nothing matched a known secret shape' },
})

const item = (n, fresh) => ({
  link_id: 'cce_synthetic_092' + n, kind: 'check_run', evidence_id: 'eb_build',
  session_id: 's_build', basis: 'declared', basis_slug: 'basis_declared',
  name: CHECK, observed: 'proven', bears: true, applies: true,
  applies_slug: 'inputs_applies', asserted_at: '2026-01-03T09:00:00Z', execution: 'not_run', execution_slug: 'execution_not_run',
  failure_line: null, evidence_class: null, evidence_class_slug: null,
  ...(fresh ? { fresh: true, fresh_slug: 'applicability_still_applies' } : {}),
})

const coverage = (readiness = structuredClone(ENGINE), fresh = [false, true]) => ({
  schema_version: 1, change_id: 'cccccccc-0000-4000-8000-00000000c928', scope_version: 1, link_rows: 2,
  executions: [], run_checks: null,
  criteria: [
    { index: 1, text: LINE_1, state: 'evidence', state_slug: 'coverage_evidence', items: [item(1, fresh[0])], removed: [] },
    { index: 2, text: LINE_2, state: 'evidence', state_slug: 'coverage_evidence', items: [item(2, fresh[1])], removed: [] },
  ],
  readiness,
})

const surfaces = (cov = coverage()) => {
  const ch = change()
  const hq = deriveChangeModel(ch, cov, null, null, [])
  const model = { ...buildPacketView({ change: ch, proposals: null, work: [], coverage: cov, evidence: [], hq }), hq }
  const html = renderPacket({ model, notes: [] })
  return {
    hq,
    html,
    md: renderExport(hq.packet, null, { format: 'md' }).text,
    htmlFile: renderExport(hq.packet, null, { format: 'html' }).text,
    json: JSON.parse(renderExport(hq.packet, null, { format: 'json' }).text),
  }
}

const decode = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, '&').replace(/&#x2F;/g, '/').replace(/&#x60;/g, '`')
const plain = (s) => decode(s.replace(/<[^>]+>/g, ' ')).replace(/\s+/g, ' ').trim()

/* ── the copy ──────────────────────────────────────────────────────────── */

test('the reason says what is missing and names the one act that would assess it', () => {
  assert.equal(typeof REASON, 'string', 'the copy file carries a sentence for the engine slug readiness_never_assessed')
  assert.ok(REASON.includes('never assessed'), REASON)
  assert.ok(REASON.includes('thruline change prepare'), REASON)
})

/* ── 1. neverAssessed() is the engine's answer, and only the engine's ──── */

test('neverAssessed() marks exactly the lines the engine names, whatever the items say', () => {
  const hq = deriveChangeModel(change(), coverage(), null, null, [])
  const named = new Set(ENGINE.obligations.filter((o) => o.unsatisfied_slug === 'readiness_never_assessed').map((o) => o.criterion_index))
  assert.deepEqual([...named], [1], 'the pinned engine answer names line 1')
  for (const c of hq.criteria) assert.equal(neverAssessed(c), named.has(c.n), `line ${c.n}`)
})

test('neverAssessed() does not re-derive: items with no assessment on a line the engine counts are not marked', () => {
  /* Both lines' items carry no assessment, and the served answer counts line
   * 2 (satisfied). The client used to decide from the items and mark both;
   * it now says what the engine said, and only that. */
  const hq = deriveChangeModel(change(), coverage(structuredClone(ENGINE), [false, false]), null, null, [])
  assert.equal(neverAssessed(hq.criteria[0]), true)
  assert.equal(neverAssessed(hq.criteria[1]), false, 'the engine counted line 2, so the client does not overrule it')
})

test('neverAssessed() follows the engine the other way too: an assessed-looking line the engine names is marked', () => {
  const hq = deriveChangeModel(change(), coverage(structuredClone(ENGINE), [true, true]), null, null, [])
  assert.equal(neverAssessed(hq.criteria[0]), true, 'the served answer, not the items, decides')
  assert.equal(neverAssessed(hq.criteria[1]), false)
})

test('no readiness served, nothing marked: absence of an answer is not an answer', () => {
  const hq = deriveChangeModel(change(), coverage(null), null, null, [])
  for (const c of hq.criteria) assert.equal(neverAssessed(c), false, `line ${c.n}`)
})

/* ── 2. every surface reads NOT ready, and says what is missing ────────── */

test('the Change screen: not ready, and the missing line carries the never-assessed reason', () => {
  const s = surfaces()
  assert.equal(s.hq.readiness.ready, false)
  assert.equal(s.hq.inboxRow.ready, false)
  assert.equal(s.hq.readiness.missing.length, 1)
  assert.equal(s.hq.readiness.missing[0].text, LINE_1)
  assert.equal(s.hq.readiness.missing[0].reason, SCREEN_REASON)
  assert.equal(s.hq.readiness.restsOn.length, 0, 'nothing is rested on')
  assert.ok(!/Ready for approval/.test(s.hq.readiness.sentence), s.hq.readiness.sentence)
})

/* EXPORT-MD-ESCAPE-R3 (D-050). The Markdown file writes a code span in the file's own words
 * (`thruline change prepare` in the reason below) as escaped TEXT, backticks escaped, so the
 * words a reader sees are the same and no renderer is asked to read a span. The old expected
 * value was the sentence as the copy file has it. Nothing between these backticks is markup. */
const mdCode = (s) => s.replaceAll('`', '\\`')

test('the packet page and every exported file: not ready, line 1 named with the reason, line 2 not', () => {
  const s = surfaces()
  const wanted = LINE_1 + ' — ' + SCREEN_REASON
  assert.deepEqual(s.hq.packet.readinessRests, [wanted])
  const page = plain(s.html)
  assert.ok(!page.includes('Ready for approval'), 'the page never says ready')
  assert.ok(page.includes(wanted), 'the page names the line and the act')
  assert.ok(page.includes(fill(MD.not_recorded_no_freshness, { what: LINE_1 })), 'and lists it as never assessed')
  assert.ok(!page.includes(fill(MD.not_recorded_no_freshness, { what: LINE_2 })), 'line 2 was assessed')
  assert.ok(!s.md.includes('Ready for approval') && s.md.includes(mdCode(wanted)), 'markdown')
  assert.ok(!plain(s.htmlFile).includes('Ready for approval') && plain(s.htmlFile).includes(wanted), 'html file')
  assert.equal(s.json.readiness.ready, false, 'json file')
  assert.deepEqual(s.json.readiness.rests_on, [wanted], 'json file')
  assert.ok(s.json.not_recorded.includes(fill(MD.not_recorded_no_freshness, { what: LINE_1 })))
  assert.ok(!s.json.not_recorded.includes(fill(MD.not_recorded_no_freshness, { what: LINE_2 })))
})

/* ── NEVER-ASSESSED-NOT-READY-R2 — "could not be assessed" ────────────────
 * When no `change prepare` can assess the line (nothing on record says what
 * the files were when it was checked), the engine serves
 * `readiness_could_not_assess` instead. The Change screen, the packet page
 * and every exported file then say why and what would settle it, never the
 * prepare promise; and the line is still listed as never assessed. */

const COULD_NOT = HQ_COPY.change.model.readiness.reasons.readiness_could_not_assess

const couldNotEngine = () => {
  const doc = structuredClone(ENGINE)
  for (const o of doc.obligations) {
    if (o.unsatisfied_slug === 'readiness_never_assessed') {
      o.unsatisfied = 'could_not_assess'
      o.unsatisfied_slug = 'readiness_could_not_assess'
    }
  }
  return doc
}

test('R2: the could-not-assess reason says why and what settles it, and does not promise a prepare', () => {
  assert.equal(typeof COULD_NOT, 'string', 'the copy file carries a sentence for readiness_could_not_assess')
  assert.ok(COULD_NOT.includes('could not be assessed'), COULD_NOT)
  assert.ok(COULD_NOT.includes('Run the check again'), COULD_NOT)
  assert.ok(!COULD_NOT.includes('change prepare'), COULD_NOT)
})

test('R2: every surface names the line with the could-not-assess reason, and lists it as never assessed', () => {
  const s = surfaces(coverage(couldNotEngine()))
  assert.equal(s.hq.readiness.ready, false)
  assert.equal(s.hq.readiness.missing[0].reason, COULD_NOT)
  assert.equal(neverAssessed(s.hq.criteria[0]), true)
  assert.equal(neverAssessed(s.hq.criteria[1]), false)
  const wanted = LINE_1 + ' — ' + COULD_NOT
  assert.deepEqual(s.hq.packet.readinessRests, [wanted])
  const page = plain(s.html)
  assert.ok(page.includes(wanted) && !page.includes(REASON), 'the page says why, and not the prepare promise')
  assert.ok(page.includes(fill(MD.not_recorded_no_freshness, { what: LINE_1 })))
  assert.ok(s.md.includes(wanted) && !s.md.includes(REASON), 'markdown')
  assert.ok(plain(s.htmlFile).includes(wanted), 'html file')
  assert.deepEqual(s.json.readiness.rests_on, [wanted], 'json file')
  assert.ok(s.json.not_recorded.includes(fill(MD.not_recorded_no_freshness, { what: LINE_1 })))
})

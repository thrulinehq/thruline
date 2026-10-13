/* EXPORT-MD-ESCAPE — the exported Markdown never carries live HTML or script
 * from recorded text.
 *
 * The defect (found by the MERGE-SEAM-C11 adversary, present at the base): a
 * request, a criterion, a quote, an exception rationale or a path that read
 * `<img src=x onerror=…>`, `<script>…</script>` or `[x](javascript:…)` went
 * into the HTML export and the JSON export escaped — and into the Markdown
 * export raw. Many Markdown renderers pass inline HTML through, so a person who
 * opens the file a teammate shares could run it.
 *
 * WHAT THESE TESTS HOLD THE FIX TO
 *  1. No recorded text, in any slot the Markdown carries it in, leaves a raw
 *     HTML tag, a link or an image with a target that is not http(s), a remote
 *     image, or a reference definition. Judged by `hazards()`, an instrument
 *     that reads a finished file the way a renderer would and is NOT the
 *     production helper turned around (see the fixture's header).
 *  2. The file's own structure is unchanged: recorded text cannot add a
 *     heading, a list item, a quote, a rule or a code fence. Judged by
 *     `shape()`, the kind of every line with none of its words.
 *  3. Ordinary text reads the same, byte for byte.
 *  4. The HTML and JSON exports are byte-identical to the base for a fixture
 *     full of hostile text, full and redacted.
 *  5. One helper, applied once.
 *
 * ROUND 2 (EXPORT-MD-ESCAPE-R2). Round 1 judged the file a line at a time, and
 * a reference definition whose label was opened in one field and closed in
 * another slipped past it (`- [a` … `x]: javascript:alert(1)`, live in a
 * renderer). `hazards()` now also reads what a renderer PRODUCES from the whole
 * file (`ui/test-support/markdown-instrument.mjs`: the vendored CommonMark
 * reference implementation, plus `marked` and `markdown-it` wherever they are
 * installed), and the sections at the end of this file hold the round-2
 * requirements: no definition forms across any pair or triple of fields, a
 * code span reads as it was recorded, and nothing round 1 held has moved.
 *
 * ROUND 3 (EXPORT-MD-ESCAPE-R3). Round 2 kept a code span a code span when it
 * looked plain, and modelled `marked` and `markdown-it` instead of running them.
 * Two shapes the model did not know were live in `marked` (a `javascript:` link,
 * and an `<img onerror>` whose handler ran in a browser). The rule is now the one
 * that cannot depend on a renderer's inline parser: every recorded backtick is
 * written escaped, and what was recorded between backticks is written as escaped
 * TEXT, showing exactly the characters that were recorded. All three renderers
 * are vendored (`ui/test-support/vendor/`) and run in every check, and the last
 * sections hold the round-3 requirements: the two breaks, a bounded exhaustive
 * enumeration through the real renderers, and the visible characters.
 *
 * ROUND 4 (EXPORT-MD-ESCAPE-R4, D-246). Round 3's own claim — no live link, no raw
 * tag, no image — did not hold for three shapes that predate it: a bare `//host`
 * (a scheme-relative link in markdown-it), `[![a\](https://…)](https://…)` (an image
 * inside a link, in marked) and `<1@b.co>` (a mail autolink, in all three). No
 * committed alphabet had a URL token. The instrument now counts as live a link
 * that is not a bare http(s) address whose words are that address, and the rule is
 * that recorded text yields no image, no raw tag, no definition and no such link:
 * every bracket and every `<` is escaped, and no `//` is left. The last sections
 * hold it, over the adversary's shapes, a URL-capable enumeration and the other
 * three Markdown writers (`markdown-surfaces.test.js`). Where an expected value
 * above changed by that ruling, it says D-050 beside it.
 *
 * Every fixture is synthetic. No string here came off a real machine.
 */

import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'
import { createHash } from 'node:crypto'
import { createRequire } from 'node:module'

import * as MI from '../../test-support/markdown-instrument.mjs'
import { enumerate, FIELD_END } from '../../test-support/enumerate.mjs'
import * as PE from './packet-export.js'
import { HQ_COPY } from './hq-copy.js'
import { deriveChangeModel } from './change-model.js'

/* ── the fixtures and the instrument ──────────────────────────────────── */

/* Every string below is synthetic. The payloads are attack SHAPES, kept in one
 * list so a test can put each of them in each slot the exported Markdown
 * carries recorded text in.
 *
 * `hazards(md)` is the instrument. It is deliberately NOT the production helper
 * turned around: it reads a finished Markdown file the way a renderer would and
 * reports what a renderer could make live — raw HTML, a link or image whose
 * target is not http(s), a reference definition, a remote image. It is more
 * conservative than any one renderer, and it treats a backslash-escaped bracket
 * by CommonMark's own parity rule, so a fix that escapes with `\]`, with an
 * entity or by breaking the paren is judged the same way.
 *
 * It lives in this test file, not in a module of its own: a non-test source
 * file under ui/src that builds a served readiness document is one the
 * readiness census (run-link-truth-r5) would have to be taught about, and one
 * that spells an entity is one the colour lint would read as a colour. */

const VERSION = 'e71c5b0aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
const SEEDED = '4719'

/* ── attack shapes ────────────────────────────────────────────────────── */

const JS = 'javascript:alert(1)'

const PAYLOADS = {
  script: '<script>alert(1)</script>',
  img_onerror: '<img src=x onerror=alert(1)>',
  svg_onload: '<svg/onload=alert(1)>',
  iframe_srcdoc: '<iframe srcdoc="<script>alert(1)</script>"></iframe>',
  anchor: '<a href="javascript:alert(1)">click</a>',
  uppercase_tag: '<SCRIPT SRC=//evil.example/x.js></SCRIPT>',
  html_comment: '<!-- x --><script>alert(1)</script>',
  processing_and_cdata: '<?php echo 1 ?><![CDATA[x]]>',
  closing_tag_first: '</p><script>alert(1)</script>',
  tag_in_middle: 'the totals <b onmouseover=alert(1)>add</b> up',
  md_link_js: `[click](${JS})`,
  md_link_js_no_parens: '[click](javascript:void)',
  md_link_js_backtick_arg: '[click](javascript:alert`1`)',
  md_link_js_colon_only: '[click](javascript:)',
  md_link_data_plain: '[click](data:text/plain,hello)',
  md_link_empty_https: '[click](https://)',
  lead_nbsp_heading: '\u00a0# Forged heading',
  lead_bom_heading: '\ufeff# Forged heading',
  md_link_js_mixed_case: '[click](JaVaScRiPt:alert(1))',
  md_link_js_tab: '[click](java\tscript:alert(1))',
  md_link_js_newline: '[click](java\nscript:alert(1))',
  md_link_js_leading_spaces: '[click](   javascript:alert(1))',
  md_link_js_control_lead: '[click](\u0001javascript:alert(1))',
  md_link_js_nul: '[click](java\u0000script:alert(1))',
  md_link_js_decimal_entity: '[click](&#106;avascript:alert(1))',
  md_link_js_hex_entity: '[click](&#x6A;avascript:alert(1))',
  md_link_js_named_entity: '[click](java&Tab;script&colon;alert(1))',
  md_link_js_angle: `[click](<${JS}>)`,
  md_link_js_backslash: '[click](javascript\\:alert(1))',
  md_link_js_percent: '[click](%6Aavascript:alert(1))',
  md_link_js_title: `[click](${JS} "title")`,
  md_link_data: '[click](data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==)',
  md_link_vbscript: '[click](vbscript:msgbox(1))',
  md_link_file: '[click](file:///etc/passwd)',
  md_link_relative: '[click](//evil.example/x)',
  md_link_https_title_html: '[click](https://ok.example "<img src=x onerror=alert(1)>")',
  md_link_https_quote_break: '[click](https://ok.example/"onmouseover="alert(1))',
  md_link_https_entity_quote: '[click](https://ok.example/&quot;onmouseover=alert(1))',
  md_link_nested: `[a [b](${JS}) c](https://ok.example)`,
  md_link_text_html: '[<img src=x onerror=alert(1)>](https://ok.example)',
  autolink_js: `<${JS}>`,
  autolink_data: '<data:text/html,x>',
  autolink_https: '<https://ok.example>',
  autolink_mixed_case: '<JaVaScRiPt:alert(1)>',
  image_js: `![x](${JS})`,
  image_data: '![x](data:image/svg+xml,<svg onload=alert(1)>)',
  image_https_remote: '![x](https://tracker.example/pixel.png)',
  image_title_handler: '![x](https://ok.example/a.png "onerror=alert(1)")',
  definition_line: `[x]: ${JS}`,
  definition_indented: `   [x]: ${JS}`,
  definition_then_use: `[x]: ${JS}\n\n[click][x]`,
  definition_after_newline: `hello\n[x]: ${JS}\n[click][x]`,
  definition_angle: '[x]: <javascript:alert(1)> "t"',
  parity_escaped_bracket: '[click\\](javascript:alert(1))',
  parity_double_backslash: '[click\\\\](javascript:alert(1))',
  parity_triple_backslash: '[click\\\\\\](javascript:alert(1))',
  parity_escaped_angle: '\\<script>alert(1)</script>',
  parity_escaped_bang: '\\![x](javascript:alert(1))',
  parity_backslash_before_paren: '[click]\\\\(javascript:alert(1))',
  windows_path_then_link: 'C:\\temp\\[x](javascript:alert(1))',
  forged_heading: 'text\n# Forged heading',
  forged_heading_two: 'text\n## Whether the records behind this could be read',
  forged_list: 'text\n- forged item\n* forged star\n+ forged plus',
  forged_ordered: 'text\n1. forged one\n2) forged two',
  forged_quote: 'text\n> forged quote',
  forged_fence: 'text\n```\n<script>alert(1)</script>\n```\nafter',
  forged_tilde_fence: 'text\n~~~\nbody\n~~~',
  forged_thematic: 'text\n---\n***\n___',
  forged_setext_equals: 'text\n=====',
  forged_indented_code: 'text\n\n        indented code',
  forged_crlf: 'text\r\n# forged\r\n- item',
  forged_carriage_return: 'text\r# forged',
  forged_unicode_line: 'text\u2028# forged\u2029- item',
  forged_next_line: 'text\u0085# forged',
  forged_form_feed: 'text\f# forged',
  lead_heading: '# Forged heading',
  lead_list: '- forged item',
  lead_star: '* forged item',
  lead_ordered: '1. forged item',
  lead_ordered_paren: '2) forged item',
  lead_quote: '> forged quote',
  lead_fence: '```\n<script>alert(1)</script>',
  lead_tilde_fence: '~~~ x',
  lead_thematic: '---',
  lead_thematic_stars: '***',
  lead_thematic_underscores: '___',
  lead_thematic_spaced_underscores: '_ _ _',
  lead_thematic_spaced_dashes: '- - -',
  md_link_https_entity_target: '[click](https://ok.example/a&amp;b)',
  lead_setext: '===',
  lead_dashes_two: '--',
  lead_indented: '        indented code',
  lead_spaces_heading: '   ### Forged',
  lead_definition_footnote: '[^1]: footnote body',
  table_cell_html: '| <img src=x onerror=alert(1)> | <script>alert(1)</script> |',
  table_full: 'a | b\n--|--\n<img src=x onerror=alert(1)> | [x](javascript:alert(1))',
  lead_task_checked: '[x] All tests pass',
  lead_task_upper: '[X] All tests pass',
  lead_task_open: '[ ] still to do',
  lead_task_end: '[x]',
  mixed_everything: '# h\n<script>alert(1)</script>\n[a](javascript:alert(1))\n![b](https://t.example/p.png)\n[c]: javascript:alert(2)\n```',
}

/* ── the instrument ───────────────────────────────────────────────────── */

/** Number of backslashes immediately before index `i`. */
function backslashesBefore(text, i) {
  let n = 0
  while (i - 1 - n >= 0 && text[i - 1 - n] === '\\') n += 1
  return n
}

/* Every character a renderer, or a tool that splits text for one, may end a line at. */
const LINE_ENDS = /\r\n|[\n\r\v\f\u001c-\u001e\u0085\u2028\u2029]/

/* What the FILE writes before a text on a line: a list bullet, a quote mark, a
 * heading mark, a label, the two spaces under an item. One of these at most. */
const OWN_PREFIX = /^(?:> - |> |- \*\*[^*]+:\*\* |\*\*[^*]+:\*\* |  \*\*[^*]+:\*\* |- |  |## |# )/

const BLOCK_STARTS = [
  /^#{1,6}(?:[ \t]|$)/, /^>/, /^[-+*](?:[ \t]|$)/, /^\d{1,9}[.)](?:[ \t]|$)/,
  /^(?:`{3,}|~{3,})/, /^([-*_])(?:[ \t]*\1){2,}[ \t]*$/, /^[=-]+[ \t]*$/,
]

/**
 * What in a Markdown file a renderer could make live. Empty means nothing.
 *
 * Three readers, and a finding from any counts. `lineHazards` is round 1's
 * conservative reader of one line at a time. `MI.hazardsRendered` reads what the
 * three vendored renderers PRODUCE from the whole file, which is what a person
 * who opens it gets. `MI.quirkHazards` reads the raw file for a backtick that
 * is not escaped (round 3 has no code spans, so a live one is one the helper did
 * not write) and for a line that is only a table's delimiter row.
 * @param {string} md
 * @returns {string[]}
 */
function hazards(md) {
  return [...lineHazards(md), ...MI.hazardsRendered(md), ...MI.quirkHazards(md)]
}

function lineHazards(md) {
  const found = []
  const text = String(md)
  /* Raw HTML, an HTML comment, a processing instruction, a declaration, CDATA
   * and an autolink all begin the same way. */
  for (const m of text.matchAll(/<(?=[A-Za-z/!?])/g)) found.push(`raw-html@${m.index}: ${JSON.stringify(text.slice(m.index, m.index + 24))}`)
  /* An image, of any target: a remote one is a request to somebody else's
   * server the moment the file is opened. */
  for (const m of text.matchAll(/!\[/g)) {
    if (backslashesBefore(text, m.index) % 2 === 0) found.push(`image@${m.index}: ${JSON.stringify(text.slice(m.index, m.index + 24))}`)
  }
  /* A live "](" starts a link destination. EXPORT-MD-ESCAPE-R4 (D-050): an
   * http(s) target used to pass here, as a link a person recorded. Recorded
   * `[text](url)` is never a live link (D-246: every `[` and `]` is escaped,
   * whatever follows), so every live "](" is a finding. */
  for (const m of text.matchAll(/\]\(/g)) {
    if (backslashesBefore(text, m.index) % 2 === 1) continue // the bracket itself is escaped
    found.push(`link@${m.index}: ${JSON.stringify(text.slice(m.index, m.index + 24))}`)
  }
  /* Line by line, the way a renderer reads lines: a lone CR ends one, and so do
   * the separators some tools split on. After the file's OWN prefix (`- `, `> `,
   * a label, …) what is left is recorded text or copy, and none of it may itself
   * begin a block — a heading, a quote, a list item, a rule, a fence, a setext
   * underline — nor a reference definition nor a task-list box. */
  for (const [i, line] of text.split(LINE_ENDS).entries()) {
    if (line === '---') continue // the file's own rule
    const body = line.replace(OWN_PREFIX, '')
    if (/^\[[^\]]*\]:\s*\S/.test(body.replace(/^\s+/, ''))) found.push(`definition@line ${i + 1}: ${JSON.stringify(body.slice(0, 40))}`)
    if (/^\[[ xX]\](\s|$)/.test(body.replace(/^\s+/, ''))) found.push(`task-box@line ${i + 1}: ${JSON.stringify(body.slice(0, 40))}`)
    if (BLOCK_STARTS.some((re) => re.test(body))) found.push(`block-start@line ${i + 1}: ${JSON.stringify(body.slice(0, 40))}`)
    if (/^\s{4,}\S/.test(body) || /^\t/.test(body)) found.push(`indented-code@line ${i + 1}: ${JSON.stringify(body.slice(0, 40))}`)
  }
  return found
}

/** The kind of every line, with none of its words: what the file's structure is. */
function shape(md) {
  return String(md).split(LINE_ENDS).map((line) => {
    if (line === '') return 'blank'
    if (/^# /.test(line)) return 'h1'
    if (/^## /.test(line)) return 'h2'
    if (/^---$/.test(line)) return 'hr'
    if (/^> - /.test(line)) return 'quoted-item'
    if (/^> /.test(line)) return 'quote'
    if (/^- /.test(line)) return 'item'
    if (/^ {2}/.test(line)) return 'under'
    if (/^(```|~~~)/.test(line)) return 'fence'
    return 'text'
  })
}

/* ── the scenario: every slot that carries recorded text, populated ───── */

/** What each slot holds when nothing hostile is in it. */
const BENIGN = {
  title: 'Totals add up on the checkout page',
  criterion: 'A declined card shows a message',
  waivedCriterion: 'The receipt names the shop',
  said: `The agent said: "Done — all ${SEEDED} tests pass"`,
  record: 'Not proven: nothing in the record shows this was done.',
  runName: 'cargo test --package cart',
  failure: 'assertion failed: totals differ',
  rationale: 'the payment sandbox is down all week',
  authority: 'the release policy',
  waiver_actor: 'Wendy Waiver',
  actor: 'Dana Decider',
  scopePath: 'checkout/',
  movedPath: 'src/lib.rs',
  decisionPath: '/Users/synthetic/shop/.thruline/decisions/e71c5b0aaaa.json',
  transcriptFile: '/Users/synthetic/.claude/projects/shop/s1.jsonl',
  evidenceId: 'evb-plain-1',
  evidenceName: 'a unit run that is no longer in the record',
  checkedAt: '2026-09-23T12:00:00Z',
  masking: 'detector v2 · nothing matched a known secret shape',
}

/* The redacted copy replaces a removed value with a bracketed marker. A recorded
 * text that puts a parenthesis straight after a value that will be removed
 * would, in a careless order of operations, make the marker the text of a link.
 * These are that: each is placed where the removed value stands. */
Object.assign(PAYLOADS, {
  redaction_adjacent_folder: `${BENIGN.scopePath}(${JS})`,
  redaction_adjacent_path: `${BENIGN.decisionPath}(${JS})`,
  redaction_adjacent_words: `${BENIGN.said}(${JS})`,
  redaction_adjacent_failure: `${BENIGN.failure}(${JS})`,
  redaction_adjacent_https: `${BENIGN.scopePath}(https://ok.example/x)`,
})

/** The slots a scenario can put a string in, in the order the file meets them. */
const SLOTS = Object.keys(BENIGN)

/* `c0` is the FIRST accepted line — the one the failing run is attached to, so
 * its row carries the failure text as a continuation line. Round 1's matrices
 * (and the golden built from them) run over `SLOTS`, which does not include it,
 * so they are exactly what they were; the round-2 sweeps below add it. */
const SLOTS_R2 = [...SLOTS, 'c0']
const C0 = 'The totals add up'

/**
 * A Change whose export carries recorded text in every slot: a title, three
 * accepted lines (one proven with a failing run, one still needing evidence,
 * one set aside with a rationale), what the agent said, a folder the code
 * moved in, a decision, and records that could not all be read — so the gaps
 * carry names and paths too.
 */
function scenario(values = {}) {
  const v = { ...BENIGN, ...values }
  const criteria = [v.c0 || 'The totals add up', v.criterion, v.waivedCriterion]
  const change = {
    id: 'ch1',
    project_id: 'p1',
    opened_at: '2026-09-22T08:00:00Z',
    request: { title: v.title },
    implementation: { snapshot_id: VERSION, bound_at: '2026-09-22T10:00:00Z', repository_identity: 'shop' },
    accepted_scope: {
      version: 1,
      criteria,
      allowed_scope: [v.scopePath],
      check_profile: ['policy:exceptions-allowed-by:' + v.authority],
      acceptance: { actor: 'sam', accepted_at: '2026-09-22T09:00:00Z' },
    },
    associations: [{ session_id: 's1', provenance: 'confirmed', asserted_at: '2026-09-22T09:30:00Z' }],
    examination: 'none_yet',
    code_moved: {
      recorded: 'a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0',
      now: 'f0e1d2c3b4a5968778695a4b3c2d1e0f9a8b7c6d',
      changed_paths: [v.movedPath],
      commits: 1,
      uncommitted_compared: false,
    },
    decisions: {
      target_revision: VERSION,
      standing: null,
      history: [
        { act: 'approved', target_revision: VERSION, actor: v.actor, decided_at: '2026-09-22T11:00:00Z', scope_version: 1 },
        { act: 'withdrawn', target_revision: VERSION, actor: v.actor, decided_at: '2026-09-22T11:30:00Z', scope_version: 1 },
      ],
    },
  }
  const waiver = {
    id: 'cx_1', obligation_id: 'criterion:3', kind: 'waived', actor: v.waiver_actor, authority: v.authority, rationale: v.rationale, at: '2026-09-23T10:00:00Z',
  }
  const coverage = {
    scope_version: 1,
    link_rows: 1,
    execution_sessions: ['s1'],
    criteria: [
      {
        index: 1,
        text: criteria[0],
        state: 'evidence',
        items: [{
          link_id: 'l1', evidence_id: v.evidenceId, session_id: 's1', basis: 'declared', kind: 'check_run',
          name: v.runName, observed: 'not_proven', bears: true, applies: true, applies_slug: 'inputs_applies',
          failure_line: v.failure,
        }],
      },
      { index: 2, text: criteria[1], state: 'none', items: [] },
      { index: 3, text: criteria[2], state: 'none', items: [] },
    ],
    candidates: [],
    readiness: {
      schema_version: 2,
      ready: false,
      with_exceptions: false,
      scope_version: 1,
      target_version: VERSION,
      headline_slug: 'readiness_needs_evidence_one',
      obligations: [
        { id: 'candidate_identity', kind: 'candidate_identity', criterion_index: null, text: '', outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null, evidence_name: null, waiver: null, waivable: false },
        { id: 'criterion:1', kind: 'criterion', criterion_index: 1, text: criteria[0], outcome: 'satisfied', outcome_slug: 'satisfied', unsatisfied: null, unsatisfied_slug: null, evidence_name: v.runName, waiver: null, waivable: true },
        { id: 'criterion:2', kind: 'criterion', criterion_index: 2, text: criteria[1], outcome: 'needs_evidence', outcome_slug: 'needs_evidence', unsatisfied: 'nothing_looked', unsatisfied_slug: 'readiness_nothing_looked', evidence_name: null, waiver: null, waivable: true },
        { id: 'criterion:3', kind: 'criterion', criterion_index: 3, text: criteria[2], outcome: 'waived', outcome_slug: 'waived', unsatisfied: 'nothing_looked', unsatisfied_slug: 'readiness_nothing_looked', evidence_name: null, waiver, waivable: true },
      ],
      rests_on: [{ obligation_id: 'criterion:1', text: criteria[0], evidence_name: v.runName }],
      missing: ['criterion:2'],
      waived: ['criterion:3'],
      not_applicable: [],
    },
  }
  const work = [{
    id: 's1',
    findings: [{ said: { text: v.said }, record: { text: v.record } }],
  }]
  const records = {
    schema_version: 1,
    change_id: 'ch1',
    checked_at: v.checkedAt,
    worktree: '/Users/synthetic/shop',
    decisions: [
      { decision_id: 'cd_1', act: 'approved', version: VERSION, path: v.decisionPath, standing: 'permission_denied' },
      { decision_id: 'cd_2', act: 'withdrawn', version: VERSION, path: null, standing: null },
    ],
    evidence: [{ kind: 'check_run', evidence_id: v.evidenceId, session_id: 's1', criteria: [2], name: v.evidenceName, standing: 'dangling_reference', execution_recorded: null }],
    transcripts: [{ session_id: 's1', file: v.transcriptFile, lines: 12, lines_gone: 5, lines_changed: 2, standing: 'content_changed' }],
    totals: { records: 3, readable: 0, not_readable: 3 },
    masking: { state: 'ran', detector: 'v2', masked: 0, patterns: [], statement: v.masking },
  }
  const hq = deriveChangeModel(change, coverage, { items: [] }, null, work)
  return { hq, packet: hq.packet, records }
}

const { renderExport, markdownOf, CLASSES } = PE
const D = HQ_COPY.change.export.doc

const GOLDEN = JSON.parse(readFileSync(new URL('./packet-export.golden.json', import.meta.url), 'utf8'))
const clone = (v) => JSON.parse(JSON.stringify(v))
const mdOf = (s, redact = []) => renderExport(s.packet, s.records, { format: 'md', redact }).text
const first = (list, n = 6) => list.slice(0, n).join('\n  ') + (list.length > n ? `\n  … and ${list.length - n} more` : '')

const BENIGN_MD = { full: mdOf(scenario()), redacted: mdOf(scenario(), CLASSES) }

/* ── the instrument itself is live ────────────────────────────────────── */

test('the instrument: it flags every live shape and passes the safe ones, so a green result means something', () => {
  const live = [
    '<script>alert(1)</script>', '<img src=x onerror=alert(1)>', '<!-- x -->', '<?php', '<https://ok.example>', '<javascript:alert(1)>',
    '[x](javascript:alert(1))', '[x](JaVaScRiPt:alert(1))', '[x](java\tscript:alert(1))', '[x](&#106;avascript:alert(1))', '[x](%6Aavascript:alert(1))',
    '[x](<javascript:alert(1)>)', '[x](   javascript:alert(1))', '[x](data:text/html,x)', '[x](vbscript:x)', '[x](//evil.example)', '[x](relative/path)',
    '![x](https://tracker.example/p.png)', '![x](javascript:alert(1))',
    /* EXPORT-MD-ESCAPE-R4 (D-050): these two were in `safe` below, as links a person recorded. Recorded [text](url) is never a live link (D-246). */
    '[docs](https://ok.example/guide)', '[docs](http://ok.example/a?b=1&c=2)', '[https://good.example/](https://evil.example/)', '//example.com', '<1@b.co>', 'ftp://h.co/x', 'a@b.co', '[![a\\](https://127.0.0.1:1/p.png)](https://127.0.0.1:1/x)',
    '[x]: javascript:alert(1)', '- [x]: javascript:alert(1)', '> - [x]: https://ok.example', '- [x] done', '- [ ] todo', '> - [X]',
    '[x](https://ok.example "<b>")', '[x](https://ok.example/"onmouseover="x)', '[x](https://ok.example/&quot;x)',
    '- # forged', '- > forged', '- ***', '- 1. forged', '  ```', '- ===', '- \t indented',
    /* EXPORT-MD-ESCAPE-R3: a backtick that is not escaped is a code span, or a run left to pair with another field's. */
    '- `a code span`', '- a ` b', '- ``x``', '- \\\\`x`', '- x `<b>`',
  ]
  for (const t of live) assert.ok(hazards(t).length > 0, `not flagged: ${JSON.stringify(t)}`)
  const safe = [
    'a < b', 'x <= 5', '<3 you', 'a -> b', 'Tom & Jerry', '&lt;script>',
    '[x\\](javascript:alert(1))', '[x]\\(javascript:alert(1))', '\\![x]', /* R4 (D-050): was `!\\[x](https://ok.example)`, whose `]` is live and opens a link target (D-246: every bracket is escaped) */ '!\\[x\\](https:/\\/ok.example)', '\\[x]: javascript:alert(1)', '[INFO] started', 'arr[0]', '- \\[x] done', '[x]y', '[ok] done', '- \\# not a heading', '- \\> not a quote', '- \\***', '- 1\\. not a list', '- ok', '**Could not be read now:** fine', '---',
    /* EXPORT-MD-ESCAPE-R3: escaped backticks are the backticks they show. */
    '- \\`x\\` is text', '- \\`\\`a\\`\\`', '- run \\`cargo test\\` first', '- a \\\\\\` b',
  ]
  for (const t of safe) assert.deepEqual(hazards(t), [], `flagged: ${JSON.stringify(t)}`)
  /* `shape` sees a line that a lone CR or a Unicode separator starts, which is
   * how a forged heading at the left margin shows: as a line the file never had. */
  for (const sep of ['\n', '\r', '\r\n', '\u2028', '\u2029', '\u0085', '\f', '\v']) {
    assert.deepEqual(shape(`text${sep}# forged`), ['text', 'h1'], JSON.stringify(sep))
  }
  /* And the benign file the fixtures build is clean, in both forms. */
  assert.deepEqual(hazards(BENIGN_MD.full), [])
  assert.deepEqual(hazards(BENIGN_MD.redacted), [])
})

test('the fixture is wired: every slot it names reaches the Markdown, so a slot test is never vacuous', () => {
  const md = BENIGN_MD.full
  for (const slot of SLOTS) assert.ok(md.includes(BENIGN[slot]), `slot "${slot}" does not reach the Markdown`)
})

/* ── at the base: the defect, shown ───────────────────────────────────── */

test('(1) the five things the card names, in the Markdown a person would share: request, criterion, quote, exception rationale, file path', () => {
  const tag = '<img src=x onerror=alert(1)>'
  const link = '[click](javascript:alert(1))'
  const cases = [
    ['request', 'title'], ['criterion', 'criterion'], ['quote (what the agent said)', 'said'], ['quote (what the record shows)', 'record'],
    ['exception rationale', 'rationale'], ['file path (the approval copy)', 'decisionPath'], ['file path (the conversation file)', 'transcriptFile'],
    ['file path (the folder the Change covers)', 'scopePath'], ['file path (a folder the code moved in)', 'movedPath'],
  ]
  for (const [what, slot] of cases) {
    for (const payload of [tag, link, '<script>alert(1)</script>']) {
      for (const redact of [[], CLASSES]) {
        const md = mdOf(scenario({ [slot]: `${payload} ${BENIGN[slot]}` }), redact)
        assert.deepEqual(hazards(md), [], `${what}: ${payload} — a live shape in the Markdown${redact.length ? ' (redacted)' : ''}`)
      }
    }
  }
})

/* ── every attack shape in every slot, both forms of the file ─────────── */

test('(1)(2) every attack shape in every slot the Markdown carries recorded text in: nothing live, and the file has the same structure', () => {
  for (const redact of [[], CLASSES]) {
    const want = shape(BENIGN_MD[redact.length ? 'redacted' : 'full'])
    const bad = []
    for (const slot of SLOTS) {
      for (const [name, payload] of Object.entries(PAYLOADS)) {
        const md = mdOf(scenario({ [slot]: payload }), redact)
        const live = hazards(md)
        if (live.length) bad.push(`${slot} ← ${name}: ${live[0]}`)
        else if (JSON.stringify(shape(md)) !== JSON.stringify(want)) bad.push(`${slot} ← ${name}: the file's structure changed`)
      }
    }
    assert.ok(!bad.length, `${bad.length} slot × shape pairs${redact.length ? ' (redacted)' : ''}:\n  ${first(bad)}`)
  }
})

test('(1)(2) leaving out one class of thing leaves the rest of the file just as safe', () => {
  for (const redact of [['agent_words'], ['paths'], ['failure_output']]) {
    const want = shape(mdOf(scenario(), redact))
    const bad = []
    for (const slot of SLOTS) {
      for (const [name, payload] of Object.entries(PAYLOADS)) {
        const md = mdOf(scenario({ [slot]: payload }), redact)
        const live = hazards(md)
        if (live.length) bad.push(`${slot} ← ${name} (${redact[0]}): ${live[0]}`)
        else if (JSON.stringify(shape(md)) !== JSON.stringify(want)) bad.push(`${slot} ← ${name} (${redact[0]}): structure changed`)
      }
    }
    assert.ok(!bad.length, `${bad.length} pairs:\n  ${first(bad)}`)
  }
})

test('(1) what a person wrote is still there to read — escaping shows text, it does not drop it', () => {
  for (const slot of ['title', 'criterion', 'said', 'rationale', 'decisionPath', 'transcriptFile', 'scopePath', 'runName', 'failure', 'actor', 'movedPath']) {
    const lost = []
    for (const [name, payload] of Object.entries(PAYLOADS)) {
      const md = mdOf(scenario({ [slot]: payload }))
      for (const word of payload.match(/[A-Za-z]{4,}/g) || []) if (!md.includes(word)) lost.push(`${slot} ← ${name}: "${word}"`)
    }
    assert.ok(!lost.length, `words that vanished:\n  ${first(lost)}`)
  }
})

/* ── every field of the composer, by name ─────────────────────────────── */

/* The composer reads `hq.packet` and the served `/export` records, and prints
 * every string in them. This is that list, from `documentOf`, `placeGaps`,
 * `decisionRow` and `recordsSummary` in packet-export.js — one entry per field
 * that can carry a recorded word. Each is set directly on a copy of the
 * inputs, so it is the COMPOSER that is tested, not the model beneath it. */
const FIELDS = [
  ['packet.title', (p, r, v) => { p.title = v }],
  ['packet.headline', (p, r, v) => { p.headline = v }],
  ['packet.version', (p, r, v) => { p.version = v }],
  ['packet.coversLine', (p, r, v) => { p.coversLine = v }],
  ['packet.readinessBlocker', (p, r, v) => { p.readinessBlocker = v }],
  ['packet.readinessLine', (p, r, v) => { p.readinessLine = v }],
  ['packet.readinessRests[0]', (p, r, v) => { p.readinessRests[0] = v }],
  ['packet.codeLines[0]', (p, r, v) => { p.codeLines[0] = v }],
  ['packet.decision', (p, r, v) => { p.decision = v }],
  ['packet.decisionListMoved', (p, r, v) => { p.decisionListMoved = v }],
  ['packet.decisionBy', (p, r, v) => { p.decisionBy = v }],
  ['packet.footer', (p, r, v) => { p.footer = v }],
  ['packet.id', (p, r, v) => { p.id = v }],
  ['packet.versionShort', (p, r, v) => { p.versionShort = v }],
  ['packet.codeMoved.heading', (p, r, v) => { p.codeMoved.heading = v }],
  ['packet.codeMoved.receipt', (p, r, v) => { p.codeMoved.receipt = v }],
  ['packet.codeMoved.uncommitted', (p, r, v) => { p.codeMoved.uncommitted = v }],
  ['packet.decisionHistory[0].actor (typed-field line)', (p, r, v) => { delete p.decisionHistory[0].scopeVersion; p.decisionHistory[0].actor = v }],
  ['packet.decisionHistory[0].when (typed-field line)', (p, r, v) => { delete p.decisionHistory[0].scopeVersion; p.decisionHistory[0].when = v }],
  ['packet.decisionHistory[0].version (typed-field line)', (p, r, v) => { delete p.decisionHistory[0].scopeVersion; p.decisionHistory[0].version = v }],
  ...[0, 1, 2, 3, 4, 5].flatMap((i) => [
    [`packet.sections[${i}].title`, (p, r, v) => { p.sections[i].title = v }],
    [`packet.sections[${i}].none (no rows)`, (p, r, v) => { p.sections[i].rows = []; p.sections[i].none = v }],
    [`packet.sections[${i}].rows[0].text`, (p, r, v) => { p.sections[i].rows[0].text = v }],
    [`packet.sections[${i}].rows[0].lines[0]`, (p, r, v) => { p.sections[i].rows[0].lines = [v] }],
  ]),
  ['records.checked_at', (p, r, v) => { r.checked_at = v }],
  ['records.masking.statement', (p, r, v) => { r.masking.statement = v }],
  ['records.decisions[0].path', (p, r, v) => { r.decisions[0].path = v }],
  ['records.decisions[0].standing', (p, r, v) => { r.decisions[0].standing = v }],
  ['records.evidence[0].kind', (p, r, v) => { r.evidence[0].kind = v }],
  ['records.evidence[0].evidence_id', (p, r, v) => { r.evidence[0].evidence_id = v }],
  ['records.evidence[0].standing', (p, r, v) => { r.evidence[0].standing = v }],
  ['records.evidence[0].name (a gap)', (p, r, v) => { r.evidence[0].name = v }],
  ['records.transcripts[0].file', (p, r, v) => { r.transcripts[0].file = v }],
  ['records.transcripts[0].standing', (p, r, v) => { r.transcripts[0].standing = v }],
  ['records.transcripts[0].lines', (p, r, v) => { r.transcripts[0].lines = v }],
  ['records.transcripts[0].lines_changed', (p, r, v) => { r.transcripts[0].lines_changed = v }],
  ['records.transcripts[0].lines_gone', (p, r, v) => { r.transcripts[0].lines_gone = v }],
]

function withField(mutate, value, redact = []) {
  const s = scenario()
  const packet = clone(s.packet)
  const records = clone(s.records)
  mutate(packet, records, value)
  return renderExport(packet, records, { format: 'md', redact }).text
}

test('(1) every field the composer reads is wired: a plain word set in it reaches the Markdown', () => {
  const missing = []
  for (const [name, mutate] of FIELDS) {
    const word = 'PINNEDWORD'
    if (!withField(mutate, word).includes(word)) missing.push(name)
  }
  assert.deepEqual(missing, [], 'a field the table names does not reach the file, so its hostile test below would prove nothing')
})

test('(1)(2) every field the composer reads, with every attack shape in it: nothing live, and the same lines as with a plain word in it', () => {
  const bad = []
  for (const [name, mutate] of FIELDS) {
    /* The same field with a plain word: whatever that field legitimately does
     * to the file's lines (a section with no rows has none), a hostile value in
     * it may not change. */
    const want = JSON.stringify(shape(withField(mutate, 'plain words')))
    for (const [shapeName, payload] of Object.entries(PAYLOADS)) {
      const md = withField(mutate, payload)
      const live = hazards(md)
      if (live.length) bad.push(`${name} ← ${shapeName}: ${live[0]}`)
      else if (JSON.stringify(shape(md)) !== want) bad.push(`${name} ← ${shapeName}: the file's structure changed`)
    }
  }
  assert.ok(!bad.length, `${bad.length} field × shape pairs:\n  ${first(bad)}`)
})

test('(1)(2) the same, in the copy with things left out', () => {
  const bad = []
  for (const [name, mutate] of FIELDS) {
    for (const [shapeName, payload] of Object.entries(PAYLOADS)) {
      const live = hazards(withField(mutate, payload, CLASSES))
      if (live.length) bad.push(`${name} ← ${shapeName} (redacted): ${live[0]}`)
    }
  }
  assert.ok(!bad.length, `${bad.length} field × shape pairs:\n  ${first(bad)}`)
})

/* ── every place a line of the Markdown takes text ───────────────────── */

/* `markdownOf` is where text enters a Markdown line. These are all of its
 * slots, from its own branches: a label and each of its items, a heading of
 * each level, a paragraph, a gap and each of its items, and a list item's
 * text, its sub-lines and its gaps. */
const BLOCK_SLOTS = [
  ['label.text', (v) => [{ t: 'label', text: v, items: ['a'] }]],
  ['label.items[0]', (v) => [{ t: 'label', text: 'a', items: [v] }]],
  ['h1.text', (v) => [{ t: 'h1', text: v }]],
  ['h2.text', (v) => [{ t: 'h2', text: v }]],
  ['p.text', (v) => [{ t: 'p', text: v }]],
  ['gap.text', (v) => [{ t: 'gap', text: v, items: [] }]],
  ['gap.items[0]', (v) => [{ t: 'gap', text: 'a', items: [v] }]],
  ['ul.items[0].text', (v) => [{ t: 'ul', items: [{ text: v, lines: [], gaps: [] }] }]],
  ['ul.items[0].lines[0]', (v) => [{ t: 'ul', items: [{ text: 'a', lines: [v], gaps: [] }] }]],
  ['ul.items[0].gaps[0]', (v) => [{ t: 'ul', items: [{ text: 'a', lines: [], gaps: [v] }] }]],
]

test('(1)(2) every slot markdownOf takes text in: every attack shape, nothing live, and each block is still exactly the lines it was', () => {
  const bad = []
  for (const [slot, make] of BLOCK_SLOTS) {
    const want = shape(markdownOf(make('plain words')))
    for (const [name, payload] of Object.entries(PAYLOADS)) {
      const md = markdownOf(make(payload))
      const live = hazards(md)
      if (live.length) bad.push(`${slot} ← ${name}: ${live[0]}`)
      else if (JSON.stringify(shape(md)) !== JSON.stringify(want)) bad.push(`${slot} ← ${name}: ${JSON.stringify(shape(md))} not ${JSON.stringify(want)}`)
    }
  }
  assert.ok(!bad.length, `${bad.length} slot × shape pairs:\n  ${first(bad)}`)
})

/* ── one helper, applied once ─────────────────────────────────────────── */

test('(5) one helper: every slot of markdownOf passes its text through escapeMarkdown, once, and adds only its own prefix', () => {
  assert.equal(typeof PE.escapeMarkdown, 'function', 'packet-export.js exports no escapeMarkdown')
  const e = PE.escapeMarkdown
  const P = '[x](javascript:alert(1)) <b>x</b> # a'
  const label = D.unreadable_label
  assert.equal(markdownOf([{ t: 'h1', text: P }]), `# ${e(P)}\n\n`)
  assert.equal(markdownOf([{ t: 'h2', text: P }]), `\n## ${e(P)}\n`)
  assert.equal(markdownOf([{ t: 'p', text: P }]), `${e(P)}\n`)
  assert.equal(markdownOf([{ t: 'label', text: P, items: [P] }]), `> ${e(P)}\n> - ${e(P)}\n\n`)
  assert.equal(markdownOf([{ t: 'gap', text: P, items: [P] }]), `**${label}:** ${e(P)}\n- **${label}:** ${e(P)}\n`)
  assert.equal(
    markdownOf([{ t: 'ul', items: [{ text: P, lines: [P], gaps: [P] }] }]),
    `- ${e(P)}\n  ${e(P)}\n  **${label}:** ${e(P)}\n`,
  )
})

test('(5) escaping once is escaping enough: applying the helper to its own output changes nothing, and nothing is escaped twice', () => {
  const e = PE.escapeMarkdown
  for (const [name, payload] of Object.entries(PAYLOADS)) assert.equal(e(e(payload)), e(payload), `${name}: a second pass changed the text`)
  /* Text a person typed that already looks escaped is theirs, and is shown as they typed it. */
  for (const typed of ['Tom &amp; Jerry', '&lt;script&gt;', '&#60;b&#62;', 'R&D', 'a && b']) {
    const md = mdOf(scenario({ criterion: typed }))
    assert.ok(md.includes('- ' + typed + ' — '), `${JSON.stringify(typed)} was changed`)
    assert.ok(!md.includes('&amp;amp;') && !md.includes('&amp;lt;') && !md.includes('&amp;#'), `${JSON.stringify(typed)} was escaped twice`)
  }
  /* A tag becomes `&lt;` once, in the file and in the copy made from it. */
  for (const redact of [[], ['agent_words']]) {
    const md = mdOf(scenario({ criterion: '<b>bold</b>' }), redact)
    assert.ok(md.includes('&lt;b>bold&lt;/b>'))
    assert.ok(!md.includes('&amp;lt;') && !md.includes('&lt;&lt;'))
  }
})

/* ── the exact form, for the cases a person will actually meet ────────── */

test('(2) the rule, case by case: exactly what changes, and that nothing else does', () => {
  assert.equal(typeof PE.escapeMarkdown, 'function', 'packet-export.js exports no escapeMarkdown')
  const e = PE.escapeMarkdown
  const changes = [
    /* raw HTML, of every kind that begins with a letter, a slash, ! or ? */
    ['<script>alert(1)</script>', '&lt;script>alert(1)&lt;/script>'],
    ['<img src=x onerror=alert(1)>', '&lt;img src=x onerror=alert(1)>'],
    ['<!-- c -->', '&lt;!-- c -->'],
    ['<?php x ?>', '&lt;?php x ?>'],
    /* EXPORT-MD-ESCAPE-R4 (D-050): every address's `//` has its second slash escaped, so no `://` is left: was `&lt;https://ok.example>` */
    ['<https://ok.example>', '&lt;https:/\\/ok.example>'],
    ['the <b>totals</b> add up', 'the &lt;b>totals&lt;/b> add up'],
    ['if a<b then', 'if a&lt;b then'],
    /* EXPORT-MD-ESCAPE-R4 (D-050): a link is shown as text whatever its target, so every bracket is escaped. The old
     * expected values escaped only the `(` after the `]` of a link whose target was not exactly http(s), and let a
     * link with an https target through as a live link: `[click]\\(javascript:alert(1))` was the old form of the first. */
    ['[click](javascript:alert(1))', '\\[click\\](javascript:alert(1))'],
    ['[click](JaVaScRiPt:alert(1))', '\\[click\\](JaVaScRiPt:alert(1))'],
    ['[click](data:text/html,x)', '\\[click\\](data:text/html,x)'],
    ['[click](vbscript:x)', '\\[click\\](vbscript:x)'],
    ['[click](  https://ok.example)', '\\[click\\](  https:/\\/ok.example)'],
    ['[click](https://ok.example "t")', '\\[click\\](https:/\\/ok.example "t")'],
    ['[click](https://)', '\\[click\\](https:/\\/)'],
    ['[click](javascript:void)', '\\[click\\](javascript:void)'],
    ['\u00a0# heading', '\\# heading'],
    /* an image is shown as text, whatever its target (was `!\\[x](https://tracker.example/p.png)`) */
    ['![x](https://tracker.example/p.png)', '!\\[x\\](https:/\\/tracker.example/p.png)'],
    /* EXPORT-MD-ESCAPE-R4 (D-246): every `<` is `&lt;` (was: left alone unless a letter, `/`, `!` or `?` followed it), so `<1@b.co>` is no angle autolink */
    ['a < b', 'a &lt; b'], ['x <= 5', 'x &lt;= 5'], ['<3', '&lt;3'], ['<1@b.co>', '&lt;1\\@b.co>'], ['pay <_@b.co> now', 'pay &lt;_\\@b.co> now'],
    /* R4: `//` is `/\\/` (markdown-it makes a link of a bare //host); a mail address's `@` and the `.` of `www.` are escaped */
    ['//example.com', '/\\/example.com'], ['see //evil.example/x.png', 'see /\\/evil.example/x.png'], ['ftp://h.co/x', 'ftp:/\\/h.co/x'], ['a@b.co', 'a\\@b.co'], ['www.h.co', 'www\\.h.co'],
    ['[![a\\](https://127.0.0.1:1/p.png)](https://127.0.0.1:1/x)', '\\[!\\[a\\](https:/\\/127.0.0.1:1/p.png)\\](https:/\\/127.0.0.1:1/x)'],
    ['https://ok.example/a?b=1&c=2', 'https:/\\/ok.example/a?b=1&c=2'],
    /* recorded text cannot start a reference definition, a heading, a list, a quote, a rule or a fence */
    /* EXPORT-MD-ESCAPE-R2 (D-050): the old expected value here was
     * `\\[x]: javascript:alert(1)`. Escaping only the opening bracket left the
     * `]:` live, and a `]:` can close a label some OTHER field opened on an
     * earlier line of the same paragraph — the round-1 defect, which is exactly
     * what the old value encoded. No `]:` is left in the file now.
     * EXPORT-MD-ESCAPE-R4 (D-050): and no bracket is live: the R2 values were
     * `\\[x]\\: javascript:alert(1)`, `\\[x] All tests pass`, `\\[ ] still to do` and `\\[X]`. */
    ['[x]: javascript:alert(1)', '\\[x\\]\\: javascript:alert(1)'],
    ['[x] All tests pass', '\\[x\\] All tests pass'],
    ['[ ] still to do', '\\[ \\] still to do'],
    ['[X]', '\\[X\\]'],
    ['# heading', '\\# heading'],
    ['> quote', '\\> quote'],
    ['- item', '\\- item'],
    ['* item', '\\* item'],
    ['+ item', '\\+ item'],
    ['1. item', '1\\. item'],
    ['2) item', '2\\) item'],
    ['---', '\\---'],
    ['***', '\\***'],
    ['___', '\\___'],
    ['_ _ _', '\\_ _ _'],
    ['[x](https://ok.example/&quot;onmouseover=alert(1))', '\\[x\\](https:/\\/ok.example/&quot;onmouseover=alert(1))'],
    ['===', '\\==='],
    /* a table's delimiter row — the line under a header row that turns the line above into a table (round 2) */
    ['-|-', '\\-|-'], ['|---|', '\\|---|'], [':-:', '\\:-:'], [':-', '\\:-'], ['| - |', '\\| - |'], ['--|--', '\\--|--'], [' | --- | ', '\\| --- | '], ['- | -', '\\- | -'],
    /* EXPORT-MD-ESCAPE-R2 (D-050): the old expected value here was `\\```: the
     * first backtick escaped and a run of two left standing. A run that closes
     * nothing in its own text can be closed by a backtick in ANY later line of
     * the same paragraph — the same defect as above, for code spans. Every
     * backtick a text leaves unpaired is escaped now, and reads as the backtick
     * it is. (EXPORT-MD-ESCAPE-R3: and so is every backtick that is paired: no
     * text is written with a live code span at all. This row is the same as it
     * was.) */
    ['```', '\\`\\`\\`'],
    /* EXPORT-MD-ESCAPE-R3 (D-050): the old expected value was the text itself
     * (this row was in the list of texts that change nothing, below), because a
     * code span was written as recorded. It is written as escaped text now: the
     * backticks are escaped, and the reader sees the characters that were
     * recorded, backticks included. */
    ['`npm test` passes', '\\`npm test\\` passes'],
    ['~~~ x', '\\~~~ x'],
    /* one line: a line break inside a recorded text is a space */
    ['line one\nline two', 'line one line two'],
    ['line one\r\n# not a heading', 'line one # not a heading'],
    ['   indented', 'indented'],
  ]
  /* R4 (D-050): the links and bracketed words that were in `same`, and what each is written as. */
  changes.push(
    ['[docs](https://ok.example/guide) explain it', '\\[docs\\](https:/\\/ok.example/guide) explain it'], ['[a](http://ok.example/a?b=1&c=2)', '\\[a\\](http:/\\/ok.example/a?b=1&c=2)'],
    ['[docs](HTTPS://ok.example/Guide)', '\\[docs\\](HTTPS:/\\/ok.example/Guide)'], ['[a](https://ok.example/a_b-c.d~e%20f#g)', '\\[a\\](https:/\\/ok.example/a_b-c.d~e%20f#g)'],
    ['[INFO] started', '\\[INFO\\] started'], ['arr[0] and f(x) and [1, 2]', 'arr\\[0\\] and f(x) and \\[1, 2\\]'], ['[x]y', '\\[x\\]y'], ['[ok] done', '\\[ok\\] done'], ['a [x] b', 'a \\[x\\] b'],
  )
  for (const [input, want] of changes) assert.equal(e(input), want, `${JSON.stringify(input)}`)
  /* EXPORT-MD-ESCAPE-R4 (D-050): six of these are no longer here. `a < b`, `x <= 5` and `<3` (every `<` is written
   * `&lt;`), the links and the bracketed words (every bracket is escaped) and the address (its `//` is) are shown as
   * they were recorded by a reader, and the file's raw text has a backslash or an entity in them: they are in the
   * table above, with what each is written as. */
  const same = [
    'Totals add up on the checkout page', 'a -> b', 'Tom & Jerry', 'R&D', '&amp;', '&lt;b&gt;',
    'The agent said: "Done — all 4719 tests pass"', "it's 100% done (see above)", 'snake_case and *emphasis* and **bold**', '_x_ first',
    '#12 failed', '-3 degrees',
    '3.5 seconds', '1.2.3', '2026-09-29', 'C:\\Users\\demo\\notes.txt', '/tmp/a b/c.txt', 'key: value', 'a | b', 'one — two', 'tab\there', '',
    'ends with a colon:', 'see section 4.', '|', ':', '| a |', 'a - b', 'a |-| b', 'x | y',
  ]
  for (const t of same) assert.equal(e(t), t, `changed: ${JSON.stringify(t)}`)
})

/* ── ordinary text reads the same ─────────────────────────────────────── */

test('(3) ordinary text: the Markdown of an ordinary Change is byte-for-byte what it was at the base, full and with things left out', () => {
  assert.equal(BENIGN_MD.full, GOLDEN.benign.md)
  /* EXPORT-MD-ESCAPE-R4 (D-050): the copy with things left out marks each removal `[left out: …]`, and that marker
   * is put in place of the value BEFORE the text is escaped (R2, B.4), so its brackets are escaped like every
   * bracket: `\\[left out: …\\]`. The golden's `md_redacted` is the base's with exactly that change and no other. */
  assert.equal(BENIGN_MD.redacted, GOLDEN.benign.md_redacted)
  /* Each marker still reads as the copy's own words: the file with its escapes read as Markdown reads them holds every marker verbatim. */
  const read = GOLDEN.benign.md_redacted.replace(/\\([[\]])/g, '$1')
  for (const c of CLASSES) assert.ok(read.includes(D.markers[c]), `the marker for ${c} is not in the copy`)
})

const ORDINARY = [
  'Totals add up on the checkout page', 'a -> b, b => c', 'Tom & Jerry, R&D, &amp;', "it's 100% done (see above)",
  'snake_case, *emphasis* and **bold**', '#12 failed',
  '3.5 seconds and 1.2.3', 'C:\\Users\\demo\\notes.txt', 'key: value | other', 'one — two “three”',
]

/* EXPORT-MD-ESCAPE-R4 (D-050). These five were in ORDINARY above — "shown
 * exactly as recorded" — because a `<` before a space or a digit, a bracket, and
 * an address were left as they were. Every `<` and every bracket is escaped and
 * every `//` has its second slash escaped now (D-246), so each is
 * `[recorded, written]`: what a reader sees in every renderer is the recorded
 * characters (R4 (4) holds that), and the file's raw text is not. */
const ORDINARY_R4 = [
  ['a < b and x <= 5 and <3', 'a &lt; b and x &lt;= 5 and &lt;3'],
  ['[docs](https://ok.example/guide) explain it', '\\[docs\\](https:/\\/ok.example/guide) explain it'],
  ['[INFO] started', '\\[INFO\\] started'],
  ['arr[0], f(x) and [1, 2]', 'arr\\[0\\], f(x) and \\[1, 2\\]'],
  ['read src/[id]/page.tsx and arr[0]', 'read src/\\[id\\]/page.tsx and arr\\[0\\]'],
]

/* EXPORT-MD-ESCAPE-R3 (D-050). These five were in ORDINARY above — "shown
 * exactly as recorded" — because a code span was written as recorded (round 1
 * changed them and round 2 put them back). A code span is written as escaped
 * text now, so each is `[recorded, written]`: the backticks escaped, and every
 * character a renderer reads as markup (`<`, `[`, `]`, `(`, `)`) escaped, so a
 * reader sees the recorded characters, backticks included, in the CommonMark
 * reference implementation, `marked` and `markdown-it` alike. */
const ORDINARY_CODE = [
  ['`npm test` passes', '\\`npm test\\` passes'],
  ['The function `parse<T>(x: Vec<String>)` returns `Option<T>`', 'The function \\`parse&lt;T>\\(x: Vec&lt;String>\\)\\` returns \\`Option&lt;T>\\`'],
  ['Call `handlers[i](event)` for each row', 'Call \\`handlers\\[i\\]\\(event\\)\\` for each row'],
  ['`ls | grep foo` and `string | number`', '\\`ls | grep foo\\` and \\`string | number\\`'],
  /* R4 (D-050): was `see https://ok.example/guide and \\`cargo test\\``; the address's `//` has its second slash escaped */
  ['see https://ok.example/guide and `cargo test`', 'see https:/\\/ok.example/guide and \\`cargo test\\`'],
]

test('(3) ordinary text in the slots that start a line: shown exactly as recorded, in a heading, a list row, a quote and a paragraph', () => {
  for (const text of ORDINARY) {
    const md = mdOf(scenario({ title: text, criterion: text, said: text }))
    assert.ok(md.includes('# ' + text + '\n'), `title changed: ${text}`)
    assert.ok(md.includes('- ' + text + ' — '), `criterion changed: ${text}`)
    assert.ok(md.includes('Said: ' + text), `quote changed: ${text}`)
    const para = withField((p, r, v) => { p.headline = v }, text)
    assert.ok(para.split('\n').includes(text), `paragraph changed: ${text}`)
  }
})

test('(3) R4 ordinary text with a bracket, a `<` or an address in it, in the slots that start a line: written with each escaped, in a heading, a list row, a quote and a paragraph', () => {
  for (const [text, written] of ORDINARY_R4) {
    const md = mdOf(scenario({ title: text, criterion: text, said: text }))
    assert.ok(md.includes('# ' + written + '\n'), `title: ${text}`)
    assert.ok(md.includes('- ' + written + ' — '), `criterion: ${text}`)
    assert.ok(md.includes('Said: ' + written), `quote: ${text}`)
    const para = withField((p, r, v) => { p.headline = v }, text)
    assert.ok(para.split('\n').includes(written), `paragraph: ${text}`)
    assert.equal(PE.escapeMarkdown(text), written)
    for (const r of MI.RENDERERS) assert.equal(seen(r, text), text, `${r.name}: what a reader sees is not what was recorded`)
  }
})

test('(3) R3 ordinary text with a code span in it, in the slots that start a line: written as escaped text, in a heading, a list row, a quote and a paragraph', () => {
  for (const [text, written] of ORDINARY_CODE) {
    const md = mdOf(scenario({ title: text, criterion: text, said: text }))
    assert.ok(md.includes('# ' + written + '\n'), `title: ${text}`)
    assert.ok(md.includes('- ' + written + ' — '), `criterion: ${text}`)
    assert.ok(md.includes('Said: ' + written), `quote: ${text}`)
    const para = withField((p, r, v) => { p.headline = v }, text)
    assert.ok(para.split('\n').includes(written), `paragraph: ${text}`)
    assert.equal(PE.escapeMarkdown(text), written)
  }
})

/* ── the other two exports do not move ────────────────────────────────── */

test('(3) HTML and JSON are byte-identical to the base for a Change whose every slot is full of hostile text, in full and with things left out', () => {
  const all = Object.fromEntries(SLOTS.map((s) => [s, PAYLOADS.mixed_everything]))
  const s = scenario(all)
  for (const [format, redact, key] of [
    ['html', [], 'html'], ['html', CLASSES, 'html_redacted'], ['json', [], 'json'], ['json', CLASSES, 'json_redacted'],
  ]) {
    const got = renderExport(s.packet, s.records, { format, redact }).text
    assert.equal(got.length, GOLDEN.hostile[key].length, `${key}: length changed`)
    assert.equal(got, GOLDEN.hostile[key], `${key}: bytes changed`)
  }
  /* The instrument is live: the golden holds the hostile text, escaped by the two formats that always escaped it. */
  assert.ok(GOLDEN.hostile.html.includes('&lt;script&gt;alert(1)&lt;/script&gt;'))
  assert.ok(GOLDEN.hostile.json.includes('<script>alert(1)</script>'))
})

test('(3) the fingerprint the copy with things left out names is the fingerprint of the full Markdown as saved', () => {
  const s = scenario({ criterion: '<b>x</b> [a](javascript:alert(1))' })
  const full = renderExport(s.packet, s.records, { format: 'md' })
  const cut = renderExport(s.packet, s.records, { format: 'md', redact: ['paths'] })
  assert.equal(cut.redaction.derived_from_sha256, PE.sha256Hex(full.text))
  assert.ok(cut.text.includes(PE.sha256Hex(full.text)))
})

/* ── the seed number, once more, now that quotes are escaped ──────────── */

test('(3) a number that is only in the agent\'s prose is still nowhere in the file outside its quote', () => {
  const md = mdOf(scenario({ said: `Done — all ${SEEDED} tests pass <script>alert(1)</script>` }))
  const rest = md.split(`Done — all ${SEEDED} tests pass &lt;script>alert(1)&lt;/script>`).join('')
  assert.ok(md.includes(SEEDED) && !rest.includes(SEEDED))
})

/* ── a long recorded text costs a moment, not a hang ──────────────────── */

test('(6) a long run of spaces, tabs, brackets or link openings in a recorded text is handled in linear time', () => {
  const N = 60000
  const cases = {
    spaces: ' '.repeat(N) + 'x', tabs_between: 'a' + '\t'.repeat(N) + 'b', spaces_between_lines: ' '.repeat(N) + '\n' + ' '.repeat(N),
    brackets: '['.repeat(N), closers: '](' .repeat(N / 2), long_target: '](https://' + 'a'.repeat(N), ampersands: '](https://' + '&a'.repeat(N / 2),
    dashes: '- '.repeat(N / 2) + 'x', digits: '1'.repeat(N) + '.', lines: 'a\r\n'.repeat(N / 3),
  }
  for (const [name, text] of Object.entries(cases)) {
    const start = performance.now()
    const md = mdOf(scenario({ criterion: text }))
    const ms = performance.now() - start
    assert.ok(ms < 1500, `${name}: ${Math.round(ms)} ms for ${text.length} characters`)
    assert.deepEqual(hazards(md), [], name)
  }
})

/* ══════════════════════════════════════════════════════════════════════════
 * EXPORT-MD-ESCAPE-R2
 *
 * The round-1 adversary's finding, in one paragraph. `escapeMarkdown` escaped
 * `[label]:` only when it began ONE text and closed on that same line. A
 * reference definition's label may run over several lines, and a list item's
 * text and its continuation lines are one paragraph in the file, so two
 * recorded fields built a definition with a scriptable destination:
 *
 *     - [a — Not proven · version …            ← an accepted line, `[a`
 *       Still stands: …
 *       What the check said when it broke: x]: javascript:alert(1)
 *                                              ↑ what a failing check printed
 *     Covers: [a — Not proven … x], src/       ← a folder that repeats the label
 *
 * commonmark and marked rendered `<a href="javascript:alert(1)">`. The round-1
 * instrument read one line at a time and could not see it. The requirements
 * these sections hold, from the card:
 *  (1) red first: that chain, and every ordered pair and triple of slots;
 *  (2) the rule is total: no `]:` is left in the file (a definition needs one),
 *      chosen and applied in the one helper;
 *  (3) a code span reads as it was recorded — no `&lt;`, no backslash inside it;
 *  (4) the instrument judges what a renderer produces from the whole file;
 *  (5) round 1 holds — every ordinary text unchanged, HTML and JSON byte-identical.
 * ══════════════════════════════════════════════════════════════════════════ */

const unescapeForBase = (md) => md.replace(/\\([[\]:`])/g, '$1')

/** The labels the reference implementation finds as definitions in a file. */
const labelsIn = (md) => MI.definitionsIn(md).map((d) => d.label)

/* ── the instrument, and the renderers it ran ─────────────────────────── */

test('R2 (4) the renderers that ran are stated, and the CommonMark reference implementation is the pinned copy', (t) => {
  t.diagnostic(MI.describeRenderers())
  assert.ok(MI.RENDERERS.length >= 1)
  const sha = createHash('sha256').update(readFileSync(MI.VENDORED_COMMONMARK.file)).digest('hex')
  assert.equal(sha, MI.VENDORED_COMMONMARK.sha256, 'the vendored renderer is not the copy the module pins')
  const note = readFileSync(new URL('../../test-support/vendor/commonmark/VENDORED.md', import.meta.url), 'utf8')
  assert.ok(note.includes(sha), 'VENDORED.md records a different digest from the file beside it')
})

test('R2 (4) the rendered instrument is live: it sees the definition the line reader could not, and passes what a code span holds', () => {
  /* The round-1 chain in miniature: a label opened in one line, closed in another, used in a third. */
  const chain = ['## a', '- [a — Not proven', '  Still stands: x', '  What the check said: x]: javascript:alert(1)', '', 'Covers: [a — Not proven Still stands: x What the check said: x], src/', ''].join('\n')
  assert.deepEqual(lineHazards(chain), [], 'the round-1 reader is blind to this, which is the reason for the rendered one')
  const seen = MI.hazardsRendered(chain)
  assert.ok(seen.some((f) => f.startsWith('definition [')), `the definition is not reported: ${seen}`)
  assert.ok(seen.some((f) => /link to "javascript:alert\(1\)"/.test(f)), `the live link is not reported: ${seen}`)
  assert.ok(hazards(chain).length >= 2)
  /* Every scheme: a definition is a finding, whatever it points at. */
  assert.ok(MI.hazardsRendered('[a]: https://ok.example\n').some((f) => f.startsWith('definition [')))
  /* And it reads rendered output, not the file's characters. */
  /* EXPORT-MD-ESCAPE-R4 (D-050): `[docs](https://ok.example/guide)` was in `inert` below. A recorded [text](url) is a link whose words are not its address (D-246). */
  for (const live of ['<img src=x onerror=alert(1)>', '<!-- x -->', '<?php x ?>', '[x](vbscript:x)', '![x](https://t.example/p.png)', '[x](https://ok.example "t")', '<script>alert(1)</script>', '[docs](https://ok.example/guide)', '//example.com', '<1@b.co>']) {
    assert.ok(MI.hazardsRendered(live).length > 0, `not flagged: ${live}`)
  }
  for (const inert of ['`<img src=x onerror=alert(1)>`', '`parse<T>(x: Vec<String>)`', '``a ` <b>``', 'https://ok.example/guide', '\\<img src=x onerror=alert(1)>', '&lt;img&gt;', '- **Could not be read now:** ok', '> quoted', '---']) {
    assert.deepEqual(MI.hazardsRendered(inert), [], `flagged: ${inert}`)
  }
  /* A code span holds what it holds; the same text with an unpaired backtick before it does not. */
  assert.deepEqual(MI.hazardsRendered('a `x`\n\n`<img src=x onerror=alert(1)>`\n'), [])
  assert.ok(MI.hazardsRendered('a ` x\n`<img src=x onerror=alert(1)>`\n').length > 0, 'a backtick left unpaired in one line steals the next line\'s code span, and the tag is live')
})

/* ── (1) the adversary's chain, module level ──────────────────────────── */

test('R2 (1)(2) the round-1 chain: an accepted line `[a`, a failing check that printed `x]: javascript:alert(1)`, and a folder that repeats the label — no definition and no link, full and with things left out', () => {
  const build = (extra = {}) => scenario({ c0: '[a', failure: 'x]: javascript:alert(1)', ...extra })
  const md0 = mdOf(build())
  /* The label the definition would have, with the file's own words in it: read
   * from the file as if nothing had been escaped. */
  const labels = labelsIn(unescapeForBase(md0))
  assert.equal(labels.length, 1, 'the fixture does not build the chain: no label would be formed even from the unescaped text')
  const use = `[${labels[0]}]`
  for (const redact of [[], CLASSES]) {
    for (const scopePath of [use, use.toLowerCase(), `${use}, src/`]) {
      const md = mdOf(build({ scopePath }), redact)
      /* Leaving out what a check printed removes it, which is the point of leaving it out. */
      if (!redact.length) assert.ok(md.includes('javascript:alert(1)'), 'what the check printed is gone from the file')
      assert.deepEqual(MI.definitionsIn(md), [], `a definition formed${redact.length ? ' (redacted)' : ''}: ${JSON.stringify(labelsIn(md))}`)
      assert.deepEqual(hazards(md), [], `live${redact.length ? ' (redacted)' : ''}`)
      assert.ok(!/<a href="javascript:/i.test(MI.commonmarkHtml(md)))
    }
  }
})

/* ── (1) every ordered pair and triple of slots ───────────────────────── */

/* A definition needs an opening `[`, a label, and a closing `]:`. Each fragment
 * below is one part of one, put in a slot of its own. The slot that holds the
 * opener and the slot that holds the closer are every ordered pair of slots the
 * file carries recorded text in; with a middle field, every ordered triple. */
const OPEN = ['[a', '[', '[a\\', '[a\\] b']
const CLOSE = ['x]: javascript:alert(1)', ']: javascript:alert(1)', 'x]: <javascript:alert(1)>', 'x]: data:text/html,x', 'x]: https://ok.example', 'x]:javascript:alert(1) "t"']

test('R2 (1) the refmap search: over every ordered pair of slots, an opener in one and a closer in the other forms no definition, of any scheme, and nothing is live in any of the three renderers (R3)', () => {
  const formed = []
  let renders = 0
  for (const a of SLOTS_R2) {
    for (const b of SLOTS_R2) {
      for (const o of OPEN) {
        for (const c of CLOSE) {
          const values = a === b ? { [a]: `${o} ${c}` } : { [a]: o, [b]: c }
          const md = mdOf(scenario(values))
          renders += 1
          /* EXPORT-MD-ESCAPE-R3: judged by all three renderers (their output, and CommonMark's reference map), not by the reference map alone. */
          const found = MI.hazardsRendered(md)
          if (found.length) formed.push(`${a} ← ${JSON.stringify(o)}, ${b} ← ${JSON.stringify(c)}: ${found[0]}`)
        }
      }
    }
  }
  assert.ok(renders >= SLOTS_R2.length * SLOTS_R2.length * OPEN.length * CLOSE.length)
  assert.ok(!formed.length, `${formed.length} of ${renders} files formed a definition:\n  ${first(formed)}`)
})

test('R2 (1) the refmap search: over every ordered triple of slots — an opener, a middle, a closer — no definition forms and nothing is live in any of the three renderers (R3), full and with things left out', () => {
  const formed = []
  let renders = 0
  for (const a of SLOTS_R2) {
    for (const b of SLOTS_R2) {
      for (const c of SLOTS_R2) {
        if (a === b || b === c || a === c) continue
        for (const [n, [o, m, k]] of [['[a', 'b', 'x]: javascript:alert(1)'], ['[', '\\', ']: javascript:alert(1)'], ['[a\\', 'b [', 'x]: data:text/html,x']].entries()) {
          for (const redact of n === 0 ? [[], CLASSES] : [[]]) {
            const md = mdOf(scenario({ [a]: o, [b]: m, [c]: k }), redact)
            renders += 1
            const found = MI.hazardsRendered(md)
            if (found.length) formed.push(`${a}, ${b}, ${c}${redact.length ? ' (redacted)' : ''}: ${found[0]}`)
          }
        }
      }
    }
  }
  assert.ok(renders >= SLOTS_R2.length * (SLOTS_R2.length - 1) * (SLOTS_R2.length - 2) * 4, `only ${renders} files were built`)
  assert.ok(!formed.length, `${formed.length} of ${renders} files formed a definition:\n  ${first(formed)}`)
})

test('R2 (1) the refmap search over every field the composer reads: an opener in one and a closer in another forms no definition', () => {
  const formed = []
  let renders = 0
  for (const [na, ma] of FIELDS) {
    for (const [nb, mb] of FIELDS) {
      if (na === nb) continue
      for (const [o, c] of [['[a', 'x]: javascript:alert(1)'], ['[', ']: <javascript:alert(1)>'], ['[a `', '` x]: javascript:alert(1)']]) {
        const s = scenario()
        const packet = clone(s.packet)
        const records = clone(s.records)
        try {
          ma(packet, records, o)
          mb(packet, records, c)
        } catch (err) {
          /* The one pair that cannot be built: a field that empties a section's
           * rows beside a field that needs a row of that same section. */
          if (/no rows/.test(na + nb)) continue
          throw err
        }
        const md = renderExport(packet, records, { format: 'md' }).text
        renders += 1
        const found = MI.definitionsIn(md)
        if (found.length) formed.push(`${na} ← ${JSON.stringify(o)}, ${nb} ← ${JSON.stringify(c)}: ${found[0].destination}`)
        const live = MI.hazardsRendered(md)
        if (live.length) formed.push(`${na}, ${nb}: ${live[0]}`)
      }
    }
  }
  assert.ok(renders > FIELDS.length * (FIELDS.length - 1), `only ${renders} files were built`)
  assert.ok(!formed.length, `${formed.length} of ${renders} files:\n  ${first(formed)}`)
})

/* ── (2) the rule is one, total, and lives in the helper ──────────────── */

test('R2 (2) no `]:` is left in the file: a definition, of any kind, needs one', () => {
  const e = PE.escapeMarkdown
  const risky = ['[x]: javascript:alert(1)', 'a]: b', ']:', ']::', 'x]:y', '[^1]: note', '*[HTML]: Hyper Text', 'a\\]: b', 'a\\\\]: b', '[a]:\n', 'error[E0308]: mismatched types', '[a](https://ok.example)]: x', '`a]: b`', 'a ` b]: c ` d']
  for (const text of risky) {
    const out = e(text)
    assert.ok(!/\]:/.test(out), `${JSON.stringify(text)} → ${JSON.stringify(out)} still holds "]:"`)
  }
  /* Whole files: not one `]:` anywhere, whichever slot holds one. */
  for (const slot of SLOTS_R2) {
    for (const redact of [[], CLASSES]) {
      const md = mdOf(scenario({ [slot]: 'a]: b [c]: d error[E0308]: e' }), redact)
      assert.ok(!/\]:/.test(md), `${slot}${redact.length ? ' (redacted)' : ''}: the file holds "]:"`)
    }
  }
  /* And it reads the same: the colon is a colon. */
  assert.equal(MI.inlineHtml(e('error[E0308]: mismatched types')), MI.inlineHtml('error[E0308]: mismatched types'))
})

test('R2 (2) text a person wrote that has no `]:` and no backtick is exactly what round 1 made of it, but for what round 4 escapes — and that is what a plain walk over the text, one character at a time, writes', () => {
  /* Round 1's helper, frozen here as the reference for "nothing else moves". */
  const LB = /[\r\n\v\f\u001c-\u001e\u0085\u2028\u2029]+/
  const one = (str) => {
    const parts = str.split(LB)
    if (parts.length === 1) return str
    const last = parts.length - 1
    return parts.map((p, i) => (i === 0 ? p.trimEnd() : i === last ? p.trimStart() : p.trim())).filter(Boolean).join(' ')
  }
  const LS = [/^#{1,6}(?=[ \t]|$)/, /^>/, /^[-+*](?=[ \t]|$)/, /^(?:[-*_][ \t]*){3,}$/, /^[-=]+[ \t]*$/, /^(?:`{3,}|~{3,})/, /^\[(?:[^\]\\]|\\.)*\]\s*:/, /^\[[ xX]\](?=[ \t]|$)/]
  const round1 = (value) => {
    let str = one(String(value)).replace(/^\s+/, '').replace(/<(?=[A-Za-z/!?])/g, '&lt;').replace(/!\[/g, '!\\[')
      .replace(/\]\((?!https?:\/\/(?:[\w.~:/?#@%+,;=-]|&(?!#|[A-Za-z][A-Za-z0-9]*;))+\))/gi, ']\\(')
    if (LS.some((re) => re.test(str))) str = '\\' + str
    else str = str.replace(/^(\d{1,9})([.)])(?=[ \t]|$)/, '$1\\$2')
    return str
  }
  /* EXPORT-MD-ESCAPE-R4 (D-050): round 4 escapes every `[`, `]` and `<`, the second slash of `//`, the `@` of a mail
   * address and the `.` of `www.`. `round4` is that, walked one character at a time over the text round 1's line-start
   * guards made of it — a second, plain statement of the same rule, not the helper's regular expression. */
  const KEPT = '!"#$%&\'()*+,-./:;=>?@[\\]^_`{|}~'
  const round4 = (value) => {
    let str = one(String(value)).replace(/^\s+/, '')
    if (LS.some((re) => re.test(str))) str = '\\' + str
    else str = str.replace(/^(\d{1,9})([.)])(?=[ \t]|$)/, '$1\\$2')
    let out = ''
    for (let i = 0; i < str.length; i += 1) {
      const c = str[i]
      if (c === '\\' && i + 1 < str.length && KEPT.includes(str[i + 1])) { out += c + str[i + 1]; i += 1 } // a person's own escape (not of `<`: it is written `&lt;`)
      else if (c === '[' || c === ']') out += '\\' + c
      else if (c === '<') out += '&lt;'
      else if (c === '/' && str[i + 1] === '/') { out += '/\\/'; i += 1 }
      else if (c === '@' && /[\w.!#$%&'*+/=?^{|}~-]/.test(str[i - 1] ?? '')) out += '\\@'
      else if (c === '.' && /^www$/i.test(str.slice(Math.max(0, i - 3), i))) out += '\\.'
      else if (c === ':' && str[i - 1] === ']') out += '\\:'
      else out += c
    }
    return out
  }
  const samples = [...Object.values(PAYLOADS), ...ORDINARY, ...ORDINARY_R4.map(([text]) => text), ...Object.values(BENIGN)]
  let compared = 0
  let sameAsRound1 = 0
  for (const text of samples) {
    if (text.includes('`')) continue
    /* The one other thing round 2 changes: a text that is only a table's delimiter row (see the table above). */
    if (/^[|: \t-]*-[|: \t-]*$/.test(one(text).replace(/^\s+/, ''))) { assert.ok(PE.escapeMarkdown(text).startsWith('\\'), JSON.stringify(text)); continue }
    compared += 1
    assert.equal(PE.escapeMarkdown(text), round4(text), JSON.stringify(text))
    /* A text with nothing round 4 escapes in it is exactly what round 1 made of it. */
    if (!/[[\]<@]|\/\/|www\./i.test(text)) { sameAsRound1 += 1; assert.equal(PE.escapeMarkdown(text), round1(text).replace(/\]:/g, ']\\:'), JSON.stringify(text)) }
  }
  assert.ok(compared > 100)
  assert.ok(sameAsRound1 > 30, `only ${sameAsRound1} texts were checked against round 1`)
  /* And the same over random text from an alphabet with no backtick. */
  let seed = 20260930
  const rnd = () => { seed = (seed * 1664525 + 1013904223) >>> 0; return seed / 4294967296 }
  const alphabet = ['<', '>', '[', ']', '(', ')', '!', '\\', '&', '#', ';', ':', '/', 'a', 'B', '1', '.', ' ', ' ', '\t', '\n', '-', '*', '_', '|', '"', '~', '=', ']:', '](', '![', 'https://ok.example/', 'javascript:', '<b>', '&lt;', ' ', '  ']
  for (let i = 0; i < 4000; i += 1) {
    let text = ''
    for (let j = 0, n = 1 + Math.floor(rnd() * 14); j < n; j += 1) text += alphabet[Math.floor(rnd() * alphabet.length)]
    if (/^[|: \t-]*-[|: \t-]*$/.test(one(text).replace(/^\s+/, ''))) continue
    assert.equal(PE.escapeMarkdown(text), round4(text), JSON.stringify(text))
    if (!/[[\]<@]|\/\/|www\./i.test(text)) assert.equal(PE.escapeMarkdown(text), round1(text).replace(/\]:/g, ']\\:'), JSON.stringify(text))
  }
})

test('R2 (2) a redaction marker is put in place of a value before the text is escaped, so its `]:` cannot close a label either — and no renderer reads a definition or a link out of it (R3: all three; and the B.4 run name)', () => {
  /* The marker is `[left out: a file or folder path]`. A recorded backslash in front of the value
   * escapes the marker's `[`, and what follows the value supplies the colon: `\[left out: …]: x`
   * closes a label some other field opened, in the copy with paths left out. */
  const PATH_SLOTS = ['scopePath', 'movedPath', 'decisionPath', 'transcriptFile']
  const bad = []
  for (const pathSlot of PATH_SLOTS) {
    for (const opener of SLOTS_R2.filter((x) => x !== pathSlot)) {
      for (const [o, tail] of [['[a', ': javascript:alert(1)'], ['[', ': <javascript:alert(1)>'], ['[a\\', ': data:text/html,x']]) {
        const md = mdOf(scenario({ [opener]: o, [pathSlot]: '\\' + BENIGN[pathSlot] + tail }), ['paths'])
        const found = MI.hazardsRendered(md)
        if (found.length) bad.push(`${opener} ← ${JSON.stringify(o)}, ${pathSlot} redacted: ${found[0]}`)
        if (/\]:/.test(md)) bad.push(`${opener}, ${pathSlot}: the file holds "]:"`)
      }
    }
  }
  /* B.4 (packet-export.test.js): a run's name carries its folder, lower-cased, and the copy that leaves paths out marks it. */
  for (const opener of SLOTS_R2.filter((x) => x !== 'runName')) {
    for (const redact of [['paths'], CLASSES]) {
      const found = MI.hazardsRendered(mdOf(scenario({ [opener]: '[a', runName: 'bash:/users/synthetic/shop:npm test' }), redact))
      if (found.length) bad.push(`B.4 ${opener} (${redact.length} class${redact.length === 1 ? '' : 'es'} left out): ${found[0]}`)
    }
  }
  assert.ok(!bad.length, `${bad.length}:\n  ${first(bad)}`)
})

/* ── (3) code spans: every one is written as escaped text ─────────────── */

/* EXPORT-MD-ESCAPE-R3 (D-050). This section held round 2's requirement "code
 * spans read the same": a recorded code span was written exactly as recorded when
 * it was a "plain" one, and the tests asserted the file's line was the recorded
 * text, byte for byte. That rule is what the round-2 adversary broke (`marked`
 * reads a link label's code span only at the top level, and masks a backslash and
 * the punctuation after it before it masks a span). It is replaced by the
 * founder-level ruling of D-226: no recorded text is ever emitted as a live
 * Markdown construct, so every recorded backtick is escaped and every code span
 * is written as escaped TEXT — visible characters the same in every renderer,
 * backticks included; the code FONT is not kept. Every expected value below that
 * changed by that ruling says so. */

/* Text a person recorded with code spans in it. */
const CODE_SPANS = [
  '`parse<T>(x: Vec<String>)`',
  '`handlers[i](event)`',
  'The function `parse<T>(x: Vec<String>)` returns `Option<T>`',
  'Call `handlers[i](event)` for each row and read src/[id]/page.tsx',
  'Run `cargo test --package cart` and see https://example.com/a?b=1&c=2#frag',
  '`npm test` passes and `npm run build` too',
  '`a && b || c` is what the guard says',
  '`ls | grep foo` lists the files',
  '`<b>` is a tag and `</b>` closes it',
  '`[x](javascript:alert(1))` was the link text',
  '`<img src=x onerror=alert(1)>` is what the page held',
  'a `x` then <b>bold</b> then `y`',
  '`\\` is a backslash and `\\n` is two characters',
  '`a\\`',
  '`&lt;` stays as typed and `&amp;` too',
]

/* Recorded texts whose every character outside a code span is a plain word, and
 * whose every code span holds characters a renderer reads as markup: for each,
 * the characters a reader sees are the characters that were recorded, in every
 * renderer. */
const VISIBLE = [
  '`parse<T>(x: Vec<String>)`', '`handlers[i](event)`', 'The function `parse<T>(x: Vec<String>)` returns `Option<T>`',
  'Run `cargo test --package cart` first', '`a * b * c` and `x_y_z` and `__init__`', '`~~x~~` and `a ~ b`', '`wow!` and `![x](y)`',
  '`[a](https://ok.example)` is a link when it is text', '`[x]: javascript:alert(1)` and `error[E0308]: mismatched types`',
  '`&lt;` and `&#106;` and `&amp;` and `R&D` and `a &b; c`', '`<b>x</b>` and `<!-- c -->` and `<?php ?>` and `a < b`',
  '`\\` and `\\\\` and `a\\b` and `C:\\dir\\file` and `\\*` and `\\[`', '`` a ` b `` and ``` a `` b ``` and `` ` ``',
  '`https://x.example/a_b_c` and `www.x.example/a_b` and `ftp://x` and `a@b.example`', '`# not a heading` and `> not a quote` and `- not an item` and `1. not a list`',
  '`|a|b|` and `-|-` and `:-:`', '`{{ x }}` and `{a}` and `$x$` and `%s` and `"q"` and `\'q\'`', '`` ` `` and `` `` `` and ```` ``` ````',
  'a `x` b `y` c `z` d', '`a``b`', '` a `', '`  `', 'Tom & Jerry `&` R&D',
]

/** The characters a reader sees of a text put in a paragraph, in one renderer. */
function seen(renderer, text) {
  return MI.textOf(renderer.render(markdownOf([{ t: 'p', text }]))).replace(/\n+$/, '')
}

test('R3 (3) a recorded code span is written as escaped text: no backtick is left live, and no renderer draws a code element', () => {
  for (const text of [...CODE_SPANS, ...VISIBLE]) {
    const out = PE.escapeMarkdown(text)
    assert.deepEqual(MI.quirkHazards(out), [], `a live backtick: ${JSON.stringify(text)} → ${JSON.stringify(out)}`)
    assert.deepEqual(MI.hazardsRendered(out), [], `live: ${JSON.stringify(text)}`)
    for (const r of MI.RENDERERS) assert.ok(!/<code/.test(r.render(out)), `${r.name}: a code element from ${JSON.stringify(text)} → ${JSON.stringify(out)}`)
  }
})

test('R3 (3) the visible characters of every renderer are the characters that were recorded, backticks included', () => {
  const bad = []
  for (const text of VISIBLE) {
    const want = text.trim()
    for (const r of MI.RENDERERS) {
      const got = seen(r, text).trim()
      if (got !== want) bad.push(`${r.name}: ${JSON.stringify(text)} shows ${JSON.stringify(got)}`)
    }
  }
  assert.ok(!bad.length, `${bad.length} texts:\n  ${first(bad)}`)
})

test('R3 (3) the two the round-1 adversary named show exactly the recorded characters: `parse<T>(x: Vec<String>)` no literal `&lt;`, `handlers[i](event)` no literal backslash', () => {
  const vec = '`parse<T>(x: Vec<String>)`'
  const handlers = '`handlers[i](event)`'
  /* Written as text (D-050: round 2 held `<code>parse&lt;T&gt;…</code>`, a code element; there is none now). */
  const vecW = '\\`parse&lt;T>\\(x: Vec&lt;String>\\)\\`'
  const handlersW = '\\`handlers\\[i\\]\\(event\\)\\`'
  assert.equal(PE.escapeMarkdown(vec), vecW)
  assert.equal(PE.escapeMarkdown(handlers), handlersW)
  for (const r of MI.RENDERERS) {
    assert.equal(seen(r, vec), vec, r.name)
    assert.equal(seen(r, handlers), handlers, r.name)
  }
  for (const [slot, text, want] of [
    ['criterion', vecW, '- ' + vecW + ' — '], ['criterion', handlersW, '- ' + handlersW + ' — '],
    ['said', vecW, 'Said: ' + vecW], ['said', handlersW, 'Said: ' + handlersW],
    ['failure', vecW, 'What the check said when it broke: ' + vecW], ['failure', handlersW, 'What the check said when it broke: ' + handlersW],
    ['rationale', vecW, ': ' + vecW], ['scopePath', handlersW, 'Covers: ' + handlersW],
  ]) {
    const recorded = text === vecW ? vec : handlers
    for (const redact of [[], ['agent_words'], ['paths'], ['failure_output']]) {
      /* A class that is left out removes the slot that holds it, which is the point of leaving it out. */
      if ((redact[0] === 'paths' && slot === 'scopePath') || (redact[0] === 'failure_output' && slot === 'failure') || (redact[0] === 'agent_words' && slot === 'said')) continue
      const md = mdOf(scenario({ [slot]: recorded }), redact)
      const line = md.split('\n').find((l) => l.includes(want))
      assert.ok(line, `${slot} ← ${recorded}${redact.length ? ` (${redact[0]} left out)` : ''}: the line is not in the file as written`)
      /* hazards() includes the raw-file check that no backtick is live (MI.quirkHazards). */
      assert.deepEqual(hazards(md), [], `${slot}: live`)
    }
  }
})

test('R3 (3) what changes about a backtick, and what is inside a code span: every backtick is escaped; a span is written as text with every character a renderer reads as markup escaped', () => {
  const e = PE.escapeMarkdown
  /* Each `[recorded, written]`. Where the row changed from round 2's table (D-050), the old value is in the comment above the group. */
  const rows = [
    /* a run of backticks that closes nothing in its own text reads as the backticks it is (unchanged since round 2) */
    ['it`s', 'it\\`s'], ['`', '\\`'], ['``', '\\`\\`'], ['a ` b', 'a \\` b'],
    ['`a``', '\\`a\\`\\`'], ['\\`x`', '\\`x\\`'],
    /* D-050: round 2 left a paired span live: `a` `b → `a` \`b ; \\`x` → \\`x` */
    ['`a` `b', '\\`a\\` \\`b'], ['\\\\`x`', '\\\\\\`x\\`'],
    /* D-050: round 2 wrote a span whose content held `]:` as text with `[` and `]` bare: \`error[E0308]\: mismatched\` */
    ['`error[E0308]: mismatched`', '\\`error\\[E0308\\]\\: mismatched\\`'], ['see `a]: b` and `c`', 'see \\`a\\]\\: b\\` and \\`c\\`'],
    /* a span that is not one backtick around text with none was text already: unchanged */
    ['``a``', '\\`\\`a\\`\\`'], ['`a`` b`', '\\`a\\`\\` b\\`'], ['``a ` <b>``', '\\`\\`a \\` &lt;b>\\`\\`'], ['a ```b``` c', 'a \\`\\`\\`b\\`\\`\\` c'],
    /* a bare web address that runs into a span: unchanged where round 2 wrote the span as text; D-050 where it left a span live */
    /* R4 (D-050): every address's `//` has its second slash escaped, in a span and outside one, so no `://` is left (each was `https://x.example…`, `HTTPS://x.example…`, `ftp://x…`; and the `.` of `www.` is escaped outside a span too) */
    ['https://x.example`<b>`', 'https:/\\/x.example\\`&lt;b>\\`'], ['HTTPS://x.example/a?b`c`', 'HTTPS:/\\/x.example/a?b\\`c\\`'], ['ftp://x`a b`c`<b>`', 'ftp:/\\/x\\`a b\\`c\\`&lt;b>\\`'],
    ['see www.x.example`y` and `<b>`', 'see www\\.x.example\\`y\\` and \\`&lt;b>\\`'],
    ['https://x.example `<b>`', 'https:/\\/x.example \\`&lt;b>\\`'], ['`https://x.example` and `<b>`', '\\`https:/\\/x.example\\` and \\`&lt;b>\\`'],
    /* a pipe changes nothing: a span with one was live or text depending on its content in round 2; it is text now */
    ['`a | <b>`', '\\`a | &lt;b>\\`'], ['`<b>` | x', '\\`&lt;b>\\` | x'], ['x | `Vec<u8>`', 'x | \\`Vec&lt;u8>\\`'],
    ['`a | [x](javascript:alert(1))`', '\\`a | \\[x\\]\\(javascript:alert\\(1\\)\\)\\`'], ['`a | ![x](y)`', '\\`a | \\!\\[x\\]\\(y\\)\\`'],
    ['`a | b` and `<b>`', '\\`a | b\\` and \\`&lt;b>\\`'], ['`ls | grep x`', '\\`ls | grep x\\`'], ['`a || b`', '\\`a || b\\`'],
    ['`string | number` and `x[i]`', '\\`string | number\\` and \\`x\\[i\\]\\`'],
    /* D-050: round 2 left a span alone whatever else it held: `<img src=x onerror=alert(1)>` → itself; `[a](javascript:x)` → itself */
    ['`<img src=x onerror=alert(1)>`', '\\`&lt;img src=x onerror=alert\\(1\\)>\\`'], ['`[a](javascript:x)`', '\\`\\[a\\]\\(javascript:x\\)\\`'],
    /* what is outside a span is escaped as it always was; the span is text (D-050 for the backticks) */
    ['<b>`x`</b>', '&lt;b>\\`x\\`&lt;/b>'], ['[a](javascript:x) `y`', '\\[a\\](javascript:x) \\`y\\`'],
    /* a text that opens with a fence is not one: its backticks are escaped (unchanged) */
    ['```js', '\\`\\`\\`js'], ['```a<b>```', '\\`\\`\\`a&lt;b>\\`\\`\\`'],
    /* new in round 3: what is inside a span is written so that it shows itself */
    ['`a * b * c`', '\\`a \\* b \\* c\\`'], ['`snake_case`', '\\`snake\\_case\\`'], ['`~~x~~`', '\\`\\~\\~x\\~\\~\\`'], ['`wow!`', '\\`wow\\!\\`'],
    ['`\\`', '\\`\\\\\\`'], ['`C:\\dir\\file`', '\\`C:\\\\dir\\\\file\\`'], ['`a\\`', '\\`a\\\\\\`'],
    ['`&lt;` and `&#106;` and `R&D`', '\\`\\&lt;\\` and \\`\\&#106;\\` and \\`R&D\\`'],
    ['`https://x.example/a_b`', '\\`https:/\\/x.example/a\\_b\\`'], ['`www.x.example`', '\\`www\\.x.example\\`'], ['`ftp://x`', '\\`ftp:/\\/x\\`'], ['`a@b.example`', '\\`a\\@b.example\\`'],
    ['`[x]: y`', '\\`\\[x\\]\\: y\\`'], ['`` a ` b ``', '\\`\\` a \\` b \\`\\`'],
    /* R4 (D-050): every `<` is written `&lt;`, in a span too (was `\\`a < b\\` and \\`<3\\` and \\`x <= 5\\``) */
    ['`a < b` and `<3` and `x <= 5`', '\\`a &lt; b\\` and \\`&lt;3\\` and \\`x &lt;= 5\\`'],
  ]
  for (const [input, want] of rows) assert.equal(e(input), want, JSON.stringify(input))
})

test('R2 (3) a backtick one field leaves unpaired cannot take the code span of the next: every field is balanced on its own', () => {
  /* Two fields on the lines of one paragraph. If the first leaves a backtick
   * that pairs with the second's, the second's span content is outside the
   * span and its tag is live. Each pair of slots, each of the leftovers. */
  const leftovers = ['a `', '`', 'a `` b', 'x ``` y', '\\``', 'a `b` `']
  const spans = ['`<img src=x onerror=alert(1)>`', '``<script>alert(1)</script>``', '`[x](javascript:alert(1))`', '`a | b`']
  const bad = []
  for (const a of SLOTS_R2) {
    for (const b of SLOTS_R2) {
      if (a === b) continue
      for (const l of leftovers) {
        for (const c of spans) {
          const md = mdOf(scenario({ [a]: l, [b]: c }))
          const live = hazards(md)
          if (live.length) bad.push(`${a} ← ${JSON.stringify(l)}, ${b} ← ${JSON.stringify(c)}: ${live[0]}`)
        }
      }
    }
  }
  assert.ok(!bad.length, `${bad.length} pairs:\n  ${first(bad)}`)
})

test('R2 (3) a code span survives a table: the span with a pipe cannot be cut in two into markup', () => {
  /* A header row, a delimiter row and a body row are three lines; the fields
   * that are whole lines are the paragraphs of the file. The rendered
   * instrument treats a table as a change of structure and markup as live. */
  for (const slot of ['title', 'criterion', 'said', 'failure', 'rationale', 'scopePath']) {
    for (const payload of ['`a | <img src=x onerror=alert(1)>`', '`x | [y](javascript:alert(1))`', '|`a|<b>`|', '`a|<script>alert(1)</script>`']) {
      const md = mdOf(scenario({ [slot]: payload }))
      assert.deepEqual(hazards(md), [], `${slot} ← ${payload}`)
    }
  }
})

/* ── (5) round 1 holds: the new shapes go through every round-1 matrix ── */

Object.assign(PAYLOADS, {
  span_html: '`<img src=x onerror=alert(1)>`',
  span_script_double: '``<script>alert(1)</script>``',
  span_then_html: '`x` <img src=x onerror=alert(1)>',
  span_link_break: '[a `b](javascript:alert(1))` c',
  span_pipe_html: '`a | <img src=x onerror=alert(1)>`',
  span_pipe_link: '`a | [x](javascript:alert(1))`',
  span_def_close: '`x]: javascript:alert(1)`',
  unmatched_then_html: 'a ` b <img src=x onerror=alert(1)>',
  unmatched_double_then_html: 'a `` b <img src=x onerror=alert(1)>',
  lone_backtick: '`',
  lone_double: '``',
  lone_fence: '```',
  html_between_backticks: '` <img src=x onerror=alert(1)> `',
  escaped_backtick_then_span: '\\`<img src=x onerror=alert(1)>`',
  span_in_link_label: '[`a](javascript:alert(1))`](https://ok.example)',
  label_open: '[a',
  label_open_backslash: '[a\\',
  label_open_escaped_bracket: '[a\\] b',
  label_close: 'x]: javascript:alert(1)',
  label_close_bare: ']: javascript:alert(1)',
  label_with_backtick: '[a `b]: javascript:alert(1)',
  label_multi_line: '[a\nb]: javascript:alert(1)',
  label_footnote_open: '[^1',
  label_footnote_def: '[^1]: javascript:alert(1)',
  label_abbrev: '*[HTML]: javascript:alert(1)',
  address_into_span: 'https://x.example`<img src=x onerror=alert(1)>` tail',
  address_www_into_span: 'www.x.example`<a href="javascript:alert(1)">x</a>`',
  address_before_lone: 'https://x.example`',
  address_then_space_span: 'https://x.example `<img src=x onerror=alert(1)>` tail',
  emphasis_over_fence: '#_`<a href="javascript:alert(1)">``_`#',
  emphasis_over_span: '*a `<img src=x onerror=alert(1)>` b*',
  table_row_span: 'a | b\n-|-\n`x | <img src=x onerror=alert(1)>` | z',
  table_row_neighbour: '`p | q` `<img src=x onerror=alert(1)>` | z',
  table_delimiter: '-|-',
  table_delimiter_colons: ':-|-:',
  table_delimiter_pipes: '|---|',
  span_double_html: '``<img src=x onerror=alert(1)>``',
  span_inner_backtick: '``a ` <img src=x onerror=alert(1)>``',
  label_double_colon: 'x]::javascript:alert(1)',
  label_no_space: 'x]:javascript:alert(1)',
  label_angle_dest: 'x]: <javascript:alert(1)> "t"',
  label_title_dest: 'x]: https://ok.example "<img src=x onerror=alert(1)>"',
  label_then_use: '[a]: javascript:alert(1)\n[a]',
  label_collapsed: '[a][]',
  label_full: '[b][a]',
})

test('R2 (5) the round-2 shapes are in every round-1 matrix: same payload table, every slot, every field, both forms of the file', () => {
  for (const name of ['span_html', 'span_pipe_html', 'label_open', 'label_close', 'lone_fence', 'label_multi_line']) assert.ok(name in PAYLOADS, name)
  /* Idempotence and the words that must survive are asserted over PAYLOADS by the round-1 tests above and below. */
  for (const [name, payload] of Object.entries(PAYLOADS)) assert.equal(PE.escapeMarkdown(PE.escapeMarkdown(payload)), PE.escapeMarkdown(payload), `${name}: not idempotent`)
})

test('R2 (5) every payload in every slot, c0 included, judged on what the renderers produce from the whole file: nothing live, no definition, the same structure', () => {
  const bad = []
  for (const redact of [[], CLASSES]) {
    const want = shape(mdOf(scenario(), redact))
    for (const slot of SLOTS_R2) {
      for (const [name, payload] of Object.entries(PAYLOADS)) {
        const md = mdOf(scenario({ [slot]: payload + ' ' + (BENIGN[slot] ?? C0) }), redact)
        const live = MI.hazardsRendered(md)
        if (live.length) bad.push(`${slot} ← ${name}${redact.length ? ' (redacted)' : ''}: ${live[0]}`)
        else if (JSON.stringify(shape(md)) !== JSON.stringify(want)) bad.push(`${slot} ← ${name}: the file's structure changed`)
      }
    }
  }
  assert.ok(!bad.length, `${bad.length} slot × shape pairs:\n  ${first(bad)}`)
})

test('R2 (5) a seeded fuzz: two slots each, a token alphabet of every markup character, judged on rendered output', () => {
  let seed = 20260930
  const rnd = () => { seed = (seed * 1664525 + 1013904223) >>> 0; return seed / 4294967296 }
  const toks = ['<', '>', '[', ']', '(', ')', '!', '\\', '`', '``', '```', '&', '#', ';', ':', ']:', '](', '![', '/', 'javascript', 'script', 'img', 'src=x', 'onerror=alert(1)', 'https://a.example/', 'data:', ' ', ' ', '\t', '\n', ',', '.', '-', '*', '_', '@', '{', '}', '|', '"', "'", '&#106;', '&lt;', '<a href=', '<!--', '-->', '<?', '=', '~~~', '1.', '+', '[^', '[a', 'x]', ']: javascript:alert(1)', '`<img src=x onerror=alert(1)>`']
  const make = () => { let t = ''; for (let j = 0, n = 2 + Math.floor(rnd() * 12); j < n; j += 1) t += toks[Math.floor(rnd() * toks.length)]; return t }
  const bad = []
  for (let i = 0; i < 6000; i += 1) {
    const a = SLOTS_R2[Math.floor(rnd() * SLOTS_R2.length)]
    const b = SLOTS_R2[Math.floor(rnd() * SLOTS_R2.length)]
    const c = SLOTS_R2[Math.floor(rnd() * SLOTS_R2.length)]
    const values = { [a]: make(), [b]: make(), [c]: make() }
    const md = mdOf(scenario(values), rnd() < 0.3 ? CLASSES : [])
    const live = MI.hazardsRendered(md)
    if (live.length) bad.push(`${JSON.stringify(values)}: ${live[0]}`)
  }
  assert.ok(!bad.length, `${bad.length} of 6000:\n  ${first(bad)}`)
})

/* ── (3) what other renderers do with a code span that CommonMark does not ─ */

/* Each of these is raw text, as a person might record it, that a renderer other
 * than the CommonMark reference reads with a tag or a link loose. Round 2 found
 * the first six by running raw text through `marked` 18 and `markdown-it` 15 in a
 * scratch check, and MODELLED them, because neither was installed here; the
 * round-2 adversary found the last two, which the model did not know. Round 3
 * runs the real renderers, vendored (EXPORT-MD-ESCAPE-R3), so every shape is
 * checked live in the renderer that reads it, every run. */
const OTHER_RENDERERS = {
  address_runs_into_span: { raw: 'https://x.example`<img src=x onerror=alert(1)>` tail', by: ['marked', 'markdown-it'] },
  emphasis_pairs_through_a_long_fence: { raw: '#_`<a href="javascript:alert(1)">``_`#', by: ['marked'] },
  table_splits_a_span: { raw: 'a | b\n-|-\n`x | <img src=x onerror=alert(1)>` | z', by: ['marked', 'markdown-it'] },
  table_pairs_a_neighbour: { raw: 'a | b\n-|-\n`p | q` `<img src=x onerror=alert(1)>` | z', by: ['marked', 'markdown-it'] },
  table_from_a_line_with_no_pipe: { raw: 'Header line\n|---|\nbody', by: ['marked'] },
  table_from_a_line_with_a_pipe: { raw: 'a | b\n-|-\nbody | x', by: ['marked', 'markdown-it'] },
  /* The two round-2 breaks (D-226; each was safe at 981f86c1 and live at 13222310). */
  link_label_reads_a_span_only_at_its_top_level: { raw: '[[a `]x](javascript:alert(1))`', by: ['marked'] },
  emphasis_pairs_through_a_span_that_ends_in_a_backslash: { raw: '_a `<img src=x onerror=alert(1)>_\\`', by: ['marked'] },
}

test('R2 (3) the instrument names what marked and markdown-it do with a code span, and the shapes are live in them (EXPORT-MD-ESCAPE-R3: both are always run)', (t) => {
  for (const [name, { raw }] of Object.entries(OTHER_RENDERERS)) {
    assert.ok(MI.quirkHazards(raw).length > 0, `${name}: the instrument does not name it`)
    assert.deepEqual(MI.liveIn(MI.commonmarkHtml(raw)), [], `${name}: CommonMark itself reads it as a code span, which is why a reference implementation alone would not see it`)
  }
  /* A `]:` inside a span is a definition CommonMark itself forms: the reference map sees it. */
  const rawDefinition = '[a\n`x]: javascript:alert(1)`'
  assert.equal(MI.definitionsIn(rawDefinition).length, 1)
  assert.ok(MI.quirkHazards(rawDefinition).length > 0)
  const ran = []
  for (const [name, { raw, by }] of Object.entries(OTHER_RENDERERS)) {
    for (const b of by) {
      const r = MI.RENDERERS.find((x) => x.name.startsWith(b))
      assert.ok(r, `${b} is not one of the renderers that run`)
      ran.push(`${r.name}: ${name}`)
      assert.ok(MI.liveIn(r.render(raw)).length > 0, `${r.name} does not read ${name} with anything loose, so the instrument names something that is not there`)
    }
  }
  t.diagnostic(`live in the vendored renderers: ${ran.length} shapes`)
  /* And none of them survives the helper, in any slot. */
  for (const [name, { raw }] of Object.entries(OTHER_RENDERERS)) {
    for (const slot of SLOTS_R2) {
      const md = mdOf(scenario({ [slot]: raw }))
      assert.deepEqual(hazards(md), [], `${slot} ← ${name}`)
    }
  }
})

test('R2 (3) every bare address in front of a code span, in every slot: no line has a span a renderer could read differently', () => {
  const fronts = ['https://x.example', 'HTTP://x.example/a?b=1', 'ftp://x', 'www.x.example', 'see https://x.example/a(b)', 'x@y.example https://z.example', '(https://x.example)', '<https://x.example>', 'https://', 'www.']
  const backs = ['`<img src=x onerror=alert(1)>`', '`x`', '`x` `<b>`', '`a b` `<b>`']
  const bad = []
  for (const slot of SLOTS_R2) {
    for (const f of fronts) {
      for (const b of backs) {
        for (const join of ['', ')', '.', '-']) {
          const md = mdOf(scenario({ [slot]: f + join + b + ' tail' }))
          const live = hazards(md)
          if (live.length) bad.push(`${slot} ← ${JSON.stringify(f + join + b)}: ${live[0]}`)
        }
      }
    }
  }
  assert.ok(!bad.length, `${bad.length}:\n  ${first(bad)}`)
})

test('R3 (3) no backtick in the file is live — every one is escaped, so no renderer has a code span to find: whatever the slots hold', () => {
  const bad = []
  for (const [name, payload] of Object.entries(PAYLOADS)) {
    for (const slot of SLOTS_R2) {
      const md = mdOf(scenario({ [slot]: payload }))
      const quirk = MI.quirkHazards(md)
      if (quirk.length) bad.push(`${slot} ← ${name}: ${quirk[0]}`)
    }
  }
  assert.ok(!bad.length, `${bad.length}:\n  ${first(bad)}`)
})

test('R2 (5) random text made of markup characters: escaping twice is escaping once, no `]:` survives, and no backtick is live (R3)', () => {
  let seed = 5
  const rnd = () => { seed = (seed * 1664525 + 1013904223) >>> 0; return seed / 4294967296 }
  const toks = ['`', '``', '```', '[', ']', ']:', '](', '![', '\\', '|', '<', '<b>', '>', ' ', ' ', '\t', 'a', 'b', ':', '(', ')', 'https://x', 'www.y', '&lt;', '#', '-', '*', '_', '~', '1.', '<img src=x onerror=1>', '\\`', '\\\\']
  const bad = []
  for (let i = 0; i < 30000; i += 1) {
    let text = ''
    for (let j = 0, n = 1 + Math.floor(rnd() * 14); j < n; j += 1) text += toks[Math.floor(rnd() * toks.length)]
    const once = PE.escapeMarkdown(text)
    if (PE.escapeMarkdown(once) !== once) bad.push(`${JSON.stringify(text)}: not idempotent`)
    else if (/\]:/.test(once)) bad.push(`${JSON.stringify(text)}: "]:" left`)
    else if (MI.quirkHazards(once).length) bad.push(`${JSON.stringify(text)}: ${MI.quirkHazards(once)[0]}`)
  }
  assert.ok(!bad.length, `${bad.length} of 30000:\n  ${first(bad)}`)
})

/* ── a long text of backticks and brackets costs a moment, not a hang ─── */

test('R2 (6) a long run of backticks, unpaired runs of every length, brackets and `]:` in a recorded text is handled in linear time', () => {
  const N = 300000
  let stairs = ''
  for (let k = 1; stairs.length < N; k += 1) stairs += '`'.repeat(k) + ' '
  const cases = {
    backticks: '`'.repeat(N), spaced: '` '.repeat(N / 2), stairs, pairs: '`a` '.repeat(N / 4), pipes_in_spans: '`a|b` '.repeat(N / 6),
    unclosed: '`a '.repeat(N / 3), colons: ']:'.repeat(N / 2), brackets: '[a'.repeat(N / 2), escapes: '\\`'.repeat(N / 2), mixed: '[`]:'.repeat(N / 4),
  }
  for (const [name, text] of Object.entries(cases)) {
    const start = performance.now()
    const out = PE.escapeMarkdown(text)
    const ms = performance.now() - start
    assert.ok(ms < 1500, `${name}: ${Math.round(ms)} ms for ${text.length} characters`)
    assert.ok(!/\]:/.test(out), name)
  }
})

/* ══════════════════════════════════════════════════════════════════════════
 * EXPORT-MD-ESCAPE-R3
 *
 * The round-2 adversary's finding, in one paragraph. Round 2 wrote a code span
 * as recorded when it was a "plain" one, and the two reasons that made a span
 * plain (a lazy regexp `marked` uses for emphasis, a bare address) were a MODEL
 * of `marked`, because `marked` was not installed here to be run. Two shapes the
 * model did not know were live in it:
 *
 *     [[a `]x](javascript:alert(1))`          a live link: `marked` reads a link
 *                                             label's code span only at the top
 *                                             level, so the "span" is not one
 *     _a `<img src=x onerror="…">_\`          a live tag, and a script that RAN:
 *                                             `marked` masks a backslash and the
 *                                             punctuation after it before it
 *                                             masks a span
 *
 * The ruling (D-226) is the rule that cannot depend on a renderer's inline
 * parser: no recorded text is ever written as a live Markdown construct — every
 * recorded backtick is escaped, and every code span is written as escaped TEXT.
 * The requirements these sections hold, from the card:
 *  (1) red first: both breaks, the minimal case and the variants, in all three
 *      renderers, full and redacted; and a bounded exhaustive enumeration through
 *      the real renderers;
 *  (2) the renderers are real: `marked` and `markdown-it` are vendored beside
 *      CommonMark, pinned, licensed, and unreachable from the app;
 *  (3) every code span is written as escaped text, and the characters a reader
 *      sees are the characters that were recorded, backticks included;
 *  (4) every round-1 and round-2 break stays fixed in all three real renderers.
 * ══════════════════════════════════════════════════════════════════════════ */

const sha256 = (buf) => createHash('sha256').update(buf).digest('hex')

/* ── (2) the renderers are real, pinned, licensed, and cannot reach the app ── */

test('R3 (2) marked and markdown-it are vendored beside CommonMark: pinned by digest, with their licences and notes, all three run every time', (t) => {
  t.diagnostic(MI.describeRenderers())
  assert.deepEqual(MI.RENDERERS.map((r) => r.name), ['commonmark 0.31.2', 'marked 18.0.14', 'markdown-it 15.0.2'])
  assert.deepEqual(Object.values(MI.VENDORED).map((v) => `${v.name} ${v.version}`), ['commonmark 0.31.2', 'marked 18.0.14', 'markdown-it 15.0.2'])
  for (const v of Object.values(MI.VENDORED)) {
    assert.equal(sha256(readFileSync(v.file)), v.sha256, `${v.name}: the vendored file is not the copy the module pins`)
    assert.equal(sha256(readFileSync(v.license)), v.license_sha256, `${v.name}: the LICENSE is not the copy the module pins`)
    const note = readFileSync(v.note, 'utf8')
    for (const must of [v.sha256, v.license_sha256, v.version, 'registry.npmjs.org', 'sha512-']) assert.ok(note.includes(must), `${v.name}: VENDORED.md does not record ${must.slice(0, 20)}`)
  }
  /* What each renderer says of itself agrees with the version the module names. */
  assert.ok(readFileSync(MI.VENDORED.marked.file, 'utf8').includes('marked v18.0.14 - a markdown parser'))
  assert.equal(new (createRequire(import.meta.url)('../../test-support/vendor/markdown-it/markdown-it.umd.min.cjs'))().render('a'), '<p>a</p>\n')
})

test('R3 (2) nothing under ui/test-support reaches the app: no source file imports it, and no built asset carries a line of the vendored renderers', () => {
  const walk = (dir, out = []) => {
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      const p = new URL(e.name + (e.isDirectory() ? '/' : ''), dir)
      if (e.isDirectory()) walk(p, out)
      else out.push(p)
    }
    return out
  }
  const src = walk(new URL('../', import.meta.url)).filter((u) => /\.(?:jsx?|mjs|cjs|css|html)$/.test(u.pathname) && !/\.test\.js$/.test(u.pathname))
  assert.ok(src.length > 50, 'the source walk found almost nothing')
  const IMPORTS = /(?:\bfrom\s*|\bimport\s*\(?\s*|\brequire\s*\(\s*)['"][^'"]*(?:test-support|\/vendor\/(?:commonmark|marked|markdown-it))/
  const importing = src.filter((u) => IMPORTS.test(readFileSync(u, 'utf8')))
  assert.deepEqual(importing.map((u) => u.pathname.split('/ui/')[1]), [], 'a source file names test-support or a vendored renderer')
  const dist = walk(new URL('../../dist/', import.meta.url)).filter((u) => /\.(?:js|css|html|map)$/.test(u.pathname))
  assert.ok(dist.length > 5, 'the built assets were not found')
  for (const v of Object.values(MI.VENDORED)) {
    const body = readFileSync(v.file, 'utf8')
    for (const at of [0.2, 0.5, 0.8]) {
      const piece = body.slice(Math.floor(body.length * at), Math.floor(body.length * at) + 80)
      for (const u of dist) assert.ok(!readFileSync(u, 'utf8').includes(piece), `${v.name}: a line of it is in ${u.pathname.split('/dist/')[1]}`)
    }
  }
  const config = readFileSync(new URL('../../vite.config.js', import.meta.url), 'utf8')
  assert.ok(!/test-support|vendor/.test(config), 'the build configuration names a vendored renderer')
})

/* ── (1) the two breaks, every variant, all three renderers ───────────── */

/* Each is raw text, as a person might record it, that the real `marked` reads
 * with a scriptable link or a tag loose (each was safe at 981f86c1 and is live
 * in a file written at 13222310). */
const BREAK_LINKS = {
  adversary_link_in_a_label: '[[a `]x](javascript:alert(1))`',
  minimal: '[[`]]()`',
  minimal_javascript: '[[`]](javascript:alert(1))`',
  minimal_data: '[[`]](data:text/html,x)`',
  minimal_bare_target: '[[`]](x)`',
  minimal_space_before_target: '[[`]]( javascript:alert(1))`',
  label_with_text_first: '[[a `]](javascript:alert(1))`',
  emphasis_link_javascript: '_a `[x](javascript:alert(1))_\\`',
  emphasis_link_data: '_a `[x](data:text/html,x)_\\`',
  emphasis_empty_link: '*`[](javascript:alert(1))*\\`',
  emphasis_anchor_tag: '_a `<a href="javascript:alert(1)">x</a>_\\`',
}
const BREAK_TAGS = {
  adversary_img_onerror: '_a `<img src=x onerror="document.title=\'PWNED-R2\'">_\\`',
  img_onerror: '_a `<img src=x onerror=alert(1)>_\\`',
  script: '_a `<script>alert(1)</script>_\\`',
  svg_onload: '_a `<svg onload=alert(1)>_\\`',
  star_emphasis: '*a `<img src=x onerror=alert(1)>*\\`',
  strong_emphasis: '__a `<img src=x onerror=alert(1)>__\\`',
}
const BREAKS = { ...BREAK_LINKS, ...BREAK_TAGS }

/* The adversary's two, as the file lines they were written at 13222310, and as they are written now. */
const ADVERSARY = [
  /* R4 (D-050): was `[[a \\`\\]x\\]…`: the two opening brackets are escaped too */
  ['[[a `]x](javascript:alert(1))`', '\\[\\[a \\`\\]x\\]\\(javascript:alert\\(1\\)\\)\\`'],
  ['_a `<img src=x onerror="document.title=\'PWNED-R2\'">_\\`', '_a \\`&lt;img src=x onerror="document.title=\'PWNED-R2\'">\\_\\\\\\`'],
]

test('R3 (1) the instrument is live: every variant of the two breaks, raw, is read by the real marked as a scriptable link or a tag with a handler', () => {
  for (const [name, raw] of Object.entries(BREAKS)) {
    const marked = MI.hazardsRendered(raw).filter((f) => f.startsWith('marked'))
    assert.ok(marked.length > 0, `${name}: marked does not read ${JSON.stringify(raw)} as anything live, so a test built on it proves nothing`)
    assert.ok(name in BREAK_LINKS ? marked.some((f) => /link to "(?:javascript|data|x|)/.test(f)) : marked.some((f) => /tag <(?:img|script|svg|a)/.test(f)), `${name}: ${marked[0]}`)
  }
})

test('R3 (1) the two round-2 breaks and every variant, in every slot the file carries recorded text in, full and with things left out: no live link, no link that is not http(s), no raw tag — in any of the three renderers', () => {
  const bad = []
  let files = 0
  for (const [name, raw] of Object.entries(BREAKS)) {
    for (const slot of SLOTS_R2) {
      for (const redact of [[], CLASSES]) {
        const md = mdOf(scenario({ [slot]: raw }), redact)
        files += 1
        const live = hazards(md)
        if (live.length) bad.push(`${slot} ← ${name}${redact.length ? ' (redacted)' : ''}: ${live[0]}`)
        for (const r of MI.RENDERERS) {
          const html = r.render(md)
          if (/href="(?!https?:|mailto:)/i.test(html)) bad.push(`${slot} ← ${name}: ${r.name} has an href that is not http(s)`)
          if (/<(?:img|script|svg|iframe)\b/i.test(html)) bad.push(`${slot} ← ${name}: ${r.name} has a ${/<(img|script|svg|iframe)\b/i.exec(html)[1]} element`)
        }
      }
    }
  }
  assert.equal(files, Object.keys(BREAKS).length * SLOTS_R2.length * 2)
  assert.ok(!bad.length, `${bad.length} findings in ${files} files:\n  ${first(bad)}`)
})

test('R3 (1) the adversary\'s two, as the lines of the file: the accepted line is written as escaped text, in the full file and in the copy with things left out', () => {
  for (const [recorded, written] of ADVERSARY) {
    assert.equal(PE.escapeMarkdown(recorded), written)
    for (const redact of [[], CLASSES]) {
      const md = mdOf(scenario({ c0: recorded }), redact)
      assert.ok(md.split('\n').some((line) => line.startsWith('- ' + written)), `${JSON.stringify(recorded)}: the line is not in the file as written${redact.length ? ' (redacted)' : ''}`)
      assert.deepEqual(hazards(md), [])
      for (const r of MI.RENDERERS) assert.ok(!/<img|<a href="javascript/i.test(r.render(md)), `${r.name}: a live tag or link`)
    }
  }
})

/* ── (1) a bounded exhaustive enumeration, through the real renderers ─── */

/* The card's alphabet: [ ] ( ) ` \ < > * _ ! : a j, and the two tokens "javascript:" and "x". */
const CARD_ALPHABET = ['[', ']', '(', ')', '`', '\\', '<', '>', '*', '_', '!', ':', 'a', 'j', 'javascript:', 'x']
const sum = (base, max) => { let n = 0; for (let k = 1, p = 1; k <= max; k += 1) { p *= base; n += p } return n }

/* Length is counted in symbols, "javascript:" being one. N = 5 over the card's
 * alphabet (1 118 480 strings) is what keeps the whole enumeration well under 60 s
 * with the strings judged by four workers (about 15 s in all; about 45 s of CPU
 * on one), so that is N for the card's alphabet. The two narrower alphabets go to
 * length 8, which is where the round-2 breaks first appear (each is 8 symbols).
 * At 13222310 each of them holds exactly two strings that `marked` reads as
 * something live — `[[`]]()`` and `[[`]](`)` in the link alphabet (a link to "" and
 * to "%60"), `_`_<x>\`` and `_`<x>_\`` in the span alphabet (a tag `<x>`) — and none
 * is live at this commit. The other two are the card's alphabet as the label of an
 * https link (where `marked` un-escapes what it finds, before reading anything
 * else) and several fields of one row at once (what one field opens another can
 * close). */
const ENUMERATIONS = [
  { name: 'the card\'s alphabet (16 symbols), every string up to length 5, as a list row', symbols: CARD_ALPHABET, max: 5, expect: sum(16, 5) },
  { name: 'the link alphabet ([ ] ( ) `), every string up to length 8 — `[[`]]()`` (a live link in marked at 13222310) is 8 symbols', symbols: ['[', ']', '(', ')', '`'], max: 8, expect: sum(5, 8) },
  { name: 'the span alphabet (_ ` < > \\ x), every string up to length 8 — `_`<x>_\\`` (a live tag in marked at 13222310) is 8 symbols', symbols: ['_', '`', '<', '>', '\\', 'x'], max: 8, expect: sum(6, 8) },
  { name: 'the card\'s alphabet, every string up to length 4, as the label of a link the person recorded with an https target', symbols: CARD_ALPHABET, max: 4, context: 'https_label', expect: sum(16, 4) },
  { name: 'the fields of one row (its text, a line under it, its labelled gap line, more lines) over [ ] ( ) ` \\ x :, every string up to length 5', symbols: ['[', ']', '(', ')', '`', '\\', 'x', ':', FIELD_END], max: 5, context: 'fields', expect: sum(9, 5) },
]

for (const e of ENUMERATIONS) {
  test(`R3 (1) exhaustive enumeration through markdownOf and all three renderers: ${e.name}`, { timeout: 300000 }, async (t) => {
    const started = performance.now()
    const r = await enumerate({ symbols: e.symbols, max: e.max, context: e.context })
    t.diagnostic(`${r.total} strings, ${r.workers} workers, ${Math.round(performance.now() - started)} ms`)
    assert.equal(r.total, e.expect, 'the enumeration is not the size it says it is')
    assert.equal(r.judged, r.total, 'a string was not judged')
    assert.equal(r.found, 0, `${r.found} of ${r.total} strings are live in a renderer:\n  ${first(r.samples.map((x) => `${JSON.stringify(x.input)} → ${x.finding}`))}`)
  })
}

/* ── (3) the visible characters, over random code-span texts ──────────── */

test('R3 (3) a seeded property over random code-span texts: every renderer shows the characters that were recorded, backticks included, and draws no code element', () => {
  let seed = 20260930
  const rnd = () => { seed = (seed * 1664525 + 1013904223) >>> 0; return seed / 4294967296 }
  const inner = ['[', ']', '(', ')', '*', '_', '!', '~', '\\', '<', '>', '&', '#', ';', ':', '|', 'a', 'b', ' ', '.', '/', '-', '@', 'https://x.example/', 'www.x.example', '<b>', '&lt;', '&#106;', '``', '```', '`', '{', '}', '"', "'", '=', '+', '^', '$', '%', ',', '?']
  const words = ['read', 'the', 'file', 'then', 'run', 'ok', 'a', 'x']
  const span = () => {
    const n = 1 + Math.floor(rnd() * 3)
    let c = ''
    for (let j = 0, k = Math.floor(rnd() * 8); j < k; j += 1) c += inner[Math.floor(rnd() * inner.length)]
    c = c.replace(/`+/g, (run) => (run.length === n ? '`'.repeat(n + 1) : run))
    if (c.startsWith('`')) c = ' ' + c
    if (c.endsWith('`')) c += ' '
    return '`'.repeat(n) + c + '`'.repeat(n)
  }
  /* Texts whose code spans CommonMark reads as they were built, with no backslash in front of a backtick (a backslash and a backtick outside a span is an escape, and the person's backslash is then not a character the reader sees). */
  const make = () => {
    const spans = []
    let t = ''
    for (let p = 0, parts = 1 + Math.floor(rnd() * 3); p < parts; p += 1) {
      const sp = span()
      spans.push(sp)
      t += (p ? ' ' : '') + words[Math.floor(rnd() * words.length)] + ' ' + sp
    }
    const read = MI.scanLine(t)
    return read.unpaired.length || read.spans.length !== spans.length || /\\`/.test(t) ? null : t
  }
  const bad = []
  let checked = 0
  for (let i = 0; i < 60000 && bad.length < 10; i += 1) {
    const text = make()
    if (text === null) continue
    const md = markdownOf([{ t: 'p', text }])
    for (const r of MI.RENDERERS) {
      const html = r.render(md)
      checked += 1
      const got = MI.textOf(html).replace(/\n+$/, '')
      if (got !== text) bad.push(`${r.name}: ${JSON.stringify(text)} shows ${JSON.stringify(got)}`)
      else if (/<code/.test(html)) bad.push(`${r.name}: ${JSON.stringify(text)} draws a code element`)
      /* R4 (D-050): a link of any kind was let through here (`link to …`); recorded text yields none that is not a bare http(s) address, and a span holds none. */
      else if (MI.liveIn(html).length) bad.push(`${r.name}: ${JSON.stringify(text)}: ${MI.liveIn(html)[0]}`)
    }
  }
  assert.ok(!bad.length, `${bad.length}:\n  ${first(bad)}`)
  assert.ok(checked > 60000, `only ${checked} renderings were checked`)
})

test('R3 (3) every backtick a text carries is written escaped, and escaping the output again changes nothing', () => {
  let seed = 9
  const rnd = () => { seed = (seed * 1664525 + 1013904223) >>> 0; return seed / 4294967296 }
  const toks = ['`', '``', '```', '````', '[', ']', '(', ')', '\\', '\\\\', '\\`', '<', '<b>', '>', ' ', 'a', ':', '*', '_', '!', '~', '&lt;', '&', 'https://x.example', 'www.x', '|', '#']
  const bad = []
  for (let i = 0; i < 30000; i += 1) {
    let text = ''
    for (let j = 0, n = 1 + Math.floor(rnd() * 14); j < n; j += 1) text += toks[Math.floor(rnd() * toks.length)]
    const once = PE.escapeMarkdown(text)
    if (MI.quirkHazards(once).some((f) => /backtick|code span/.test(f))) bad.push(`${JSON.stringify(text)} → ${JSON.stringify(once)}: a live backtick`)
    else if (PE.escapeMarkdown(once) !== once) bad.push(`${JSON.stringify(text)}: not idempotent`)
    else if (/\]:/.test(once)) bad.push(`${JSON.stringify(text)}: "]:" left`)
    else if ((text.match(/`/g) || []).length !== (once.match(/`/g) || []).length) bad.push(`${JSON.stringify(text)} → ${JSON.stringify(once)}: a backtick was dropped or added`)
  }
  assert.ok(!bad.length, `${bad.length} of 30000:\n  ${first(bad)}`)
})

/* ── (4) every round-1 and round-2 break stays fixed in all three ─────── */

/* The round-1 chain, the refmap pairs, triples and fields, the marker chain and B.4 are the round-2 tests above: they now judge what all three real renderers produce, so every round-1 and round-2 break stays fixed in commonmark, marked and markdown-it (D-050 for none of their expected values: each asserts the same, of more). */

test('R3 (4) the HTML and JSON exports have not moved by a byte since the base the golden records in its `base` field, for text full of backticks, spans and both round-2 breaks', () => {
  const all = Object.fromEntries(SLOTS.map((s) => [s, PAYLOADS.mixed_everything]))
  const s = scenario(all)
  for (const [format, redact, key] of [['html', [], 'html'], ['html', CLASSES, 'html_redacted'], ['json', [], 'json'], ['json', CLASSES, 'json_redacted']]) {
    assert.equal(renderExport(s.packet, s.records, { format, redact }).text, GOLDEN.hostile[key], `${key}: bytes changed`)
  }
  /* And with every slot holding a code span or a break, the two formats still carry the recorded characters as recorded. */
  const spans = Object.fromEntries(SLOTS.map((slot) => [slot, '`parse<T>(x: Vec<String>)` [[a `]x](javascript:alert(1))`']))
  const t = scenario(spans)
  const json = renderExport(t.packet, t.records, { format: 'json' }).text
  assert.ok(json.includes('`parse<T>(x: Vec<String>)`'), 'the JSON carries the recorded characters as recorded')
  const html = renderExport(t.packet, t.records, { format: 'html' }).text
  assert.ok(html.includes('`parse&lt;T&gt;(x: Vec&lt;String&gt;)`'), 'the HTML carries them escaped, with the backticks as backticks')
})

/* ══════════════════════════════════════════════════════════════════════════
 * EXPORT-MD-ESCAPE-R4
 *
 * The round-3 adversary (run 2, at 4f6a84fa: gates/adversary-EXPORT-MD-ESCAPE-R3/
 * TRANSCRIPT-4f6a84fa-run2.md) found three shapes that predate round 3 and sit
 * inside round 3's own claim. Every committed alphabet had been made of markup
 * characters and a word or two; none had a URL token, which is why these were
 * missed.
 *
 *   1.  //example.com            markdown-it (linkify on) makes <a href="//example.com">,
 *                                which is file://example.com/ in a local file
 *   2.  [![a\](https://…/p.png)](https://…/x)
 *                                marked reads `\[` and `\]` inside a link label and lexes
 *                                the label again: <a><img src="https://…/p.png"></a>, a
 *                                remote image fetched on open (a tracking beacon)
 *   3.  <1@b.co>                 the `<` guard stopped at [A-Za-z/!?], so a digit, `_` `.` `-`
 *                                `#` `$` `%` `&` `'` `*` `+` `=` `{` `|` `}` `~` before the `@`
 *                                is an angle autolink — `mailto:` — in all three renderers
 *
 * The ruling (D-246) is the invariant, for all three real renderers with the
 * instrument's options (linkify on): recorded text yields NO `<img>`, NO raw tag
 * or handler, NO reference definition, and NO `<a>` except one whose address is an
 * absolute http(s) URL and whose words are that address (a viewer's own
 * autolinking of a bare address). Recorded `[text](url)` is never a live link:
 * every `[` and `]` is escaped, whatever follows. Every `<` is escaped. The
 * visible characters equal the recorded characters (R3 Done-when 3).
 * ══════════════════════════════════════════════════════════════════════════ */

/* The adversary's three, verbatim, and each one's variants. */
const R4_ADDRESS = {
  adversary_bare_host: '//example.com',
  adversary_bare_host_in_a_sentence: 'see //evil.example/x.png',
  host_with_path_query_fragment: 'x //h.co/a?b=c#d',
  host_in_parentheses: '(//h.co)',
  host_in_quotes: '"//h.co"',
  host_after_a_comma: 'a,//h.co',
  host_with_userinfo: '//user:pass@h.co',
  host_as_an_address: '//127.0.0.1:1/p.png',
  host_local: '//localhost/x',
  three_slashes: '///h.co',
  four_slashes: '////h.co',
  after_a_colon: ':////h.co',
  after_https_and_a_third_slash: 'https:///h.co',
  after_https_and_a_fourth_slash: 'https:////h.co',
  after_a_backslash: '\\//h.co',
  after_an_escaped_backslash: '\\\\//h.co',
  two_hosts: '//h.co //k.co',
  at_the_end: 'go to //h.co',
  ftp_address: 'ftp://h.co/x',
  ftp_upper: 'FTP://h.co',
  ftp_three_slashes: 'ftp:///h.co',
  mailto_address: 'mailto:a@b.co',
  email_address: 'a@b.co',
  email_after_a_span: '`x`@b.co',
  www_address: 'www.h.co/a[b',
  https_address_with_a_bracket: 'https://h.co/a[b]',
  https_address_with_ipv6: 'http://[::1]:3000/x',
  scheme_less_link_target: '[a](//h.co)',
  scheme_relative_image: '![a](//h.co/p.png)',
}
const R4_IMAGE = {
  adversary_image_in_a_label: '[![a\\](https://127.0.0.1:1/p.png)](https://127.0.0.1:1/x)',
  another_host: '[![a\\](https://evil.example/p.png)](https://ok.example/)',
  http_scheme: '[![a\\](http://127.0.0.1:1/p.png)](http://127.0.0.1:1/x)',
  empty_alt: '[![\\](https://127.0.0.1:1/p.png)](https://127.0.0.1:1/x)',
  long_alt: '[![a b c\\](https://127.0.0.1:1/p.png)](https://127.0.0.1:1/x)',
  no_outer_target: '[![a\\](https://127.0.0.1:1/p.png)]',
  outer_target_relative: '[![a\\](https://127.0.0.1:1/p.png)](x)',
  outer_target_javascript: '[![a\\](https://127.0.0.1:1/p.png)](javascript:alert(1))',
  outer_target_empty: '[![a\\](https://127.0.0.1:1/p.png)]()',
  double_backslash: '[![a\\\\](https://127.0.0.1:1/p.png)](https://127.0.0.1:1/x)',
  nested_labels: '[[![a\\]](https://127.0.0.1:1/p.png)]](https://127.0.0.1:1/x)',
  image_inside_a_label: '[a ![b](https://127.0.0.1:1/p.png) c](https://127.0.0.1:1/x)',
  bare_image: '![a](https://127.0.0.1:1/p.png)',
  escaped_bang_image: '\\![a](https://127.0.0.1:1/p.png)',
  image_then_a_span: '[![a\\](https://127.0.0.1:1/p.png)](https://127.0.0.1:1/x) `y`',
  plain_link: '[docs](https://ok.example/guide)',
  plain_link_http: '[docs](http://ok.example/a?b=1&c=2)',
  link_with_a_title: '[docs](https://ok.example/guide "title")',
  link_with_an_angle_target: '[docs](<https://ok.example/guide>)',
  link_with_an_empty_label: '[](https://ok.example/)',
  link_whose_words_are_an_address: '[https://good.example/](https://evil.example/)',
  angle_https_autolink: '<https://ok.example/>',
  address_then_bracket_then_target: 'https://x.example/a[b https://y.example/c](https://evil.example/)',
}
const R4_EMAIL = {
  adversary_digit: '<1@b.co>',
  adversary_in_a_sentence: 'pay <1@evil.example?subject=x&body=y> now',
  ...Object.fromEntries([...'0123456789_.-#$%&\'*+={|}~^!/?'].map((c) => [`leading_${c.codePointAt(0).toString(16)}`, `<${c}@b.co>`])),
  letter_led: '<x@b.co>',
  angle_mailto: '<mailto:a@b.co>',
  angle_javascript: '<javascript:alert(1)>',
  angle_with_a_backslash: '<a\\\\@b.co>',
  angle_email_in_a_span: '`<1@b.co>`',
  two_in_a_row: '<1@b.co><2@c.co>',
  /* The stopped first adversary's BREAK-mailto files (a host with no dot is a mail autolink too), and the mixed Change of the run-2 transcript. */
  single_label_host: '<1@b>',
  single_label_host_symbol_first: '<-a@b>',
  break_mailto_sentence: 'see <1@b> too',
  break_mailto_with_spans: 'run `<1@b>` and `<!@b.co>` before shipping',
  break_mailto_dash: 'the total shows `<-a@b>` on screen',
  transcript_mixed_change_four: 'a`b<img src=x onerror=1>``[x]: javascript:1 //example.com www.e.com/&#106;avascript:1',
  transcript_title_change_four: '//evil.example/a.png',
}
const R4_ATTACKS = { ...R4_ADDRESS, ...R4_IMAGE, ...R4_EMAIL }

/* What each of the adversary's three is, raw, in the renderer that reads it: the instrument sees them, so a green result on the helper's output means something. */
const R4_LIVE_RAW = [
  ['//example.com', ['markdown-it'], /link to "\/\/example\.com"/],
  ['see //evil.example/x.png', ['markdown-it'], /link to "\/\/evil\.example\/x\.png"/],
  ['[![a\\](https://127.0.0.1:1/p.png)](https://127.0.0.1:1/x)', ['marked'], /tag <img>/],
  ['<1@b.co>', ['commonmark', 'marked', 'markdown-it'], /link to "mailto:1@b\.co"/],
  ['pay <1@evil.example?subject=x&body=y> now', ['marked', 'markdown-it'], /link to "mailto:1@evil\.example/],
  ['ftp://h.co/x', ['marked', 'markdown-it'], /link to "ftp:\/\/h\.co\/x"/],
  ['a@b.co', ['marked', 'markdown-it'], /link to "mailto:a@b\.co"/],
  ['[docs](https://ok.example/guide)', ['commonmark', 'marked', 'markdown-it'], /a link whose words are not its address: "docs"/],
  ['[https://good.example/](https://evil.example/)', ['commonmark', 'marked', 'markdown-it'], /a link whose words are not its address/],
]

test('R4 (1) the instrument is live: the adversary\'s three are read by the real renderers as a link to a bare host, a link that holds an image, and a mail link — and a recorded [text](url) is a link whose words are not its address', () => {
  for (const [raw, by, finding] of R4_LIVE_RAW) {
    for (const name of by) {
      const r = MI.RENDERERS.find((x) => x.name.startsWith(name))
      assert.ok(r, `${name} is not one of the renderers that run`)
      const found = MI.liveIn(r.render('- ' + raw + '\n'))
      assert.ok(found.some((f) => finding.test(f)), `${r.name} does not read ${JSON.stringify(raw)} as ${finding}: ${JSON.stringify(found)}`)
    }
  }
  /* A bare address that a renderer makes a link of, with the address as its words, is the one link that is let through. */
  for (const html of ['<a href="https://ok.example/x?a=1">https://ok.example/x?a=1</a>', '<a href="http://www.h.co">www.h.co</a>', '<a href="https://ok.example/a%20b">https://ok.example/a b</a>']) assert.deepEqual(MI.liveIn(html), [], html)
  for (const html of ['<a href="mailto:a@b.co">a@b.co</a>', '<a href="ftp://h.co/">ftp://h.co/</a>', '<a href="//h.co">//h.co</a>', '<a href="https://ok.example/">docs</a>', '<a>x</a>', '<a href="https://a.example/x"><img src="https://a.example/p.png" alt="a"></a>']) assert.ok(MI.liveIn(html).length > 0, html)
  /* And the raw-file reader: a bracket is live unless an odd number of backslashes stands in front of it, and every `<` is. */
  assert.deepEqual(MI.unescapedMarkup('- a \\[b\\] &lt;c \\\\\\[d\\] \\\\\\\\\\['), [])
  assert.equal(MI.unescapedMarkup('- [a] \\\\[b] <c').length, 5)
})

test('R4 (1) every variant of the three, in every slot the file carries recorded text in, full and with things left out: no image, no raw tag, no definition, no link that is not a bare http(s) address, no live bracket — in any of the three renderers', () => {
  const bad = []
  let files = 0
  for (const redact of [[], CLASSES]) {
    const want = JSON.stringify(shape(BENIGN_MD[redact.length ? 'redacted' : 'full']))
    for (const [name, raw] of Object.entries(R4_ATTACKS)) {
      for (const slot of SLOTS_R2) {
        const md = mdOf(scenario({ [slot]: raw }), redact)
        files += 1
        const live = [...hazards(md), ...MI.unescapedMarkup(md)]
        if (live.length) bad.push(`${slot} ← ${name}${redact.length ? ' (redacted)' : ''}: ${live[0]}`)
        else if (JSON.stringify(shape(md)) !== want) bad.push(`${slot} ← ${name}: the file's structure changed`)
      }
    }
  }
  assert.equal(files, Object.keys(R4_ATTACKS).length * SLOTS_R2.length * 2)
  assert.ok(!bad.length, `${bad.length} of ${files} files:\n  ${first(bad)}`)
})

test('R4 (1) the adversary\'s three, as the lines of the file: exactly how each is written now', () => {
  for (const [recorded, written] of [
    ['//example.com', '/\\/example.com'],
    ['see //evil.example/x.png', 'see /\\/evil.example/x.png'],
    ['[![a\\](https://127.0.0.1:1/p.png)](https://127.0.0.1:1/x)', '\\[!\\[a\\](https:/\\/127.0.0.1:1/p.png)\\](https:/\\/127.0.0.1:1/x)'],
    ['<1@b.co>', '&lt;1\\@b.co>'],
  ]) {
    assert.equal(PE.escapeMarkdown(recorded), written)
    for (const redact of [[], CLASSES]) {
      const md = mdOf(scenario({ criterion: recorded }), redact)
      assert.ok(md.split('\n').some((line) => line.startsWith('- ' + written)), `${JSON.stringify(recorded)}: the line is not in the file as written${redact.length ? ' (redacted)' : ''}`)
    }
    /* Each shows the characters that were recorded — but for the backslash the person typed in front of `]`, which is theirs and which Markdown reads as an escape. */
    for (const r of MI.RENDERERS) assert.equal(seen(r, recorded), recorded.replace('a\\]', 'a]'), r.name)
  }
})

test('R4 (1) across two fields: an opener in one slot and a closer in another — the image, the link and the address that need two texts — form nothing live, in any ordered pair of slots', () => {
  const OPEN = ['[![a\\', '[a ![b\\']
  const CLOSE = ['](https://127.0.0.1:1/p.png)](https://127.0.0.1:1/x)', '](https://ok.example/)']
  const bad = []
  let files = 0
  for (const a of SLOTS_R2) {
    for (const b of SLOTS_R2) {
      for (const o of OPEN) {
        for (const c of CLOSE) {
          const md = mdOf(scenario(a === b ? { [a]: `${o} ${c}` } : { [a]: o, [b]: c }))
          files += 1
          const live = [...hazards(md), ...MI.unescapedMarkup(md)]
          if (live.length) bad.push(`${a} ← ${JSON.stringify(o)}, ${b} ← ${JSON.stringify(c)}: ${live[0]}`)
        }
      }
    }
  }
  assert.equal(files, SLOTS_R2.length * SLOTS_R2.length * OPEN.length * CLOSE.length)
  assert.ok(!bad.length, `${bad.length} of ${files} files:\n  ${first(bad)}`)
})

/* ── (1) bounded exhaustive enumerations with a URL-capable alphabet ──── */

/* Round 3's alphabets had no URL token. This one is the card's: [ ] ( ) ! \ < > / . : @ h s x, the two tokens "https:" and "//", and a space (an address ends at one). */
const R4_URL_ALPHABET = ['[', ']', '(', ')', '!', '\\', '<', '>', '/', '.', ':', '@', 'h', 's', 'x', 'https:', '//', ' ']
/* Backticks, `www`, `ftp:`, `mailto:` and `&` beside the address atoms. */
const R4_WIDE_ALPHABET = ['[', ']', '(', ')', '!', '\\', '`', '<', '/', '//', ':', '@', '.', 'x', 'https:', 'ftp:', 'www', 'mailto:', ' ']
const R4_FIELD_ALPHABET = ['[', ']', '(', ')', '!', '\\', '<', '/', '//', '.', ':', '@', 'x', 'https:', ' ', FIELD_END]

/* N = 5 for the URL alphabet: 2 000 718 strings, judged by four workers through
 * markdownOf and the three real renderers (about 8 s here; about 30 s of CPU on
 * one). N = 5 for the wide alphabet (2 613 659) and for the fields (1 118 480);
 * N = 4 where the alphabet is wrapped in a link the person recorded (111 150).
 * At 4f6a84fa the URL alphabet alone finds 460 strings that are live. */
const R4_ENUMERATIONS = [
  { name: 'the URL alphabet (18 symbols: [ ] ( ) ! \\ < > / . : @ h s x, "https:", "//" and a space), N = 5, as a list row', symbols: R4_URL_ALPHABET, max: 5, expect: sum(18, 5) },
  { name: 'the URL alphabet, N = 4, as the label of a link the person recorded with an https target', symbols: R4_URL_ALPHABET, max: 4, context: 'https_label', expect: sum(18, 4) },
  { name: 'a wide alphabet (19 symbols: brackets, a backtick, "www", "ftp:", "mailto:", "//", "https:", "@" and a space), N = 5, as a list row', symbols: R4_WIDE_ALPHABET, max: 5, expect: sum(19, 5) },
  { name: 'the fields of one row (its text, a line under it, its labelled gap line, more lines) over the URL atoms, N = 5', symbols: R4_FIELD_ALPHABET, max: 5, context: 'fields', expect: sum(16, 5) },
]

for (const e of R4_ENUMERATIONS) {
  test(`R4 (1) exhaustive enumeration through markdownOf and all three renderers: ${e.name}`, { timeout: 300000 }, async (t) => {
    const started = performance.now()
    const r = await enumerate({ symbols: e.symbols, max: e.max, context: e.context })
    t.diagnostic(`${r.total} strings, ${r.workers} workers, ${Math.round(performance.now() - started)} ms`)
    assert.equal(r.total, e.expect, 'the enumeration is not the size it says it is')
    assert.equal(r.judged, r.total, 'a string was not judged')
    assert.equal(r.found, 0, `${r.found} of ${r.total} strings are live in a renderer:\n  ${first(r.samples.map((x) => `${JSON.stringify(x.input)} → ${x.finding}`))}`)
  })
}

/* ── (4) the visible characters, over addresses, brackets and angle brackets ─ */

/* Recorded texts with no backslash, no entity and no emphasis mark, where the
 * only thing that could change what a reader sees is a renderer making a link,
 * an image or a tag of it. The characters a reader sees are the characters that
 * were recorded, in every renderer. */
const R4_VISIBLE = [
  '<1@b.co>', 'pay <1@evil.example?subject=x> now', '[docs](https://ok.example/guide) explain it', '[a](//h.co) and //h.co and ![b](//h.co/p.png)',
  'see //evil.example/x.png and ftp://h.co/x and mailto:a@b.co and a@b.co and www.h.co/a[b', 'https://h.co/a[b] and http://[::1]:3000/x and https://h.co//x',
  '[![a](https://127.0.0.1:1/p.png)](https://127.0.0.1:1/x)', 'a < b and x <= 5 and <3 and <b>bold</b> and <!-- c --> and <?php ?>', 'arr[0], f(x), [1, 2], [INFO] started',
  '<https://ok.example/> and <javascript:alert(1)> and <mailto:a@b.co>', 'git@github.com:x/y.git and user@host', '`https://x.example/a` and `<1@b.co>` and `[a](//h.co)`',
]

test('R4 (4) the visible characters of every renderer are the characters that were recorded, for addresses, links, images, brackets and angle brackets', () => {
  const bad = []
  for (const text of R4_VISIBLE) {
    for (const r of MI.RENDERERS) {
      const got = seen(r, text).trim()
      if (got !== text.trim()) bad.push(`${r.name}: ${JSON.stringify(text)} shows ${JSON.stringify(got)}`)
    }
  }
  assert.ok(!bad.length, `${bad.length} texts:\n  ${first(bad)}`)
})

test('R4 (4) a seeded property: random texts of words, brackets, angle brackets, addresses and code spans show, in every renderer, exactly the characters that were recorded, and nothing in them is live', () => {
  let seed = 20261004
  const rnd = () => { seed = (seed * 1664525 + 1013904223) >>> 0; return seed / 4294967296 }
  const toks = ['read', 'the', 'file', 'x', 'h.co', 'a', 'ok', '[', ']', '(', ')', '!', '<', '>', '/', '//', ':', '@', '.', ',', ';', '?', 'https://', 'http://', 'ftp://', 'www.', 'mailto:', '1', ' ', ' ', ' ', '`', '``', '"', "'", '=', '%', '+', '{', '}', '^', '$', '](', '![', '<1@', 'b.co>', '](//', '](https://h.co/']
  const bad = []
  let checked = 0
  for (let i = 0; i < 40000 && bad.length < 10; i += 1) {
    let text = ''
    for (let j = 0, n = 1 + Math.floor(rnd() * 12); j < n; j += 1) text += toks[Math.floor(rnd() * toks.length)]
    text = text.trim()
    if (!text) continue
    /* A text whose backticks CommonMark reads as a span that does not close cleanly shows those backticks too (R3), but a leading list or heading marker is the helper's own guard: compare what a reader sees. */
    const md = markdownOf([{ t: 'p', text }])
    for (const r of MI.RENDERERS) {
      const html = r.render(md)
      checked += 1
      const got = MI.textOf(html).replace(/\n+$/, '').trim()
      if (got !== text) bad.push(`${r.name}: ${JSON.stringify(text)} shows ${JSON.stringify(got)}`)
      else if (MI.liveIn(html).length) bad.push(`${r.name}: ${JSON.stringify(text)}: ${MI.liveIn(html)[0]}`)
    }
    const definitions = MI.definitionsIn(md)
    if (definitions.length) bad.push(`a definition formed from ${JSON.stringify(text)}`)
    if (MI.unescapedMarkup(md).length) bad.push(`${JSON.stringify(text)}: ${MI.unescapedMarkup(md)[0]}`)
  }
  assert.ok(!bad.length, `${bad.length}:\n  ${first(bad)}`)
  assert.ok(checked > 100000, `only ${checked} renderings were checked`)
})

/* ── (2) the helper, property: nothing it writes is a live bracket or `<`, and it is idempotent ─ */

test('R4 (2) a seeded property over the URL atoms: every `[`, `]` and `<` the helper writes is escaped, no `]:` is left, no backtick is live, and a second pass changes nothing', () => {
  let seed = 8
  const rnd = () => { seed = (seed * 1664525 + 1013904223) >>> 0; return seed / 4294967296 }
  const toks = [...R4_WIDE_ALPHABET, 'https://', 'http://', '](', '![', ']:', '<1@b.co>', '\\[', '\\]', '\\\\', '\\<', '&lt;', 'a b', '\t']
  const bad = []
  for (let i = 0; i < 30000; i += 1) {
    let text = ''
    for (let j = 0, n = 1 + Math.floor(rnd() * 14); j < n; j += 1) text += toks[Math.floor(rnd() * toks.length)]
    const once = PE.escapeMarkdown(text)
    const loose = MI.unescapedMarkup(once)
    if (loose.length) bad.push(`${JSON.stringify(text)} → ${JSON.stringify(once)}: ${loose[0]}`)
    else if (PE.escapeMarkdown(once) !== once) bad.push(`${JSON.stringify(text)}: not idempotent`)
    else if (/\]:/.test(once)) bad.push(`${JSON.stringify(text)}: "]:" left`)
    else if (MI.quirkHazards(once).some((f) => /backtick|code span/.test(f))) bad.push(`${JSON.stringify(text)}: a live backtick`)
  }
  assert.ok(!bad.length, `${bad.length} of 30000:\n  ${first(bad)}`)
})

/* ── (6) the licence notices of what the markdown-it bundle contains ──── */

test('R4 (6) the markdown-it LICENSE carries the notices of the four packages its bundle contains: linkify-it, mdurl (and the Node.js url code it is based on), entities and uc.micro', () => {
  const licence = readFileSync(MI.VENDORED.markdownIt.license, 'utf8')
  for (const must of [
    'Vitaly Puzrin, Alex Kocharin', 'linkify-it', 'Copyright (c) 2015 Vitaly Puzrin.',
    'mdurl', 'Joyent, Inc. and other Node contributors',
    'entities', 'Felix Böhm', 'Redistributions in binary form must reproduce the above copyright notice',
    'uc.micro', 'Mathias Bynens',
  ]) assert.ok(licence.includes(must), `LICENSE does not carry ${JSON.stringify(must)}`)
  /* The note names the same four, with the versions the notices were read from. */
  const note = readFileSync(MI.VENDORED.markdownIt.note, 'utf8')
  for (const must of ['linkify-it', 'mdurl', 'entities', 'uc.micro', '6.1.0', '2.1.0', '8.1.0', '3.0.0']) assert.ok(note.includes(must), `VENDORED.md does not name ${must}`)
})

test('R4 (1) the whole Change of the BREAK-mailto files and of the run-2 transcript\'s mixed Change: three accepted lines that hold the mail shapes, and a title and a criterion that hold the address shapes — full and with things left out, nothing live in any renderer, every recorded word still there', () => {
  const shapes = [
    { c0: 'run `<1@b>` and `<!@b.co>` before shipping', criterion: 'see <1@b> too', waivedCriterion: 'the total shows `<-a@b>` on screen' },
    { title: '//evil.example/a.png', c0: 'a`b<img src=x onerror=1>``[x]: javascript:1 //example.com www.e.com/&#106;avascript:1', criterion: '[![a\\](https://127.0.0.1:1/p.png)](https://127.0.0.1:1/x)' },
  ]
  const bad = []
  for (const values of shapes) {
    for (const redact of [[], CLASSES]) {
      const md = mdOf(scenario(values), redact)
      const live = [...hazards(md), ...MI.unescapedMarkup(md)]
      if (live.length) bad.push(`${JSON.stringify(values)}${redact.length ? ' (redacted)' : ''}: ${live[0]}`)
      for (const r of MI.RENDERERS) {
        const html = r.render(md)
        if (/<a\b/.test(html) && MI.liveIn(html).length) bad.push(`${r.name}: ${MI.liveIn(html)[0]}`)
        for (const word of ['shipping', 'total', 'screen']) if (values.c0 && values.c0.includes(word) && !MI.textOf(html).includes(word)) bad.push(`${r.name}: "${word}" is gone`)
      }
    }
  }
  assert.ok(!bad.length, `${bad.length}:\n  ${first(bad)}`)
})

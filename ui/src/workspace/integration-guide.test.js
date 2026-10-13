/* T-6-6 · B — the agent-integration guide describes only what exists.
 *
 * `docs/AGENT-INTEGRATION-GUIDE.md` is read and every interface it names is
 * checked against the product, not against the guide's own say-so:
 *
 *   - every `thruline <verb>` must be on a fixed allow-list whose every entry
 *     cites its source and is confirmed against the command's own definition
 *     in `crates/cli/src/main.rs` (read-only). `thruline change <verb>` is
 *     checked against the `ChangeVerb` enum, parsed out of that same file;
 *   - every `/api/...` address must be one the UI source actually builds, or
 *     one listed in `change-workspace.test.js`'s address map;
 *   - every agent tool name (a backticked snake_case word) must exist in
 *     `spec/` or `crates/cli/src` (read-only);
 *   - a sentence about something not built fails unless it says, in exactly
 *     these words, "arrives in a later version" — and so does any sentence
 *     naming a verb or address the product does not have;
 *   - the banned words, the Plain vocabulary outside the Technical section,
 *     and the five state words the copy file already uses.
 */
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join, dirname, extname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { HQ_COPY } from './hq-copy.js'
import { fill } from './hq-narrate.js'

const here = dirname(fileURLToPath(import.meta.url))
const REPO = join(here, '..', '..', '..')
const GUIDE_PATH = join(REPO, 'docs', 'AGENT-INTEGRATION-GUIDE.md')
const GUIDE = readFileSync(GUIDE_PATH, 'utf8')
const LATER = 'arrives in a later version'

/* ── the guide, in sentences and sections ──────────────────────────────── */

/** Every sentence of the guide (list items and code lines are their own). */
export function sentencesOf(md) {
  const out = []
  for (const line of md.split('\n')) {
    const t = line.replace(/^\s*[-*]\s+/, '').trim()
    if (!t) continue
    for (const s of t.split(/(?<=[.!?;])\s+(?=[A-Z`"(])/)) if (s.trim()) out.push(s.trim())
  }
  return out
}

/** `## ` sections, each with its heading. The preamble has heading ''.
 *
 * A fenced code block is not a section: since R-2026-09-23-10 the guide quotes
 * the packet renderer's own `## TASK` … `## EXCLUSIONS` headings inside a
 * fence, and reading those as sections would file the prose that follows them
 * under a heading the guide does not have. Fenced lines stay in the body of
 * the section that contains the fence, so the vocabulary checks still see them.
 */
export function sectionsOf(md) {
  const out = [{ heading: '', body: '' }]
  let fenced = false
  for (const line of md.split('\n')) {
    if (/^\s*```/.test(line)) fenced = !fenced
    if (!fenced && /^##\s/.test(line)) out.push({ heading: line.replace(/^##\s+/, ''), body: '' })
    else out[out.length - 1].body += line + '\n'
  }
  assert.ok(!fenced, 'the guide has an unclosed code fence')
  return out
}

/* ── the allow-list of verbs ───────────────────────────────────────────── */

const MAIN_RS = readFileSync(join(REPO, 'crates', 'cli', 'src', 'main.rs'), 'utf8')

/** A clap variant name as the command line spells it: `CancelRun` → `cancel-run`. */
const kebab = (name) => name.replace(/([a-z])([A-Z])/g, '$1-$2').toLowerCase()

/** The top-level variants of `enum Command` and the verbs of `enum ChangeVerb`, from main.rs. */
export function definedVerbs(rs) {
  const block = (name) => {
    const m = new RegExp('\\n(?:pub(?:\\([a-z]+\\))? )?enum ' + name + ' \\{').exec(rs)
    assert.ok(m, `crates/cli/src/main.rs has no enum ${name}`)
    const at = m.index
    let depth = 0
    for (let i = rs.indexOf('{', at); i < rs.length; i += 1) {
      if (rs[i] === '{') depth += 1
      else if (rs[i] === '}') { depth -= 1; if (depth === 0) return rs.slice(at, i) }
    }
    return ''
  }
  const variants = (src) => {
    const out = new Set()
    const lines = src.split('\n')
    lines.forEach((line, i) => {
      const m = /^ {4}([A-Z][A-Za-z]+)\s*(?:\{|\(|,)/.exec(line)
      if (!m) return
      /* A `#[command(... name = "x")]` just above renames the verb. */
      let name = kebab(m[1])
      for (let j = i - 1; j >= 0 && /^\s*(#\[|\/\/\/)/.test(lines[j]); j -= 1) {
        const n = /name\s*=\s*"([a-z-]+)"/.exec(lines[j])
        if (n) { name = n[1]; break }
      }
      out.add(name)
    })
    return out
  }
  return { top: variants(block('Command')), change: variants(block('ChangeVerb')) }
}

/* The allow-list: the verbs the guide may name, each with where it is defined.
 * Every entry is confirmed below against `enum Command` in main.rs, so a verb
 * that is renamed or removed fails here rather than lingering in the guide. */
const FIXED = Object.freeze([
  { verb: 'ui', source: 'main.rs `Ui(UiArgs)`: serves the app on this machine' },
  { verb: 'upgrade', source: 'main.rs `Upgrade { … }`: migrates the store to the installed version' },
  { verb: 'init', source: 'main.rs `Init { … }`: installs the capture, the MCP server and the hooks' },
  { verb: 'perf', source: 'main.rs `Perf { … }`: prints the measured figures and the basis they were taken under' },
  { verb: 'ingest', source: 'main.rs `Ingest { … }`: ingests transcripts from the supported agents' },
  { verb: 'verify', source: 'main.rs `Verify(Box<verify_cmd::VerifyArgs>)`: re-reads raw files and recomputes every hash' },
  { verb: 'export', source: 'main.rs `Export { … }`: derives a project and writes the JSON contract' },
  { verb: 'brief', source: 'main.rs `Brief { … }`: the session-start hook' },
  { verb: 'audit', source: 'main.rs `Audit { … }`: the stop hook' },
  { verb: 'digest', source: 'main.rs `Digest { … }`: the session-end hook' },
  { verb: 'change', source: 'main.rs `Change(ChangeArgs)`; `thruline change cancel-run` stops one running check' },
  { verb: 'check', source: 'main.rs `Check { … for_change … }`; hq-copy.js `change.coverage.run_check_command`' },
  { verb: 'doctor', source: 'main.rs `Doctor { … }`; state.js `SESSION_REMEDY`' },
  { verb: 'mcp', source: 'main.rs `Mcp { agent }`; `init_mcp_install_into` writes `mcp --agent claude`' },
  { verb: 'identity', source: 'main.rs `Identity { … }`; quoted by packet.rs\'s WORKSPACE STATE empty note, "run `thruline identity backfill`"' },
  { verb: 'verify-claims', source: 'main.rs `#[command(name = "verify-claims")] VerifyClaims`' },
])

const DEFINED = definedVerbs(MAIN_RS)
const ALLOWED = new Set(FIXED.map((f) => f.verb))

/** Every `thruline …` the text names, as `verb` or `change sub`. */
export function verbsNamed(text) {
  const out = []
  for (const m of text.matchAll(/\bthruline ([a-z][a-z-]*)(?: ([a-z][a-z-]*))?/g)) {
    out.push(m[1] === 'change' && m[2] ? 'change ' + m[2] : m[1])
  }
  return out
}

export function verbAllowed(v) {
  if (v.startsWith('change ')) return ALLOWED.has('change') && DEFINED.change.has(v.slice(7))
  return ALLOWED.has(v) && DEFINED.top.has(v)
}

/* ── the addresses the UI builds ───────────────────────────────────────── */

function walk(dir, keep, out = []) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name)
    const st = statSync(p)
    if (st.isDirectory()) { if (name !== 'node_modules') walk(p, keep, out) }
    else if (keep(p, st)) out.push(p)
  }
  return out
}

/** `'/api/changes/' + encodeURIComponent(id) + '/proposals'` → `/api/changes/*\/proposals`. */
export function routesInSource(text) {
  const out = new Set()
  const re = /'(\/api\/[^']*)'((?:\s*\+\s*(?:'[^']*'|[A-Za-z_$][\w$.]*\([^()]*\)|[A-Za-z_$][\w$.]*))*)/g
  for (const m of text.matchAll(re)) {
    let route = m[1]
    for (const t of m[2].matchAll(/\+\s*('([^']*)'|[^+]+)/g)) route += t[2] !== undefined ? t[2] : '*'
    out.add(normalRoute(route))
  }
  return out
}

export function normalRoute(r) {
  return r.replace(/<[^>]+>/g, '*').replace(/\?.*$/, '').replace(/[.,;:)]+$/, '').replace(/\*+/g, '*')
}

const UI_SRC = join(here, '..')
const SOURCE_ROUTES = new Set()
/* Product source only: tests and in-repo fixtures (`a11y-fixture.js`, the demo
 * corpus) are not addresses the product builds. */
for (const f of walk(UI_SRC, (p) => /\.(js|jsx)$/.test(p) && !/\.test\.js$|fixture|demo-data/.test(p))) {
  for (const r of routesInSource(readFileSync(f, 'utf8'))) SOURCE_ROUTES.add(r)
}
/* change-workspace.test.js's address map: the write addresses it enumerates. */
for (const r of routesInSource(readFileSync(join(here, 'change-workspace.test.js'), 'utf8'))) SOURCE_ROUTES.add(r)

export function routesNamed(text) {
  return [...text.matchAll(/\/api\/[^\s`'")\]]+/g)].map((m) => normalRoute(m[0]))
}

/* ── where agent tool names may be confirmed ───────────────────────────── */

const TOOL_CORPUS = walk(join(REPO, 'spec'), (p, st) => st.size < 2_000_000 && ['.json', '.md', '.mjs', '.js', '.txt', '.toml', '.yaml', '.yml', ''].includes(extname(p)))
  .concat(walk(join(REPO, 'crates', 'cli', 'src'), (p, st) => st.size < 4_000_000))
  /* The MCP tools and the packet compiler are defined in core, not in the CLI:
   * `crates/core/src/mcp.rs` holds the three tool names and their argument
   * names, `packet.rs` holds the section titles and the empty-section notes.
   * Without this the guide could not name `get_context_packet` at all. */
  .concat(walk(join(REPO, 'crates', 'core', 'src'), (p, st) => st.size < 4_000_000 && /\.rs$/.test(p)))
  .map((p) => readFileSync(p, 'utf8'))
  .join('\n')

export function toolNames(md) {
  return [...new Set([...md.matchAll(/`([a-z]+(?:_[a-z]+)+)`/g)].map((m) => m[1]))]
}

/* ── what is not built ─────────────────────────────────────────────────── *
 * From `docs/ADAPTER-CAPABILITIES.md` (every cell not marked Supported), the
 * card's own list, and the screen's off slots. A sentence matching one of
 * these must say it arrives in a later version. */
export const NOT_BUILT = Object.freeze([
  /recovery injection/i,
  /what the last one knew/i,
  /\bgemini\b/i,
  /assured capture/i,
  /\bask hq\b/i,
  /asking questions of (the|this) record/i,
  /asking for a review/i,
  /\blive capture\b/i,
  /capturing codex live/i,
  /\bproject name\b/i,
  /\blive pill\b/i,
  /\bsaved views\b/i,
  /\bremove\b.*\bstor/i,
])

export function unlabelledNotBuilt(md) {
  const bad = []
  for (const s of sentencesOf(md)) {
    if (s.includes(LATER)) continue
    if (NOT_BUILT.some((re) => re.test(s))) bad.push(s)
    for (const v of verbsNamed(s)) if (!verbAllowed(v)) bad.push(`${s}  [verb: ${v}]`)
    for (const r of routesNamed(s)) if (!SOURCE_ROUTES.has(r)) bad.push(`${s}  [address: ${r}]`)
  }
  return bad
}

/* ── the tests ─────────────────────────────────────────────────────────── */

test('B · every allow-list entry is a command the binary defines', () => {
  for (const f of FIXED) {
    assert.ok(DEFINED.top.has(f.verb), `allow-list entry ${f.verb} (${f.source}) is not a command main.rs defines`)
    assert.ok(f.source.length > 10, `allow-list entry ${f.verb} cites no source`)
  }
  for (const v of ['new', 'propose', 'accept', 'suggest', 'associate', 'prepare', 'results', 'cancel-run']) {
    assert.ok(DEFINED.change.has(v), `ChangeVerb has no ${v}`)
  }
})

test('B · every `thruline <verb>` the guide names is on the allow-list and defined', () => {
  const named = verbsNamed(GUIDE)
  assert.ok(named.length >= 15, `only ${named.length} commands found; the parse is not reading the guide`)
  for (const v of named) assert.ok(verbAllowed(v), `the guide names \`thruline ${v}\`, which is not on the allow-list or not defined`)
})

test('B · every address the guide names is one the UI builds, or one the address map lists', () => {
  const named = routesNamed(GUIDE)
  assert.ok(named.length >= 10, `only ${named.length} addresses found`)
  assert.ok(SOURCE_ROUTES.has('/api/changes/*/decisions/withdraw'), 'the source parse did not reach change-workspace.jsx')
  for (const r of named) assert.ok(SOURCE_ROUTES.has(r), `the guide names ${r}, and nothing in ui/src builds it`)
  for (const r of ['/api/changes/*', '/api/changes/*/proposals', '/api/changes/*/coverage', '/api/changes/*/applicability', '/api/explain', '/api/claims', '/api/changes/*/decisions', '/api/changes/*/decisions/withdraw', '/api/changes/*/runs/*/cancel']) {
    assert.ok(named.includes(r), `the guide leaves out ${r}, an address the card requires it to name`)
  }
})

test('B · every agent tool the guide names exists in spec/ or crates/cli/src', () => {
  const tools = toolNames(GUIDE)
  assert.ok(tools.includes('claim_completion'), 'the guide names no agent tool at all')
  for (const t of tools) assert.ok(TOOL_CORPUS.includes(t), `the guide names \`${t}\`, which is in neither spec/ nor crates/cli/src`)
})

/* --- R-2026-09-23-10: the two state tools -------------------------------- *
 * The guide must describe `get_context_packet` and `check_context` as the
 * product serves them: both names, every heading the packet renderer emits,
 * and both empty-section forms -- because an empty section that does not say
 * WHY it is empty reads as a pass, and that is the failure the forms exist to
 * prevent. Each expectation below is derived from the Rust source, so a
 * rename in `packet.rs` fails here rather than rotting the guide. */

const PACKET_RS = readFileSync(join(REPO, 'crates', 'core', 'src', 'packet.rs'), 'utf8')
const MCP_RS = readFileSync(join(REPO, 'crates', 'core', 'src', 'mcp.rs'), 'utf8')

/** The `## ` headings `Packet::render` emits: SECTION_ORDER's titles, then EXCLUSIONS. */
export function packetHeadings(rs) {
  const order = /pub const SECTION_ORDER: \[Section; (\d+)\] = \[([\s\S]*?)\];/.exec(rs)
  assert.ok(order, 'packet.rs has no SECTION_ORDER to derive the headings from')
  const titles = new Map()
  for (const m of rs.matchAll(/Section::([A-Za-z]+) => "([^"]+)"/g)) titles.set(m[1], m[2])
  const out = []
  for (const m of order[2].matchAll(/Section::([A-Za-z]+)/g)) {
    const t = titles.get(m[1])
    assert.ok(t, `SECTION_ORDER names Section::${m[1]}, which title() does not render`)
    out.push(t)
  }
  assert.equal(out.length, Number(order[1]), 'SECTION_ORDER length and its contents disagree')
  /* `render` appends this one after the loop over SECTION_ORDER. */
  assert.ok(rs.includes('out.push_str("## EXCLUSIONS'), 'render no longer appends EXCLUSIONS')
  out.push('EXCLUSIONS')
  return out
}

/** The two sentences `Empty::render` writes when a section carries nothing. */
export function emptyForms(rs) {
  const out = []
  for (const m of rs.matchAll(/\(nothing — (no source object exists|the source was queried and is empty): \{/g)) out.push(m[1])
  return out
}

test('B · the guide names both state tools, and they are the tools the server offers', () => {
  /* `McpServer::tools()` only — `"name": "thruline"` in the server's own
   * handshake is not a tool. */
  const toolsFn = /pub fn tools\(\) -> Value \{([\s\S]*?)\n    \}/.exec(MCP_RS)
  assert.ok(toolsFn, 'mcp.rs has no tools() to read the tool list from')
  const offered = [...toolsFn[1].matchAll(/"name": "([a-z_]+)"/g)].map((m) => m[1])
  assert.deepEqual(offered, ['get_context_packet', 'check_context', 'claim_completion'],
    'the MCP tool list changed; the guide describes a set the server no longer offers')
  for (const t of ['get_context_packet', 'check_context']) {
    assert.ok(GUIDE.includes('`' + t + '`'), `the guide does not name \`${t}\``)
  }
  /* Named in the Plain half too, not only in the Technical section. */
  const plain = sectionsOf(GUIDE).filter((x) => !/technical/i.test(x.heading)).map((x) => x.body).join('\n')
  for (const t of ['get_context_packet', 'check_context']) {
    assert.ok(plain.includes('`' + t + '`'), `\`${t}\` appears only in the Technical section`)
  }
  /* The fields the observed run returned, and the v1 limit the tool states. */
  for (const f of ['packet_md', 'chain_position', 'latest_chain_position', 'handoff_of', 'budget_tokens', 'deltas']) {
    assert.ok(GUIDE.includes('`' + f + '`'), `the guide does not name the field \`${f}\``)
  }
})

/* R-2026-09-23-10 fix round: `deltas` is NOT absent in v1.
 *
 * `check_context`'s own description string says "Deltas are not in v1", and the
 * `Changed` arm four lines below it returns `lines.iter().map(delta_json)` --
 * a computed diff from `packet::diff_lines`. The first version of this guide
 * believed the string; this test believes the arms, so a guide that tells an
 * integrator the array is inert goes red. The three arms are read out of
 * `mcp.rs` rather than listed here. */

/** The `status` value each arm of `check_context` returns, and what it puts in `deltas`. */
export function checkContextShapes(rs) {
  const fn = /fn check_context\(&self[\s\S]*?\n    \}/.exec(rs)
  assert.ok(fn, 'mcp.rs has no check_context to read the answer shapes from')
  const out = []
  for (const m of fn[0].matchAll(/"status": "([a-z_]+)",\s*\n\s*"current": (true|false),[\s\S]*?"deltas": ([^,\n]+)/g)) {
    out.push({ status: m[1], current: m[2] === 'true', deltas: m[3].trim() })
  }
  return out
}

test('B \u00b7 the guide describes all three check_context answers, and does not call deltas absent', () => {
  const shapes = checkContextShapes(MCP_RS)
  assert.deepEqual(shapes.map((x) => x.status), ['current', 'changed', 'unknown_position'],
    'check_context no longer returns the three answers the guide documents')
  for (const x of shapes) {
    assert.ok(GUIDE.includes('`status: "' + x.status + '"`'), `the guide does not name the answer status "${x.status}"`)
  }
  /* The empty list, the computed list and the null are three different things. */
  assert.equal(shapes[0].deltas, '[]')
  assert.match(shapes[1].deltas, /delta_json/, 'the changed arm no longer computes deltas')
  assert.match(shapes[2].deltas, /Null/, 'the unknown arm no longer returns null')
  /* The Changed entry's own fields, quoted from `delta_json` in the same file. */
  const dj = /fn delta_json\([\s\S]*?\n\}/.exec(MCP_RS)
  assert.ok(dj, 'mcp.rs has no delta_json')
  const fields = [...dj[0].matchAll(/"([a-z]+)":/g)].map((m) => m[1])
  for (const f of new Set(fields)) {
    assert.ok(GUIDE.includes('`' + f + '`'), `the guide does not name the delta field \`${f}\``)
  }
  const kinds = [...PACKET_RS.matchAll(/DeltaKind::[A-Za-z]+ => "([a-z-]+)"/g)].map((m) => m[1])
  assert.deepEqual(kinds, ['added', 'removed', 'axis-changed'], 'DeltaKind changed under the guide')
  for (const kind of kinds) {
    assert.ok(GUIDE.includes('`' + kind + '`'), `the guide does not name the delta kind \`${kind}\``)
  }
  /* And the claim that started this: no sentence may say the array is inert. */
  for (const phrase of ['always `[]`', 'always empty', 'present and always', 'deltas are not in v1',
    'telling an agent *what* changed rather than *that* it changed arrives in a later version']) {
    assert.ok(!GUIDE.includes(phrase), `the guide calls deltas inert: "${phrase}"`)
  }
})

test('B · the guide lists every heading the packet renderer emits, in the order it emits them', () => {
  const headings = packetHeadings(PACKET_RS)
  assert.equal(headings.length, 15, `the renderer emits ${headings.length} headings; the guide was written for 15`)
  let at = -1
  for (const h of headings) {
    const i = GUIDE.indexOf('## ' + h + '\n')
    assert.ok(i > 0, `the guide does not list the heading ${h}`)
    assert.ok(i > at, `the guide lists ${h} out of the order the renderer emits it in`)
    at = i
  }
})

test('B · the guide gives both empty-section forms, and says they differ', () => {
  const forms = [...new Set(emptyForms(PACKET_RS))].sort()
  assert.deepEqual(forms, ['no source object exists', 'the source was queried and is empty'],
    'Empty::render no longer writes the two forms the guide quotes')
  for (const f of forms) {
    assert.ok(GUIDE.includes(f), `the guide does not quote the empty-section form "${f}"`)
    assert.ok(GUIDE.includes('(nothing — ' + f + ':'), `the guide does not show "${f}" as the reader will see it`)
  }
  /* The distinction itself, not only the strings: an empty section is not a pass. */
  assert.match(GUIDE, /they do not mean the same thing|they are not interchangeable/,
    'the guide quotes both forms without saying they mean different things')
  assert.match(GUIDE, /NOT filled from `supported`/,
    'the guide drops the VERIFIED AND CURRENT note that keeps supported from reading as verified')
})

test('B · a sentence about something not built says "arrives in a later version", and so does any unknown verb or address', () => {
  assert.deepEqual(unlabelledNotBuilt(GUIDE), [])
  /* Live instrument: each of these is caught. */
  const probes = [
    'Recovery injection hands the next session a capsule.',
    'ThruLine reads Gemini too.',
    'Run `thruline teleport` to move a Change.',
    'The screen also reads `/api/changes/<id>/telepathy`.',
    'Use Ask HQ to question the record.',
  ]
  for (const p of probes) assert.ok(unlabelledNotBuilt(p).length > 0, `not caught: ${p}`)
  assert.deepEqual(unlabelledNotBuilt('Recovery injection arrives in a later version.'), [])
})

test('B · no banned word, Plain words outside Technical, and exactly the five state words', () => {
  const banned = [...HQ_COPY.banned.map((w) => new RegExp('\\b' + w + '\\b', 'i')),
    /\bhallucinat/i, /\bcaught\b/i, /bullshit/i, /bug-free/i, /\ball clear\b/i, /\bsafe\b/i, /\d+\s*%/]
  for (const re of banned) assert.doesNotMatch(GUIDE, re, `banned: ${re}`)
  const PLAIN_ONLY = [/\bpacket\b/i, /\bledger\b/i, /\bevents?\b/i, /\breceipts?\b/i, /\bpredicates?\b/i, /\bsession id\b/i, /\bapplicability\b/i]
  let technical = 0
  for (const { heading, body } of sectionsOf(GUIDE)) {
    if (/technical/i.test(heading)) { technical += 1; continue }
    for (const re of PLAIN_ONLY) assert.doesNotMatch(body, re, `"${heading || 'preamble'}" uses ${re} outside the Technical section`)
  }
  assert.equal(technical, 1, 'there is one section headed Technical')
  for (const s of Object.values(HQ_COPY.states)) assert.ok(GUIDE.includes('**' + s.word + '**'), `the state word ${s.word} is not in the guide`)
  assert.doesNotMatch(GUIDE, /needs a re-check|\*\*in progress\*\*|\*\*couldn.t check\*\*/i, 'a state word outside the copy file\'s five')
})

test('B · the guide quotes every command an empty state offers, so the screen and the guide agree', () => {
  const commands = [HQ_COPY.change.wayin.command, HQ_COPY.empty.no_project.command, HQ_COPY.empty.no_recordings.command,
    fill(HQ_COPY.change.coverage.run_check_command, { change: '<change>' })]
  for (const c of commands) assert.ok(GUIDE.includes(c), `the guide does not quote ${c}`)
})

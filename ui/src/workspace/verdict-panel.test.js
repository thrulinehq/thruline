import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const temporary = await mkdtemp(join(tmpdir(), 'thruline-verdict-'))
after(() => rm(temporary, { recursive: true, force: true }))
const BUNDLE = {
  stdin: { contents: `
    import React from 'react';
    import { renderToStaticMarkup } from 'react-dom/server';
    import { VerdictPanel, CheckedLine, CheckedSlot } from './verdict-panel.jsx';
    import { HqWorkspace } from './hq.jsx';
    import { CheckingSequence } from './checking-sequence.jsx';
    import { HQ_COPY } from './hq-copy.js';
    export * from './verdict-panel.jsx';
    export { readPhaseCache, landing, schedule, phaseLogHasUnit, phaseLogHasTiming } from './checking-sequence.jsx';
    export { HQ_COPY };
    export const renderPanel = props => renderToStaticMarkup(React.createElement(VerdictPanel, props));
    export const renderHq = props => renderToStaticMarkup(React.createElement(HqWorkspace, props));
    export const renderChecked = props => renderToStaticMarkup(React.createElement(CheckedLine, props));
    export const renderSlot = props => renderToStaticMarkup(React.createElement(CheckedSlot, props));
    export const renderChecking = props => renderToStaticMarkup(React.createElement(CheckingSequence, props));
  `, resolveDir: dirname(fileURLToPath(import.meta.url)) },
  bundle: true, write: false, format: 'esm', platform: 'node',
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
}
const result = await build(BUNDLE)
const compiled = join(temporary, 'verdict-panel.mjs')
await writeFile(compiled, result.outputFiles[0].text)
const M = await import(pathToFileURL(compiled).href)
const { renderPanel, renderHq, renderChecked, renderChecking, renderSlot } = M

/* The same modules over a PROBE copy: HQ_COPY with two strings changed. A surface that renders a string
 * through HQ_COPY renders the probe; one that carries a literal with the real words renders those words and
 * fails. Equality with the real copy alone cannot tell those apart — `tree ${hash}` and
 * fill('tree {hash}', …) render the same bytes (ADJUDICATIONS item D third ruling, X4a / X6). */
const PROBE = { sub_tree: 'probe-sub-tree {hash}', tabs_label: 'probe-tabs-label' }
const COPY_FILE = join(dirname(fileURLToPath(import.meta.url)), 'hq-copy.js')
const probed = await build({
  ...BUNDLE,
  plugins: [{
    name: 'probe-copy',
    setup(b) {
      b.onResolve({ filter: /(^|\/)hq-copy\.js$/ }, (args) => (args.namespace === 'probe-copy' ? { path: COPY_FILE } : { path: 'probe', namespace: 'probe-copy' }))
      b.onLoad({ filter: /.*/, namespace: 'probe-copy' }, () => ({
        resolveDir: dirname(COPY_FILE), loader: 'js',
        contents: `import { HQ_COPY as REAL } from './hq-copy.js'
          const copy = JSON.parse(JSON.stringify(REAL))
          copy.phases.sub_tree = ${JSON.stringify(PROBE.sub_tree)}
          copy.chrome.tabs_label = ${JSON.stringify(PROBE.tabs_label)}
          export const HQ_COPY = copy
          export default copy`,
      }))
    },
  }],
})
const probedFile = join(temporary, 'verdict-panel.probe.mjs')
await writeFile(probedFile, probed.outputFiles[0].text)
const PM = await import(pathToFileURL(probedFile).href)
import { buildVerdict } from './verdict-model.js'
import { HQ_COPY } from './hq-copy.js'
import { fill } from './hq-narrate.js'
import { parse } from '@babel/parser'

/* ── the HQ_COPY fence: a rule over the source, not a list of strings ── */

/** A letter in any script. Text with none — `·`, `→`, `←`, `—`, `+`, `✓`, `(`, `)`, spaces — is layout, not copy. */
const LETTER = /\p{L}/u
/** Stands for an interpolated value when a template literal's static parts are read as prose. */
const SLOT = '\u0000'
/**
 * HQ_COPY's own placeholder syntax, exactly as `fill` reads it (hq-narrate.js `PLACEHOLDER`). A `{name}` in a
 * literal is a value slot like `${…}`: `fill('{n} s', …)` renders the same bytes as HQ_COPY's `elapsed`, so it is
 * read as prose with the placeholder standing for a word (ADJUDICATIONS item D fourth ruling, finding 1, N7b).
 */
const PLACEHOLDER = /\{[a-z_]+\}/g
/**
 * Natural language: two words side by side, at least one of them letters, the other letters, digits or an
 * interpolated value — "Hello stranger", "HTTP 502", "tree ${hash}", "tree {hash}", "{n} s", "3 events". Class
 * names, keys, ids and paths (`ws-verdict-card`, `thruline-phases:${id}`, `/api/phases?session=`) have no
 * whitespace between word characters and never match; nor do `${a} ${b}` and `{a} {b}`, which have no letter of
 * their own. Punctuation beside the whitespace does not keep two words apart: "Hello, stranger", "Checked: ${n}",
 * "{n}: done", "events · trees" (ADJUDICATIONS item D fifth ruling, finding 4).
 */
const NATURAL_LANGUAGE = /\p{L}[\p{P}\s]*\s[\p{P}\s]*[\p{L}\p{N}\u0000]|[\p{L}\p{N}\u0000][\p{P}\s]*\s[\p{P}\s]*\p{L}/u
/** ARIA attributes whose value is an ID reference, a token or true/false (WAI-ARIA 1.2 value types): never announced as text. */
const NON_TEXT_ARIA = new Set([
  'aria-activedescendant', 'aria-controls', 'aria-describedby', 'aria-details', 'aria-errormessage', 'aria-flowto', 'aria-labelledby', 'aria-owns',
  'aria-atomic', 'aria-autocomplete', 'aria-busy', 'aria-checked', 'aria-current', 'aria-disabled', 'aria-expanded', 'aria-haspopup', 'aria-hidden',
  'aria-invalid', 'aria-live', 'aria-modal', 'aria-multiline', 'aria-multiselectable', 'aria-orientation', 'aria-pressed', 'aria-readonly',
  'aria-relevant', 'aria-required', 'aria-selected', 'aria-sort',
])
/** Attributes never read or heard: class names, React keys, ids, roles, label targets, input/button types, data-*, and the non-text ARIA set. */
const nonTextAttribute = (name) => typeof name === 'string' && (['className', 'key', 'id', 'role', 'htmlFor', 'type'].includes(name) || name.startsWith('data-') || NON_TEXT_ARIA.has(name))
/** Attributes a user reads or hears: title, alt, placeholder, and every aria-* outside the non-text set (aria-label, aria-description, aria-roledescription, aria-valuetext, …). */
const visibleAttribute = (name) => typeof name === 'string' && (['title', 'alt', 'placeholder'].includes(name) || (name.startsWith('aria-') && !NON_TEXT_ARIA.has(name)))
const NOT_CHILDREN = new Set(['loc', 'start', 'end', 'extra', 'range', 'leadingComments', 'trailingComments', 'innerComments', 'comments', 'tokens'])
const isFunction = (n) => !!n && (n.type === 'ArrowFunctionExpression' || n.type === 'FunctionExpression' || n.type === 'FunctionDeclaration' || n.type === 'ObjectMethod' || n.type === 'ClassMethod')
const propertyName = (p) => (p.key.type === 'Identifier' && !p.computed ? p.key.name : p.key.type === 'StringLiteral' ? p.key.value : null)

/** Pre-order walk; `path` holds the ancestors, nearest last, each with the key the walk stepped through. */
function walk(node, visit, path = []) {
  if (Array.isArray(node)) { for (const n of node) walk(n, visit, path); return }
  if (!node || typeof node.type !== 'string') return
  if (visit(node, path) === false) return
  for (const key of Object.keys(node)) {
    if (NOT_CHILDREN.has(key) || !node[key] || typeof node[key] !== 'object') continue
    path.push({ node, key })
    walk(node[key], visit, path)
    path.pop()
  }
}

/** The string and template literals an expression can put on screen when it sits in a JSX child, a visible attribute or an Error message. */
function renderedLiterals(e, out = []) {
  if (!e) return out
  switch (e.type) {
    case 'StringLiteral': out.push(e); break
    case 'TemplateLiteral': out.push(e); for (const x of e.expressions) renderedLiterals(x, out); break
    case 'ConditionalExpression': renderedLiterals(e.consequent, out); renderedLiterals(e.alternate, out); break
    case 'LogicalExpression': if (e.operator !== '&&') renderedLiterals(e.left, out); renderedLiterals(e.right, out); break
    case 'SequenceExpression': renderedLiterals(e.expressions[e.expressions.length - 1], out); break
    case 'ArrayExpression': for (const el of e.elements) renderedLiterals(el, out); break
    case 'BinaryExpression': if (e.operator === '+') { renderedLiterals(e.left, out); renderedLiterals(e.right, out) } break
    case 'CallExpression': {
      const method = e.callee.type === 'MemberExpression' && !e.callee.computed ? e.callee.property.name : null
      const fn = e.arguments[0]
      if (method === 'map' && isFunction(fn)) {
        if (fn.body.type !== 'BlockStatement') renderedLiterals(fn.body, out)
        else walk(fn.body, (n) => { if (isFunction(n)) return false; if (n.type === 'ReturnStatement') renderedLiterals(n.argument, out); return true })
      } else if (method === 'join' || method === 'concat') {
        renderedLiterals(e.callee.object, out)
        for (const a of e.arguments) renderedLiterals(a, out)
      } else if (method === 'filter' || method === 'slice') renderedLiterals(e.callee.object, out)
      break
    }
    default: break
  }
  return out
}

/**
 * The syntactic context, if any, in which a literal cannot render. Each is named; no string is listed.
 *   'module source'       the source of an import / export, or of a dynamic import()
 *   'property key'        the key of an object or class property or method (its value is not exempt)
 *   'comparison operand'  either side of `===` or `!==`
 *   'console argument'    an argument of a `console.*` call (up to the nearest function)
 *   'media query'         the one argument of a call to `matchMedia` or `window.matchMedia`, a CSS media query
 *                         (`(prefers-reduced-motion: reduce)`); a second argument, or `other.matchMedia(…)`, is not exempt
 *   'non-text attribute'  inside the value of a `nonTextAttribute` (up to the nearest JSX element or function)
 */
function nonRendering(path) {
  const parent = path[path.length - 1]
  if (!parent) return null
  const { node: p, key: k } = parent
  if (k === 'source' && /^(Import|ExportNamed|ExportAll)Declaration$/.test(p.type)) return 'module source'
  if (p.type === 'ImportExpression' || (p.type === 'CallExpression' && p.callee.type === 'Import')) return 'module source'
  if (k === 'key' && /^(ObjectProperty|ObjectMethod|ClassProperty|ClassMethod)$/.test(p.type)) return 'property key'
  if (p.type === 'BinaryExpression' && (p.operator === '===' || p.operator === '!==')) return 'comparison operand'
  if (p.type === 'CallExpression' && k === 'arguments' && p.arguments.length === 1) {
    const c = p.callee
    const onWindow = c.type === 'MemberExpression' && !c.computed && c.object.type === 'Identifier' && c.object.name === 'window' && c.property.type === 'Identifier' && c.property.name === 'matchMedia'
    if ((c.type === 'Identifier' && c.name === 'matchMedia') || onWindow) return 'media query'
  }
  for (let i = path.length - 1; i >= 0; i -= 1) {
    const { node: a, key } = path[i]
    if (a.type === 'JSXAttribute') return a.name.type === 'JSXIdentifier' && nonTextAttribute(a.name.name) ? 'non-text attribute' : null
    if (a.type === 'CallExpression' && key === 'arguments' && a.callee.type === 'MemberExpression' && a.callee.object.type === 'Identifier' && a.callee.object.name === 'console') return 'console argument'
    if (a.type === 'JSXElement' || a.type === 'JSXFragment' || isFunction(a)) return null
  }
  return null
}

/**
 * Every literal a JSX or JS source could put on screen instead of reading HQ_COPY. Pure over the source text.
 * Four rules that name where text renders, and one deny-by-default rule over everything else:
 *   (a) 'text'       JSX text containing a letter;
 *   (b) 'expression' a string or template literal containing a letter in a rendered position of a JSX
 *                    child: the expression itself, either branch of `?:`, the operands of `||`/`??` and the
 *                    right of `&&`, array elements, both operands of `+`, a template's interpolations, the
 *                    receiver and arguments of `.join` / `.concat` (through `.filter` / `.slice`), and what a
 *                    `.map` callback returns;
 *   (c) 'attribute'  a literal containing a letter, in those positions, as the value of `title`, `alt`,
 *                    `placeholder` or any `aria-*` outside the non-text ARIA set — on the element or inside
 *                    an object spread onto it (`{...{ 'aria-label': … }}`);
 *   (e) 'error'      a literal containing a letter, in those positions, as the message of `new …Error(…)`,
 *                    `…Error(…)` or `throw` — an Error message reaches the screen as a failure reason;
 *   (d) 'prose'      ANY other string literal or template literal, anywhere in the scanned code, whose text
 *                    is natural language (`NATURAL_LANGUAGE`; an interpolation `${…}` and an HQ_COPY
 *                    `{placeholder}` each count as a word — `fill('{n} s', …)` is prose), unless it
 *                    sits in one of the contexts `nonRendering` names.
 * A literal is reported once, under the first rule that flags it. HQ_COPY lookups and `fill(...)` calls
 * carry no literal and are never flagged.
 *
 * Scope: with no `component`, the whole file. With `component`, the top-level declaration of that name,
 * plus every top-level declaration it references by name (a component it renders, a module-scope binding it
 * reads), transitively; a name with no top-level declaration throws.
 *
 * @returns {{followed: string[], hits: Array<{kind: 'text'|'expression'|'attribute'|'error'|'prose', text: string, line: number}>}}
 */
export function copyLiterals(source, { component } = {}) {
  const ast = parse(source, { sourceType: 'module', plugins: ['jsx'] })
  const hits = []
  const flagged = new Set()
  const flag = (kind, node, text, probe = text, test = LETTER) => {
    if (flagged.has(node) || !test.test(probe)) return
    flagged.add(node)
    hits.push({ kind, text: text.trim(), line: node.loc.start.line })
  }
  const textOf = (lit) => (lit.type === 'StringLiteral' ? lit.value : lit.quasis.map((q) => q.value.cooked).join('${…}'))
  const proseOf = (lit) => (lit.type === 'StringLiteral' ? lit.value : lit.quasis.map((q) => q.value.cooked).join(SLOT)).replace(PLACEHOLDER, SLOT)
  const isErrorCallee = (c) => c.type === 'Identifier' && /Error$/.test(c.name)
  const scan = (root) => walk(root, (n, path) => {
    if (n.type === 'JSXText') flag('text', n, n.value)
    else if (n.type === 'JSXElement' || n.type === 'JSXFragment') {
      for (const c of n.children) if (c.type === 'JSXExpressionContainer') for (const lit of renderedLiterals(c.expression)) flag('expression', lit, textOf(lit))
    } else if (n.type === 'JSXAttribute' && n.name.type === 'JSXIdentifier' && visibleAttribute(n.name.name) && n.value) {
      const lits = n.value.type === 'StringLiteral' ? [n.value] : n.value.type === 'JSXExpressionContainer' ? renderedLiterals(n.value.expression) : []
      for (const lit of lits) flag('attribute', lit, textOf(lit))
    } else if (n.type === 'JSXSpreadAttribute' && n.argument.type === 'ObjectExpression') {
      for (const prop of n.argument.properties) if (prop.type === 'ObjectProperty' && visibleAttribute(propertyName(prop))) for (const lit of renderedLiterals(prop.value)) flag('attribute', lit, textOf(lit))
    } else if ((n.type === 'NewExpression' || n.type === 'CallExpression') && isErrorCallee(n.callee)) {
      for (const lit of renderedLiterals(n.arguments[0])) flag('error', lit, textOf(lit))
    } else if (n.type === 'ThrowStatement') {
      for (const lit of renderedLiterals(n.argument)) flag('error', lit, textOf(lit))
    } else if ((n.type === 'StringLiteral' || n.type === 'TemplateLiteral') && !nonRendering(path)) {
      flag('prose', n, textOf(n), proseOf(n), NATURAL_LANGUAGE)
    }
    return true
  })
  if (!component) { scan(ast.program); return { followed: [], hits } }
  const declared = new Map()
  for (const st of ast.program.body) {
    const d = st.type === 'ExportNamedDeclaration' || st.type === 'ExportDefaultDeclaration' ? st.declaration : st
    if (d && (d.type === 'FunctionDeclaration' || d.type === 'ClassDeclaration') && d.id) declared.set(d.id.name, d)
    if (d && d.type === 'VariableDeclaration') for (const v of d.declarations) if (v.id.type === 'Identifier') declared.set(v.id.name, v)
  }
  if (!declared.has(component)) throw new Error(`no top-level declaration named ${component}: the region to scan cannot be located`)
  const followed = []
  const seen = new Set([component])
  const queue = [component]
  while (queue.length) {
    const node = declared.get(queue.shift())
    scan(node)
    walk(node, (n) => {
      const ref = n.type === 'Identifier' || n.type === 'JSXIdentifier' ? n.name : null
      if (ref && declared.has(ref) && !seen.has(ref)) { seen.add(ref); followed.push(ref); queue.push(ref) }
      return true
    })
  }
  return { followed, hits }
}

/** A sessionStorage stand-in for one test; the panel reads it through `typeof sessionStorage`. */
function withSessionStorage(seed, fn) {
  const store = new Map(Object.entries(seed || {}))
  globalThis.sessionStorage = { getItem: (k) => (store.has(k) ? store.get(k) : null), setItem: (k, v) => store.set(k, String(v)), removeItem: (k) => store.delete(k) }
  try { return fn(store) } finally { delete globalThis.sessionStorage }
}

/** The direct children of the first element with this class, as HTML strings (no DOM harness here). */
function childrenOf(html, cls) {
  const open = html.indexOf(`<${'header'} class="${cls}`)
  assert.ok(open >= 0, `no <header class="${cls}…">`)
  const start = html.indexOf('>', open) + 1
  let depth = 0
  let i = start
  const out = []
  let childStart = -1
  const tag = /<(\/?)([a-zA-Z][\w-]*)[^>]*?(\/?)>/g
  tag.lastIndex = start
  let m
  while ((m = tag.exec(html))) {
    const closing = m[1] === '/'
    const selfClosing = m[3] === '/'
    if (closing) {
      depth -= 1
      if (depth < 0) break
      if (depth === 0) out.push(html.slice(childStart, m.index + m[0].length))
    } else {
      if (depth === 0) childStart = m.index
      if (!selfClosing) depth += 1
      else if (depth === 0) out.push(m[0])
    }
    i = m.index + m[0].length
  }
  assert.ok(i > start, 'header body parsed')
  return out
}

const SESSION = {
  key: 'k', label: 'live-0009', harness: 'Claude', finalTree: 'T2', capture: { degraded: true, condition: 'degraded' },
  events: [
    { seq: 5, kind: 'exec', text: 'cargo test → exit 1' },
    { seq: 6, kind: 'snapshot', text: 'tree T1' },
    { seq: 12, kind: 'snapshot', text: 'tree T2' },
  ],
  claims: [
    { id: 'c1', node: 'clm', kind: 'task_complete', text: 'Checkout total fixed. Tests pass.', verdict: 'unsupported', verifier: 't@1', evidence: ['k1'], reasons: [], findings: [{ key: 'test_run_failed', severity: 'fail', detail: 'exit 1', node: 'f_test_run_failed', path: 'checkout/api.py' }] },
    { id: 'c2', node: 'clm2', kind: 'task_complete', text: 'Docs updated.', verdict: 'supported', verifier: 't@1', evidence: ['k1'], reasons: [], findings: [] },
    { id: 'c3', node: 'clm3', kind: 'task_complete', text: 'Lint clean.', verdict: 'unsupported', verifier: 't@1', evidence: [], reasons: [], findings: [] },
  ],
  promises: [{ id: 'p1', node: 'prm', kind: 'follow_up', status: 'dropped', text: 'I will add a test for api.py.' }],
  checks: [{ id: 'k1', node: 'chk', runner: 'cargo test', exit: 1, inputTree: 'T1' }],
  nodes: [{ id: 'ev5', objType: 'event', kind: 'exec', seq: 5, title: 'cargo test', lines: ['cargo test → exit 1'] }],
}
const noop = () => {}
const hqProps = { session: SESSION, mode: undefined, onMode: noop, selected: null, reading: false, onInspect: noop, onCopy: noop, inspector: null, actions: {} }

function sentences(html) {
  return [...html.matchAll(/<div class="(ws-verdict-sentence[^"]*)">([\s\S]*?)<\/div>/g)].map((m) => ({ cls: m[1], body: m[2] }))
}

test('HQ has exactly two tabs — Verdict and Trace — and no Ask box or record-based guide', () => {
  const html = renderHq(hqProps)
  const tabs = [...html.matchAll(/role="tab"/g)].length
  assert.equal(tabs, 2)
  assert.match(html, /id="hq-tab-verdict"[^>]*aria-selected="true"/)
  assert.match(html, /id="hq-tab-trace"[^>]*aria-selected="false"/)
  assert.doesNotMatch(html, /Ask about this session/)
  assert.doesNotMatch(html, /Record-based guide/)
  assert.doesNotMatch(html, /Ask HQ|>Correct<|Verify &amp; setup|Inspect record/)
  assert.match(html, /class="ws-verdict"/, 'unset mode renders the Verdict tab')
})

test('mode trace renders the inspector once, with no second action row of its own; mode checks renders the checks panel', () => {
  const trace = renderHq({ ...hqProps, mode: 'trace', inspector: 'INSPECTOR-HERE' })
  assert.match(trace, /INSPECTOR-HERE/)
  /* The Inspector owns the action row. HQ must not render a duplicate under
   * it — with the inspector stubbed to a string, none of its labels may appear. */
  assert.doesNotMatch(trace, /ws-hq-actions/)
  for (const label of ['Show me the proof', 'Surrounding events', 'Copy reference', 'Open receipt', 'Copy Markdown', 'Command drawer']) assert.doesNotMatch(trace, new RegExp(label), `HQ renders its own "${label}" button`)
  assert.doesNotMatch(renderHq(hqProps), /ws-hq-actions/, 'the Verdict tab renders an action row')
  assert.doesNotMatch(trace, /class="ws-verdict"/)
  assert.match(trace, /id="hq-tab-trace"[^>]*aria-selected="true"/)
  const checks = renderHq({ ...hqProps, mode: 'checks' })
  assert.match(checks, /class="ws-checks-panel"(?! hidden)/)
  assert.doesNotMatch(checks, /class="ws-verdict"/)
  const verdict = renderHq(hqProps)
  assert.match(verdict, /class="ws-checks-panel" hidden/)
})

/* SESSION's own record as a phase log (3 events, 3 statements, 1 cited check, 2 trees, 1 checker), cached under an id
 * with the landing remembered, so the landed "Checked:" line renders from a log. Before ADJUDICATIONS item D fourth
 * ruling (finding 2) these tests rendered SESSION with no log and read the model's counts under the log's nouns; with
 * no log the line's place now holds a reason and no count (the test "with no phase log …" below). */
const SESSION_LOG = { contract: 'thruline.phases.v1', session: 'ses-log', final_tree_hash: 'T2', phases: [
  { phase: 'read_record', count: 3, elapsed_ms: 1 }, { phase: 'find_claims', count: 3, elapsed_ms: 1 }, { phase: 'match_proof', count: 1, elapsed_ms: 0 },
  { phase: 'workspace_since', count: 2, elapsed_ms: 1 }, { phase: 'run_verifiers', count: 1, detail: 't@1', elapsed_ms: 1 }, { phase: 'write_up', count: 3, elapsed_ms: 0 },
] }
const renderWithLog = (props) => withSessionStorage({ 'thruline-phases:ses-log': JSON.stringify(SESSION_LOG), 'thruline-verdict-landed:ses-log': '1' }, () => renderPanel({ ...props, session: { ...SESSION, id: 'ses-log' } }))

test('the Verdict tab speaks the panel vocabulary only', () => {
  const html = renderWithLog({ selected: 'ev5', onShowProof: noop, onCopy: noop, onRecheck: noop, onOpenTrace: noop })
  for (const banned of ['check receipt', 'review point', 'certify', 'unverified', 'Not checked', 'Ask about this session', 'Record-based guide']) {
    assert.ok(!html.includes(banned), `panel renders "${banned}"`)
  }
  /* "events" is banned as a headline count only (brief §4); the Checked: line may say it. */
  const headline = html.match(/<p class="ws-verdict-headline">([\s\S]*?)<\/p>/)
  assert.ok(headline, 'headline element present')
  assert.doesNotMatch(headline[1], /\bevents?\b/i, 'headline counts events')
  assert.doesNotMatch(html.match(/<h2 class="ws-verdict-word">([\s\S]*?)<\/h2>/)[1], /\bevents?\b/i)
  const states = new Set(['proven', 'not_proven', 'couldnt', 'loose', 'fog'])
  for (const m of html.matchAll(/ws-verdict-(?:chip|dot)-(\w+)/g)) assert.ok(states.has(m[1]) || m[1] === 'ref', `state class outside the five: ${m[0]}`)
  assert.match(html, /Not proven/)
  assert.match(html, /Checked:/)
  assert.match(html, /3 events · 3 completion statements · 1 check · 2 trees · 1 checker/)
  assert.doesNotMatch(html, /\d+ claims/, 'the Checked: line and the pills never share the noun "claims" (they count different things)')
  assert.match(html, /Four questions, answered from the record/)
  assert.match(html, /Export summary/)
  assert.match(html, /Open full trace/)
  assert.match(html, /Re-check/, 'the cited check ran on T1, the tree moved, so Re-check shows')
  assert.match(html, /aria-pressed="true"[^>]*>Plain</)
  assert.match(html, /1 thing holds up/)
  assert.match(html, /You clicked: a command the AI ran at step 5\./)
  assert.doesNotMatch(html, /\bseq \d/, 'no engine jargon in a when-phrase')
  assert.match(html, /Show raw record →/)
  assert.match(html, /Was this right\?/)
})

test('the first card carries the primary Copy prompt button and later cards do not', () => {
  const html = renderPanel({ session: SESSION, onShowProof: noop, onCopy: noop })
  const cards = [...html.matchAll(/<article class="ws-verdict-card /g)].length
  assert.ok(cards >= 3, `expected several cards, found ${cards}`)
  assert.equal([...html.matchAll(/ws-verdict-primary/g)].length, 1)
  const firstCard = html.slice(html.indexOf('<article class="ws-verdict-card '), html.indexOf('</article>'))
  assert.match(firstCard, /ws-verdict-primary/)
  assert.match(firstCard, /Command failed, then reported done/)
  assert.ok([...html.matchAll(/ws-verdict-secondary/g)].length >= 1)
  assert.ok([...html.matchAll(/Show proof →/g)].length >= 3)
})

test('every sentence carries a receipt chip or the inference class', () => {
  const html = renderPanel({ session: SESSION, selected: 'ev5', onShowProof: noop, onCopy: noop })
  const all = sentences(html)
  assert.ok(all.length >= 8, `found ${all.length} sentences`)
  for (const s of all) {
    const chipped = s.body.includes('ws-verdict-chip-ref')
    const inference = s.cls.includes('ws-verdict-inference')
    assert.ok(chipped || inference, `sentence without a receipt or inference styling: ${s.body.slice(0, 120)}`)
  }
  assert.ok(all.some((s) => s.body.includes('ws-verdict-chip-ref')), 'at least one grounded sentence')
  assert.ok(all.some((s) => s.cls.includes('ws-verdict-inference')), 'at least one inference (the Do lines)')
})

test('every number-bearing element under the header carries a receipt chip or the "what was checked" link', () => {
  /* ADJUDICATIONS item D finding 3: the headline and the Checked: line carry
   * numbers the `.ws-verdict-sentence` guard above cannot see. The header's
   * direct children are walked; any that contains a digit must carry a chip
   * or the replay link. The headline and the pills share one block because
   * they count the same claim ids — the chips after the headline are the
   * receipts for both. */
  const html = renderWithLog({ onShowProof: noop, onCopy: noop, onReplayChecks: noop })
  const kids = childrenOf(html, 'ws-verdict-head')
  assert.ok(kids.length >= 4, `header children: ${kids.length}`)
  const numbered = kids.filter((k) => /\d/.test(k.replace(/<[^>]+>/g, '')))
  assert.ok(numbered.length >= 2, 'the headline block and the Checked: line both carry numbers')
  for (const k of numbered) {
    const chip = k.includes('ws-verdict-chip-ref')
    const link = k.includes('ws-verdict-checked-link')
    assert.ok(chip || link, `numbered header element without a chip or the replay link: ${k.replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').slice(0, 140)}`)
  }
  /* The headline's chips are the claim ids it counts, at most three, then "+N". */
  const headline = html.match(/<p class="ws-verdict-headline">([\s\S]*?)<\/p>/)[1]
  assert.deepEqual([...headline.matchAll(/ws-verdict-chip-ref">([^<]+)</g)].map((m) => m[1]), ['clm', 'clm2', 'clm3'])
  assert.match(headline, /<span class="ws-verdict-more-refs">\+1<\/span>/, 'the fourth id (the loose promise) is the +N')
  /* The Checked: line's receipt is the replay link, and it is a real button. */
  const checked = html.match(/<p class="ws-verdict-checked">([\s\S]*?)<\/p>/)[1]
  assert.match(checked, /<button type="button" class="ws-text-button ws-verdict-checked-link">what was checked<\/button>/)
  /* A headline that counts nothing carries no number and no chip. */
  const zero = renderPanel({ session: { key: 'z', harness: 'Claude', events: [], claims: [], promises: [], checks: [], nodes: [], capture: { degraded: false } }, onReplayChecks: noop })
  const zh = zero.match(/<p class="ws-verdict-headline">([\s\S]*?)<\/p>/)[1]
  assert.doesNotMatch(zh.replace(/<[^>]+>/g, ''), /\d/)
  assert.doesNotMatch(zh, /ws-verdict-chip-ref/)
})

test('the Checked: line counts extractor-matched statements, and names the verifier-derived claim with its id when the log count is lower', () => {
  /* ADJUDICATIONS item D finding 2: the live session read "1 reported"
   * on the pills and "0 claims" on the Checked: line. The only claim is the
   * one the verifier synthesised (`task_complete` / `unclaimed_completion@1`);
   * the engine's `find_claims.count` is 0. */
  const session = { key: 's', id: 'ses-2', harness: 'Claude', finalTree: 'T9', capture: { degraded: false }, events: [{ seq: 1, kind: 'exec', text: 'x' }], promises: [], checks: [], nodes: [],
    claims: [{ id: 'c1', node: 'clm', kind: 'task_complete', text: 'unfinished work', verdict: 'unverifiable', verifier: 'unclaimed_completion@1', evidence: [], reasons: [], findings: [] }] }
  const log = { contract: 'thruline.phases.v1', session: 'ses-2', phases: [
    { phase: 'read_record', count: 5240, elapsed_ms: 1 }, { phase: 'find_claims', count: 0, detail: 'no completion claim in this session', elapsed_ms: 1 },
    { phase: 'match_proof', count: 0, elapsed_ms: 0 }, { phase: 'workspace_since', count: 1681, elapsed_ms: 1 }, { phase: 'run_verifiers', count: 1, detail: 'unclaimed_completion@1', elapsed_ms: 1 }, { phase: 'write_up', count: 0, elapsed_ms: 0 },
  ] }
  const V = buildVerdict(session)
  assert.equal(V.counts.reported, 1)
  const checked = M.checkedFrom(log, V)
  assert.deepEqual(checked, { events: 5240, claims: 0, checks: 0, trees: 1681, verifiers: 1, derived: { n: 1, refs: ['clm'] } })
  const html = renderChecked({ checked, onReplayChecks: noop, onShowProof: noop })
  assert.match(html, /5240 events · 0 completion statements · 0 checks · 1681 trees · 1 checker/)
  assert.match(html, /<span class="ws-verdict-checked-derived">1 derived from unfinished work <button type="button" class="ws-verdict-chip-ref">clm<\/button><\/span>/)
  assert.doesNotMatch(html, /\b0 claims\b|\b1 reported\b/, 'the line does not restate the pills\' noun')
  /* The same session rendered whole, with the log in the browser cache and the landing remembered: pills and line on one screen agree. */
  const whole = withSessionStorage({ 'thruline-phases:ses-2': JSON.stringify({ ...log, final_tree_hash: 'T9' }), 'thruline-verdict-landed:ses-2': '1' }, () => renderPanel({ session, onShowProof: noop, onReplayChecks: noop }))
  assert.match(whole, /ws-verdict-pill-reported">1 reported</)
  assert.match(whole, /0 completion statements/)
  assert.match(whole, /1 derived from unfinished work/)
  /* No log, no counts. This asserted the model's counts as the intended fallback; live, the landed verdict then read
   * "1 completion statement · 1 tree" beside a trace card reading "1681 trees recorded" (ADJUDICATIONS item D fourth
   * ruling, finding 2). The line's nouns are the log's, so without the log nothing fills them. */
  assert.equal(M.checkedFrom(null, V), null)
  assert.equal(M.checkedFrom({ contract: 'thruline.phases.v1' }, V), null, 'a doc with no phases is no log')
  /* A phase the log does not carry stays empty: its unit is not rendered, and no model count stands in for it. */
  const partial = M.checkedFrom({ ...log, phases: log.phases.filter((p) => p.phase !== 'workspace_since' && p.phase !== 'find_claims') }, V)
  assert.deepEqual(partial, { events: 5240, claims: null, checks: 0, trees: null, verifiers: 1, derived: null })
  const partialHtml = renderChecked({ checked: partial, onReplayChecks: noop })
  assert.match(partialHtml, /<span class="ws-verdict-checked-units">5240 events · 0 checks · 1 checker<\/span>/)
  assert.doesNotMatch(partialHtml, /\btrees?\b|completion statements?|derived/, 'no model count under a noun the log did not produce')
  /* A log that agrees with the model names nothing. */
  const agree = M.checkedFrom({ ...log, phases: log.phases.map((p) => (p.phase === 'find_claims' ? { ...p, count: 1 } : p)) }, V)
  assert.equal(agree.derived, null)
  assert.doesNotMatch(renderChecked({ checked: agree }), /derived/)
})

test('the checking-sequence row and the Checked: line count find_claims under one noun, on the one reduced-motion screen', () => {
  /* ADJUDICATIONS item D second ruling (refutation 2) / item E second ruling
   * (finding 3): under reduced motion the row read "0 claims" one block above
   * the pill "1 reported" and the line "0 completion statements". */
  assert.equal(HQ_COPY.phases.counts.find_claims, HQ_COPY.phases.checked.claims, 'row and line share the plural noun')
  assert.equal(HQ_COPY.phases.counts.find_claims_one, HQ_COPY.phases.checked.claims_one, 'row and line share the singular noun')
  const session = { key: 's', id: 'ses-6', harness: 'Claude', finalTree: 'T9', capture: { degraded: false }, events: [{ seq: 1, kind: 'exec', text: 'x' }], promises: [], checks: [], nodes: [],
    claims: [{ id: 'c1', node: 'clm', kind: 'task_complete', text: 'unfinished work', verdict: 'unverifiable', verifier: 'unclaimed_completion@1', evidence: [], reasons: [], findings: [] }] }
  const phases = (n) => [
    { phase: 'read_record', count: 5240, elapsed_ms: 1 }, { phase: 'find_claims', count: n, elapsed_ms: 1 }, { phase: 'match_proof', count: 0, elapsed_ms: 0 },
    { phase: 'workspace_since', count: 1681, elapsed_ms: 1 }, { phase: 'run_verifiers', count: 1, detail: 'unclaimed_completion@1', elapsed_ms: 1 }, { phase: 'write_up', count: 0, elapsed_ms: 0 },
  ]
  for (const [n, noun] of [[0, 'completion statements'], [1, 'completion statement']]) {
    const log = { contract: 'thruline.phases.v1', session: 'ses-6', final_tree_hash: 'T9', phases: phases(n) }
    const html = withSessionStorage({ 'thruline-phases:ses-6': JSON.stringify(log) }, () => renderPanel({ session, onShowProof: noop, onReplayChecks: noop, reducedMotion: true }))
    assert.match(html, /ws-verdict-static-checks/, 'the static list is on the screen')
    assert.match(html, /ws-verdict-pill-reported">1 reported</, 'with the pill')
    const row = html.match(/data-phase="find_claims">[\s\S]*?<span class="ws-checking-count">([^<]*)<\/span>/)
    const units = html.match(/<span class="ws-verdict-checked-units">([^<]*)<\/span>/)
    assert.ok(row, 'find_claims row rendered')
    assert.ok(units, 'Checked: line rendered')
    const line = units[1].split(' · ')[1]
    assert.equal(row[1], `${n} ${noun}`, 'the row counts completion statements')
    assert.equal(line, row[1], 'the Checked: line says exactly what the row says')
    assert.doesNotMatch(html.replace(/<[^>]+>/g, ' '), /\b\d+ claims?\b/, 'no "N claims" anywhere on the Plain reduced-motion screen')
  }
})

test('at most five cards show before "Show all N"', () => {
  /* ADJUDICATIONS item D finding 4: `SHOWN` was unguarded. Six not-proven claims → five cards and "Show all 6". */
  const six = { key: 'six', harness: 'Claude', capture: { degraded: false }, events: [], promises: [], checks: [], nodes: [],
    claims: [1, 2, 3, 4, 5, 6].map((i) => ({ id: `c${i}`, node: `clm${i}`, kind: 'task_complete', text: `Thing ${i} is done.`, verdict: 'unsupported', verifier: 't@1', evidence: [], reasons: [], findings: [] })) }
  const html = renderPanel({ session: six, onShowProof: noop, onCopy: noop })
  assert.equal([...html.matchAll(/<article class="ws-verdict-card /g)].length, 5)
  assert.match(html, /<button type="button" class="ws-verdict-show-all">Show all 6<\/button>/)
  assert.match(html, /6 findings · most serious first/)
  assert.equal(M.SHOWN, 5)
  const five = { ...six, claims: six.claims.slice(0, 5) }
  const h5 = renderPanel({ session: five, onShowProof: noop, onCopy: noop })
  assert.equal([...h5.matchAll(/<article class="ws-verdict-card /g)].length, 5)
  assert.doesNotMatch(h5, /Show all/, 'exactly five needs no button')
})

/* Does not catch — what still escapes the fence below, so its title and its reach agree:
 *   - a one-word literal — no two words side by side (punctuation beside whitespace does not keep words apart, so
 *     `'Hello, stranger'` is caught; `'Hello,stranger'`, with no whitespace at all, is one word, and words kept apart by a
 *     symbol rather than punctuation, `'events → trees'` or `'a + b'`, are not side by side either), and no word separated by whitespace from an interpolation
 *     `${…}` or an HQ_COPY `{placeholder}` (those are caught: `fill('{n} s', …)`); a word glued to one with no
 *     whitespace, `'{n}s'` or `` `${n}s` ``, still counts as one word — outside the rendered
 *     positions of rules (a)–(c) and (e): bound to a name and rendered later (`const w = 'Hello'` … `{w}`),
 *     passed as a prop to a component that renders it (`<Sentence text="Hello" />`), or assembled at runtime
 *     from one-word pieces stored first (`const p = ['Hello', 'stranger']` … `{p.join(' ')}`,
 *     `String.fromCharCode(…)`);
 *   - a multi-word literal in a context `nonRendering` exempts that renders anyway: a `data-*` / `id` / `key`
 *     value that CSS `attr()` or other code puts on screen, a comparison operand the same expression then
 *     renders (`k === 'Hi there' ? k : …`), an object key later enumerated for display, a `console.*` argument, a
 *     local function named `matchMedia` that renders its one argument;
 *   - text from other modules: everything imported (hq-narrate.js, readable.js, checks-panel.jsx,
 *     check-catalog.jsx — the catalog renders into this tabpanel's id and is a separate surface), and every file
 *     and region not named in the title — shell.jsx, and hq.jsx outside HqWorkspace (the Connections modal);
 *   - text from the record — claim quotes, phase details, session labels, harness names — and from the
 *     environment: the message of a TypeError thrown by `fetch` itself, passed through as the failure reason
 *     (`phaseFailureReason`), and whatever `Intl.*` formatters (`NumberFormat` with a unit, `ListFormat`,
 *     `RelativeTimeFormat`, `DateTimeFormat`) or `toLocale*` methods write — "2.0 seconds", "a, b, and c",
 *     "in 3 days", a month or weekday name. The runtime writes those words; their options are one-word
 *     literals (`'second'`, `'long'`) the rules above do not flag;
 *   - words and numbers that meet only at runtime (`${n} ${noun}` with both from variables). */
test('HQ_COPY fence — verdict-panel.jsx, checking-sequence.jsx and verdict-model.js whole; hq.jsx HqWorkspace (its tablist and Verdict tabpanel) with the same-file declarations it references (Reading, CatalogBoundary): no literal copy', () => {
  /* ADJUDICATIONS item D finding 5, its second ruling (M-D6) and its third ruling (X1–X9): a list of five
   * literals, then a rule over four rendered shapes that ten escapes passed. It is now those shapes plus a
   * deny-by-default rule over any natural-language literal outside named non-rendering contexts. */
  const read = (file) => readFileSync(new URL(`./${file}`, import.meta.url), 'utf8')
  for (const file of ['verdict-panel.jsx', 'checking-sequence.jsx', 'verdict-model.js']) {
    const { hits } = copyLiterals(read(file))
    assert.deepEqual(hits, [], `${file} carries literal copy instead of HQ_COPY: ${JSON.stringify(hits)}`)
  }
  const hq = copyLiterals(read('hq.jsx'), { component: 'HqWorkspace' })
  for (const name of ['Reading', 'CatalogBoundary']) assert.ok(hq.followed.includes(name), `the fence follows ${name}, which HqWorkspace renders; followed: ${hq.followed}`)
  assert.ok(!hq.followed.includes('Connections'), 'the Connections modal is a top-bar surface, not the Verdict tab: outside this fence')
  assert.ok(copyLiterals(read('hq.jsx'), { component: 'Connections' }).hits.length > 0, 'and it does carry literal copy today — out of scope, not clean')
  assert.deepEqual(hq.hits, [], `hq.jsx HqWorkspace carries literal copy: ${JSON.stringify(hq.hits)}`)
  /* The strings item D moved stay moved. */
  const src = read('hq.jsx')
  for (const literal of ['The catalog could not load', 'Loading the verification catalog', 'Reading this session', 'The graph and explanation will update together', 'Reload workspace']) {
    assert.ok(!src.includes(`>${literal}`) && !src.includes(`"${literal}`) && !src.includes(`'${literal}`), `hq.jsx carries the literal "${literal}"`)
  }
  for (const [file, literal] of [['verdict-model.js', "'The AI'"], ['checking-sequence.jsx', "'the AI'"], ['checking-sequence.jsx', "'no reason recorded'"], ['verdict-panel.jsx', "'fetch is not available'"]]) {
    assert.ok(!readFileSync(new URL(`./${file}`, import.meta.url), 'utf8').includes(literal), `${file} carries the literal ${literal}`)
  }
  const reading = renderHq({ ...hqProps, reading: true })
  assert.match(reading, new RegExp(`<h2>${HQ_COPY.chrome.reading.title}</h2><p>${HQ_COPY.chrome.reading.body}</p>`))
  assert.match(renderHq({ ...hqProps, reading: true, mode: 'trace' }), new RegExp(HQ_COPY.chrome.reading.title))
  assert.equal(HQ_COPY.chrome.catalog_failed.title, 'The catalog could not load')
  assert.equal(HQ_COPY.chrome.catalog_loading, 'Loading the verification catalog…')
})

test('the HQ_COPY fence is a live instrument: it flags every escape the third ruling found, and nothing in a named non-rendering context', () => {
  const panel = (jsx, extra = '') => `import { HQ_COPY } from './hq-copy.js'\nimport { fill } from './hq-narrate.js'\nconst C = HQ_COPY.chrome\n${extra}\nexport function P({ k, n, items }) { return <div className="ws-hq-content" role="tabpanel" id="hq-panel-verdict">${jsx}</div> }\n`
  const hitsOf = (jsx, extra) => copyLiterals(panel(jsx, extra), { component: 'P' }).hits
  const hitsIn = (src, component) => copyLiterals(src, component ? { component } : {}).hits
  const kinds = (hits) => hits.map((h) => [h.kind, h.text])
  /* Must flag: the verifier's X1–X9 (ADJUDICATIONS item D third ruling, R1), each in the shape it was found. */
  const HQ = (tablist) => `import { HQ_COPY } from './hq-copy.js'\nconst C = HQ_COPY.chrome\nexport function HqWorkspace() { return <>${tablist}<div className="ws-hq-content" role="tabpanel" id="hq-panel-verdict">{C.yes}</div></> }\n`
  for (const [id, hits, expected] of [
    ['X1 module-scope binding rendered as {hello}', hitsOf('{hello}', "const hello = 'Hello stranger'"), [['prose', 'Hello stranger']]],
    ['X2 literal prop to a same-file component', hitsOf('<Sentence text="Hello stranger, all is fine" role="inference" />', 'function Sentence({ text }) { return <div>{text}</div> }'), [['prose', 'Hello stranger, all is fine']]],
    ['X3 aria-label in an object spread', hitsOf("<p className=\"ws-verdict-foot\" {...{ 'aria-label': 'Hello stranger' }}>{C.footer}</p>"), [['attribute', 'Hello stranger']]],
    ['X4a tablist aria-label literal, outside the tabpanel', hitsIn(HQ('<div className="ws-hq-tabs" role="tablist" aria-label="ThruLine HQ modes" />'), 'HqWorkspace'), [['attribute', 'ThruLine HQ modes']]],
    ['X4b text inside the tablist', hitsIn(HQ('<div className="ws-hq-tabs" role="tablist" aria-label={C.tabs_label}><span>Hello stranger</span></div>'), 'HqWorkspace'), [['text', 'Hello stranger']]],
    ['X5 concatenation', hitsOf("{'Hel' + 'lo there'}"), [['expression', 'Hel'], ['expression', 'lo there']]],
    ['X6 template sub line, built into a binding first', hitsIn("export function P({ k, n }) { const sub = [k, n ? `tree ${n}` : null].filter(Boolean).join(' · '); return <p className=\"ws-checking-sub\">{sub}</p> }", 'P'), [['prose', 'tree ${…}']]],
    ['X7 aria-description', hitsOf('<p aria-description="Hello stranger">{C.footer}</p>'), [['attribute', 'Hello stranger']]],
    ['X8 join of one-word literals', hitsOf("{['Hello', 'stranger'].join(' ')}"), [['expression', 'Hello'], ['expression', 'stranger']]],
    ['X9 Error message', hitsIn("export function load() { return fetch('/x').then((r) => { if (!r.ok) throw new Error('Hello stranger, the server said something odd'); return r }) }"), [['error', 'Hello stranger, the server said something odd']]],
    ['X9, one word', hitsIn("export const load = () => Promise.reject(new Error('unexpected'))"), [['error', 'unexpected']]],
    ['HTTP status template in an Error', hitsIn('export const e = (r) => new Error(`HTTP ${r.status}`)'), [['error', 'HTTP ${…}']]],
    ['a word beside a number, anywhere', hitsIn("export const reason = '3 events'"), [['prose', '3 events']]],
    ['title in an object spread', hitsOf("<p {...{ title: 'Hi' }} />"), [['attribute', 'Hi']]],
    /* ADJUDICATIONS item D fourth ruling, finding 1 (N7a, N7b): a literal in HQ_COPY's own `{placeholder}` syntax passed to fill. */
    ['N7a sub line as a literal fill template', hitsOf("{fill('tree {hash}', { hash: k })}"), [['prose', 'tree {hash}']]],
    ['N7b elapsed counter as a literal fill template', hitsOf("{fill('{n} s', { n })}"), [['prose', '{n} s']]],
    ['a count as a literal fill template', hitsOf("{fill('{n} findings', { n })}"), [['prose', '{n} findings']]],
    ['words either side of a placeholder', hitsOf("{fill('Hello {n} strangers', { n })}"), [['prose', 'Hello {n} strangers']]],
    /* ADJUDICATIONS item D fifth ruling, finding 4 (PN1): words kept apart only by punctuation and whitespace. */
    ['PN1 words separated by a comma, bound to a name', hitsIn("export const hello = 'Hello, stranger'"), [['prose', 'Hello, stranger']]],
    ['a label and an interpolation after a colon', hitsIn("export const line = (n) => `Checked: ${n}`"), [['prose', 'Checked: ${…}']]],
    ['a placeholder, a colon and a word', hitsOf("{fill('{n}: done', { n })}"), [['prose', '{n}: done']]],
    ['two words either side of a middle dot', hitsIn("export const pair = 'events · trees'"), [['prose', 'events · trees']]],
    ['punctuation only after the whitespace', hitsIn("export const aside = 'Hello (stranger)'"), [['prose', 'Hello (stranger)']]],
    /* ADJUDICATIONS item D sixth ruling, finding 2: the media-query context is the global call's one argument only. */
    ['a renderer named matchMedia on another object', hitsIn("export const m = (say) => say.matchMedia('Hello stranger')"), [['prose', 'Hello stranger']]],
    ['a second argument to matchMedia', hitsIn("export const m = (q) => matchMedia(q, 'Hello stranger')"), [['prose', 'Hello stranger']]],
    ['a computed member on window named by a variable', hitsIn("export const m = (matchMedia) => window[matchMedia]('Hello stranger')"), [['prose', 'Hello stranger']]],
  ]) {
    assert.deepEqual(kinds(hits), expected, `the fence misses ${id}`)
  }
  /* Every string-valued ARIA attribute (its value is announced as text) is flagged with a one-word value, which only
   * rule (c) catches — so adding any of them to NON_TEXT_ARIA exempts it and fails here (ADJUDICATIONS item D fourth
   * ruling, finding 3, M-T2: only aria-label had a control). */
  for (const name of ['aria-label', 'aria-description', 'aria-roledescription', 'aria-valuetext', 'aria-placeholder', 'aria-keyshortcuts', 'aria-braillelabel', 'aria-brailleroledescription', 'aria-colindextext', 'aria-rowindextext']) {
    assert.deepEqual(kinds(hitsOf(`<p ${name}="Hello">{C.footer}</p>`)), [['attribute', 'Hello']], `the fence does not flag ${name}="Hello" — has it been added to NON_TEXT_ARIA?`)
  }
  for (const [jsx, kind, text] of [
    ['<p>Hello stranger, everything is fine.</p>', 'text', 'Hello stranger, everything is fine.'],
    ["{'Hello stranger'}", 'expression', 'Hello stranger'],
    ['{"Hello"}', 'expression', 'Hello'],
    ['{`Hello ${k}`}', 'expression', 'Hello ${…}'],
    ["{k ? 'Hello' : null}", 'expression', 'Hello'],
    ["{k || 'Nothing here'}", 'expression', 'Nothing here'],
    ["{items.map((i) => 'Hello')}", 'expression', 'Hello'],
    ["{items.map((i) => { if (i) return C.yes; return 'Hello' })}", 'expression', 'Hello'],
    ['<span title="Hello" />', 'attribute', 'Hello'],
    ["<span title={'Hello'} />", 'attribute', 'Hello'],
    ['<button aria-label="Close" />', 'attribute', 'Close'],
    ['<input placeholder="Type here" />', 'attribute', 'Type here'],
    ['<img alt="Logo" />', 'attribute', 'Logo'],
    ['<p>Überprüft</p>', 'text', 'Überprüft'],
  ]) {
    assert.deepEqual(hitsOf(jsx).map((h) => [h.kind, h.text]), [[kind, text]], `the fence misses ${jsx}`)
  }
  /* Must not flag: each named non-rendering context — and the same words one step outside it are flagged, so each exemption is by syntax, not by string. */
  for (const [context, hits, outside] of [
    ['module source', hitsIn("import { a } from './a module with spaces.js'\nexport { b } from './b module.js'\nexport * from './c module.js'\nconst L = React.lazy(() => import('./lazy module.jsx'))\nexport function P() { return <L /> }"), hitsIn("export const path = './a module with spaces.js'")],
    ['className, and className template parts', hitsOf("<p className=\"ws-text-button ws-verdict-proof\" /><p className={`ws-verdict-card ws-verdict-card-${k}`} /><p className={k ? 'ws-a ws-b' : 'ws-c ws-d'} />"), hitsOf('<p lang="ws-text-button ws-verdict-proof" />')],
    ['key, id, role, htmlFor, type, data-*', hitsOf('<p key="first row" id="hq panel" role="presentation none" data-phase="find claims now" /><label htmlFor="some field" /><button type="reset button" />'), hitsOf('<p lang="first row" />')],
    ['non-text aria-* (ID references, tokens, true/false)', hitsOf('<p aria-labelledby="hq-tab-verdict hq-title" aria-hidden="true" aria-live="polite" aria-relevant="additions text" aria-controls={`hq-panel ${k}`} />'), hitsOf('<p aria-label="polite" />')],
    ['object property keys', hitsIn("const LABELS = { 'two words': 1, \"three words here\": 2 }\nexport function P() { return <p>{LABELS.x}</p> }", 'P'), hitsIn("const LABELS = { x: 'two words' }\nexport function P() { return <p>{LABELS.x}</p> }", 'P')],
    ['comparison operands of === and !==', hitsOf("{k === 'Hello stranger' ? C.yes : C.no}{k !== 'two words' ? C.yes : null}"), hitsOf("{k < 'two words' ? C.yes : null}")],
    ['console.* arguments', hitsOf("{items.map((i) => { console.warn('this row has no id', i); return C.yes })}"), hitsOf("{items.map((i) => { logger.warn('this row has no id', i); return C.yes })}")],
    ['matchMedia argument (a CSS media query)', hitsIn("export const reduced = () => matchMedia('(prefers-reduced-motion: reduce)').matches && window.matchMedia('(max-width: 600px)').matches"), hitsIn("export const reduced = () => query('(prefers-reduced-motion: reduce)').matches")],
  ]) {
    assert.deepEqual(hits, [], `the fence flags a literal in a non-rendering context (${context}): ${JSON.stringify(hits)}`)
    assert.equal(outside.length, 1, `the same words outside that context (${context}) are flagged: ${JSON.stringify(outside)}`)
  }
  /* A declaration in the same file that the component references is followed; one it never references is not scanned. */
  const inner = 'function Inner() { return <p>Hello from inside</p> }'
  assert.deepEqual(hitsOf('<Inner />', inner).map((h) => h.text), ['Hello from inside'])
  assert.deepEqual(hitsOf('{C.yes}', inner), [], 'a declaration the component never references is out of scope')
  /* Not copy: HQ_COPY lookups, fill(), comparisons, class names, keys, ids, roles, data-*, aria-hidden, punctuation-only text, comments. */
  const clean = [
    '{C.went_wrong}', '{fill(C.show_all, { n })}', '{k === \'fog\' ? C.counts.fog_one : fill(C.counts[k], { n })}',
    '<span className="ws-hello" key="hello" id="hello-id" role="status" data-phase="find_claims" aria-hidden="true" aria-pressed={k === \'yes\'} />',
    "<span className={k ? 'ws-verdict-primary' : 'ws-verdict-secondary'} title={C.detector_label} aria-label={C.register_label} />",
    ' · ', '<q>{k}</q> — <span>{n}</span>', '<button>← {C.tabs.verdict}</button>', "{n > 0 ? '+' : ''}", "{k === 'done' ? '✓' : ''}", '( ) → + ✓',
    '{/* a comment with words in it */}', "{items.map((r) => <button key={r}>{r}</button>)}", "{[k, 'tree'].filter(Boolean).length}",
  ]
  for (const jsx of clean) assert.deepEqual(hitsOf(jsx), [], `the fence flags non-copy: ${jsx}`)
  assert.throws(() => copyLiterals('export const X = () => <div>ok</div>', { component: 'HqWorkspace' }), /cannot be located/, 'a missing region fails loud')
})

test('"Explain this →" opens HQ on the Verdict tab', () => {
  /* ADJUDICATIONS item D finding 1: the one door into HQ set `evidenceTab: 'inspect'`, which `normalizeMode` maps to Trace. */
  const shell = readFileSync(new URL('./shell.jsx', import.meta.url), 'utf8')
  const door = shell.match(/className="ws-hq-open" onClick=\{\(\) => this\.setState\(\{ evidenceTab: '(\w+)'/)
  assert.ok(door, 'the Explain this → button sets evidenceTab')
  assert.equal(door[1], 'verdict')
  assert.equal([...shell.matchAll(/className="ws-hq-open"/g)].length, 1, 'one door')
})

test('a landing remembered for the session renders the verdict at once on a return; a fresh session still checks first', () => {
  /* ADJUDICATIONS item D finding 6: the panel remounts on Verdict → Trace → Verdict. */
  const session = { ...SESSION, id: 'ses-3' }
  const back = withSessionStorage({ 'thruline-verdict-landed:ses-3': '1', 'thruline-phases:ses-3': JSON.stringify({ ...SESSION_LOG, session: 'ses-3' }) }, () => renderPanel({ session, onShowProof: noop, onCopy: noop, onReplayChecks: noop }))
  assert.match(back, /ws-verdict-headline/)
  assert.doesNotMatch(back, /ws-verdict-checking/)
  assert.match(back, /ws-verdict-checked-link">what was checked</, 'the replay link is still offered on a return, with the log cached')
  /* A return whose log is not cached lands at once, in normal motion, with the log still on its way: the Checked: line's
   * place says so, with no count and no replay link (ADJUDICATIONS item D fourth ruling, finding 2). */
  const backNoLog = withSessionStorage({ 'thruline-verdict-landed:ses-3': '1' }, () => renderPanel({ session, onShowProof: noop, onCopy: noop, onReplayChecks: noop }))
  assert.match(backNoLog, /ws-verdict-headline/)
  assert.ok(backNoLog.includes(`<p class="ws-verdict-log-loading">${HQ_COPY.phases.checked_loading}</p>`), 'the loading reason stands in the line\'s place')
  assert.doesNotMatch(backNoLog, /ws-verdict-checked/, 'no Checked: line, units or replay link without a log')
  const fresh = withSessionStorage({}, () => renderPanel({ session, onShowProof: noop, onCopy: noop }))
  assert.match(fresh, /ws-verdict-checking/)
  assert.doesNotMatch(fresh, /ws-verdict-headline/)
  const other = withSessionStorage({ 'thruline-verdict-landed:ses-3': '1' }, () => renderPanel({ session: { ...session, id: 'ses-4' }, onShowProof: noop, onCopy: noop }))
  assert.match(other, /ws-verdict-checking/, 'another session is not remembered by this one\'s key')
  const store = new Map()
  const storage = { getItem: (k) => (store.has(k) ? store.get(k) : null), setItem: (k, v) => store.set(k, v) }
  assert.equal(M.readLanded('ses-3', storage), false)
  M.writeLanded('ses-3', storage)
  assert.equal(M.readLanded('ses-3', storage), true)
  assert.equal(M.landedKey('ses-3'), 'thruline-verdict-landed:ses-3')
  assert.match(readFileSync(new URL('./hq.jsx', import.meta.url), 'utf8'), /<VerdictPanel key=\{session && session\.id/, 'a session switch resets the panel; a tab round trip does not')
})

test('one phase fetch per (session, final tree) however many mounts hold it; an orphaned fetch is not aborted, completes into the cache, and a later return reuses it', async () => {
  /* ADJUDICATIONS item E finding 1 (two `claims --json --phases` runs per first
   * open), and its second ruling, refutation 2: the server runs the CLI to
   * completion whatever the socket does, so aborting an orphan saved nothing,
   * and a return to the tab paid a second 11-second run. The fake fetch
   * rejects on abort, as a real one does. */
  const tick = () => new Promise((r) => setTimeout(r, 5))
  const calls = []
  const realFetch = globalThis.fetch
  globalThis.fetch = (url, init) => new Promise((resolve, reject) => {
    const signal = init && init.signal
    if (signal) signal.addEventListener('abort', () => reject(new Error('aborted')))
    calls.push({ url, init, resolve })
  })
  const ok = (session) => ({ ok: true, status: 200, json: () => Promise.resolve({ contract: 'thruline.phases.v1', session, phases: [{ phase: 'read_record', count: 3, elapsed_ms: 1 }] }) })
  const aborted = (i) => !!(calls[i].init && calls[i].init.signal && calls[i].init.signal.aborted)
  const store = new Map()
  globalThis.sessionStorage = { getItem: (k) => (store.has(k) ? store.get(k) : null), setItem: (k, v) => store.set(k, String(v)), removeItem: (k) => store.delete(k) }
  try {
    assert.equal(M.inflightPhaseFetches(), 0)
    const a = M.acquirePhaseLog('ses-9', 'T9')
    const b = M.acquirePhaseLog('ses-9', 'T9')
    assert.equal(calls.length, 1, 'two holders → one request')
    assert.equal(calls[0].url, '/api/phases?session=ses-9')
    assert.equal(a.promise, b.promise, 'both holders share the one promise')
    assert.equal(M.inflightPhaseFetches(), 1)
    /* Every holder leaves mid-run (Verdict → Trace). */
    a.release()
    b.release()
    b.release()
    await tick()
    assert.equal(aborted(0), false, 'no holder left, and the request is still not aborted')
    assert.equal(calls.length, 1, 'releasing starts no request')
    assert.equal(M.inflightPhaseFetches(), 1, 'the orphan stays in flight until it settles')
    /* The return, after a real timer tick, while the run is still going. */
    const c = M.acquirePhaseLog('ses-9', 'T9')
    assert.equal(c.promise, a.promise, 'the returning mount re-acquires the same pending promise')
    assert.equal(calls.length, 1, 'no second request on return')
    c.release()
    await tick()
    assert.equal(M.readPhaseCache('ses-9', 'T9'), null, 'nothing is cached before the run completes')
    /* The run completes with nobody holding it: its result is kept, not thrown away. */
    calls[0].resolve(ok('ses-9'))
    const doc = await a.promise
    await tick()
    const cached = M.readPhaseCache('ses-9', 'T9')
    assert.ok(cached, 'an orphan\'s result is written to the phase cache for (session, final tree)')
    assert.deepEqual(cached.phases, doc.phases)
    assert.equal(M.inflightPhaseFetches(), 0, 'a settled fetch leaves the map; the result lives in the cache')
    assert.equal(calls.length, 1)
    /* A different session gets its own request. */
    const e = M.acquirePhaseLog('ses-10', 'T10')
    assert.notEqual(e.promise, a.promise, 'sessions do not share')
    assert.equal(calls.length, 2)
    /* A failed fetch leaves the map, caches nothing, and the next acquisition fetches afresh. */
    calls[1].resolve({ ok: false, status: 502, json: () => Promise.resolve({}) })
    await assert.rejects(e.promise, (err) => err.message === fill(HQ_COPY.phases.failed_http, { status: 502 }))
    e.release()
    await tick()
    assert.equal(M.inflightPhaseFetches(), 0, 'a failure leaves the map')
    assert.equal(M.readPhaseCache('ses-10', 'T10'), null, 'a failure caches nothing')
    const f = M.acquirePhaseLog('ses-10', 'T10')
    assert.equal(calls.length, 3, 'after a failure the next acquisition fetches afresh')
    assert.notEqual(f.promise, e.promise)
    /* Held at completion, the result is cached the same way. */
    calls[2].resolve(ok('ses-10'))
    await f.promise
    await tick()
    f.release()
    assert.ok(M.readPhaseCache('ses-10', 'T10'), 'a held fetch fills the cache too')
    assert.equal(M.inflightPhaseFetches(), 0)
    /* Keyed like the cache (ADJUDICATIONS item E third ruling, refutation 5): the same (session, tree) shares one
     * request; the same session on a moved final tree starts its own instead of re-acquiring the old tree's promise. */
    const g1 = M.acquirePhaseLog('ses-11', 'T11')
    const g2 = M.acquirePhaseLog('ses-11', 'T11')
    assert.equal(calls.length, 4, 'same (session, tree) → one request')
    assert.equal(g1.promise, g2.promise, 'same (session, tree) → one shared promise')
    const moved = M.acquirePhaseLog('ses-11', 'T11-moved')
    assert.equal(calls.length, 5, 'same session, different final tree → a second request')
    assert.notEqual(moved.promise, g1.promise, 'the moved tree does not re-acquire the old tree\'s promise')
    assert.equal(M.inflightPhaseFetches(), 2)
    calls[3].resolve(ok('ses-11'))
    await g1.promise
    await tick()
    assert.ok(M.readPhaseCache('ses-11', 'T11'), 'the old tree\'s run is cached under the old tree')
    assert.equal(M.inflightPhaseFetches(), 1, 'the moved tree\'s fetch is still in flight on its own key')
    calls[4].resolve(ok('ses-11'))
    await moved.promise
    await tick()
    for (const lease of [g1, g2, moved]) lease.release()
    assert.ok(M.readPhaseCache('ses-11', 'T11-moved'), 'the moved tree\'s run is cached under the moved tree')
    assert.equal(M.inflightPhaseFetches(), 0)
  } finally {
    globalThis.fetch = realFetch
    delete globalThis.sessionStorage
  }
})

const unescapeHtml = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')

test('a failed phase fetch renders its reason from HQ_COPY: a 502, a wrong-contract 200 and a non-JSON 200; a network error passes through as the browser wrote it', async () => {
  /* ADJUDICATIONS item D third ruling, finding 1: `new Error(\`HTTP ${r.status}\`)` and `new Error('unexpected body')`
   * reached the screen through `phases.failed`. The lease's rejection is read exactly as usePhaseLog reads it
   * (`phaseFailureReason`) and rendered by the real CheckingSequence. */
  const P = HQ_COPY.phases
  const tick = () => new Promise((r) => setTimeout(r, 5))
  const realFetch = globalThis.fetch
  const session = { id: 'ses-fail', harness: 'Claude', finalTree: 'abcdef0123456789' }
  const reasonLine = (reason) => {
    const html = renderChecking({ session, phases: [], status: { kind: 'failed', reason }, register: 'manager' })
    const m = html.match(/<p class="ws-checking-reason">([^<]*)<\/p>/)
    assert.ok(m, 'the failure line renders')
    return unescapeHtml(m[1])
  }
  try {
    for (const [label, response, copy] of [
      ['502', { ok: false, status: 502, json: () => Promise.resolve({}) }, fill(P.failed_http, { status: 502 })],
      ['wrong contract, 200', { ok: true, status: 200, json: () => Promise.resolve({ contract: 'something.else', phases: [] }) }, P.failed_body],
      ['not JSON, 200', { ok: true, status: 200, json: () => Promise.reject(new SyntaxError("Unexpected token '<', \"<!doctype \"... is not valid JSON")) }, P.failed_body],
    ]) {
      globalThis.fetch = () => Promise.resolve(response)
      const lease = M.acquirePhaseLog(`ses-fail-${label}`, 'T')
      const e = await lease.promise.then(() => null, (err) => err)
      lease.release()
      assert.ok(e, `${label}: the lease rejects`)
      assert.equal(M.phaseFailureReason(e), copy, `${label}: the reason is HQ_COPY text`)
      assert.equal(reasonLine(M.phaseFailureReason(e)), fill(P.failed, { reason: copy }), `${label}: the rendered sentence is HQ_COPY's`)
    }
    assert.equal(reasonLine(fill(P.failed_http, { status: 502 })), 'The record of what was checked could not be loaded — the server answered with status 502. The verdict below does not depend on it.')
    /* Out of this fence's scope, by decision: the browser's own network error is record-of-the-environment text. */
    globalThis.fetch = () => Promise.reject(new TypeError('Failed to fetch'))
    const lease = M.acquirePhaseLog('ses-fail-network', 'T')
    const e = await lease.promise.then(() => null, (err) => err)
    lease.release()
    assert.equal(M.phaseFailureReason(e), 'Failed to fetch')
    await tick()
    assert.equal(M.inflightPhaseFetches(), 0, 'every failure left the in-flight map')
  } finally {
    globalThis.fetch = realFetch
  }
})

test('the checking sub line renders its tree segment from HQ_COPY.phases.sub_tree, and the HQ tablist its aria-label from HQ_COPY.chrome.tabs_label — a literal with the same words fails the probe', () => {
  /* ADJUDICATIONS item D third ruling, finding 2: both moves reverted byte-for-byte with the suite green (X4a, X6). */
  const finalTree = 'e71c5b04deadbeefcafe0123'
  const session = { ...SESSION, id: 'ses-sub', finalTree }
  const subOf = (html) => { const m = html.match(/<p class="ws-checking-sub">([^<]*)<\/p>/); assert.ok(m, 'the sub line renders'); return unescapeHtml(m[1]) }
  const tablistLabelOf = (html) => { const m = html.match(/role="tablist" aria-label="([^"]*)"/); assert.ok(m, 'the tablist renders'); return unescapeHtml(m[1]) }
  /* The real copy, on the fresh checking screen. */
  const fresh = withSessionStorage({}, () => renderPanel({ session, onShowProof: noop }))
  assert.equal(subOf(fresh), [SESSION.label, fill(HQ_COPY.phases.sub_tree, { hash: finalTree.slice(0, 8) })].join(' · '))
  assert.equal(tablistLabelOf(renderHq(hqProps)), HQ_COPY.chrome.tabs_label)
  /* The probe copy: the rendered strings follow HQ_COPY, not a literal carrying today's words. */
  assert.notEqual(PM.HQ_COPY.phases.sub_tree, HQ_COPY.phases.sub_tree, 'the probe changes the sub_tree string')
  assert.notEqual(PM.HQ_COPY.chrome.tabs_label, HQ_COPY.chrome.tabs_label, 'the probe changes the tabs_label string')
  const probedSub = subOf(PM.renderChecking({ session, phases: [], status: { kind: 'loading' } }))
  assert.equal(probedSub, [SESSION.label, fill(PROBE.sub_tree, { hash: finalTree.slice(0, 8) })].join(' · '), 'the sub line renders phases.sub_tree through fill')
  assert.equal(tablistLabelOf(PM.renderHq(hqProps)), PROBE.tabs_label, 'the tablist renders chrome.tabs_label')
})

test('reduced motion renders the static, all-ticked list and the landed verdict together, with no timers', () => {
  /* ADJUDICATIONS item E finding 4: the static list was never on screen and "what was checked" was inert. */
  const session = { ...SESSION, id: 'ses-5' }
  const log = { contract: 'thruline.phases.v1', session: 'ses-5', final_tree_hash: 'T2', phases: [
    { phase: 'read_record', count: 3, elapsed_ms: 40 }, { phase: 'find_claims', count: 3, elapsed_ms: 5 }, { phase: 'match_proof', count: 1, elapsed_ms: 0 },
    { phase: 'workspace_since', count: 2, elapsed_ms: 12 }, { phase: 'run_verifiers', count: 1, detail: 't@1', elapsed_ms: 900 }, { phase: 'write_up', count: 3, elapsed_ms: 2 },
  ] }
  const html = withSessionStorage({ 'thruline-phases:ses-5': JSON.stringify(log) }, () => renderPanel({ session, onShowProof: noop, onCopy: noop, reducedMotion: true }))
  assert.match(html, /class="ws-verdict ws-verdict-reduced"/)
  assert.match(html, /ws-verdict-static-checks/)
  assert.match(html, /ws-checking-static/, 'the list is the static one')
  const rows = [...html.matchAll(/<li class="ws-checking-row ws-checking-(\w+)"/g)].map((m) => m[1])
  assert.equal(rows.length, 6)
  assert.ok(rows.every((r) => r === 'done'), JSON.stringify(rows))
  assert.match(html, /ws-verdict-headline/, 'and the verdict is on the same screen')
  assert.match(html, /<span class="ws-checking-elapsed"><\/span>/, 'no elapsed counter')
  assert.doesNotMatch(html, /ws-checking-spinning/)
  assert.doesNotMatch(html, /ws-verdict-landing/, 'no landing animation class under reduced motion')
  assert.ok(html.indexOf('ws-verdict-static-checks') < html.indexOf('ws-verdict-head'), 'the list sits above the verdict')
  assert.match(html, /ws-verdict-checked-link">what was checked</, 'the replay link is offered; it re-shows and scrolls to the list')
  /* Without a cached log the list section still renders (rows arrive with the fetch) and the verdict is not withheld. */
  const loading = withSessionStorage({}, () => renderPanel({ session, onShowProof: noop, onCopy: noop, reducedMotion: true }))
  assert.match(loading, /ws-verdict-static-checks/)
  assert.match(loading, /ws-verdict-headline/)
  /* The source: "what was checked" under reduced motion scrolls to the list rather than replaying. */
  const src = readFileSync(new URL('./verdict-panel.jsx', import.meta.url), 'utf8')
  assert.match(src, /if \(reduced\) \{ setListHidden\(false\); setScrollTick/)
  assert.match(src, /scrollIntoView\(\{ block: 'start' \}\)/)
})

test('with no phase log the landed verdict holds the reason in the Checked: line\'s place, in both motion settings — no counts under the log\'s nouns, no replay link', async () => {
  /* ADJUDICATIONS item D fourth ruling, finding 2. In normal motion `landing()` returns 'immediate' for a failed log, so
   * the verdict landed before the checking sequence's reason painted (0 of 1096 frames, live), and the landed verdict
   * showed no reason, no link, and the model's counts under the log's nouns: "1 completion statement · 1 tree" beside a
   * trace card reading "1681 trees recorded".
   *
   * SSR runs no effects, so the failure path is tested in its pieces:
   *   exercised      the real lease rejecting (a 502, a wrong-contract 200, a network TypeError) → `phaseFailureReason`
   *                  → `failedLog`, the state usePhaseLog sets → `landing(status, false, 0)` → the landed `CheckedSlot`
   *                  (it takes no motion input, so one render stands for both settings); and the whole landed panel,
   *                  in normal and reduced motion, for the no-log states SSR reaches (no session id; the log loading).
   *   NOT exercised  the effects that join the pieces in a browser — usePhaseLog's `.catch` calling setLog,
   *                  CheckingSequence's 'immediate' effect calling onDone, VerdictPanel's `land` setting `landed` — and
   *                  the reduced-motion panel withholding its list for a failed log. Those are pinned on the source at
   *                  the end, which a rewrite could pass while behaving differently. */
  const P = HQ_COPY.phases
  const realFetch = globalThis.fetch
  const tick = () => new Promise((r) => setTimeout(r, 5))
  const V = buildVerdict(SESSION)
  const noLogCounts = (html, label) => {
    assert.doesNotMatch(html, /ws-verdict-checked/, `${label}: no Checked: line, label, units or replay link`)
    assert.doesNotMatch(html, /<button/, `${label}: no replay link in the line's place`)
    assert.doesNotMatch(unescapeHtml(html.replace(/<[^>]+>/g, ' ')), /\b\d+ (?:events?|completion statements?|checks?|trees?|checkers?)\b/, `${label}: no count under the log's nouns`)
  }
  try {
    for (const [label, fetchImpl, reason] of [
      ['502', () => Promise.resolve({ ok: false, status: 502, json: () => Promise.resolve({}) }), fill(P.failed_http, { status: 502 })],
      ['wrong contract, 200', () => Promise.resolve({ ok: true, status: 200, json: () => Promise.resolve({ contract: 'something.else', phases: [] }) }), P.failed_body],
      ['network TypeError', () => Promise.reject(new TypeError('Failed to fetch')), 'Failed to fetch'],
    ]) {
      globalThis.fetch = fetchImpl
      const lease = M.acquirePhaseLog(`ses-landed-${label}`, 'T')
      const e = await lease.promise.then(() => null, (err) => err)
      lease.release()
      assert.ok(e, `${label}: the lease rejects`)
      const log = M.failedLog(M.phaseFailureReason(e))
      assert.deepEqual(log, { doc: null, status: { kind: 'failed', reason } }, `${label}: the log state carries the reason`)
      assert.equal(M.landing(log.status, false, 0), 'immediate', `${label}: in normal motion a failure lands the verdict at once`)
      assert.equal(M.checkedFrom(log.doc, V), null, `${label}: no counts without the log`)
      const html = renderSlot({ log, verdict: V, onReplayChecks: noop, onShowProof: noop })
      assert.equal(unescapeHtml(html), `<p class="ws-verdict-log-failed ws-verdict-inference">${fill(P.failed_landed, { reason })}</p>`, `${label}: the failure sentence, with its reason, in the line's place`)
      noLogCounts(html, label)
      assert.doesNotMatch(unescapeHtml(html.replace(/<[^>]+>/g, '')).replace(reason, ''), /\d/, `${label}: the notice's only digits are the transport status inside its reason — no record count without a receipt`)
    }
    await tick()
    assert.equal(M.inflightPhaseFetches(), 0)
  } finally {
    globalThis.fetch = realFetch
  }
  assert.equal(unescapeHtml(renderSlot({ log: M.failedLog(HQ_COPY.chrome.no_fetch), verdict: V, onReplayChecks: noop })), `<p class="ws-verdict-log-failed ws-verdict-inference">${fill(P.failed_landed, { reason: HQ_COPY.chrome.no_fetch })}</p>`, 'no fetch at all: the same sentence, its reason from copy')
  assert.match(P.failed_landed, /The verdict above does not depend on it\.$/, 'the landed sentence sits under the verdict and says so')
  /* The whole landed panel. Its header's last child is the Checked: line's place. */
  const slotOf = (html) => { const kids = childrenOf(html, 'ws-verdict-head'); return kids[kids.length - 1] }
  const noId = slotOf(renderPanel({ session: SESSION, onShowProof: noop, onCopy: noop, onReplayChecks: noop }))
  assert.equal(unescapeHtml(noId), `<p class="ws-verdict-log-failed ws-verdict-inference">${fill(P.failed_landed, { reason: HQ_COPY.chrome.no_session_id })}</p>`, 'no session id (normal motion): the reason stands in the line\'s place')
  noLogCounts(noId, 'no session id')
  const loading = slotOf(withSessionStorage({ 'thruline-verdict-landed:ses-landed-loading': '1' }, () => renderPanel({ session: { ...SESSION, id: 'ses-landed-loading' }, onShowProof: noop, onReplayChecks: noop })))
  assert.equal(loading, `<p class="ws-verdict-log-loading">${P.checked_loading}</p>`, 'a remembered landing with the log still loading (normal motion) says so')
  noLogCounts(loading, 'loading, normal motion')
  const reducedLoading = slotOf(withSessionStorage({}, () => renderPanel({ session: { ...SESSION, id: 'ses-landed-reduced' }, onShowProof: noop, onReplayChecks: noop, reducedMotion: true })))
  assert.equal(reducedLoading, `<p class="ws-verdict-log-loading">${P.checked_loading}</p>`, 'and under reduced motion')
  noLogCounts(reducedLoading, 'loading, reduced motion')
  /* Source pins for what SSR cannot run (named above). */
  const panelSrc = readFileSync(new URL('./verdict-panel.jsx', import.meta.url), 'utf8')
  assert.match(panelSrc, /\.catch\(\(e\) => \{ if \(alive\) setLog\(failedLog\(phaseFailureReason\(e\)\)\) \}\)/, 'usePhaseLog sets failedLog on a rejected lease')
  assert.match(panelSrc, /<CheckedSlot log=\{log\} verdict=\{V\} /, 'the landed header renders the slot from the log')
  assert.equal((panelSrc.match(/<CheckedLine /g) || []).length, 1, 'the Checked: line renders only through CheckedSlot')
  assert.match(panelSrc, /\{reduced && seqStatus && seqStatus\.kind !== 'failed' \? <div ref=\{listRef\}/, 'under reduced motion a failed log has no list above the verdict')
  const seqSrc = readFileSync(new URL('./checking-sequence.jsx', import.meta.url), 'utf8')
  assert.match(seqSrc, /if \(mode === 'immediate'\) \{\s*setTicked\(rows\.length\)\s*if \(doneRef\.current\) doneRef\.current\(\)/, 'the immediate landing calls onDone')
})

test('a phase log with no countable row is not in the expected form everywhere: the lease rejects it, the cache ignores it, the slot and the panel show no row of it — and no copy reason carries a digit of its own', async () => {
  /* ADJUDICATIONS item D fifth ruling, findings 2 and 3; sixth ruling, findings 1, 4 and 6. §2.1.1 promises the six
   * phases once each. A contract-valid body with no countable row rendered "Checked: · what was checked" (fifth); then
   * the slot called it "not in the expected form" while the reduced-motion list ticked its rows and normal motion
   * played them beside that sentence (sixth). The decision is now made where a log is accepted — the lease and the
   * cache — so such a log is a failure on every surface. The digit rule was asserted only by stripping the whole
   * reason, so a digit inside a copy reason passed. */
  const P = HQ_COPY.phases
  const V = buildVerdict(SESSION)
  const realFetch = globalThis.fetch
  try {
    for (const [label, phases] of [
      ['phases: []', []],
      ['unknown ids only', [{ phase: 'mystery', count: 7 }]],
      ['write_up only', [{ phase: 'write_up', count: 3 }]],
      ['counts that are not whole numbers', [{ phase: 'read_record', count: '5240' }, { phase: 'workspace_since', count: null }, { phase: 'match_proof', count: -1 }, { phase: 'run_verifiers', count: 1.5 }]],
    ]) {
      const doc = { contract: 'thruline.phases.v1', phases }
      assert.equal(M.checkedFrom(doc, V), null, `${label}: no unit, no counts`)
      const html = unescapeHtml(renderSlot({ log: { doc, status: { kind: 'ready', cached: false } }, verdict: V, onReplayChecks: noop, onShowProof: noop }))
      assert.equal(html, `<p class="ws-verdict-log-failed ws-verdict-inference">${fill(P.failed_landed, { reason: P.failed_body })}</p>`, `${label}: the slot, if such a log reached it, says "not in the expected form"`)
      assert.doesNotMatch(html, /ws-verdict-checked|<button/, `${label}: no Checked: line, no replay link`)
      const id = `ses-nounit-${label}`
      globalThis.fetch = () => Promise.resolve({ ok: true, status: 200, json: () => Promise.resolve(doc) })
      const writes = []
      const priorStorage = globalThis.sessionStorage
      globalThis.sessionStorage = { getItem: () => null, setItem: (k) => writes.push(k), removeItem: () => {} }
      const lease = M.acquirePhaseLog(id, 'T2')
      const err = await lease.promise.then(() => null, (x) => x)
      lease.release()
      await new Promise((r) => setTimeout(r, 0))
      if (priorStorage === undefined) delete globalThis.sessionStorage
      else globalThis.sessionStorage = priorStorage
      assert.ok(err, `${label}: the lease rejects a contract-valid 200 with no countable row`)
      assert.deepEqual(writes, [], `${label}: the lease writes nothing to the phase cache (seventh ruling, finding 4)`)
      assert.equal(M.phaseFailureReason(err), P.failed_body, `${label}: as not in the expected form`)
      const cached = JSON.stringify({ ...doc, final_tree_hash: 'T2' })
      assert.equal(M.readPhaseCache(id, 'T2', { getItem: (k) => (k === `thruline-phases:${id}` ? cached : null) }), null, `${label}: the cache ignores it`)
      const panel = unescapeHtml(withSessionStorage({ [`thruline-phases:${id}`]: cached }, () => renderPanel({ session: { ...SESSION, id }, onShowProof: noop, onReplayChecks: noop, reducedMotion: true })))
      assert.doesNotMatch(panel, /ws-checking-done|mystery/, `${label}: the whole panel under reduced motion ticks no row of that log`)
      assert.match(panel, /class="ws-verdict-log-loading"/, `${label}: the cached log is a miss, so the panel is still asking for one (for phases: [] this assertion, not the row one, is what fails if the cache admits the log)`)
    }
  } finally {
    globalThis.fetch = realFetch
  }
  /* One countable row is enough for a line — any of the five. */
  for (const [phase, unit] of Object.entries({ read_record: 'events', find_claims: 'claims', match_proof: 'checks', workspace_since: 'trees', run_verifiers: 'verifiers' })) {
    const c = M.checkedFrom({ phases: [{ phase, count: 3 }] }, V)
    assert.ok(c, `${phase} alone makes a line`)
    assert.equal(c[unit], 3, `${phase} alone counts ${unit}`)
  }
  /* Beside a countable row, a count that is not whole and non-negative is still not a unit (seventh ruling, finding 2). */
  assert.deepEqual(M.checkedFrom({ phases: [{ phase: 'read_record', count: 5240 }, { phase: 'match_proof', count: -1 }, { phase: 'workspace_since', count: 1.5 }, { phase: 'run_verifiers', count: '1' }] }, V), { events: 5240, claims: null, checks: null, trees: null, verifiers: null, derived: null })
  /* "derived from unfinished work" counts the verifier's derived claims, one chip each — never a bare difference. */
  const noDerived = { ...V, counts: { ...V.counts, reported: 5 }, checked: { ...V.checked, derivedRefs: [] } }
  assert.equal(M.checkedFrom({ phases: [{ phase: 'find_claims', count: 2 }] }, noDerived).derived, null, 'a difference with no derived claim is not called derived')
  assert.deepEqual(M.checkedFrom({ phases: [{ phase: 'find_claims', count: 2 }] }, { ...noDerived, checked: { ...V.checked, derivedRefs: ['c1', 'c2'] } }).derived, { n: 2, refs: ['c1', 'c2'] })
  /* The notice's only digits are an HTTP status: no copy reason carries one, and failed_http's come from {status}. */
  for (const [path, text] of [
    ['phases.failed_landed', P.failed_landed.replace('{reason}', '')],
    ['phases.failed_http', P.failed_http.replace('{status}', '')],
    ['phases.failed_body', P.failed_body],
    ['phases.failed_timing', P.failed_timing],
    ['chrome.no_fetch', HQ_COPY.chrome.no_fetch],
    ['chrome.no_session_id', HQ_COPY.chrome.no_session_id],
    ['chrome.no_reason', HQ_COPY.chrome.no_reason],
  ]) assert.doesNotMatch(text, /\d/, `${path} carries a digit of its own: ${text}`)
})

test('a phase log with a row the engine could not time is refused everywhere with its own reason: the lease rejects it, the cache ignores it, the landed slot says so — no count, no replay link, no schedule from a coerced 0', async () => {
  /* An unknown duration stays unknown: a row with no measured time has no schedule and is never coerced to 0. The engine
   * wrote `unwrap_or(u64::MAX)` for a time it could not represent, and this panel's schedule turned any missing time
   * into 0 and played the log on it. The engine now writes `elapsed_ms: null`; a log carrying one — or any time that is
   * not a whole, non-negative number — is refused where a log is accepted, beside the no-unit refusal. */
  const P = HQ_COPY.phases
  const V = buildVerdict(SESSION)
  const timed = [
    { phase: 'read_record', count: 3, elapsed_ms: 40 }, { phase: 'find_claims', count: 1, elapsed_ms: 5 }, { phase: 'match_proof', count: 0, elapsed_ms: 0 },
    { phase: 'workspace_since', count: 2, elapsed_ms: 12 }, { phase: 'run_verifiers', count: 1, detail: 'tests_pass@1', elapsed_ms: 900 }, { phase: 'write_up', count: 0, elapsed_ms: 2 },
  ]
  const realFetch = globalThis.fetch
  try {
    for (const [label, bad] of [['null, the engine\'s unknown', null], ['absent', undefined], ['negative', -1], ['fractional', 1.5], ['a string', '12']]) {
      const phases = timed.map((p, i) => (i !== 3 ? p : bad === undefined ? { phase: p.phase, count: p.count } : { ...p, elapsed_ms: bad }))
      const doc = { contract: 'thruline.phases.v1', session: 'x', phases }
      assert.equal(M.phaseLogHasUnit(doc), true, `${label}: premise — the log has units, so its timing alone is what refuses it`)
      assert.equal(M.phaseLogHasTiming(doc), false, label)
      assert.equal(M.schedule(phases, { cached: false }), null, `${label}: no schedule is built`)
      const id = `ses-untimed-${label}`
      globalThis.fetch = () => Promise.resolve({ ok: true, status: 200, json: () => Promise.resolve(doc) })
      const writes = []
      const priorStorage = globalThis.sessionStorage
      globalThis.sessionStorage = { getItem: () => null, setItem: (k) => writes.push(k), removeItem: () => {} }
      const lease = M.acquirePhaseLog(id, 'T3')
      const err = await lease.promise.then(() => null, (x) => x)
      lease.release()
      await new Promise((r) => setTimeout(r, 0))
      if (priorStorage === undefined) delete globalThis.sessionStorage
      else globalThis.sessionStorage = priorStorage
      assert.ok(err, `${label}: the lease rejects a contract-valid 200 with an untimed row`)
      assert.equal(M.phaseFailureReason(err), P.failed_timing, `${label}: with the timing reason`)
      assert.deepEqual(writes, [], `${label}: nothing is written to the phase cache`)
      const cached = JSON.stringify({ ...doc, final_tree_hash: 'T3' })
      assert.equal(M.readPhaseCache(id, 'T3', { getItem: (k) => (k === `thruline-phases:${id}` ? cached : null) }), null, `${label}: the cache ignores it`)
      const log = M.failedLog(M.phaseFailureReason(err))
      assert.equal(M.landing(log.status, false, 0), 'immediate', `${label}: the verdict lands at once`)
      assert.equal(M.checkedFrom(log.doc, V), null, `${label}: no counts without the log`)
      const html = unescapeHtml(renderSlot({ log, verdict: V, onReplayChecks: noop, onShowProof: noop }))
      assert.equal(html, `<p class="ws-verdict-log-failed ws-verdict-inference">${fill(P.failed_landed, { reason: P.failed_timing })}</p>`, `${label}: the existing failure notice, with the timing reason`)
      assert.doesNotMatch(html, /ws-verdict-checked|<button/, `${label}: no Checked: line, no replay link`)
      assert.doesNotMatch(html.replace(/<[^>]+>/g, ''), /\d/, `${label}: no digit — neither a count nor a time`)
      /* The whole panel under reduced motion with that log in the cache: a miss, still asking; no row of it ticked. */
      const panel = unescapeHtml(withSessionStorage({ [`thruline-phases:${id}`]: cached }, () => renderPanel({ session: { ...SESSION, id }, onShowProof: noop, onReplayChecks: noop, reducedMotion: true })))
      assert.doesNotMatch(panel, /ws-checking-done/, `${label}: no row of the refused log is ticked`)
      assert.match(panel, /class="ws-verdict-log-loading"/, `${label}: the cached log is a miss`)
    }
  } finally {
    globalThis.fetch = realFetch
  }
  /* The same log fully timed is accepted: the refusal is about the missing time and nothing else. */
  assert.equal(M.phaseLogHasTiming({ contract: 'thruline.phases.v1', phases: timed }), true)
  assert.deepEqual(M.schedule(timed, { cached: false }), timed.map((p) => p.elapsed_ms))
})

test('with a session id the checking sequence renders first, all rows pending, and the verdict is not yet on screen', () => {
  const html = renderPanel({ session: { ...SESSION, id: 'ses-1' }, onShowProof: noop, onCopy: noop })
  assert.match(html, /class="ws-verdict ws-verdict-checking"/)
  assert.match(html, /Checking what Claude said against the record/)
  assert.match(html, /Skip to verdict/)
  assert.doesNotMatch(html, /<li class="ws-checking-row/, 'no rows before the log arrives — none are invented')
  assert.doesNotMatch(html, /ws-verdict-headline|What went wrong/, 'the verdict lands after the sequence')
  const src = readFileSync(new URL('./verdict-panel.jsx', import.meta.url), 'utf8')
  assert.match(src, /fetch\(`\/api\/phases\?session=\$\{encodeURIComponent\(sessionId\)\}`/, 'the one fetch is /api/phases for this session')
  assert.equal((src.match(/fetch\(/g) || []).length, 1)
})

test('Technical shows the detector id in a code chip; Plain maps it to a phrase', () => {
  const plain = renderPanel({ session: SESSION, onShowProof: noop, onCopy: noop, register: 'manager' })
  assert.doesNotMatch(plain, /ws-verdict-detector|>test_run_failed</, 'the only test_run_failed in Plain is the fixture node id f_test_run_failed inside a receipt chip')
  const tech = renderPanel({ session: SESSION, onShowProof: noop, onCopy: noop, register: 'operator' })
  assert.match(tech, /<code class="ws-verdict-detector"[^>]*>test_run_failed<\/code>/)
})

/* ── the captured claim fixtures, read the way the shell reads them ──────
 * docs/audit/narrator-baseline/claims__0N-*: the served bytes go through data.js readProjection, shell.jsx
 * Workspace#adapt and buildVerdict — the node path of the Step 0 exact-bytes audit. STEP0 audit §2.6: the
 * holds-up line ended "exited 0 ." (an empty {check_ref}), claims__03 printed [f_test_run_failed] in prose,
 * and the demo Fog line read "cause not recorded.. What happened". */
const CAPTURED = [
  ['claims__01-three-asks-one-done-one-claimed-one-untouched', {
    findings: [{ kind: 'couldnt_check', key: 'verdict/couldnt_check/couldnt', refs: ['clm2'], text: "Recorded: what the record holds cannot settle this — no test check was captured in this session; the claim may be true and there is no evidence either way.", fallbacks: [] }],
    holdsUp: [{ key: 'verdict/*/proven', refs: ['clm'], text: 'Proven: the record backs it — ran on the final tree; `cargo build` exited 0.', fallbacks: [] }],
  }],
  ['claims__02-supported-lint-claim', {
    findings: [],
    holdsUp: [{ key: 'verdict/*/proven', refs: ['clm'], text: 'Proven: the record backs it — ran on the final tree; `cargo clippy --all-targets` exited 0.', fallbacks: [] }],
  }],
  ['claims__03-contradicted-tests-claim', {
    findings: [{ kind: 'command_failed_then_done', key: 'verdict/command_failed_then_done/not_proven', refs: ['clm', 'f_test_run_failed'], text: 'Not proven: before this claim the record shows 1 failed command exit and 1 failed tool call, and no successful re-run afterwards.', fallbacks: [] }],
    holdsUp: [],
  }],
  ['claims__04-insufficient-evidence-tests-claim', {
    findings: [{ kind: 'couldnt_check', key: 'verdict/couldnt_check/couldnt', refs: ['clm'], text: "Recorded: what the record holds cannot settle this — ran on the final tree; the runner's output could not be parsed, so the run's result is unknown — which is not the same as passing.", fallbacks: [] }],
    holdsUp: [],
  }],
  ['claims__05-open-promise-beside-supported-build', {
    findings: [{ kind: null, key: 'verdict/*/loose', refs: ['prm'], text: 'Loose thread: the record shows nothing done about it after a point in this session.', fallbacks: [] }],
    holdsUp: [{ key: 'verdict/*/proven', refs: ['clm'], text: 'Proven: the record backs it — ran on the final tree; `cargo build` exited 0.', fallbacks: [] }],
  }],
]
/** A rendered string that prints a ref token, leaves a space before punctuation, doubles a full stop, holds a
 * double space where an empty slot was, or puts a full stop after a quotation that already ended its sentence
 * (`."."`, `?".`, `…".`: commit G p1p2-followups, p2 verifier F6). A bracket a slot value supplies would trip the
 * first pattern too; no string these fixtures or the demo sessions put on screen carries one (fix pass 1 measurement). */
const wordingDefects = (text) => [/\[[^\[\]\s]+\]/, /\s[.,;:!?]/, /\.\./, / {2}/, /[.!?…]["”]\./].filter((re) => re.test(text)).map(String)
/** Every string buildVerdict puts on screen for a session, by surface. */
const onScreen = (V, S) => ({
  headline: [V.headline],
  record: V.findings.map((c) => c.record.text),
  do: V.findings.map((c) => c.do.text),
  prompt: V.findings.map((c) => c.do.prompt).filter((p) => p != null),
  holdsUp: V.holdsUp.map((h) => h.record.text),
  answers: V.questions.map((q) => q.answer),
  clicked: (S.nodes || []).map((n) => V.clicked(n.id)).filter(Boolean).flatMap((c) => [c.title, c.body.text]),
})
/** Sweeps every on-screen string for wording defects and returns how many strings it read from each surface, so a
 * surface the sweep stops reading changes the count it is held to. */
function sweepOnScreen(V, S, label) {
  const read = {}
  for (const [surface, texts] of Object.entries(onScreen(V, S))) {
    read[surface] = 0
    for (const text of texts) {
      read[surface] += 1
      assert.deepEqual(wordingDefects(text), [], `${label} ${surface}: ${text}`)
    }
  }
  return read
}
/* How many strings each surface puts on screen, measured on these bytes and on the demo sessions (fix pass 1
 * probe), the same in both registers. No demo node resolves to a clicked body. */
const SWEPT = {
  'claims__01-three-asks-one-done-one-claimed-one-untouched': { headline: 1, record: 1, do: 1, prompt: 1, holdsUp: 1, answers: 4, clicked: 28 },
  'claims__02-supported-lint-claim': { headline: 1, record: 0, do: 0, prompt: 0, holdsUp: 1, answers: 4, clicked: 26 },
  'claims__03-contradicted-tests-claim': { headline: 1, record: 1, do: 1, prompt: 1, holdsUp: 0, answers: 4, clicked: 28 },
  'claims__04-insufficient-evidence-tests-claim': { headline: 1, record: 1, do: 1, prompt: 1, holdsUp: 0, answers: 4, clicked: 26 },
  'claims__05-open-promise-beside-supported-build': { headline: 1, record: 1, do: 1, prompt: 1, holdsUp: 1, answers: 4, clicked: 28 },
  'demo-narrowed': { headline: 1, record: 5, do: 5, prompt: 5, holdsUp: 0, answers: 4, clicked: 0 },
  'demo-honest': { headline: 1, record: 0, do: 0, prompt: 0, holdsUp: 1, answers: 4, clicked: 0 },
  'real-fog-redacted': { headline: 1, record: 1, do: 1, prompt: 0, holdsUp: 0, answers: 4, clicked: 0 },
}
/* Exact clicked text for a captured node whose value ends in the full stop its template also supplies: the body
 * read "…in `src/cli.rs`.. What you asked for…" (fix pass 1, verifier finding 1). Since commit G p2-quotations the
 * user's prompt is a quotation of the record's text, verbatim with its own closing stop, inside the template's
 * quotation marks. Since commit G p1p2-followups (p2 verifier F6) a quotation that ends its own sentence takes the place of
 * the stop the template puts after the closing mark: this read `…src/cli.rs`.". What you asked…`, a stop after a stop. */
const CLICKED = {
  'claims__05-open-promise-beside-supported-build': {
    ev8: { title: 'You clicked: something you asked at step 8.', body: 'This is your own message to the AI: "Rename the `--verbose` flag to `--debug` in `src/cli.rs`." What you asked for is measured against what the AI reported done.' },
  },
}

test('captured fixtures: the exact Technical Record sentences and clicked text; no string on screen prints a ref token, a stray space, a doubled full stop, a double space or a stop after a quotation that ended its sentence, anywhere in the string', async () => {
  const out = await build({ ...BUNDLE, stdin: { ...BUNDLE.stdin, contents: `
    export { Workspace } from './shell.jsx';
    export { readProjection, API } from './data.js';
    export { buildVerdict } from './verdict-model.js';
    export { DEMO_SESSIONS } from './demo-data.js';
  ` } })
  const file = join(temporary, 'captured-fixtures.mjs')
  await writeFile(file, out.outputFiles[0].text)
  const R = await import(pathToFileURL(file).href)
  const capture = new URL('../../../docs/audit/narrator-baseline/', import.meta.url)
  let clickedChecked = 0
  /* The longest string the sweep reads below, so the self-check at the end can plant its defects past it. */
  let longest = 0
  const measure = (V, S) => { for (const t of Object.values(onScreen(V, S)).flat()) longest = Math.max(longest, t.length) }
  const realFetch = globalThis.fetch
  const served = new Map()
  globalThis.fetch = async (url) => {
    const hit = served.get(String(url))
    if (!hit) return new Response('{}', { status: 404 })
    return new Response(hit.bytes, { status: 200, headers: { 'Content-Type': 'application/json; charset=utf-8', 'X-ThruLine-Projection': hit.projection, 'X-ThruLine-Exit': String(hit.exit) } })
  }
  try {
    for (const [name, expected] of CAPTURED) {
      const at = (f) => readFileSync(new URL(`${name}/${f}`, capture))
      const exits = JSON.parse(at('exit-codes.json').toString('utf8'))
      const id = JSON.parse(at('claims.json').toString('utf8')).session
      const explainUrl = R.API.explain + '?session=' + encodeURIComponent(id)
      const claimsUrl = R.API.claims + '?session=' + encodeURIComponent(id)
      served.set(explainUrl, { bytes: at('explain.json'), exit: exits.explain, projection: 'explain --json' })
      served.set(claimsUrl, { bytes: at('claims.json'), exit: exits.claims, projection: 'claims --json' })
      const [explain, claims] = await Promise.all([R.readProjection(explainUrl), R.readProjection(claimsUrl)])
      assert.equal(explain.status, 'ready', name)
      assert.equal(claims.status, 'ready', name)
      const S = R.Workspace.prototype.adapt.call(Object.create(R.Workspace.prototype), explain, null, null, claims)
      const V = R.buildVerdict(S, { register: 'operator' })
      assert.deepEqual({
        findings: V.findings.map((c) => ({ kind: c.kind, key: c.record.key, refs: c.record.refs, text: c.record.text, fallbacks: c.record.fallbacks })),
        holdsUp: V.holdsUp.map((h) => ({ key: h.record.key, refs: h.record.refs, text: h.record.text, fallbacks: h.record.fallbacks })),
      }, expected, name)
      for (const register of ['manager', 'operator']) {
        const W = R.buildVerdict(S, { register })
        assert.deepEqual(sweepOnScreen(W, S, `${name} [${register}]`), SWEPT[name], `${name} [${register}]: strings read per surface`)
        measure(W, S)
        for (const [node, want] of Object.entries(CLICKED[name] || {})) {
          const c = W.clicked(node)
          assert.deepEqual(c && { title: c.title, body: c.body.text }, want, `${name} [${register}] clicked ${node}`)
          clickedChecked += 1
        }
      }
    }
  } finally {
    globalThis.fetch = realFetch
  }
  assert.equal(clickedChecked, 2, 'the exact clicked text was checked in both registers')

  const gap = R.buildVerdict(R.DEMO_SESSIONS['real-fog-redacted'], { register: 'operator' }).findings.find((c) => c.kind === 'recording_gap')
  assert.equal(gap.record.text, 'Fog: the record for this session is incomplete — The recorder is not vouched for this session — cause not recorded. What happened in the gap cannot be settled.')
  assert.equal(gap.record.key, 'verdict/recording_gap/fog')
  assert.deepEqual(gap.record.refs, ['rcpt'])
  assert.deepEqual(gap.record.fallbacks, [])
  assert.deepEqual(Object.keys(R.DEMO_SESSIONS), ['demo-narrowed', 'demo-honest', 'real-fog-redacted'], 'every demo session is swept')
  for (const [key, session] of Object.entries(R.DEMO_SESSIONS)) {
    for (const register of ['manager', 'operator']) {
      const W = R.buildVerdict(session, { register })
      assert.deepEqual(sweepOnScreen(W, session, `${key} [${register}]`), SWEPT[key], `${key} [${register}]: strings read per surface`)
      measure(W, session)
    }
  }
  /* The defect check is a live instrument. */
  assert.deepEqual(wordingDefects('Proven: the record backs it — `cargo build` exited 0 .'), ['/\\s[.,;:!?]/'])
  assert.deepEqual(wordingDefects('last failure [f_test_run_failed].'), ['/\\[[^\\[\\]\\s]+\\]/'])
  assert.deepEqual(wordingDefects('cause not recorded.. What happened'), ['/\\.\\./'])
  assert.deepEqual(wordingDefects('A test in  changed in this session.'), ['/ {2}/'])
  assert.deepEqual(wordingDefects('This is your own message to the AI: "Rename the flag.". What you asked for is measured.'), ['/[.!?…]["”]\\./'])
  assert.deepEqual(wordingDefects('Earlier you said "Is that next?". Is that done?'), ['/[.!?…]["”]\\./'])
  assert.deepEqual(wordingDefects('Proven: the record backs it — `cargo build` exited 0.'), [])
  assert.deepEqual(wordingDefects('This is your own message to the AI: "fix the totals". What you asked for is measured.'), [], 'a stop after a quotation that did not end its sentence is the sentence\'s stop')

  /* …and it reads the whole of every string, and every string, on every surface. Each defect is planted at the start,
   * in the middle and at the end of a string whose clean filler, on each side of the defect that has one, is longer than
   * the longest string the sweep read above. That string is the first, the middle or the last of a run of strings on its
   * surface; the middle one has more strings on each side of it than the most strings any surface put on screen above.
   * The sweep must stop at the planted string with its exact message. So a sweep that reads only a window of each string
   * no longer than the longest string on screen, or only a run of each surface's strings no longer than the most strings
   * on screen, misses a defect here. History: a sweep that read only a string's first 60 characters passed the checks
   * above, whose defects all sit within 53 characters of the start (p1 verifier pass 3, break P3-11); then a sweep that
   * read only the last 120 characters, or checked only the first 2 strings of each surface, passed plantings made only at
   * the end of a surface's one string (p1p2-followups pass-1 verifier, breaks F2b and F2c). */
  assert.ok(longest > 60, `the swept strings are longer than the truncation this check was written against: ${longest}`)
  const most = Math.max(...Object.values(SWEPT).flatMap((counts) => Object.values(counts)))
  assert.equal(most, 28, 'the most strings one surface put on screen above')
  const run = 2 * most + 3
  const clean = 'The record holds the request, and what came back is the next step in the trace. '
  const filler = clean.repeat(Math.ceil((longest + 1) / clean.length))
  const AT = { start: (d) => `${d} ${filler}`, middle: (d) => `${filler}${d} ${filler}`, end: (d) => `${filler}${d}` }
  const CLOSE = { start: [false, true], middle: [true, true], end: [true, false] }
  const SURFACES = ['headline', 'record', 'do', 'prompt', 'holdsUp', 'answers', 'clicked title', 'clicked body']
  /* A screen whose `surface` holds `run` strings (the headline holds its one), all clean but the one at `index`; every
   * other surface holds one clean string. `read` is how many strings the sweep must read from each surface of it. */
  const screenWith = (surface, index, text) => {
    const n = (s) => (s.includes(surface) ? run : 1)
    const at = (i, s) => (s === surface && i === index ? text : clean)
    const V = {
      headline: surface === 'headline' ? text : clean,
      findings: Array.from({ length: n(['record', 'do', 'prompt']) }, (_, i) => ({ record: { text: at(i, 'record') }, do: { text: at(i, 'do'), prompt: at(i, 'prompt') } })),
      holdsUp: Array.from({ length: n(['holdsUp']) }, (_, i) => ({ record: { text: at(i, 'holdsUp') } })),
      questions: Array.from({ length: n(['answers']) }, (_, i) => ({ answer: at(i, 'answers') })),
      clicked: (id) => ({ title: at(Number(id), 'clicked title'), body: { text: at(Number(id), 'clicked body') } }),
    }
    const S = { nodes: Array.from({ length: n(['clicked title', 'clicked body']) }, (_, i) => ({ id: String(i) })) }
    const read = { headline: 1, record: V.findings.length, do: V.findings.length, prompt: V.findings.length, holdsUp: V.holdsUp.length, answers: V.questions.length, clicked: 2 * S.nodes.length }
    return { V, S, read }
  }
  /* Each defect with the exact patterns the sweep must report for it. */
  const LATE = [
    ['Proven: the record backs it — `cargo build` exited 0 .', ['/\\s[.,;:!?]/']],
    ['last failure [f_test_run_failed].', ['/\\[[^\\[\\]\\s]+\\]/']],
    ['cause not recorded.. What happened', ['/\\.\\./']],
    ['A test in  changed in this session.', ['/ {2}/']],
    ['This is your own message to the AI: "Rename the flag.". What you asked for is measured.', ['/[.!?…]["”]\\./']],
  ]
  let planted = 0
  let controls = 0
  for (const surface of SURFACES) {
    const shown = surface.startsWith('clicked') ? 'clicked' : surface
    for (const index of surface === 'headline' ? [0] : [0, most + 1, run - 1]) {
      for (const [position, place] of Object.entries(AT)) {
        /* The same screen with a clean sentence where the defect goes passes, and is read in full. */
        const control = screenWith(surface, index, place('Proven: the record backs it — `cargo build` exited 0.'))
        assert.deepEqual(sweepOnScreen(control.V, control.S, 'late'), control.read, `${surface} #${index} ${position}: the clean screen passes and is read in full`)
        controls += 1
        for (const [defect, patterns] of LATE) {
          const text = place(defect)
          const offset = text.indexOf(defect)
          assert.deepEqual([offset > longest, text.length - offset - defect.length > longest], CLOSE[position], `${position}: the defect's clean filler is longer than the longest string the sweep read`)
          const { V, S } = screenWith(surface, index, text)
          /* Node puts the diff after an assertion's own message, so the message is read up to its first line break (the
           * text holds none) and the reported patterns are read from the error itself, exactly. */
          assert.throws(() => sweepOnScreen(V, S, 'late'), (e) => e instanceof assert.AssertionError &&
            e.message.split('\n')[0] === `late ${shown}: ${text}` && JSON.stringify(e.actual) === JSON.stringify(patterns) && JSON.stringify(e.expected) === '[]',
          `the sweep misses "${defect}" at character ${offset} of string ${index} of ${surface}`)
          planted += 1
        }
      }
    }
  }
  assert.equal(controls, 66, 'three positions in one headline string, and in three strings of each of seven other surfaces')
  assert.equal(planted, 330, 'five defects at each of those places')
})

test('captured fixture 03 through the shell path: the panel names one failed command exit and one failed tool call, and never "2 commands", in both registers (commit G p3-wording)', async () => {
  /* STEP0 audit §2.6: one failed `cargo test` rendered "2 commands failed". The exec row (seq 24) and the failed tool
   * result (seq 28) were added up as commands. The record does not link them (docs/INTERFACE-REQUESTS.md rows 20-22), so
   * each is named by its record kind, and neither is said to be the same run as the other, or a different one. */
  const out = await build({ ...BUNDLE, stdin: { ...BUNDLE.stdin, contents: `
    export { Workspace } from './shell.jsx';
    export { readProjection, API } from './data.js';
    export { buildVerdict } from './verdict-model.js';
  ` } })
  const file = join(temporary, 'captured-03-failures.mjs')
  await writeFile(file, out.outputFiles[0].text)
  const R = await import(pathToFileURL(file).href)
  const at = (f) => readFileSync(new URL(`../../../docs/audit/narrator-baseline/claims__03-contradicted-tests-claim/${f}`, import.meta.url))
  const exits = JSON.parse(at('exit-codes.json').toString('utf8'))
  const id = JSON.parse(at('claims.json').toString('utf8')).session
  const explainUrl = R.API.explain + '?session=' + encodeURIComponent(id)
  const claimsUrl = R.API.claims + '?session=' + encodeURIComponent(id)
  const served = new Map([
    [explainUrl, { bytes: at('explain.json'), exit: exits.explain, projection: 'explain --json' }],
    [claimsUrl, { bytes: at('claims.json'), exit: exits.claims, projection: 'claims --json' }],
  ])
  const realFetch = globalThis.fetch
  let S
  try {
    globalThis.fetch = async (url) => {
      const hit = served.get(String(url))
      if (!hit) return new Response('{}', { status: 404 })
      return new Response(hit.bytes, { status: 200, headers: { 'Content-Type': 'application/json; charset=utf-8', 'X-ThruLine-Projection': hit.projection, 'X-ThruLine-Exit': String(hit.exit) } })
    }
    const [explainRead, claimsRead] = await Promise.all([R.readProjection(explainUrl), R.readProjection(claimsUrl)])
    assert.equal(explainRead.status, 'ready')
    assert.equal(claimsRead.status, 'ready')
    S = R.Workspace.prototype.adapt.call(Object.create(R.Workspace.prototype), explainRead, null, null, claimsRead)
  } finally {
    globalThis.fetch = realFetch
  }
  assert.deepEqual(S.events.filter((e) => e.kind === 'exec' || e.kind === 'tool_result').map((e) => [e.seq, e.kind, e.text]),
    [[16, 'tool_result', 'Edit ok'], [24, 'exec', 'cargo test → exit 101'], [28, 'tool_result', 'Bash FAILED']], 'the served rows: one failed command exit, one failed tool call, and nothing that links them')
  const shared = { counts: [1, 1], label: 'Which commands or tool calls failed?', answer: '1 failed command exit and 1 failed tool call', refs: ['ev24', 'ev28'] }
  const EXPECTED = {
    manager: { ...shared,
      headline: 'The AI said this session finished. Of what it reported done, the record backs 0 of 1; 1 has no proof. The record shows 1 failed command exit and 1 failed tool call with no successful re-run afterwards.',
      record: 'Not proven: before this was reported done, the record shows 1 failed command exit and 1 failed tool call, and no successful re-run afterwards.' },
    operator: { ...shared,
      headline: 'The AI claimed completion. Claims the record backs: 0/1; 1 without proof. In the record: 1 failed command exit and 1 failed tool call, no successful re-run.',
      record: 'Not proven: before this claim the record shows 1 failed command exit and 1 failed tool call, and no successful re-run afterwards.' },
  }
  const RUN_COUNT = /\b\d+ (?:failed )?(?:commands?|runs?)\b(?! exits?\b)/g
  assert.deepEqual('2 commands failed, 1 command failed, 1 failed command exit'.match(RUN_COUNT), ['2 commands', '1 command'], 'the run-count pattern is live')
  for (const register of ['manager', 'operator']) {
    const V = R.buildVerdict(S, { register })
    const card = V.findings.find((c) => c.kind === 'command_failed_then_done')
    assert.deepEqual({ counts: [V.counts.failed_exits, V.counts.failed_tool_calls], label: V.questions[1].label, answer: V.questions[1].answer, refs: V.questions[1].refs, headline: V.headline, record: card && card.record.text }, EXPECTED[register], register)
    /* Landed, as `renderWithLog` renders it: a session with an id otherwise shows its checking sequence first. */
    const html = withSessionStorage({ [`thruline-verdict-landed:${S.id}`]: '1' }, () => renderPanel({ session: S, register, onShowProof: noop, onCopy: noop }))
    assert.ok(S.id && html.includes('ws-verdict-headline'), `[${register}] the landed verdict rendered`)
    /* Each part is read from its own element and compared exactly (commit G p3-wording fix 2, verifier pass 2 blocking):
     * the answer phrase also sits in the headline and in the card, so `html.includes(answer)` held with the answer cut
     * short, not rendered, or rendered in the wrong element. The single space before the chips is the JSX's own. */
    const CHIP = /<button type="button" class="ws-verdict-chip-ref">([^<]*)<\/button>/g
    const own = (inner) => (typeof inner === 'string' ? { text: inner.replace(CHIP, ''), chips: [...inner.matchAll(CHIP)].map((m) => m[1]) } : { text: null, chips: null })
    const questions = [...html.matchAll(/<div class="ws-verdict-question"><dt>([\s\S]*?)<\/dt><dd class="[^"]*">([\s\S]*?)<\/dd><\/div>/g)].map((m) => ({ label: m[1], ...own(m[2]) }))
    assert.deepEqual(questions.map((q) => q.label), V.questions.map((q) => q.label), `[${register}] one rendered question per question, in order`)
    const cards = [...html.matchAll(/<article class="ws-verdict-card [\s\S]*?<\/article>/g)].map((m) => m[0])
    const recordRow = cards.length === 1 ? new RegExp(`<span class="ws-verdict-rowlabel">${M.HQ_COPY.chrome.rows.record}</span><div class="ws-verdict-sentence(?: ws-verdict-inference)?">([\\s\\S]*?)</div></div>`).exec(cards[0]) : null
    assert.deepEqual({
      headline: own(html.match(/<p class="ws-verdict-headline">([\s\S]*?)<\/p>/)?.[1]).text,
      record: own(recordRow?.[1]).text,
      label: questions[1]?.label,
      answer: questions[1]?.text,
      refs: questions[1]?.chips,
    }, {
      headline: `${EXPECTED[register].headline} `,
      record: `${EXPECTED[register].record} `,
      label: EXPECTED[register].label,
      answer: `${EXPECTED[register].answer} `,
      refs: EXPECTED[register].refs,
    }, `[${register}] the headline, the card's Record sentence, and the failed-rows question's own <dt> and <dd> render exactly`)
    assert.deepEqual(html.replace(/<[^>]*>/g, ' ').match(RUN_COUNT), null, `[${register}] no number on the panel stands for commands or runs`)
  }
})

test('Fog on a real session: the served capture_degraded reaches the panel through the shell path, and Fog renders exactly when it is true (commit G p4-fog)', async () => {
  /* STEP0 audit §3.6, docs/INTERFACE-REQUESTS.md row 19: the adapter read `degraded` only from a DSSE receipt and the
   * shell passes none, so eval__02, whose receipt says capture_degraded: true, adapted to Recorded with no Fog card,
   * and no live session could show Fog. `claims --json` now serves the session view's own `capture_degraded`. The
   * captures at the narrator-baseline tag predate that field, so each is served here with the value its own captured
   * receipt carries (the premise is asserted); the next baseline capture carries it on the bytes. */
  const out = await build({ ...BUNDLE, stdin: { ...BUNDLE.stdin, contents: `
    export { Workspace } from './shell.jsx';
    export { readProjection, API } from './data.js';
    export { buildVerdict } from './verdict-model.js';
  ` } })
  const file = join(temporary, 'captured-fog.mjs')
  await writeFile(file, out.outputFiles[0].text)
  const R = await import(pathToFileURL(file).href)
  const capture = new URL('../../../docs/audit/narrator-baseline/', import.meta.url)
  const receiptSays = (name) => JSON.parse(readFileSync(new URL(`${name}/receipt-v0.1.json`, capture), 'utf8')).payload_decoded.predicate.capture.capture_degraded
  /** The Session the shell builds from served bytes: explain as captured, and claims as captured with `edit` applied. */
  const served = async (name, edit) => {
    const at = (f) => readFileSync(new URL(`${name}/${f}`, capture))
    const exits = JSON.parse(at('exit-codes.json').toString('utf8'))
    const doc = JSON.parse(at('claims.json').toString('utf8'))
    const explainUrl = R.API.explain + '?session=' + encodeURIComponent(doc.session)
    const claimsUrl = R.API.claims + '?session=' + encodeURIComponent(doc.session)
    const bytes = new Map([
      [explainUrl, { bytes: at('explain.json'), exit: exits.explain, projection: 'explain --json' }],
      [claimsUrl, { bytes: Buffer.from(JSON.stringify(edit(doc))), exit: exits.claims, projection: 'claims --json' }],
    ])
    const realFetch = globalThis.fetch
    try {
      globalThis.fetch = async (url) => {
        const hit = bytes.get(String(url))
        if (!hit) return new Response('{}', { status: 404 })
        return new Response(hit.bytes, { status: 200, headers: { 'Content-Type': 'application/json; charset=utf-8', 'X-ThruLine-Projection': hit.projection, 'X-ThruLine-Exit': String(hit.exit) } })
      }
      const [explainRead, claimsRead] = await Promise.all([R.readProjection(explainUrl), R.readProjection(claimsUrl)])
      assert.equal(explainRead.status, 'ready', name)
      assert.equal(claimsRead.status, 'ready', name)
      return R.Workspace.prototype.adapt.call(Object.create(R.Workspace.prototype), explainRead, null, null, claimsRead)
    } finally {
      globalThis.fetch = realFetch
    }
  }
  const landed = (S, register) => withSessionStorage({ [`thruline-verdict-landed:${S.id}`]: '1' }, () => renderPanel({ session: S, register, onShowProof: noop, onCopy: noop }))
  const FOG_CARD = /<article class="ws-verdict-card ws-verdict-card-fog">[\s\S]*?<\/article>/g
  const recordRow = (card) => new RegExp(`<span class="ws-verdict-rowlabel">${M.HQ_COPY.chrome.rows.record}</span><div class="(ws-verdict-sentence(?: ws-verdict-inference)?)">([\\s\\S]*?)</div></div>`).exec(card)

  /* Degraded: eval__02 as the fixed CLI serves it. */
  const DEGRADED = 'eval__02-cargo-false-claim-no-run-at-all'
  assert.equal(receiptSays(DEGRADED), true, `premise: ${DEGRADED}'s captured receipt says capture was degraded`)
  const S = await served(DEGRADED, (d) => ({ ...d, capture_degraded: true }))
  assert.equal(S.capture.degraded, true, 'the served flag reached the adapted session')
  const RECORD = {
    manager: 'Fog: part of this session was not recorded, so what happened there cannot be settled.',
    operator: 'Fog: the record for this session is incomplete — degraded. What happened in the gap cannot be settled.',
  }
  for (const register of ['manager', 'operator']) {
    const V = R.buildVerdict(S, { register })
    assert.equal(V.state, 'fog', `[${register}] session state`)
    /* No record that shows the degradation is served (the flag is a session attribute, not an event), so the Record line
     * cites nothing and stays an inference. No ref is invented for it. */
    assert.deepEqual(V.findings.map((c) => ({ kind: c.kind, state: c.state, key: c.record.key, role: c.record.role, refs: c.record.refs, proofNode: c.proofNode, text: c.record.text, fallbacks: c.record.fallbacks })),
      [{ kind: 'recording_gap', state: 'fog', key: 'verdict/recording_gap/fog', role: 'inference', refs: [], proofNode: null, text: RECORD[register], fallbacks: [] }], `[${register}] the one card is the Fog card`)
    const html = landed(S, register)
    const cards = html.match(FOG_CARD) || []
    assert.equal(cards.length, 1, `[${register}] one Fog card rendered`)
    const row = recordRow(cards[0])
    assert.deepEqual(row && [row[1], row[2]], ['ws-verdict-sentence ws-verdict-inference', `${RECORD[register]} `], `[${register}] the Fog card's Record line renders exactly, in inference styling, with no receipt chip`)
    for (const text of [V.headline, ...V.findings.flatMap((c) => [c.record.text, c.do.text])]) assert.deepEqual(wordingDefects(text), [], `[${register}] ${text}`)
  }

  /* Closed world: only a served boolean true is degraded. With the field absent (a server older than it), false, the
   * string "true", 1 or null, the same bytes adapt to not degraded, with no Fog card. */
  for (const [label, edit] of [
    ['absent', ({ capture_degraded: _absent, ...d }) => d],
    ['false', (d) => ({ ...d, capture_degraded: false })],
    ['the string "true"', (d) => ({ ...d, capture_degraded: 'true' })],
    ['1', (d) => ({ ...d, capture_degraded: 1 })],
    ['null', (d) => ({ ...d, capture_degraded: null })],
  ]) {
    const T = await served(DEGRADED, edit)
    assert.equal(T.capture.degraded, false, `capture_degraded ${label}`)
    for (const register of ['manager', 'operator']) {
      const V = R.buildVerdict(T, { register })
      assert.deepEqual({ state: V.state, cards: V.findings.map((c) => c.kind), fog: (landed(T, register).match(FOG_CARD) || []).length }, { state: 'couldnt', cards: [], fog: 0 }, `capture_degraded ${label} [${register}]`)
    }
  }

  /* Not degraded: the five claim captures, each served with the false its captured receipt carries. Their states are
   * the ones the captured-fixtures test above reads from the same bytes. */
  const CLEAN = {
    'claims__01-three-asks-one-done-one-claimed-one-untouched': 'couldnt',
    'claims__02-supported-lint-claim': 'proven',
    'claims__03-contradicted-tests-claim': 'not_proven',
    'claims__04-insufficient-evidence-tests-claim': 'couldnt',
    'claims__05-open-promise-beside-supported-build': 'loose',
  }
  for (const [name, state] of Object.entries(CLEAN)) {
    assert.equal(receiptSays(name), false, `premise: ${name}'s captured receipt says capture was not degraded`)
    const T = await served(name, (d) => ({ ...d, capture_degraded: false }))
    assert.equal(T.capture.degraded, false, name)
    for (const register of ['manager', 'operator']) {
      const V = R.buildVerdict(T, { register })
      assert.deepEqual({ state: V.state, gap: V.findings.filter((c) => c.kind === 'recording_gap').length, fogClaims: V.counts.fog, fog: (landed(T, register).match(FOG_CARD) || []).length }, { state, gap: 0, fogClaims: 0, fog: 0 }, `${name} [${register}]`)
    }
  }

  /* The same bytes with the flag true: claims__04's unverifiable claim takes Fog instead of Recorded (RSV §3.1), and
   * the Fog card joins it. A counterfactual on real bytes: the flag alone moves the reading. */
  const flipped = await served('claims__04-insufficient-evidence-tests-claim', (d) => ({ ...d, capture_degraded: true }))
  assert.deepEqual(flipped.claims.map((c) => [c.verdict, c.word]), [['unverifiable', 'fog']])
  for (const register of ['manager', 'operator']) {
    const V = R.buildVerdict(flipped, { register })
    assert.deepEqual({ state: V.state, gap: V.findings.filter((c) => c.kind === 'recording_gap').length, fogClaims: V.counts.fog, fogCards: V.findings.filter((c) => c.state === 'fog').length, fog: (landed(flipped, register).match(FOG_CARD) || []).length },
      { state: 'fog', gap: 1, fogClaims: 1, fogCards: 2, fog: 2 }, `claims__04 with the flag true [${register}]`)
  }
})

test('captured fixture through the adapter: a recorded command that ends in dots reads verbatim in its clicked body, and one that ends in a blank loses only the blank, in both registers', async () => {
  /* Fix pass 2, verifier finding 1, found through this same path: `cd ..` rendered "— Ran cd." once the whole trailing
   * run of stops was dropped. The text of the fixture's one exec event is replaced in memory; no file changes.
   * commit G p1p2-followups, p1 verifier pass 3 P3-03: with the trailing blanks of a value that has no closing stop
   * kept, `cargo build ` rendered "— Ran cargo build . Commands…" here and every test passed. */
  const out = await build({ ...BUNDLE, stdin: { ...BUNDLE.stdin, contents: `
    export { Workspace } from './shell.jsx';
    export { readProjection, API } from './data.js';
    export { buildVerdict } from './verdict-model.js';
  ` } })
  const file = join(temporary, 'captured-dots.mjs')
  await writeFile(file, out.outputFiles[0].text)
  const R = await import(pathToFileURL(file).href)
  const at = (f) => readFileSync(new URL(`../../../docs/audit/narrator-baseline/claims__05-open-promise-beside-supported-build/${f}`, import.meta.url))
  const exits = JSON.parse(at('exit-codes.json').toString('utf8'))
  const id = JSON.parse(at('claims.json').toString('utf8')).session
  const explainUrl = R.API.explain + '?session=' + encodeURIComponent(id)
  const claimsUrl = R.API.claims + '?session=' + encodeURIComponent(id)
  const original = JSON.parse(at('explain.json').toString('utf8'))
  assert.deepEqual(original.what_happened.filter((e) => e.kind === 'exec').map((e) => [e.seq, e.text]), [[24, 'cargo build → exit 0']], 'the one exec event this test rewrites')
  const lead = 'This is a command run in the terminal, with its exit code and output — '
  const tail = ' Commands are the strongest proof in the record: they either passed or they did not.'
  const realFetch = globalThis.fetch
  let checked = 0
  try {
    for (const [command, shown] of [['cd ..', 'Ran cd ..'], ['ls -la .', 'Ran ls -la .'], ['git diff main..', 'Ran git diff main..'], ['cargo build', 'Ran cargo build.'], ['cargo build ', 'Ran cargo build.']]) {
      const explain = JSON.parse(JSON.stringify(original))
      explain.what_happened.find((e) => e.seq === 24).text = command
      const served = new Map([
        [explainUrl, { bytes: Buffer.from(JSON.stringify(explain)), exit: exits.explain, projection: 'explain --json' }],
        [claimsUrl, { bytes: at('claims.json'), exit: exits.claims, projection: 'claims --json' }],
      ])
      globalThis.fetch = async (url) => {
        const hit = served.get(String(url))
        if (!hit) return new Response('{}', { status: 404 })
        return new Response(hit.bytes, { status: 200, headers: { 'Content-Type': 'application/json; charset=utf-8', 'X-ThruLine-Projection': hit.projection, 'X-ThruLine-Exit': String(hit.exit) } })
      }
      const [explainRead, claimsRead] = await Promise.all([R.readProjection(explainUrl), R.readProjection(claimsUrl)])
      assert.equal(explainRead.status, 'ready', command)
      assert.equal(claimsRead.status, 'ready', command)
      const S = R.Workspace.prototype.adapt.call(Object.create(R.Workspace.prototype), explainRead, null, null, claimsRead)
      for (const register of ['manager', 'operator']) {
        const c = R.buildVerdict(S, { register }).clicked('ev24')
        assert.deepEqual(c && { title: c.title, body: c.body.text }, { title: 'You clicked: a command the AI ran at step 24.', body: lead + shown + tail }, `[${register}] ${command}`)
        checked += 1
      }
    }
  } finally {
    globalThis.fetch = realFetch
  }
  /* 10 since commit G p1p2-followups: a fifth command, `cargo build ` with a trailing blank (P3-03), was added. */
  assert.equal(checked, 10, 'five commands, both registers')
})

/** verdict-model.js bundled over a copy of HQ_COPY that `edit(copy)` changes, swapped in the way PROBE swaps the
 * copy: the only way to reach the narrate() paths the shipped copy never takes. */
async function modelOverEditedCopy(name, edit) {
  const out = await build({
    ...BUNDLE,
    stdin: { ...BUNDLE.stdin, contents: "export { buildVerdict } from './verdict-model.js';" },
    plugins: [{
      name: 'edited-copy',
      setup(b) {
        b.onResolve({ filter: /(^|\/)hq-copy\.js$/ }, (args) => (args.namespace === 'edited-copy' ? { path: COPY_FILE } : { path: 'edited', namespace: 'edited-copy' }))
        b.onLoad({ filter: /.*/, namespace: 'edited-copy' }, () => ({
          resolveDir: dirname(COPY_FILE), loader: 'js',
          contents: `import { HQ_COPY as REAL } from './hq-copy.js'
            const copy = JSON.parse(JSON.stringify(REAL))
            ;(${edit.toString()})(copy)
            export const HQ_COPY = copy
            export default copy`,
        }))
      },
    }],
  })
  const file = join(temporary, `edited-copy-${name}.mjs`)
  await writeFile(file, out.outputFiles[0].text)
  return import(pathToFileURL(file).href)
}
const oneClaim = (verdict, over = {}) => ({
  key: 'k', harness: 'Claude', finalTree: 'T2', capture: { degraded: false, condition: 'intact' }, events: [], promises: [], checks: [], nodes: [],
  claims: [{ id: 'c1', node: 'clm', kind: 'task_complete', text: 'Thing 1 is done.', verdict, verifier: 'tests_pass@1', evidence: [], reasons: [], findings: [], ...over }],
})
const sentence = (r) => ({ key: r.key, text: r.text, refs: r.refs, prompt: r.prompt, violations: r.violations, fallbacks: r.fallbacks })

test('narrate over an edited copy: a chain key with no family and a Do prompt that cannot fill are reported fallbacks, never silent skips', async () => {
  /* Fix pass 1, verifier finding 8: both were passed over with no entry in `fallbacks`. */
  const M = await modelOverEditedCopy('silent', (copy) => {
    delete copy.families['verdict/couldnt_check/couldnt']
    copy.families['verdict/*/couldnt'].operator.prompt = 'Paste {constructor} here.'
  })
  const card = M.buildVerdict(oneClaim('unverifiable'), { register: 'operator' }).findings[0]
  assert.deepEqual(sentence(card.record), { key: 'verdict/*/couldnt', text: "Recorded: the record cannot settle it — no reason recorded.", refs: ['clm'], prompt: null, violations: [], fallbacks: [
    { key: 'verdict/couldnt_check/couldnt', violations: [{ kind: 'no_family' }] },
  ] })
  assert.deepEqual(sentence(card.do), { key: 'verdict/*', text: 'Open the proof and follow it.', refs: [], prompt: null, violations: [], fallbacks: [
    { key: 'verdict/couldnt_check/couldnt', violations: [{ kind: 'no_family' }] },
    { key: 'verdict/*/couldnt', violations: [{ kind: 'missing_slot', slot: 'constructor', part: 'prompt' }] },
  ] })
})

test('narrate over an edited copy: a missing template and an inherited slot name are reported fallbacks, and when every family fails, the sentence shown is not listed among its own fallbacks', async () => {
  const gaps = await modelOverEditedCopy('gaps', (copy) => {
    delete copy.families['verdict/couldnt_check/couldnt'].operator.record
    copy.families['verdict/said_done_no_proof/not_proven'].operator.record = 'Not proven: {constructor}.'
  })
  const couldnt = gaps.buildVerdict(oneClaim('unverifiable'), { register: 'operator' }).findings[0]
  assert.deepEqual(sentence(couldnt.record), { key: 'verdict/*/couldnt', text: "Recorded: the record cannot settle it — no reason recorded.", refs: ['clm'], prompt: null, violations: [], fallbacks: [
    { key: 'verdict/couldnt_check/couldnt', violations: [{ kind: 'no_template', part: 'record' }] },
  ] })
  const said = gaps.buildVerdict(oneClaim('unsupported'), { register: 'operator' }).findings[0]
  assert.deepEqual(sentence(said.record), { key: 'verdict/*/not_proven', text: 'Not proven: nothing in the record shows this — no reason recorded.', refs: ['clm'], prompt: null, violations: [], fallbacks: [
    { key: 'verdict/said_done_no_proof/not_proven', violations: [{ kind: 'missing_slot', slot: 'constructor' }] },
  ] })

  const allFail = await modelOverEditedCopy('all-fail', (copy) => {
    copy.families['verdict/*'].operator.record = 'The record was read for this claim — {reason}.'
  })
  const banned = allFail.buildVerdict(oneClaim('unverifiable', { reasons: ['lazy evaluation not recorded'] }), { register: 'operator' }).findings[0]
  const lazy = [{ kind: 'banned_token', token: 'lazy', word: 'lazy' }]
  assert.deepEqual(sentence(banned.record), { key: 'verdict/*', text: 'The record was read for this claim — lazy evaluation not recorded.', refs: ['clm'], prompt: null, violations: lazy, fallbacks: [
    { key: 'verdict/couldnt_check/couldnt', violations: lazy },
    { key: 'verdict/*/couldnt', violations: lazy },
  ] })
})

test('no Re-check when the tree has not moved, and the zero state does not look healthy', () => {
  const still = { ...SESSION, capture: { degraded: false }, checks: [{ id: 'k1', node: 'chk', runner: 'cargo test', exit: 1, inputTree: 'T2' }] }
  assert.doesNotMatch(renderPanel({ session: still, onRecheck: noop }), /Re-check/)
  const zero = renderPanel({ session: { key: 'z', events: [], claims: [], promises: [], checks: [], nodes: [], capture: { degraded: false } } })
  assert.match(zero, /Recorded/)
  assert.match(zero, /reported nothing as done/)
  assert.doesNotMatch(zero, /ws-verdict-dot-proven|What went wrong|Everything holds up/)
})

/* ── quotations on screen (STEP0 audit §1.2, §3.4; commit G, p2-quotations) ──
 * The Said row rendered `<q>{card.said.quote}</q>` with no grounding, and the clicked body rendered a claim's text and
 * the user's own prompt inside a sentence with `role="grounded"` written into the panel. A quotation renders in a
 * `<q class="ws-verdict-quote">` marked with whose words they are; the words around it are the grounded sentence. */
const QUOTE_PROBE = 'The agent lied; all 14 tests pass [c9] {n}'
const saidRows = (html) => [...html.matchAll(/<div class="ws-verdict-row"><span class="ws-verdict-rowlabel">Said<\/span>(<div class="ws-verdict-sentence[^"]*">[\s\S]*?<\/div>)<\/div>/g)].map((m) => m[1])
const clickedSentence = (html) => {
  const m = html.match(/<section class="ws-verdict-clicked"[^>]*><div class="ws-verdict-section-head"><h3>[^<]*<\/h3><\/div>(<div class="ws-verdict-sentence[^"]*">[\s\S]*?<\/div>)<button/)
  assert.ok(m, 'the clicked section renders')
  return m[1]
}
const occurrences = (html, s) => html.split(s).length - 1

test('the Said row renders the said template\'s own words as the sentence and the claim text as a marked quotation of the agent\'s words, verbatim; words that are not the record\'s render no quotation', () => {
  const base = { key: 'q', harness: 'Claude', finalTree: 'T2', capture: { degraded: false, condition: 'intact' }, events: [], promises: [], checks: [], nodes: [] }
  const probeClaim = { id: 'c1', node: 'clm', kind: 'task_complete', text: QUOTE_PROBE, verdict: 'unsupported', verifier: 't@1', seq: 3, evidence: [], reasons: [], findings: [] }
  const one = { ...base, claims: [probeClaim] }
  /* The label names the agent in the register's own word: Technical's read "The AI's words" until commit G p1p2-followups
   * (p2 coder loose thread 3), so the operator markup's exact value changes for that reason alone. */
  const Q = `<q class="ws-verdict-quote" data-quote="agent" title="The AI&#x27;s words, as recorded">${QUOTE_PROBE}</q>`
  const QT = `<q class="ws-verdict-quote" data-quote="agent" title="The agent&#x27;s words, as recorded">${QUOTE_PROBE}</q>`
  assert.deepEqual(saidRows(renderPanel({ session: one, onShowProof: noop, onCopy: noop })), [`<div class="ws-verdict-sentence">The AI said this was done at step 3: ${Q} <button type="button" class="ws-verdict-chip-ref">clm</button></div>`])
  assert.deepEqual(saidRows(renderPanel({ session: one, register: 'operator', onShowProof: noop, onCopy: noop })), [`<div class="ws-verdict-sentence">The agent wrote: ${QT} — step 3 <button type="button" class="ws-verdict-chip-ref">clm</button></div>`])
  for (const register of ['manager', 'operator']) {
    const html = renderPanel({ session: one, register, onShowProof: noop, onCopy: noop })
    const marked = register === 'operator' ? QT : Q
    assert.equal(occurrences(html, marked), 1, `[${register}] one quotation of the agent's words`)
    assert.equal(occurrences(html, QUOTE_PROBE), occurrences(html, marked), `[${register}] the agent's words render outside their quotation`)
  }
  /* The second claim shares the first one's ref: its words are not that record's, so no quotation renders for it. Since
   * commit G p1p2-followups every Plain said template quotes, so Plain reaches the Said line that quotes nothing, and that
   * line points at no record (p2 coder loose thread 1, p2 F5); it read "The AI reported task complete at step 3.". */
  const shared = { ...base, claims: [{ ...probeClaim, id: 'c0', text: 'Thing 1 is done.', seq: 2 }, { ...probeClaim, id: 'c2' }] }
  const sharedHtml = renderPanel({ session: shared, onShowProof: noop, onCopy: noop })
  assert.deepEqual(saidRows(sharedHtml), [
    '<div class="ws-verdict-sentence">The AI wrote about this at step 3; its words are not shown here. <button type="button" class="ws-verdict-chip-ref">clm</button></div>',
    '<div class="ws-verdict-sentence">The AI said this was done at step 2: <q class="ws-verdict-quote" data-quote="agent" title="The AI&#x27;s words, as recorded">Thing 1 is done.</q> <button type="button" class="ws-verdict-chip-ref">clm</button></div>',
  ])
  assert.equal(sharedHtml.includes(QUOTE_PROBE), false, 'the unmatched words are nowhere in the panel')
  /* No ref at all: no quotation, no chip, and the inference styling. */
  const noRefHtml = renderPanel({ session: { ...base, claims: [{ ...probeClaim, node: undefined }] }, register: 'operator', onShowProof: noop, onCopy: noop })
  /* It read "…; see the record." with no chip behind it until commit G p1p2-followups (p2 verifier F5). */
  assert.deepEqual(saidRows(noRefHtml), ['<div class="ws-verdict-sentence ws-verdict-inference">The agent wrote about this at step 3; its words are not shown here. </div>'])
  assert.equal(noRefHtml.includes(QUOTE_PROBE), false)
})

test('the clicked body renders the user\'s own prompt and ask as a marked quotation of their words, never as the grounded sentence; with no record behind the node it renders no quotation', () => {
  const session = {
    key: 'u', harness: 'Claude', finalTree: 'T2', capture: { degraded: false, condition: 'intact' }, claims: [], promises: [], checks: [],
    events: [{ seq: 8, kind: 'user_prompt', text: QUOTE_PROBE }],
    asks: [{ id: 'a1', node: 'ask1', kind: 'feature', text: QUOTE_PROBE }],
    nodes: [
      { id: 'ev8', objType: 'event', kind: 'user_prompt', seq: 8, title: QUOTE_PROBE, lines: [] },
      { id: 'ask1', objType: 'ask', title: `“${QUOTE_PROBE}”`, lines: [] },
      { id: 'ev9', objType: 'event', kind: 'user_prompt', seq: 9, title: QUOTE_PROBE, lines: [] },
    ],
  }
  const UQ = `<q class="ws-verdict-quote" data-quote="user" title="Your words, as recorded">${QUOTE_PROBE}</q>`
  /* A user prompt is the timeline's text: the message's first line, shortened when long. Its label said "Your words, as
   * recorded" until commit G p1p2-followups (p2 verifier F3); the ask, whose text is not cut that way, keeps that label. */
  const UQX = `<q class="ws-verdict-quote" data-quote="user" title="The first line of your message, shortened if it was long">${QUOTE_PROBE}</q>`
  for (const register of ['manager', 'operator']) {
    const ev8 = renderPanel({ session, register, selected: 'ev8', onShowProof: noop, onCopy: noop })
    assert.equal(clickedSentence(ev8), `<div class="ws-verdict-sentence">This is your own message to the AI: ${UQX}. What you asked for is measured against what the AI reported done. <button type="button" class="ws-verdict-chip-ref">ev8</button></div>`, register)
    const ask = renderPanel({ session, register, selected: 'ask1', onShowProof: noop, onCopy: noop })
    assert.equal(clickedSentence(ask), `<div class="ws-verdict-sentence">This is a request from you: ${UQ}. ThruLine measures what the AI reported done against it. <button type="button" class="ws-verdict-chip-ref">ask1</button></div>`, register)
    for (const [html, marked] of [[ev8, UQX], [ask, UQ]]) {
      assert.equal(occurrences(html, marked), 1, `[${register}] one quotation of the user's words`)
      assert.equal(occurrences(html, QUOTE_PROBE), 1, `[${register}] the user's words render outside their quotation`)
    }
    const ev9 = renderPanel({ session, register, selected: 'ev9', onShowProof: noop, onCopy: noop })
    assert.equal(clickedSentence(ev9), '<div class="ws-verdict-sentence">This is your own message to the AI. What you asked for is measured against what the AI reported done. <button type="button" class="ws-verdict-chip-ref">ev9</button></div>', register)
    assert.equal(ev9.includes(QUOTE_PROBE), false, `[${register}] the node's title does not stand in for a record`)
  }
})

/** VerdictPanel bundled over verdict-model.js with `wrap(V)` applied to what buildVerdict returns: the way to hand the
 * panel a model output the shipped model does not produce, and so to see whether the panel reads a field or writes it. */
async function panelOverWrappedModel(name, wrap) {
  const MODEL_FILE = join(dirname(fileURLToPath(import.meta.url)), 'verdict-model.js')
  const out = await build({
    ...BUNDLE,
    stdin: { ...BUNDLE.stdin, contents: "import React from 'react'; import { renderToStaticMarkup } from 'react-dom/server'; import { VerdictPanel } from './verdict-panel.jsx'; export const renderPanel = (props) => renderToStaticMarkup(React.createElement(VerdictPanel, props));" },
    plugins: [{
      name: 'wrapped-model',
      setup(b) {
        b.onResolve({ filter: /(^|\/)verdict-model\.js$/ }, (args) => (args.namespace === 'wrapped-model' ? { path: MODEL_FILE } : { path: 'wrapped', namespace: 'wrapped-model' }))
        b.onLoad({ filter: /.*/, namespace: 'wrapped-model' }, () => ({
          resolveDir: dirname(MODEL_FILE), loader: 'js',
          contents: `import { buildVerdict as real } from './verdict-model.js'
            export const buildVerdict = (s, o) => (${wrap.toString()})(real(s, o))
            export default buildVerdict`,
        }))
      },
    }],
  })
  const file = join(temporary, `wrapped-model-${name}.mjs`)
  await writeFile(file, out.outputFiles[0].text)
  return import(pathToFileURL(file).href)
}

test('the clicked body\'s inference styling follows the role the model grounded it with, never a role the panel writes', async () => {
  const wrapped = await panelOverWrappedModel('clicked-role', (V) => ({ ...V, clicked: (id) => { const c = V.clicked(id); return c && { ...c, body: { ...c.body, role: 'inference' } } } }))
  const body = 'This is a command run in the terminal, with its exit code and output — cargo test → exit 1. Commands are the strongest proof in the record: they either passed or they did not. <button type="button" class="ws-verdict-chip-ref">ev5</button>'
  assert.equal(clickedSentence(renderPanel({ session: SESSION, selected: 'ev5', onShowProof: noop })), `<div class="ws-verdict-sentence">${body}</div>`, 'grounded by the model: no inference class')
  assert.equal(clickedSentence(wrapped.renderPanel({ session: SESSION, selected: 'ev5', onShowProof: noop })), `<div class="ws-verdict-sentence ws-verdict-inference">${body}</div>`, 'the same body with the model\'s role set to inference: the class follows the role, chip and all')
  assert.doesNotMatch(readFileSync(new URL('./verdict-panel.jsx', import.meta.url), 'utf8'), /role="grounded"/, 'no role written into the panel')
})

test('narrate over an edited copy: a quotation licenses no number outside itself — a said template that writes the quote\'s own number is refused', async () => {
  const edited = await modelOverEditedCopy('said-licence', (copy) => {
    copy.families['verdict/said_done_no_proof/not_proven'].operator.said = 'The agent wrote 14 words: "{claim_quote}" — {when}'
  })
  const said = edited.buildVerdict(oneClaim('unsupported', { text: QUOTE_PROBE, seq: 3 }), { register: 'operator' }).findings[0].said
  assert.deepEqual({ key: said.key, text: said.text, parts: said.parts, quote: said.quote, role: said.role, refs: said.refs, violations: said.violations, fallbacks: said.fallbacks }, {
    key: 'verdict/*/not_proven', text: `The agent wrote: "${QUOTE_PROBE}"`,
    parts: [{ text: 'The agent wrote: ' }, { quote: QUOTE_PROBE, speaker: 'agent' }],
    quote: QUOTE_PROBE, role: 'grounded', refs: ['clm'], violations: [],
    fallbacks: [{ key: 'verdict/said_done_no_proof/not_proven', violations: [{ kind: 'unlicensed_number', number: '14' }] }],
  })
})

/* ── commit G p1p2-followups: the p1 and p2 verifiers' minor findings ── */

test('selecting a node whose kind or type is an Object.prototype name, or the link label, renders the panel with no clicked section and no throw', () => {
  /* p2 verifier F8: `V.clicked` threw "fill: template must be a string" for these, and took the panel with it. */
  const KINDS = ['constructor', 'toString', '__proto__', 'hasOwnProperty', 'link']
  const nodes = [...KINDS.map((kind, i) => ({ id: `ev${i + 1}`, objType: 'event', kind, seq: i + 1, title: 't', lines: [] })), ...KINDS.map((kind, i) => ({ id: `ob${i + 1}`, objType: kind, title: 't', lines: [] }))]
  const session = { key: 'proto', harness: 'Claude', capture: { degraded: false }, events: [], claims: [], promises: [], checks: [], nodes }
  let rendered = 0
  for (const register of ['manager', 'operator']) {
    for (const n of nodes) {
      const html = renderPanel({ session, register, selected: n.id, onShowProof: noop })
      assert.match(html, /<div class="ws-verdict">/, `[${register}] ${n.id}: the panel renders`)
      assert.doesNotMatch(html, /ws-verdict-clicked/, `[${register}] ${n.id}: no clicked section`)
      rendered += 1
    }
  }
  assert.equal(rendered, 20, 'ten nodes, both registers')
})

/** A style sheet's rules as { selector, decls }. Comments are dropped; a nested block (@media, @keyframes) yields its inner rules. */
function cssRules(css) {
  return [...css.replace(/\/\*[\s\S]*?\*\//g, '').matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
    selector: m[1].trim(),
    decls: Object.fromEntries(m[2].split(';').map((d) => d.trim()).filter(Boolean).map((d) => [d.slice(0, d.indexOf(':')).trim(), d.slice(d.indexOf(':') + 1).trim()])),
  }))
}

/** The selector of every style rule that holds a block of its own (CSS nesting). `cssRules` reads innermost blocks only,
 * so it loses such a rule's own selector and declarations. A block inside an at-rule (@media, @keyframes) is not
 * nesting. A brace inside a string or after a backslash is not a block. */
function nestedRules(css) {
  const out = []
  const stack = []
  let prelude = ''
  let quote = null
  let escaped = false
  for (const ch of css.replace(/\/\*[\s\S]*?\*\//g, '')) {
    if (escaped) { escaped = false; prelude += ch; continue }
    if (ch === '\\') { escaped = true; prelude += ch; continue }
    if (quote) { if (ch === quote) quote = null; prelude += ch; continue }
    if (ch === '"' || ch === "'") { quote = ch; prelude += ch; continue }
    if (ch === '{') {
      const outer = stack[stack.length - 1]
      if (outer !== undefined && !outer.startsWith('@') && !out.includes(outer)) out.push(outer)
      stack.push(prelude.trim().replace(/\s+/g, ' '))
      prelude = ''
    } else if (ch === '}') { stack.pop(); prelude = '' } else if (ch === ';') prelude = ''
    else prelude += ch
  }
  return out
}

/** Splits at depth 0, outside brackets, parentheses and quoted strings. */
function splitTop(s, sep) {
  const out = []
  let depth = 0
  let cur = ''
  let quote = null
  for (const ch of s) {
    if (quote) { cur += ch; if (ch === quote) quote = null; continue }
    if (ch === '"' || ch === "'") quote = ch
    else if (ch === '[' || ch === '(') depth += 1
    else if (ch === ']' || ch === ')') depth -= 1
    if (depth === 0 && sep.test(ch)) { out.push(cur); cur = ''; continue }
    cur += ch
  }
  out.push(cur)
  return out.map((x) => x.trim()).filter(Boolean)
}

/** Whether a selector could reach `element` ({ type, classes, attributes }). Ruled out only when the selector's subject
 * names a type, a class, an id or an attribute the element does not have; anything this reader does not recognise keeps
 * the rule in. At module scope so the reach test and the gap test below measure the same reader. */
function reachesElement(selector, element) {
  if (selector.startsWith('@') || /^(from|to|\d+(\.\d+)?%)$/.test(selector)) return false
  /* An escape or a namespace can spell any name, and this reader decodes neither: a backslash or a `|` keeps the rule in. */
  if (/[\\|]/.test(selector)) return true
  const subject = splitTop(selector, /[\s>+~]/).pop()
  for (const t of subject.match(/::?[\w-]+(\((?:[^()]|\([^()]*\))*\))?|\.[\w-]+|#[\w-]+|\[[^\]]*\]|\*|[a-zA-Z][\w-]*|./g) || []) {
    if (t === '*' || t.startsWith(':')) continue
    /* A class is compared without case: that keeps in more than a no-quirks document matches, never less. */
    if (t.startsWith('.')) { if (!element.classes.includes(t.slice(1).toLowerCase())) return false; continue }
    if (t.startsWith('#')) return false
    if (t.startsWith('[')) {
      /* Ruled out only when read whole: `[name]` or `[name op value flag]`, with blanks where CSS allows them, and the
       * name compared without case, as an HTML document compares it. Anything else in brackets keeps the rule in. */
      const attribute = t.match(/^\[\s*([a-zA-Z_-][\w-]*)\s*(?:[~^$*]?=\s*(?:"[^"]*"|'[^']*'|[\w-]+)\s*(?:[iIsS]\s*)?)?\]$/)
      if (!attribute) return true
      if (!element.attributes.includes(attribute[1].toLowerCase())) return false
      continue
    }
    if (/^[a-zA-Z]/.test(t)) { if (t.toLowerCase() !== element.type) return false; continue }
    return true
  }
  return true
}

/** The element the panel renders for a quotation, read off the rendered panel. */
function quotationElement() {
  const html = renderPanel({ session: { key: 'css', harness: 'Claude', capture: { degraded: false }, events: [], promises: [], checks: [], nodes: [], claims: [{ id: 'c1', node: 'clm', kind: 'task_complete', text: 'Thing 1 is done.', verdict: 'unsupported', verifier: 't@1', seq: 3, evidence: [], reasons: [], findings: [] }] }, onShowProof: noop, onCopy: noop })
  const tags = html.match(/<q\b[^>]*>/g) || []
  return { html, tags }
}

test('a quotation is marked by more than a colour close to its sentence, and nothing in its mark is the inference style or a receipt chip', () => {
  /* p2 verifier F4: the quotation was #f1f4f9 beside its sentence's #dfe5ef, both upright, so inside a grounded line only
   * its quotation marks set it apart; and removing its rule left the whole suite green (break V19). P6-6: a quotation is
   * the agent's words, and §4 gives the agent's words violet — violet ink on a violet 10 % tint behind a violet bar. */
  const rules = cssRules(readFileSync(new URL('./workspace.css', import.meta.url), 'utf8'))
  const one = (selector) => {
    const hit = rules.filter((r) => r.selector === selector)
    assert.equal(hit.length, 1, `one rule for ${selector}`)
    return hit[0].decls
  }
  assert.deepEqual(rules.filter((r) => r.selector.includes('ws-verdict-quote')), [{ selector: '.ws-verdict-sentence q.ws-verdict-quote', decls: {
    'font-style': 'normal', color: 'var(--ws-violet)', background: 'var(--ws-violet-tint)', 'border-left': '2px solid var(--ws-violet)', padding: '0 4px', 'border-radius': '2px',
    '-webkit-box-decoration-break': 'clone', 'box-decoration-break': 'clone',
  } }])
  const quote = one('.ws-verdict-sentence q.ws-verdict-quote')
  const sentence = one('.ws-verdict-sentence')
  const inference = one('.ws-verdict-inference')
  const chip = one('.ws-hq-content .ws-verdict-chip-ref')
  assert.equal(inference['font-style'], 'italic')
  assert.equal(quote['font-style'], 'normal', 'upright, never the inference italic, even inside an inference line')
  assert.ok(![sentence.color, inference.color, chip.color].includes(quote.color), 'a colour of its own')
  assert.ok(quote.background && quote['border-left'], 'a ground and a bar: more than a colour')
  assert.equal(sentence.background, undefined, 'the sentence around it has no ground of its own')
  assert.notEqual(quote.background, chip.background, 'not a receipt chip\'s ground')
  assert.equal(quote['font-family'], undefined, 'not a receipt chip\'s monospace')
  /* Nothing else in the sheet restyles a quotation inside a verdict line. */
  assert.deepEqual(rules.filter((r) => r.selector.includes('ws-verdict') && /(^|\s)q(\.|\s|$)/.test(r.selector)).map((r) => r.selector), ['.ws-verdict-sentence q', '.ws-verdict-sentence q.ws-verdict-quote'])
  /* The rule selects what the panel renders: a <q class="ws-verdict-quote"> directly inside a .ws-verdict-sentence. */
  const html = renderPanel({ session: { key: 'css', harness: 'Claude', capture: { degraded: false }, events: [], promises: [], checks: [], nodes: [], claims: [{ id: 'c1', node: 'clm', kind: 'task_complete', text: 'Thing 1 is done.', verdict: 'unsupported', verifier: 't@1', seq: 3, evidence: [], reasons: [], findings: [] }] }, onShowProof: noop, onCopy: noop })
  assert.match(html, /<div class="ws-verdict-sentence">[^<]*<q class="ws-verdict-quote" /)
})

test('no rule this reader parses out of the sheets the app loads reaches a quotation but the two pinned here; the CSS shapes it cannot parse are named in its comment and measured by the test after it', () => {
  /* p1p2-followups pass-1 verifier, break F6b: `.ws-hq-content q[class] { background: none; border-left: 0; }`, appended
   * to workspace.css, erased the quotation's ground and bar in the rendered panel, and the test above, which reads only
   * selectors that name ws-verdict, stayed green. This test reads statically, with no browser. It matches the subject of
   * every selector, in every sheet the app loads, against the element the panel renders. A rule is ruled out only when
   * its subject names a type, a class, an id or an attribute the quotation does not have. A pseudo-class, a
   * pseudo-element or anything this reader does not recognise keeps the rule in, so a rule it cannot rule out is pinned
   * here and never passed. Pass-2 verifier, breaks K6a, K6b and K6d: `q[CLASS]`, `q[ class ]` and `q[data\-quote]` were
   * ruled out, yet each erased the mark in Chrome. So names are compared without case, an attribute selector is ruled out
   * only when it is read whole, a backslash or a `|` anywhere keeps the rule in, and a sheet that nests a style rule in a
   * style rule fails. Each pinned rule was read: none sets the quotation's ground, bar, padding, colour or style on
   * the element except its own two rules. Specificity and order are not computed: a rule that reaches is pinned even if it
   * loses.
   *
   * What this reader does not parse, named so the title above stays literal — it claims the rules this reader parses,
   * and no more. A style a script injects at run time (none today); an @layer statement before a rule, which folds into
   * the next selector; a duplicate declaration inside one block, where the later value hides an earlier erasing
   * `!important` one; a comment marker inside a string, which swallows the rules that follow it; and bare declarations
   * inside @scope. The follow-ups verifier appended each of those four to a sheet the app loads and watched the
   * quotation's mark disappear in Chrome while this suite stayed green (its pass-3 entry, breaks C1, C1acc, C2, C3 and
   * C4). The test after this one measures that this reader misses them, so widening the reader turns a row there red. */
  const at = (p) => fileURLToPath(new URL(`../../${p}`, import.meta.url))
  const sheets = new Set()
  for (const f of readdirSync(at('src'), { recursive: true })) {
    if (!/\.(jsx?|mjs)$/.test(f) || /\.test\./.test(f)) continue
    /* HQ Next (src/hq/) is its own document: main.jsx mounts it OR the workspace this panel lives in, never both, so
     * its sheets never share a page with the quotation. index.html's links are still read below, for both. */
    if (f.startsWith('hq/')) continue
    for (const m of readFileSync(join(at('src'), f), 'utf8').matchAll(/import\s*(?:\(\s*)?['"](\.{1,2}\/[^'"]+\.css)['"]/g)) sheets.add(join('src', dirname(f), m[1]))
  }
  const page = readFileSync(at('index.html'), 'utf8')
  assert.equal(/<style[\s>]|\sstyle=/i.test(page), false, 'index.html carries no inline style')
  for (const tag of page.match(/<link\b[^>]*>/gi) || []) {
    if (!/stylesheet/i.test(tag)) continue
    const href = tag.match(/\shref="\/([^"]+)"/)
    assert.ok(href, `a stylesheet link this test can read: ${tag}`)
    sheets.add(join('public', href[1]))
  }
  assert.deepEqual([...sheets].sort(), ['public/fonts/geist/gf/geist.css', 'public/fonts/plex.css', 'src/acceptance.css', 'src/workspace/workspace.css'], 'every stylesheet the app loads: index.html, main.jsx → shell.jsx, and acceptance.jsx, which main.jsx imports into the same document')

  /* The element the panel renders for a quotation. */
  const { tags } = quotationElement()
  assert.deepEqual(tags.map((t) => [...t.matchAll(/\s([\w-]+)="/g)].map((m) => m[1])), [['class', 'data-quote', 'title']], 'one quotation, with these attributes')
  assert.deepEqual(tags[0].match(/\sclass="([^"]*)"/)[1].split(/\s+/), ['ws-verdict-quote'])
  const QUOTE = { type: 'q', classes: ['ws-verdict-quote'], attributes: ['class', 'data-quote', 'title'] }

  /* `splitTop` and `reachesElement` are at module scope, so the gap test below measures this exact reader. */
  const reaches = (selector) => reachesElement(selector, QUOTE)
  /* The reader is a live instrument: it keeps what could reach the quotation, and rules out only what cannot. */
  assert.deepEqual(['.ws-hq-content q[class]', '.x q', 'Q', '[data-quote]', '.ws-hq-content :is(p, q)', '.x *', '.x ::selection', 'q:not(.a)', '.x > .ws-verdict-quote', '*|q'].map(reaches), Array(10).fill(true))
  assert.deepEqual(['.x p', '.x q.other', 'blockquote.ws-verdict-quote', 'q#id', '.x [hidden]', '.ws-verdict-quote p', '@font-face', '50%'].map(reaches), Array(8).fill(false))
  /* p1p2-followups pass-2 verifier, breaks K6a, K6b and K6d: `q[CLASS]`, `q[ class ]` and `q[data\-quote]` erased the
   * quotation's ground and bar in Chrome, and the reader ruled all three out. An HTML document matches an attribute name
   * without case and allows blanks inside the brackets; an escape or a namespace can spell any name. */
  const KEPT = ['.x q[CLASS]', '.x q[ class ]', '.x q[data\\-quote]', '.x q[TITLE]', '.x q[ data-quote = "agent" ]', '.x q[Data-Quote|="agent"]', '.x q[*|class]', '.x q[title="a]b"]', '.x .ws-verdict\\-quote', '.x .ws\\2d verdict-quote', '.x .WS-VERDICT-QUOTE', '.x \\71', '.x html|q']
  assert.deepEqual(KEPT.map((s) => [s, reaches(s)]), KEPT.map((s) => [s, true]))
  const OUT = ['.x q[HIDDEN]', '.x q[ lang ]', '.x blockquote[ class ]', '.x q[ hidden = "" i ]', '.x p[CLASS]', '.x q.OTHER']
  assert.deepEqual(OUT.map((s) => [s, reaches(s)]), OUT.map((s) => [s, false]))
  /* A style rule nested in a style rule hides its own selector from `cssRules`, which reads innermost blocks only:
   * `.x q { background: none; & .y {} }` reads as the one selector `background: none; & .y`. */
  assert.deepEqual(nestedRules('.a q { color: red; & .x { color: blue; } } @media (x) { .b { color: red; } } @keyframes k { from { top: 0; } } .c { content: "{"; }'), ['.a q'])

  const reach = {}
  for (const sheet of [...sheets].sort()) {
    const css = readFileSync(at(sheet), 'utf8')
    assert.equal(/@import/.test(css), false, `${sheet} imports no other sheet`)
    assert.deepEqual(nestedRules(css), [], `${sheet} nests no style rule inside a style rule, which this reader cannot read`)
    reach[sheet] = cssRules(css).filter((r) => splitTop(r.selector, /,/).some(reaches)).map((r) => ({ selector: r.selector.replace(/\s+/g, ' '), decls: r.decls }))
  }
  assert.deepEqual(reach, {
    'public/fonts/geist/gf/geist.css': [],
    'public/fonts/plex.css': [],
    'src/acceptance.css': [{ selector: '.accept-page *', decls: { 'box-sizing': 'border-box' } }],
    'src/workspace/workspace.css': [
      /* P6-6: the focus ring reaches all three screen roots, in the §4 focus token. */
      { selector: '.ws-root *:focus-visible, .ws-change *:focus-visible, .ws-inbox *:focus-visible', decls: { outline: 'var(--ws-focus-width) solid var(--ws-focus)', 'outline-offset': 'var(--ws-focus-offset)' } },
      { selector: '.ws-root ::-webkit-scrollbar', decls: { width: '9px', height: '9px' } },
      { selector: '.ws-root ::-webkit-scrollbar-thumb', decls: { background: 'var(--ws-hairline-strong)', 'border-radius': '2px' } },
      { selector: '.ws-root ::-webkit-scrollbar-track', decls: { background: 'transparent' } },
      { selector: '.ws-root ::selection', decls: { background: 'var(--ws-hairline-strong)' } },
      /* P6-6: reduced motion reaches all three screen roots. */
      { selector: '.ws-root *, .ws-root *::before, .ws-root *::after, .ws-change *, .ws-change *::before, .ws-change *::after, .ws-inbox *, .ws-inbox *::before, .ws-inbox *::after', decls: { transition: 'none !important', animation: 'none !important' } },
      { selector: '.ws-appbar > *', decls: { 'flex-shrink': '0' } },
      { selector: '.ws-checks-panel *, .ws-checks-dialog *', decls: { 'box-sizing': 'border-box' } },
      { selector: '.ws-checks-panel :is(button, summary, select):focus-visible', decls: { outline: 'var(--ws-focus-width) solid var(--ws-focus)', 'outline-offset': 'var(--ws-focus-offset)' } },
      { selector: '.ws-verdict-sentence q', decls: { quotes: "'“' '”'" } },
      { selector: '.ws-verdict-sentence q.ws-verdict-quote', decls: {
        'font-style': 'normal', color: 'var(--ws-violet)', background: 'var(--ws-violet-tint)', 'border-left': '2px solid var(--ws-violet)', padding: '0 4px', 'border-radius': '2px',
        '-webkit-box-decoration-break': 'clone', 'box-decoration-break': 'clone',
      } },
    ],
  })
})

test('the stylesheet reader\'s blind spots, measured: four CSS shapes this reader does not report, and a title that claims no more than it reads (commit G, integrated finding 3)', () => {
  /* The follow-ups verifier refuted the reach test's universal claim three passes running. In Chrome 152 it appended
   * each shape below to a sheet the app loads, watched the quotation's ground and bar disappear, and the suite stayed
   * green (its pass-3 entry, breaks C1, C1acc, C2, C3, C4). That measurement is the verifier's, in a browser; this test
   * neither repeats it nor claims anything about what Chrome applies. It measures the other half — that this reader
   * does not report those shapes — so the title above can be read literally, and so a later widening of the reader
   * turns one of these rows red instead of passing unnoticed. Why each is missed:
   *   - an @layer statement before a rule: `cssRules` has no statement terminator, so it folds `@layer x;` into the
   *     next selector, and a selector starting with `@` is ruled out;
   *   - a duplicate declaration: `Object.fromEntries` keeps the later one, so an erasing `!important` declaration
   *     earlier in the same block is invisible to the declaration pin, whatever its selector;
   *   - a comment marker inside a string: the comment stripper is not string-aware, so every rule between a `content`
   *     value that opens a comment and one that closes it is swallowed;
   *   - bare declarations inside @scope: the reader treats every `@` prelude as an at-rule with no style of its own. */
  const { tags } = quotationElement()
  const QUOTE = { type: 'q', classes: ['ws-verdict-quote'], attributes: ['class', 'data-quote', 'title'] }
  assert.deepEqual(tags.map((t) => [...t.matchAll(/\s([\w-]+)="/g)].map((m) => m[1])), [QUOTE.attributes], 'the same element the reach test reads')
  /* What the reach test reports for a sheet: every rule it parses whose selector it cannot rule out. */
  const reported = (css) => cssRules(css).filter((r) => splitTop(r.selector, /,/).some((s) => reachesElement(s, QUOTE))).map((r) => r.selector.replace(/\s+/g, ' '))
  const RULE = '.ws-root .ws-hq-content q[title] { background: none; border-left: 0; }'
  /* The instrument works: written plainly, that rule is reported. Without this row the four below prove nothing. */
  assert.deepEqual(reported(RULE), ['.ws-root .ws-hq-content q[title]'], 'the reader does not see a plain erasing rule')

  assert.deepEqual(reported(`@layer verifier;\n${RULE}`), [], 'C1 (@layer statement) is now reported: widen the reach test\'s title, or this row')
  assert.deepEqual(cssRules(`@layer verifier;\n${RULE}`).map((r) => r.selector.replace(/\s+/g, ' ')), ['@layer verifier; .ws-root .ws-hq-content q[title]'], 'C1: the statement folds into the next selector')

  const duplicated = '.ws-verdict-sentence q.ws-verdict-quote { background: none !important; border-left: 0 !important; background: #1a2540; border-left: 2px solid #708aae; }'
  assert.deepEqual(cssRules(duplicated)[0].decls, { background: '#1a2540', 'border-left': '2px solid #708aae' }, 'C2: an erasing duplicate declaration is now visible to the declaration pin')

  assert.deepEqual(reported(`.verifier-none::after { content: "/*"; }\n${RULE}\n.verifier-none::before { content: "*/"; }`), [], 'C3 (comment marker inside a string) is now reported')
  assert.deepEqual(reported('@scope (q[title]) { background: none !important; border-left: 0 !important; }'), [], 'C4 (@scope) is now reported')

  /* And the title above claims no more than that. */
  const source = readFileSync(new URL('./verdict-panel.test.js', import.meta.url), 'utf8')
  const titles = [...source.matchAll(/^test\('((?:[^'\\]|\\.)*)'/gm)].map((m) => m[1].replace(/\\'/g, "'"))
  const reachTitle = titles.filter((t) => t.includes('pinned here'))
  assert.deepEqual(reachTitle, ['no rule this reader parses out of the sheets the app loads reaches a quotation but the two pinned here; the CSS shapes it cannot parse are named in its comment and measured by the test after it'], 'the reach test\'s title')
  for (const universal of [/whatever its selector/i, /no rule in any stylesheet/i, /every rule in every/i]) {
    assert.equal(titles.some((t) => universal.test(t)), false, `a test title in this file claims coverage this reader does not have: ${universal}`)
  }
  /* The gap is named where the reader is used, not only here. */
  const start = source.indexOf(reachTitle[0])
  const body = source.slice(start, source.indexOf('\ntest(', start))
  for (const shape of ['@layer', '@scope', 'duplicate declaration', 'a comment marker inside a string']) {
    assert.ok(body.includes(shape), `the reach test's comment does not name a shape it cannot parse: ${shape}`)
  }
})

test('a quotation\'s label names whose words they are in the register\'s own word, and a user prompt, which is the first line of the message shortened when long, is never labelled as the words in full', () => {
  /* p2 coder loose thread 3 ("The AI's words" in Technical) and p2 verifier F3 (a clipped, first-line user prompt
   * titled "Your words, as recorded"). */
  const session = {
    key: 'lbl', harness: 'Claude', finalTree: 'T2', capture: { degraded: false, condition: 'intact' }, checks: [], promises: [],
    events: [{ seq: 8, kind: 'user_prompt', text: 'Three things for the demo crate, please:' }],
    claims: [{ id: 'c1', node: 'clm', kind: 'task_complete', text: 'Thing 1 is done.', verdict: 'unsupported', verifier: 't@1', seq: 3, evidence: [], reasons: [], findings: [] }],
    asks: [{ id: 'a1', node: 'ask1', kind: 'feature', text: 'Add a debug flag.' }],
    nodes: [{ id: 'ev8', objType: 'event', kind: 'user_prompt', seq: 8, title: 'x', lines: [] }, { id: 'clm', objType: 'claim', title: 'x', lines: [] }, { id: 'ask1', objType: 'ask', title: 'x', lines: [] }],
  }
  const titles = (html) => [...html.matchAll(/<q class="ws-verdict-quote" data-quote="(\w+)" title="([^"]*)">/g)].map((m) => [m[1], m[2]])
  const EXCERPT = 'The first line of your message, shortened if it was long'
  for (const [register, agent] of [['manager', 'The AI&#x27;s words, as recorded'], ['operator', 'The agent&#x27;s words, as recorded']]) {
    const at = (selected) => titles(renderPanel({ session, register, selected, onShowProof: noop, onCopy: noop }))
    assert.deepEqual(at(null), [['agent', agent]], `[${register}] the Said row`)
    assert.deepEqual(at('clm'), [['agent', agent], ['agent', agent]], `[${register}] the Said row and a claim's clicked body`)
    assert.deepEqual(at('ask1'), [['agent', agent], ['user', 'Your words, as recorded']], `[${register}] an ask`)
    assert.deepEqual(at('ev8'), [['agent', agent], ['user', EXCERPT]], `[${register}] a user prompt`)
  }
})

/* ── nothing replaces a landed verdict (commit G p5-landed) ──────────────────────────────────────────── */

/* THE ONLY MOUNTED TESTS IN THIS FILE, and they are mounted because the defect is a live one: an observation
 * re-read replaced a verdict that was already on screen, and a re-read whose claims read failed flipped it to
 * claim-free (STEP0-AUDIT "where the prompt and the repository disagree" item 4 and the request 14 row;
 * `shell.jsx:921-941`, `:929-937`). A server render cannot see a replacement — it renders once — so these
 * tests mount the panel with react-dom/client over the smallest DOM it needs and record every mutation of a
 * connected node: the same count the verifier's MutationObserver takes in Chrome, taken here in node.
 *
 * The record they land on and re-read is the committed synthetic capture, through the shell's own
 * `readProjection` → `adapt` → `onObservationUpdate` path. Nothing is hand-shaped except the bytes a re-read
 * answers with. */

/** The least DOM react-dom/client needs, recording every mutation of a node connected to the root. */
function domShim() {
  const log = []
  let recording = false
  let ROOT = null
  const connected = (n) => { let c = n; while (c) { if (c === ROOT) return true; c = c.parentNode } return false }
  const note = (type, target, detail) => { if (recording && connected(target)) log.push({ type, ...detail }) }
  class Nd {
    constructor(nodeType, nodeName) { this.nodeType = nodeType; this.nodeName = nodeName; this.childNodes = []; this.parentNode = null; this.ownerDocument = null }
    get firstChild() { return this.childNodes[0] || null }
    get lastChild() { return this.childNodes[this.childNodes.length - 1] || null }
    get nextSibling() { const p = this.parentNode; if (!p) return null; return p.childNodes[p.childNodes.indexOf(this) + 1] || null }
    appendChild(c) { return this.insertBefore(c, null) }
    insertBefore(c, ref) {
      if (c.parentNode) c.parentNode.removeChild(c, true)
      const i = ref ? this.childNodes.indexOf(ref) : this.childNodes.length
      this.childNodes.splice(i < 0 ? this.childNodes.length : i, 0, c)
      c.parentNode = this
      note('childList:add', this, { node: c.nodeName, text: c.textContent })
      return c
    }
    removeChild(c, moving) {
      const i = this.childNodes.indexOf(c)
      if (!moving) note('childList:remove', this, { node: c.nodeName, text: c.textContent })
      if (i >= 0) this.childNodes.splice(i, 1)
      c.parentNode = null
      return c
    }
    contains(o) { let c = o; while (c) { if (c === this) return true; c = c.parentNode } return false }
    addEventListener() {}
    removeEventListener() {}
    get textContent() { return this.childNodes.map((c) => c.textContent).join('') }
    set textContent(v) {
      note('textContent', this, { text: String(v) })
      this.childNodes.forEach((c) => { c.parentNode = null })
      this.childNodes = []
      if (v !== '') this.appendChild(this.ownerDocument.createTextNode(String(v)))
    }
  }
  class Tx extends Nd {
    constructor(data, doc) { super(3, '#text'); this.d = String(data); this.ownerDocument = doc }
    get nodeValue() { return this.d }
    set nodeValue(v) { if (String(v) !== this.d) note('characterData', this, { from: this.d, to: String(v) }); this.d = String(v) }
    get data() { return this.d }
    get textContent() { return this.d }
    set textContent(v) { this.nodeValue = v }
  }
  class El extends Nd {
    constructor(tag, doc) { super(1, tag.toUpperCase()); this.tagName = tag.toUpperCase(); this.at = new Map(); this.style = {}; this.ownerDocument = doc; this.namespaceURI = 'http://www.w3.org/1999/xhtml' }
    setAttribute(k, v) { const had = this.at.get(k); if (had !== String(v)) note('attributes', this, { name: k, from: had === undefined ? null : had, to: String(v) }); this.at.set(k, String(v)) }
    setAttributeNS(_ns, k, v) { this.setAttribute(k, v) }
    removeAttribute(k) { if (this.at.has(k)) note('attributes', this, { name: k, from: this.at.get(k), to: null }); this.at.delete(k) }
    getAttribute(k) { return this.at.has(k) ? this.at.get(k) : null }
    hasAttribute(k) { return this.at.has(k) }
    html() { return `<${this.tagName.toLowerCase()}${[...this.at].map(([k, v]) => ` ${k}="${v}"`).join('')}>${this.childNodes.map((c) => (c.nodeType === 3 ? c.nodeValue : c.html ? c.html() : '')).join('')}</${this.tagName.toLowerCase()}>` }
  }
  class Doc extends Nd {
    constructor() { super(9, '#document') }
    createElement(tag) { return new El(tag, this) }
    createElementNS(_ns, tag) { return new El(tag, this) }
    createTextNode(t) { return new Tx(t, this) }
    createComment() { const c = new Nd(8, '#comment'); c.ownerDocument = this; return c }
    get activeElement() { return this.body }
  }
  const document = new Doc()
  document.documentElement = document.createElement('html')
  document.body = document.createElement('body')
  document.appendChild(document.documentElement)
  document.documentElement.appendChild(document.body)
  ROOT = document.createElement('div')
  document.body.appendChild(ROOT)
  const walk = (n, out = []) => { out.push(n); n.childNodes.forEach((c) => walk(c, out)); return out }
  return {
    document,
    root: ROOT,
    /* Every root mounted on this shim, so a test that fails an assertion still has its panel unmounted:
     * an unmounted CheckingSequence leaves its elapsed-counter interval running and the runner never exits. */
    roots: [],
    html: () => ROOT.html(),
    text: () => ROOT.textContent,
    nodes: () => walk(ROOT),
    record: (on) => { recording = on },
    mutations: () => log.slice(),
    clear: () => { log.length = 0 },
  }
}

/** The panel, mounted, with the shim and the stores in place for the duration. Removed afterwards, whatever happens. */
async function withDom(fn) {
  const dom = domShim()
  const prior = { document: globalThis.document, window: globalThis.window, session: globalThis.sessionStorage, local: globalThis.localStorage, act: globalThis.IS_REACT_ACT_ENVIRONMENT, fetch: globalThis.fetch, MessageChannel: globalThis.MessageChannel }
  /* React's `act` enqueues every flush on a fresh MessageChannel and never closes it — inside an ES-module
   * bundle its `require('timers').setImmediate` path is unavailable, so it falls back to the channel. Node
   * holds an open handle for each, and a test file that mounts anything would never exit: `npm test` runs
   * every file in one process, so it would hang for everyone. They are tracked here and closed below. */
  const channels = []
  class TrackedMessageChannel extends MessageChannel { constructor() { super(); channels.push(this) } }
  globalThis.MessageChannel = TrackedMessageChannel
  const store = new Map()
  const storage = { getItem: (k) => (store.has(k) ? store.get(k) : null), setItem: (k, v) => store.set(k, String(v)), removeItem: (k) => store.delete(k) }
  class HTMLIFrameElement {}
  globalThis.document = dom.document
  /* `location` and `innerWidth` are here because the shell reads them on construction (`sessionFromQuery`,
   * the panel widths): this window stands in for a browser, so it answers as one with no `?session=` address. */
  globalThis.window = { document: dom.document, HTMLIFrameElement, event: undefined, location: { search: '', hash: '', pathname: '/' }, innerWidth: 1440, innerHeight: 900, addEventListener() {}, removeEventListener() {} }
  globalThis.sessionStorage = storage
  globalThis.localStorage = storage
  globalThis.IS_REACT_ACT_ENVIRONMENT = true
  try { return await fn(dom) } finally {
    for (const root of dom.roots) { try { await L.React.act(async () => { root.unmount() }) } catch { /* the test's own failure is the report */ } }
    for (const c of channels) { try { c.port1.close(); c.port2.close() } catch { /* already closed */ } }
    for (const [k, v] of [['document', prior.document], ['window', prior.window], ['sessionStorage', prior.session], ['localStorage', prior.local], ['IS_REACT_ACT_ENVIRONMENT', prior.act], ['fetch', prior.fetch], ['MessageChannel', prior.MessageChannel]]) {
      if (v === undefined) delete globalThis[k]
      else globalThis[k] = v
    }
  }
}

/* The shell and the panel over one module instance: the Session the panel is given is the one the shell holds. */
const LIVE = await build({ ...BUNDLE, stdin: { ...BUNDLE.stdin, contents: `
  import React from 'react';
  import { createRoot } from 'react-dom/client';
  import { VerdictPanel } from './verdict-panel.jsx';
  export { Workspace } from './shell.jsx';
  export { readProjection, API } from './data.js';
  export { React, createRoot, VerdictPanel };
` } })
const liveFile = join(temporary, 'verdict-panel.live.mjs')
await writeFile(liveFile, LIVE.outputFiles[0].text)
const L = await import(pathToFileURL(liveFile).href)

const LANDED_CAPTURE = new URL('../../../docs/audit/narrator-baseline/claims__05-open-promise-beside-supported-build/', import.meta.url)
const LANDED_BYTES = { explain: readFileSync(new URL('explain.json', LANDED_CAPTURE)), claims: readFileSync(new URL('claims.json', LANDED_CAPTURE)) }
const SESSION_ID = JSON.parse(LANDED_BYTES.claims.toString('utf8')).session
const EXPLAIN_URL = L.API.explain + '?session=' + encodeURIComponent(SESSION_ID)
const CLAIMS_URL = L.API.claims + '?session=' + encodeURIComponent(SESSION_ID)
const OK = (bytes, projection) => new Response(bytes, { status: 200, headers: { 'Content-Type': 'application/json; charset=utf-8', 'X-ThruLine-Projection': projection, 'X-ThruLine-Exit': '0' } })
const REFUSED = () => new Response(JSON.stringify({ error: 'projection_failed', command: 'claims --json', exit_code: 1, stderr: 'a synthetic refusal' }), { status: 502 })
/** A 200 with an empty body: `readProjection` calls it ready with no document, and `readClaimsFor` returns null. */
const EMPTY_BODY = () => OK('', 'claims --json')
/** The same session, re-read with no recorded event in it — the all-empty document the server answers for any valid id. */
const NO_EVENTS = () => OK(Buffer.from(JSON.stringify({ ...JSON.parse(LANDED_BYTES.explain.toString('utf8')), what_happened: [] })), 'explain --json')
/** A record that has moved: one more tree, and the claim the verdict backed now unsupported. */
const MOVED_CLAIMS = () => {
  const d = JSON.parse(LANDED_BYTES.claims.toString('utf8'))
  return OK(Buffer.from(JSON.stringify({ ...d, final_tree_hash: 'f'.repeat(40), claims: d.claims.map((c) => ({ ...c, verdict: 'unsupported' })) })), 'claims --json')
}

/** Every address the shell and the panel ask for, answered from memory. Anything else is a 404, including /api/phases. */
function routing(table) {
  globalThis.fetch = async (url) => {
    const answer = table[String(url)]
    return answer ? answer() : new Response('{}', { status: 404 })
  }
  return table
}

/** A Workspace with the captured session open, driven directly: `setState` applied in place, no React around it. */
async function shellOpen() {
  const table = routing({ [EXPLAIN_URL]: () => OK(LANDED_BYTES.explain, 'explain --json'), [CLAIMS_URL]: () => OK(LANDED_BYTES.claims, 'claims --json') })
  const ws = new L.Workspace({})
  ws.setState = (u, cb) => { ws.state = { ...ws.state, ...(typeof u === 'function' ? u(ws.state, ws.props) : u) }; if (cb) cb() }
  ws.live = true
  const [explain, claims] = await Promise.all([L.readProjection(EXPLAIN_URL), L.readProjection(CLAIMS_URL)])
  const session = ws.adapt(explain, null, { id: SESSION_ID }, claims)
  ws.state = { ...ws.state, session: session.key, sessions: { status: 'ready', rows: [{ id: SESSION_ID }], total: 1, refusal: null }, corpus: { order: [session.key], sessions: { [session.key]: session } } }
  ws.table = table
  ws.ticks = 0
  return ws
}

/** One observation tick, with these answers in place: exactly what a live `--observe-codex` backend drives. */
async function reread(ws, answers) {
  Object.assign(ws.table, { [EXPLAIN_URL]: () => OK(LANDED_BYTES.explain, 'explain --json'), [CLAIMS_URL]: () => OK(LANDED_BYTES.claims, 'claims --json') }, answers)
  ws.ticks += 1
  await ws.onObservationUpdate({ sessions: [SESSION_ID], state: 'watching', events_written: 10 + ws.ticks, last_observed_at: 1700000000 + ws.ticks })
}

/**
 * Mount the panel and let it land. Nothing answers `/api/phases` here, so the phase log fails and the verdict
 * lands through `CheckingSequence`'s `onDone` → `land()` — the landing itself, not a timer.
 */
async function mounted(dom, session) {
  const root = L.createRoot(dom.root)
  dom.roots.push(root)
  const show = async (S) => {
    await L.React.act(async () => { root.render(L.React.createElement(L.VerdictPanel, { session: S, register: 'manager', onShowProof() {}, onCopy() {} })) })
    await L.React.act(async () => {})
  }
  await show(session)
  return { show, unmount: async () => { await L.React.act(async () => root.unmount()) } }
}

const headlineText = (dom) => (dom.html().match(/<p class="ws-verdict-headline">([\s\S]*?)<\/p>/) || [, ''])[1].replace(/<[^>]+>/g, '')
const stateWord = (dom) => (dom.html().match(/<h2 class="ws-verdict-word">([^<]*)</) || [, ''])[1]
const reactProps = (node) => node[Object.keys(node).find((k) => k.startsWith('__reactProps$'))]
async function click(dom, label) {
  const button = dom.nodes().find((n) => n.tagName === 'BUTTON' && n.textContent === label)
  assert.ok(button, `no button labelled "${label}" on screen`)
  await L.React.act(async () => { reactProps(button).onClick({ preventDefault() {}, stopPropagation() {} }) })
  await L.React.act(async () => {})
}

test('a landed verdict survives a failed re-read: no text is replaced, and the shell keeps the session it landed on (commit G p5-landed)', async () => {
  await withDom(async (dom) => {
    const ws = await shellOpen()
    const landedOn = ws.S()
    const panel = await mounted(dom, landedOn)
    assert.match(dom.html(), /ws-verdict-headline/, 'the verdict landed')
    assert.equal(stateWord(dom), HQ_COPY.states.loose.word, 'the captured session lands as a loose thread')
    const before = { text: dom.text(), headline: headlineText(dom) }
    assert.match(before.headline, /\S/, 'the landed headline carries words')

    dom.record(true)
    await reread(ws, { [CLAIMS_URL]: REFUSED })
    await panel.show(ws.S())
    assert.deepEqual(dom.mutations(), [], 'a failed re-read replaced part of the landed verdict')
    assert.equal(dom.text(), before.text)
    assert.equal(ws.S(), landedOn, 'the shell replaced the open session from a re-read whose claims read failed')

    /* The panel refuses the swap on its own too, and not only because the shell withheld it: handed the
     * claim-free session the defect produced, a landed verdict does not take it. What the panel cannot do is
     * tell a read that FAILED from a record whose claims are gone — that is interface request 14, a
     * claims-read status carried through to the panel, and it has not landed — so this session, which the
     * shell no longer hands down, reads here as a record that moved and raises the notice. Nothing of the
     * verdict is rewritten either way, which is the rule: the landed text stands, and the reader chooses. */
    const explain = await L.readProjection(EXPLAIN_URL)
    const claimFree = ws.adapt(explain, null, { id: SESSION_ID }, null)
    assert.equal(claimFree.claims.length, 0, 'premise: a claims read that failed adapts to a session with no claim')
    dom.clear()
    await panel.show(claimFree)
    assert.deepEqual([...new Set(dom.mutations().map((m) => m.type))], ['childList:add'], `the landed verdict was rewritten for a claim-free re-read: ${JSON.stringify(dom.mutations().filter((m) => m.type !== 'childList:add').slice(0, 3))}`)
    assert.equal(headlineText(dom), before.headline, 'the headline was replaced')
    assert.equal(stateWord(dom), HQ_COPY.states.loose.word, 'the state word was replaced')
    await panel.unmount()
  })
})

test('a landed verdict survives an empty re-read: an empty claims body, and an explain document with no recorded event (commit G p5-landed)', async () => {
  await withDom(async (dom) => {
    const ws = await shellOpen()
    const landedOn = ws.S()
    assert.ok(landedOn.events.length > 0 && landedOn.claims.length > 0, 'premise: the landed session holds events and a claim')
    const panel = await mounted(dom, landedOn)
    const before = dom.text()

    dom.record(true)
    await reread(ws, { [CLAIMS_URL]: EMPTY_BODY })
    await panel.show(ws.S())
    assert.equal(ws.S(), landedOn, 'an empty claims body replaced the open session')
    assert.deepEqual(dom.mutations(), [], 'an empty claims body replaced part of the landed verdict')

    /* The record is append-only, so a re-read holding fewer events than the one on screen is not the record
     * moving: it is a read that came back empty. */
    await reread(ws, { [EXPLAIN_URL]: NO_EVENTS })
    await panel.show(ws.S())
    assert.equal(ws.S(), landedOn, 'an explain document with no recorded event replaced the open session')
    assert.deepEqual(dom.mutations(), [], 'an emptied record replaced part of the landed verdict')
    assert.equal(dom.text(), before)
    await panel.unmount()
  })
})

test('a landed verdict survives an identical re-read, with no mutation and no notice (commit G p5-landed)', async () => {
  /* This one passes before the fix as well as after: React writes nothing when the text it renders is the same,
   * so the defect leaves no mark here. It is kept as the guard on the other side of the fix — a pin that
   * treated every new object as a changed record would put a notice on screen after a re-read that changed
   * nothing, and this is what would catch it (broken and restored in the p5 evidence). */
  await withDom(async (dom) => {
    const ws = await shellOpen()
    const landedOn = ws.S()
    const panel = await mounted(dom, landedOn)
    const before = dom.text()

    dom.record(true)
    await reread(ws, {})
    assert.notEqual(ws.S(), landedOn, 'premise: an identical re-read still hands the panel a new object')
    assert.deepEqual(ws.S().claims.map((c) => [c.id, c.verdict]), landedOn.claims.map((c) => [c.id, c.verdict]), 'premise: with the same record behind it')
    await panel.show(ws.S())
    assert.deepEqual(dom.mutations(), [], 'an identical re-read mutated the landed verdict')
    assert.equal(dom.text(), before)
    assert.doesNotMatch(dom.html(), /ws-verdict-record-changed/, 'a re-read that changed nothing raised the changed-record notice')
    await panel.unmount()
  })
})

test('a changed record leaves the landed verdict in place and says so; the reader chooses when it is checked (commit G p5-landed)', async () => {
  await withDom(async (dom) => {
    const ws = await shellOpen()
    const landedOn = ws.S()
    const panel = await mounted(dom, landedOn)
    const before = { headline: headlineText(dom), word: stateWord(dom) }

    dom.record(true)
    await reread(ws, { [CLAIMS_URL]: MOVED_CLAIMS })
    const moved = ws.S()
    assert.notEqual(moved.finalTree, landedOn.finalTree, 'premise: the re-read carries a different final tree')
    assert.deepEqual(moved.claims.map((c) => c.verdict), ['unsupported'], 'premise: and a claim whose verdict moved')
    await panel.show(moved)

    /* Nothing is rewritten: every mutation is the notice arriving, and the verdict's own text is untouched. */
    const kinds = [...new Set(dom.mutations().map((m) => m.type))]
    assert.deepEqual(kinds, ['childList:add'], `the landed verdict was rewritten: ${JSON.stringify(dom.mutations().filter((m) => m.type !== 'childList:add').slice(0, 4))}`)
    assert.equal(headlineText(dom), before.headline, 'the headline was replaced')
    assert.equal(stateWord(dom), before.word, 'the state word was replaced')
    assert.ok(dom.mutations().some((m) => m.text.includes(HQ_COPY.chrome.record_changed)), 'the notice never arrived')
    assert.match(dom.html(), /ws-verdict-record-changed/)
    assert.ok(dom.text().includes(HQ_COPY.chrome.record_changed) && dom.text().includes(HQ_COPY.chrome.record_changed_check), 'the notice reads from HQ_COPY')

    /* The reader chooses. Only then is the new record checked, and the notice goes with it. */
    await click(dom, HQ_COPY.chrome.record_changed_check)
    assert.equal(stateWord(dom), HQ_COPY.states.not_proven.word, 'the reader asked for the new record and it was not checked')
    assert.notEqual(headlineText(dom), before.headline)
    assert.doesNotMatch(dom.html(), /ws-verdict-record-changed/, 'the notice outlived the check the reader asked for')

    /* And the verdict that has now landed on the new record is itself not replaced by the next re-read. */
    dom.clear()
    await reread(ws, { [CLAIMS_URL]: REFUSED })
    await panel.show(ws.S())
    assert.deepEqual(dom.mutations(), [], 'the re-landed verdict was replaced by a failed re-read')
    await panel.unmount()
  })
})

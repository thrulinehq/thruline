/* The instrument for the exported Markdown (EXPORT-MD-ESCAPE-R2, R3).
 *
 * Round 1 judged the file by reading it a LINE at a time (`hazards()` in
 * packet-export-md-escape.test.js). A Markdown renderer does not read lines: a
 * link reference definition may span lines and fields, a list item's text and
 * its continuation lines are one paragraph, and a code span binds tighter than
 * a link. This instrument judges the RENDERED output of the whole file.
 *
 * ROUND 3: THE RENDERERS ARE REAL, ALL THREE, EVERY RUN. Round 2 vendored the
 * CommonMark reference implementation and MODELLED `marked` and `markdown-it`
 * (a per-line "quirk model" of what each was known to read differently), because
 * neither was installed in `ui/node_modules`. Two shapes the model did not know
 * were live in `marked` — a `javascript:` link and an `<img onerror>` whose
 * handler ran in a browser — and the tests passed. A model of a renderer's inline
 * parser cannot be made complete, so it is gone: `marked` 18.0.14 and
 * `markdown-it` 15.0.2 are vendored beside CommonMark 0.31.2
 * (`vendor/<name>/`, each with its LICENSE and a VENDORED.md that pins the
 * digest below) and every check renders with all three. Nothing here imports
 * from `ui/node_modules`, so a machine with no `npm install` runs the same
 * three renderers.
 *
 * ROUND 4 (D-246): WHAT COUNTS AS A LINK. Rounds 2 and 3 let a link through when
 * its address was http(s), `ftp` or a mail address. `marked` reads `\[` and `\]`
 * inside a link label and lexes the label again, so a recorded
 * `[![a\](https://…/p.png)](https://…/x)` was a link holding an IMAGE that a
 * browser fetches on open, and `markdown-it` makes a link of a bare `//host`
 * (scheme-relative, so `file://host` in a local file) and of `<1@b.co>` in all
 * three. Recorded text now yields NO `<img>`, no raw tag or handler, no reference
 * definition, and no `<a>` except one whose address is an absolute `http://` or
 * `https://` URL AND whose words are that address: a viewer's own autolinking of
 * a bare address. A `[text](url)` that a renderer made into a link has words that
 * are not its address (`<a href="https://ok.example/">docs</a>`), and that is
 * the shape a person is misled by, so it is live whatever the address is.
 *
 * What it does
 *  - Renders the whole file with every renderer (`RENDERERS`).
 *  - Reads what came out: every tag must be one the file's own structure uses
 *    (headings, paragraphs, lists, a quote, a rule, bold, emphasis, code, a
 *    line break, a link), no tag carries an attribute except a link's `href`,
 *    that `href` is an absolute http(s) URL, and the link's words are that
 *    address (round 4). Anything else is live.
 *  - Reads the CommonMark reference implementation's reference map after the
 *    parse: the file has no reference definition of its own, so ANY entry — any
 *    scheme, any label — is a definition that recorded text formed.
 *  - `textOf(html)` is the visible characters of a renderer's output, for the
 *    round-3 requirement that a recorded text shows exactly the characters that
 *    were recorded, backticks included.
 *  - `quirkHazards` reads each line of the raw file for the two things no
 *    renderer needs to be asked about: a backtick that is not escaped (round 3
 *    writes every recorded backtick escaped, so a live one is a code span and
 *    round 3 has none), and a line that is only a table's delimiter row.
 *
 * It is not the production helper turned around: it shares no code with
 * `escapeMarkdown`. `scanLine` is CommonMark's code-span rule read again, on
 * one line.
 *
 * This file lives outside `ui/src` on purpose: it is a test instrument, and a
 * non-test source file under `ui/src/workspace` is one the source lints walk.
 * Nothing under `ui/test-support` is imported by `ui/src`, bundled by `vite
 * build`, or reachable from `ui/dist` or the binary.
 */

import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)

/** The pinned copies of the three renderers. Each digest is the sha256 of `file` and is repeated in that directory's VENDORED.md; the test recomputes both. */
export const VENDORED = {
  commonmark: {
    name: 'commonmark',
    version: '0.31.2',
    file: new URL('./vendor/commonmark/commonmark.min.cjs', import.meta.url),
    note: new URL('./vendor/commonmark/VENDORED.md', import.meta.url),
    license: new URL('./vendor/commonmark/LICENSE', import.meta.url),
    sha256: '2de0f8ecbca0a6470da57c8b2ad043777ae999c5132f9abf12e8c332d4e46164',
    license_sha256: '6cc4b9b28cf68e5bd20f9e94859a85cde35095ef4f9d0f7385b0b6166642f50b',
  },
  marked: {
    name: 'marked',
    version: '18.0.14',
    file: new URL('./vendor/marked/marked.esm.mjs', import.meta.url),
    note: new URL('./vendor/marked/VENDORED.md', import.meta.url),
    license: new URL('./vendor/marked/LICENSE', import.meta.url),
    sha256: '528a1b88bef88fc27277e06036ce7f4afb6a220b18110ba57c09e292b24a7ce0',
    license_sha256: '8e3a3f82f59a60958f56ca08f445647c32a4733dc7ca6c2c46f6eb898471ab9c',
  },
  markdownIt: {
    name: 'markdown-it',
    version: '15.0.2',
    file: new URL('./vendor/markdown-it/markdown-it.umd.min.cjs', import.meta.url),
    note: new URL('./vendor/markdown-it/VENDORED.md', import.meta.url),
    license: new URL('./vendor/markdown-it/LICENSE', import.meta.url),
    sha256: '635972b985228e8af9f0143647c68616b7a3bb09f6946e7e4a52e43dcf5e7be5',
    license_sha256: '570ffa10225644e9684044e6e90ca5b15604aaf58656f4ce564aa0aceead08e5',
  },
}

/** Round 2's name for the first of them, kept so what read it still does. */
export const VENDORED_COMMONMARK = VENDORED.commonmark

const commonmark = require('./vendor/commonmark/commonmark.min.cjs')
const { marked } = await import('./vendor/marked/marked.esm.mjs')
const MarkdownIt = require('./vendor/markdown-it/markdown-it.umd.min.cjs')

/* Raw HTML passed through and bare addresses made links: the worst case a reader can be in. */
const markdownIt = new MarkdownIt({ html: true, linkify: true })

/** Parse with the reference implementation: the tree and its reference map. */
export function commonmarkParse(md) {
  const parser = new commonmark.Parser()
  const doc = parser.parse(String(md))
  return { doc, refmap: parser.refmap || {} }
}

/* Raw HTML passed through: the worst case a reader can be in. The renderer keeps no state between documents. */
const commonmarkRenderer = new commonmark.HtmlRenderer({ safe: false })

/** HTML from the reference implementation, raw HTML passed through (the worst case a reader can be in). */
export function commonmarkHtml(md) {
  return commonmarkRenderer.render(commonmarkParse(md).doc)
}

/** HTML from `marked`, its default options (`gfm` on). */
export function markedHtml(md) {
  return marked.parse(String(md), { gfm: true })
}

/** HTML from `markdown-it`, raw HTML passed through and `linkify` on. */
export function markdownItHtml(md) {
  return markdownIt.render(String(md))
}

/** The inline HTML of one line of text, without its paragraph (CommonMark). */
export function inlineHtml(text) {
  return commonmarkHtml(text).replace(/^<p>/, '').replace(/<\/p>\n?$/, '')
}

/** Every renderer: `{ name, render(md) → html }`. All three, always. */
export const RENDERERS = [
  { name: `commonmark ${VENDORED.commonmark.version}`, render: commonmarkHtml },
  { name: `marked ${VENDORED.marked.version}`, render: markedHtml },
  { name: `markdown-it ${VENDORED.markdownIt.version}`, render: markdownItHtml },
]

/** One sentence saying which renderers ran. */
export function describeRenderers() {
  return `renderers that ran: ${RENDERERS.map((r) => r.name).join(', ')} (all three vendored under ui/test-support/vendor/, run every time)`
}

/* The visible characters of a renderer's output: the tags removed, the entities read. */
const NAMED_TEXT = { amp: '&', lt: '<', gt: '>', quot: '"', apos: "'" }

/**
 * What a reader sees of a renderer's HTML, as characters: every tag removed and
 * every entity replaced by the character it stands for. (A renderer writes a
 * text `<` as `&lt;`, so a `<` that is left after the tags come out is a tag it
 * did not recognise as one; `liveIn` is what looks for those.)
 * @param {string} html
 * @returns {string}
 */
export function textOf(html) {
  return String(html)
    .replace(/<[^>]*>/g, '')
    .replace(/&(?:#(\d+)|#[xX]([0-9a-fA-F]+)|([A-Za-z]+));/g, (whole, dec, hex, name) => {
      try {
        if (dec) return String.fromCodePoint(Number(dec))
        if (hex) return String.fromCodePoint(parseInt(hex, 16))
      } catch { return whole }
      return name in NAMED_TEXT ? NAMED_TEXT[name] : whole
    })
}

/* ── what in rendered HTML is live ────────────────────────────────────── */

/* The tags the file's own structure and the ordinary reading of recorded text
 * produce. Nothing else may come out of a renderer: not an image, a script, a
 * form control (a task-list box), a table, a code block or a heading the file
 * did not write. */
const ALLOWED_TAGS = new Set(['h1', 'h2', 'p', 'ul', 'li', 'blockquote', 'hr', 'strong', 'em', 'code', 'br', 'a', 'del', 's'])

const TAG = /<(\/?)([A-Za-z][A-Za-z0-9-]*)((?:\s+[^\s"'<>/=]+(?:\s*=\s*(?:"[^"]*"|'[^']*'|[^\s"'=<>`]+))?)*)\s*(\/?)>/g
const ATTR = /([^\s"'<>/=]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'=<>`]+)))?/g

const NAMED = { colon: ':', Tab: '\t', NewLine: '\n', lpar: '(', rpar: ')', sol: '/', amp: '&' }

/** An attribute value the way a browser would resolve it before using it as a URL. */
function resolved(raw) {
  return String(raw)
    .replace(/&#(\d+);?/g, (_, n) => { try { return String.fromCodePoint(Number(n)) } catch { return _ } })
    .replace(/&#[xX]([0-9a-fA-F]+);?/g, (_, n) => { try { return String.fromCodePoint(parseInt(n, 16)) } catch { return _ } })
    .replace(/&([A-Za-z]+);/g, (whole, name) => (name in NAMED ? NAMED[name] : whole))
    .replace(/[\t\n\r]/g, '')
    .replace(/^[\u0000- ]+/, '')
}

/* An address the way a link's words show it: no scheme, no trailing slash, percent escapes read, case ignored. */
function bareWords(s) {
  let words = String(s)
  try { words = decodeURI(words) } catch { /* not percent-encoded: leave it */ }
  return words.replace(/^https?:\/\//i, '').replace(/\/+$/, '').toLowerCase()
}

/**
 * What in a renderer's HTML output is live. Empty means nothing.
 * @param {string} html
 * @param {{tags?: string[]}} [options] more tags a file's OWN structure uses (the verdict summary has `###` headings, so `h3`)
 * @returns {string[]}
 */
export function liveIn(html, { tags = [] } = {}) {
  const found = []
  const allowed = tags.length ? new Set([...ALLOWED_TAGS, ...tags]) : ALLOWED_TAGS
  const rest = String(html).replace(TAG, (whole, closing, name, attrs) => {
    const tag = name.toLowerCase()
    if (!allowed.has(tag)) {
      found.push(`tag <${tag}>: ${whole.slice(0, 80)}`)
      return ''
    }
    if (closing) return ''
    for (const m of attrs.matchAll(ATTR)) {
      const attr = m[1].toLowerCase()
      const value = m[2] ?? m[3] ?? m[4] ?? ''
      if (tag !== 'a' || attr !== 'href') {
        found.push(`attribute ${attr} on <${tag}>: ${whole.slice(0, 80)}`)
      } else if (!/^https?:\/\/./i.test(resolved(value))) {
        found.push(`link to ${JSON.stringify(resolved(value).slice(0, 60))}`)
      }
    }
    if (tag === 'a' && !/(?:^|\s)href\s*=/i.test(attrs)) found.push(`a link with no address: ${whole.slice(0, 80)}`)
    return ''
  })
  /* Round 4: a link that is a bare address has that address as its words. */
  for (const m of String(html).matchAll(/<a\b([^>]*)>([\s\S]*?)<\/a>/gi)) {
    const href = /(?:^|\s)href\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'=<>`]+))/i.exec(m[1])
    if (!href) continue
    const address = resolved(href[1] ?? href[2] ?? href[3] ?? '')
    if (!/^https?:\/\/./i.test(address)) continue // already reported above
    if (/<[A-Za-z/!?]/.test(m[2])) found.push(`a link with markup inside it: ${m[0].slice(0, 80)}`)
    else if (bareWords(textOf(m[2])) !== bareWords(address)) found.push(`a link whose words are not its address: ${JSON.stringify(textOf(m[2]).slice(0, 40))} → ${JSON.stringify(address.slice(0, 60))}`)
  }
  /* A `<` no tag pattern took is a comment, a processing instruction, a
   * declaration, CDATA or something broken. A renderer writes a text `<` as `&lt;`. */
  const at = rest.indexOf('<')
  if (at >= 0) found.push(`markup that is not a tag: ${JSON.stringify(rest.slice(Math.max(0, at - 10), at + 30))}`)
  return found
}

/**
 * The whole file, rendered by all three renderers, and what could be live in
 * what came out, plus every reference definition the reference implementation
 * found (the file has none of its own, so any is a finding). CommonMark is parsed
 * once, and its tree is both rendered and asked for its reference map, which
 * keeps the exhaustive enumeration in the tests inside its time.
 * @param {string} md
 * @param {{tags?: string[]}} [options] see `liveIn`
 * @returns {string[]}
 */
export function hazardsRendered(md, options) {
  const found = []
  const text = String(md)
  const { doc, refmap } = commonmarkParse(text)
  for (const [label, ref] of Object.entries(refmap)) {
    found.push(`definition [${label.slice(0, 50)}] → ${JSON.stringify(String(ref.destination).slice(0, 60))}`)
  }
  const htmls = [commonmarkRenderer.render(doc), markedHtml(text), markdownItHtml(text)]
  for (const [i, html] of htmls.entries()) for (const f of liveIn(html, options)) found.push(`${RENDERERS[i].name}: ${f}`)
  return found
}

/** The reference definitions the reference implementation found in a file. */
export function definitionsIn(md) {
  return Object.entries(commonmarkParse(md).refmap).map(([label, ref]) => ({ label, destination: ref.destination }))
}

/* ── backticks, read by CommonMark's own rule ─────────────────────────── */

const ESCAPABLE = /[!-/:-@[-`{-~]/

/**
 * One line, read by CommonMark's rule for code spans: a backslash and the
 * punctuation after it are a unit outside a span; a run of N backticks opens a
 * span that the next run of exactly N closes; if there is none the run is
 * literal. Independent of the production helper.
 * @param {string} line
 * @returns {{spans: {i: number, n: number, close: number, content: string}[], unpaired: {i: number, n: number}[]}}
 */
export function scanLine(line) {
  const spans = []
  const unpaired = []
  let i = 0
  while (i < line.length) {
    const c = line[i]
    if (c === '\\' && i + 1 < line.length && ESCAPABLE.test(line[i + 1])) { i += 2; continue }
    if (c !== '`') { i += 1; continue }
    let j = i
    while (line[j] === '`') j += 1
    const n = j - i
    let k = j
    let close = -1
    while (k < line.length) {
      if (line[k] !== '`') { k += 1; continue }
      let m = k
      while (line[m] === '`') m += 1
      if (m - k === n) { close = k; break }
      k = m
    }
    if (close < 0) { unpaired.push({ i, n }); i = j; continue }
    spans.push({ i, n, close, content: line.slice(j, close) })
    i = close + n
  }
  return { spans, unpaired }
}

/* ── the two things in the raw file that no renderer has to be asked about ─ */

/**
 * The lines of a file that hold a backtick that is not escaped, or that are only
 * a table's delimiter row. Empty means none.
 *
 * Round 3 writes every recorded backtick escaped (`\``), so a backtick that
 * `scanLine` finds live, opening a span or standing unpaired, is one the helper
 * did not write: a code span, which is what round 3 does not have (renderers
 * disagree about where one ends, and two of the three round-2 breaks were the
 * disagreement). A line of nothing but `|`, `:`, `-` and spaces (with a `-`) is
 * a table's delimiter row, and turns the line above it into a table header
 * (`marked` even when that line has no `|`; `markdown-it` when it has one).
 * @param {string} md
 * @returns {string[]}
 */
export function quirkHazards(md) {
  const found = []
  for (const [at, line] of String(md).split('\n').entries()) {
    /* The file's own rule is `---` on a line of its own; anything else that is only delimiter characters is recorded text. */
    if (line !== '---' && /^(?:> - |> |- |  )?[|: \t-]*-[|: \t-]*$/.test(line)) found.push(`line ${at + 1}: ${JSON.stringify(line.slice(0, 40))} is a table's delimiter row`)
    const { spans, unpaired } = scanLine(line)
    for (const sp of spans) found.push(`line ${at + 1}: a live code span ${JSON.stringify(line.slice(sp.i, sp.close + sp.n).slice(0, 40))}`)
    for (const u of unpaired) found.push(`line ${at + 1}: a live run of ${u.n} backtick${u.n === 1 ? '' : 's'}`)
  }
  return found
}

/* ── round 4: every `[`, `]` and `<` in a recorded text is written escaped ─ */

/**
 * The places in a raw file where a `[`, a `]` or a `<` is not escaped: a bracket
 * with an even number of backslashes in front of it (`\\[` is an escaped
 * backslash and a live bracket), and any `<` at all (the helper writes one as
 * `&lt;`, never with a backslash). Round 4 (D-246): recorded `[text](url)` is
 * never a live link, whatever follows the bracket, because no bracket is live —
 * a `\]` that `marked` unescapes inside a link label and lexes again is what
 * made a link that held an image. This reads the file's characters, not a
 * renderer's output, because it is a statement about what was WRITTEN. It is a
 * check on the output of `escapeMarkdown`, and on a file whose own structure has
 * no brackets (the exported packet's does not); the Markdown the verdict panel
 * writes has its own `[clm]` receipts and is judged by the renderers instead.
 * @param {string} md
 * @returns {string[]}
 */
export function unescapedMarkup(md) {
  const found = []
  for (const [at, line] of String(md).split('\n').entries()) {
    for (const m of line.matchAll(/[[\]<]/g)) {
      let slashes = 0
      while (m.index - 1 - slashes >= 0 && line[m.index - 1 - slashes] === '\\') slashes += 1
      if (m[0] === '<' || slashes % 2 === 0) found.push(`line ${at + 1}: a live ${JSON.stringify(m[0])} in ${JSON.stringify(line.slice(Math.max(0, m.index - 12), m.index + 12))}`)
    }
  }
  return found
}

#!/usr/bin/env node
/* dc2jsx.mjs — port one HQ Next canvas artboard (`<Name>.dc.html`) to React,
 * markup for markup, so the product screen is the founder's design and not a
 * redrawing of it.
 *
 *   node scripts/hq/dc2jsx.mjs <Name.dc.html> <outDir>
 *
 * writes three files into <outDir>:
 *   template.jsx  the artboard's <x-dc> markup as JSX. Every `{{ a.b }}` hole
 *                 reads `v.a?.b` (or a loop variable), every <sc-for> is a map,
 *                 every <sc-if> a conditional, inline styles become objects
 *                 with the same declarations in the same order.
 *   logic.js      the artboard's `class Component extends DCLogic`, verbatim,
 *                 exported, over ui/src/hq/dc/runtime.jsx.
 *   styles.css    the artboard's <helmet><style>, verbatim. Font <link>s are
 *                 dropped: Geist is self-hosted (ui/public/fonts/geist).
 *
 * WHY A STRICT TOKENIZER, NOT AN HTML PARSER: the artboards are well-formed
 * (every element closed, every attribute quoted — the canvas format requires
 * it), and an HTML parser would foster-parent <sc-for> out of <table> rows.
 * Anything this tokenizer does not understand is an error, never a guess.
 *
 * It is a ONE-TIME port: once generated, the three files are product source,
 * owned and edited like any other.
 */
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs'
import { basename, join } from 'node:path'

const VOID = new Set(['area', 'base', 'br', 'col', 'embed', 'hr', 'img', 'input', 'link', 'meta', 'param', 'source', 'track', 'wbr'])
const RAW = new Set(['style', 'script'])
const ENT = { amp: '&', lt: '<', gt: '>', quot: '"', apos: "'", nbsp: ' ', middot: '·', mdash: '—', ndash: '–', hellip: '…', rarr: '→', larr: '←', times: '×', copy: '©' }
const ATTR_RENAME = { class: 'className', for: 'htmlFor', tabindex: 'tabIndex', spellcheck: 'spellCheck', autocomplete: 'autoComplete', readonly: 'readOnly', maxlength: 'maxLength', colspan: 'colSpan', rowspan: 'rowSpan', contenteditable: 'contentEditable', 'xlink:href': 'xlinkHref', 'accept-charset': 'acceptCharset', 'http-equiv': 'httpEquiv', srcset: 'srcSet', crossorigin: 'crossOrigin', autofocus: 'autoFocus', novalidate: 'noValidate', enterkeyhint: 'enterKeyHint', inputmode: 'inputMode' }

function decode(s) {
  return s.replace(/&(#x[0-9a-fA-F]+|#[0-9]+|[a-zA-Z]+);/g, (m, e) => {
    if (e[0] === '#') return String.fromCodePoint(e[1] === 'x' || e[1] === 'X' ? parseInt(e.slice(2), 16) : parseInt(e.slice(1), 10))
    if (Object.hasOwn(ENT, e)) return ENT[e]
    throw new Error(`unknown entity ${m}`)
  })
}

function tokenize(src) {
  const out = []
  let i = 0
  const n = src.length
  while (i < n) {
    if (src.startsWith('<!--', i)) {
      const end = src.indexOf('-->', i + 4)
      if (end < 0) throw new Error('unclosed comment')
      i = end + 3
      continue
    }
    if (src[i] === '<' && src[i + 1] === '/') {
      const end = src.indexOf('>', i)
      out.push({ t: 'close', name: src.slice(i + 2, end).trim() })
      i = end + 1
      continue
    }
    if (src[i] === '<' && /[a-zA-Z]/.test(src[i + 1] || '')) {
      let j = i + 1
      while (j < n && /[a-zA-Z0-9:-]/.test(src[j])) j += 1
      const name = src.slice(i + 1, j)
      const attrs = []
      let selfClose = false
      for (;;) {
        while (j < n && /\s/.test(src[j])) j += 1
        if (src[j] === '>') { j += 1; break }
        if (src[j] === '/' && src[j + 1] === '>') { selfClose = true; j += 2; break }
        let k = j
        while (k < n && !/[\s=>/]/.test(src[k])) k += 1
        const an = src.slice(j, k)
        if (!an) throw new Error(`bad attribute in <${name}> at ${j}: ${JSON.stringify(src.slice(j, j + 40))}`)
        j = k
        while (j < n && /\s/.test(src[j])) j += 1
        if (src[j] === '=') {
          j += 1
          while (j < n && /\s/.test(src[j])) j += 1
          const q = src[j]
          if (q !== '"' && q !== "'") throw new Error(`unquoted attribute ${an} in <${name}> at ${j}`)
          const end = src.indexOf(q, j + 1)
          attrs.push([an, decode(src.slice(j + 1, end))])
          j = end + 1
        } else {
          attrs.push([an, null])
        }
      }
      out.push({ t: 'open', name, attrs, selfClose: selfClose || VOID.has(name.toLowerCase()) })
      i = j
      if (RAW.has(name.toLowerCase()) && !selfClose) {
        const end = src.indexOf(`</${name}>`, i)
        out.push({ t: 'raw', text: src.slice(i, end) })
        out.push({ t: 'close', name })
        i = end + name.length + 3
      }
      continue
    }
    let j = src.indexOf('<', i + 1)
    if (j < 0) j = n
    out.push({ t: 'text', text: decode(src.slice(i, j)) })
    i = j
  }
  return out
}

function tree(tokens) {
  const root = { name: '#root', attrs: [], children: [] }
  const stack = [root]
  for (const tk of tokens) {
    const top = stack[stack.length - 1]
    if (tk.t === 'text') top.children.push({ text: tk.text })
    else if (tk.t === 'raw') top.children.push({ raw: tk.text })
    else if (tk.t === 'open') {
      const el = { name: tk.name, attrs: tk.attrs, children: [] }
      top.children.push(el)
      if (!tk.selfClose) stack.push(el)
    } else if (tk.t === 'close') {
      if (VOID.has(tk.name.toLowerCase())) continue
      if (top.name !== tk.name) throw new Error(`</${tk.name}> closes <${top.name}>`)
      stack.pop()
    }
  }
  if (stack.length !== 1) throw new Error(`unclosed <${stack[stack.length - 1].name}>`)
  return root
}

/* ── holes ─────────────────────────────────────────────────────────────── */

const HOLE = /\{\{\s*([^}]*?)\s*\}\}/g

function expr(path, scope) {
  if (/^(true|false|null|undefined|-?\d+(\.\d+)?)$/.test(path)) return path
  if (/^'[^']*'$|^"[^"]*"$/.test(path)) return path
  if (!/^[A-Za-z_$][\w$]*(\.[\w$]+)*$/.test(path)) throw new Error(`hole is not a dotted lookup: {{ ${path} }}`)
  const segs = path.split('.')
  const head = scope.has(segs[0]) ? segs[0] : `v?.${segs[0]}`
  return [head, ...segs.slice(1)].join('?.')
}

function parts(s, scope) {
  const out = []
  let last = 0
  for (const m of s.matchAll(HOLE)) {
    if (m.index > last) out.push({ lit: s.slice(last, m.index) })
    out.push({ ex: expr(m[1], scope) })
    last = m.index + m[0].length
  }
  if (last < s.length) out.push({ lit: s.slice(last) })
  return out
}

function valueOf(s, scope) {
  const p = parts(s, scope)
  if (p.length === 1 && p[0].ex) return p[0].ex
  if (p.every((x) => x.lit !== undefined)) return JSON.stringify(s)
  return '`' + p.map((x) => (x.lit !== undefined ? x.lit.replace(/[`\\]/g, '\\$&').replace(/\$\{/g, '\\${') : '${' + x.ex + " ?? ''}")).join('') + '`'
}

/* ── styles ────────────────────────────────────────────────────────────── */

function splitDecls(s) {
  const out = []
  let depth = 0
  let q = null
  let cur = ''
  for (let i = 0; i < s.length; i += 1) {
    const c = s[i]
    if (s.startsWith('{{', i)) {
      const end = s.indexOf('}}', i)
      cur += s.slice(i, end + 2)
      i = end + 1
      continue
    }
    if (q) { if (c === q) q = null; cur += c; continue }
    if (c === '"' || c === "'") { q = c; cur += c; continue }
    if (c === '(') depth += 1
    if (c === ')') depth -= 1
    if (c === ';' && depth === 0) { out.push(cur); cur = ''; continue }
    cur += c
  }
  if (cur.trim()) out.push(cur)
  return out.map((d) => d.trim()).filter(Boolean)
}

const camel = (p) => (p.startsWith('--') ? p : p.replace(/^-ms-/, 'ms-').replace(/-([a-z])/g, (_, c) => c.toUpperCase()))

function styleObject(s, scope, root) {
  const entries = []
  for (const d of splitDecls(s)) {
    const whole = d.match(/^\{\{\s*([^}]*?)\s*\}\}$/)
    if (whole) {
      /* A declaration list computed by the logic, e.g. style="{{ o.style }}". */
      entries.push(`...cssText(${expr(whole[1], scope)})`)
      continue
    }
    const k = d.indexOf(':')
    if (k < 0) throw new Error(`bad style declaration: ${d}`)
    let prop = d.slice(0, k).trim()
    let val = d.slice(k + 1).trim()
    if (root) {
      /* The artboard is a fixed 1512-wide frame; the product fills the window. */
      if (prop === 'width' && /^\d+px$/.test(val)) val = '100%'
      if (prop === 'height' && /^\d+px$/.test(val)) {
        if (val === '920px') val = '100vh'
        else { prop = 'min-height'; val = '100vh' }
      }
    }
    const key = camel(prop)
    entries.push(`${/^[A-Za-z_$][\w$]*$/.test(key) ? key : JSON.stringify(key)}: ${valueOf(val, scope)}`)
  }
  return `{ ${entries.join(', ')} }`
}

/* ── emit ──────────────────────────────────────────────────────────────── */

const FORM = new Set(['input', 'textarea', 'select'])
const BOOLEAN_ATTRS = new Set(['disabled', 'checked', 'readonly', 'required', 'hidden', 'open', 'selected', 'multiple', 'autofocus', 'novalidate', 'inert', 'default', 'reversed', 'async', 'defer', 'nomodule', 'playsinline', 'loop', 'muted', 'controls', 'autoplay', 'formnovalidate', 'itemscope', 'allowfullscreen'])

function attrName(name, an, names) {
  if (Object.hasOwn(ATTR_RENAME, an)) return ATTR_RENAME[an]
  if (/^on[A-Z]/.test(an)) return an === 'onInput' && FORM.has(name) && !names.has('onChange') ? 'onChange' : an
  if (an.startsWith('aria-') || an.startsWith('data-')) return an
  if (an.includes('-')) return camel(an)
  return an
}

function emitAttrs(el, scope, root) {
  const out = []
  const names = new Set(el.attrs.map(([a]) => a))
  for (const [an, av] of el.attrs) {
    if (an.startsWith('hint-placeholder')) continue
    const name = attrName(el.name, an, names)
    /* HTML boolean attributes are on whenever present, whatever the value —
       `disabled=""` disables. React reads "" as false, so a literal value is
       written as {true}; only a hole can turn one off. */
    if (av === null || (BOOLEAN_ATTRS.has(an.toLowerCase()) && !/\{\{/.test(av))) { out.push(`${name}={true}`); continue }
    if (an === 'style') { out.push(`style={${styleObject(av, scope, root)}}`); continue }
    if (an === 'href' && /\.dc\.html/.test(av)) { out.push(`href={link(${valueOf(av, scope)})}`); continue }
    if (an === 'value' && FORM.has(el.name) && !names.has('onInput') && !names.has('onChange')) {
      out.push(`defaultValue={${valueOf(av, scope)}}`)
      continue
    }
    const val = valueOf(av, scope)
    out.push(val.startsWith('"') ? `${name}=${val}` : `${name}={${val}}`)
  }
  return out.length ? ' ' + out.join(' ') : ''
}

function emitText(text, scope, pre) {
  const t = pre ? text : text.replace(/[ \t\n\r\f]+/g, ' ')
  if (!t) return ''
  return parts(t, scope)
    .map((p) => (p.lit !== undefined ? `{${JSON.stringify(p.lit)}}` : `{${p.ex}}`))
    .join('')
}

const TABLEISH = new Set(['table', 'thead', 'tbody', 'tfoot', 'tr', 'colgroup', 'select'])

function emit(node, scope, ctx) {
  /* HTML drops whitespace between table parts; React would make it a text
     node inside <tr>, which is invalid and shifts the columns. */
  if (node.text !== undefined) return ctx.table && !node.text.trim() ? '' : emitText(node.text, scope, ctx.pre)
  if (node.raw !== undefined) return ''
  const name = node.name
  const kids = (sc, c = ctx) => node.children.map((k) => emit(k, sc, c)).join('')
  if (name === 'sc-for') {
    const list = node.attrs.find(([a]) => a === 'list')[1]
    const as = node.attrs.find(([a]) => a === 'as')[1]
    const inner = new Set(scope)
    inner.add(as)
    const m = list.match(/^\{\{\s*([^}]*?)\s*\}\}$/)
    if (!m) throw new Error(`sc-for list is not a hole: ${list}`)
    return `{(${expr(m[1], scope)} || []).map((${as}, ${as}$i) => (<React.Fragment key={${as}$i}>${kids(inner)}</React.Fragment>))}`
  }
  if (name === 'sc-if') {
    const value = node.attrs.find(([a]) => a === 'value')[1]
    const m = value.match(/^\{\{\s*([^}]*?)\s*\}\}$/)
    if (!m) throw new Error(`sc-if value is not a hole: ${value}`)
    return `{${expr(m[1], scope)} ? (<>${kids(scope)}</>) : null}`
  }
  if (name.startsWith('sc-') || name.startsWith('dc-')) throw new Error(`unsupported template element <${name}>`)
  const isPre = ctx.pre || name === 'pre'
  if (name === 'textarea' && node.children.length) {
    const text = node.children.map((k) => k.text || '').join('')
    node = { ...node, attrs: [...node.attrs, ['value', text]], children: [] }
  }
  const attrs = emitAttrs(node, scope, ctx.root)
  const sub = { ...ctx, pre: isPre, root: false, table: TABLEISH.has(name) }
  const body = node.children.map((k) => emit(k, scope, sub)).join('')
  if (VOID.has(name) || (!body && node.children.length === 0 && /^(path|circle|rect|line|polygon|polyline|ellipse|stop|use)$/.test(name))) return `<${name}${attrs} />`
  return `<${name}${attrs}>${body}</${name}>`
}

/* ── the artboard ──────────────────────────────────────────────────────── */

export function compile(html, label) {
  const xs = html.indexOf('<x-dc>')
  const xe = html.lastIndexOf('</x-dc>')
  if (xs < 0 || xe < 0) throw new Error('no <x-dc> block')
  const root = tree(tokenize(html.slice(xs + 6, xe)))
  let css = ''
  const body = []
  for (const k of root.children) {
    if (k.name === 'helmet') {
      for (const h of k.children) if (h.name === 'style') css += h.children.map((c) => c.raw || '').join('')
      continue
    }
    if (k.text !== undefined && !k.text.trim()) continue
    body.push(k)
  }
  const elements = body.filter((k) => k.name)
  const jsx = body.map((k) => emit(k, new Set(), { pre: false, root: elements.length === 1 && k === elements[0] })).join('')

  const sm = html.match(/<script type="text\/x-dc"[^>]*data-props='([^']*)'[^>]*>([\s\S]*?)<\/script>/)
  if (!sm) throw new Error('no <script type="text/x-dc"> block')
  const props = sm[1]
  const logicSrc = sm[2]
  if (!/class Component extends DCLogic/.test(logicSrc)) throw new Error('logic is not `class Component extends DCLogic`')

  const template = `/* ${label}: the founder's HQ Next artboard, ported markup for markup by
 * scripts/hq/dc2jsx.mjs. The markup is the design; edit it only to make the
 * product true (real data, real actions), never to restyle it. */
import React from 'react'
import { cssText } from '../../dc/runtime.jsx'
import './styles.css'

export default function Template({ v, link = (h) => h }) {
  return (<>${jsx}</>)
}
`
  const logic = `/* ${label}: the artboard's logic, verbatim from the canvas. renderVals()
 * returns everything template.jsx reads. */
import { DCLogic } from '../../dc/runtime.jsx'

export const PROPS = ${props}

export default ${logicSrc.trim()}
`
  return { template, logic, css: css.trim() + '\n' }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const [input, outDir] = process.argv.slice(2)
  if (!input || !outDir) {
    console.error('usage: dc2jsx.mjs <Name.dc.html> <outDir>')
    process.exit(2)
  }
  const label = basename(input)
  const r = compile(readFileSync(input, 'utf8'), label)
  mkdirSync(outDir, { recursive: true })
  writeFileSync(join(outDir, 'template.jsx'), r.template)
  writeFileSync(join(outDir, 'logic.js'), r.logic)
  writeFileSync(join(outDir, 'styles.css'), r.css)
  console.log(`${label} → ${outDir} (template ${r.template.length} B, logic ${r.logic.length} B, css ${r.css.length} B)`)
}

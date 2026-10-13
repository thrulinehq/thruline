/* ThruLine workspace — THE HUMANISER (DESIGN-readable-trace.md §1).
 *
 * PURE. No fetch, no network, no `window`, no React, no clock, no locale.
 * Same event in → same line out, always (§1.9 rule 7). The source record is
 * never changed: this is a presentation preview only, and every surface keeps
 * the untouched recorded value one disclosure deeper — the same discipline
 * `../lib/record-text.js` states in its own header.
 *
 * WHAT THIS IS NOT. Nothing here is written by a model, and nothing here reads
 * one. Every label below is a table lookup, a string parse, or an equality test
 * over a field the record already carries. The verb is a CONSTANT per tool name
 * and is never inferred from argument text (§1.9 rule 1): taking the first
 * token of a Bash command as the verb labels 59% of this corpus's cards `cd`
 * (ground-corpus §C.2).
 *
 * `Bash.description` is agent-authored text. It is never the headline and never
 * fills a missing command (§0 K2, §1.9 rule 2); it is reachable at level 2
 * under its own attribution, which is a consumer's job, not this file's.
 *
 * TWO FEEDERS, ONE FUNCTION (§1.1). `readableLine(view)` reads a normalised
 * view. `viewFromEntry(entry)` builds that view from the timeline entry the
 * server sends today — `{seq, kind, text, risk}`, already flattened and cut to
 * 50 characters at `crates/cli/src/explain_cmd.rs:435` (Stage B). A widened
 * transport (Stage A) would build the same view from the payload and call the
 * same function: fidelity changes, phrasing does not.
 *
 * NEVER `JSON.parse` ON THE STAGE-B PATH. 8.2% of this session's tool inputs
 * are cut mid-JSON at the 2,000-character capture cap (ground-corpus §C.3), and
 * the 50-character transport cut makes it ~100%. Everything is read by the
 * tolerant prefix extractor below, which reports whether the value it found was
 * complete or cut.
 */

import { countPhrase } from '../lib/voice.js'
import { ADVANCE_BODY, ADVANCE_MONO } from './logic.js'

/** The single elision glyph. One character, U+2026 — never three periods. */
export const ELLIPSIS = '…'

/* ── The budget, derived from the geometry rather than chosen (§1.6.1) ─────── */

/* `logic.js estH` is the height authority. It computes title lines as
 * `ceil(len * adv / cw)` with `cw = max(60, w - 16)`, and `w` is fixed at 260
 * by `adapter.js`'s auto-layout, so `cw = 244`. A `tool_call`/`exec`/`note`
 * card renders its title in mono (`shell.jsx`, `titleFont: n.mono`), whose
 * advance at 12px is ~7.2px; every other title renders proportional at ~6.6px.
 * Three lines, not two: the measured Bash first line is p50 156 characters
 * (ground-corpus §D.2), so two lines would cut the median command in half and
 * reproduce the current defect at a larger size. */
/* Imported, not restated: `logic.js titleLines` is the height authority and the
 * budget must read the SAME constant or the two disagree and cards overlap. */
export const ADV_MONO = ADVANCE_MONO
export const ADV_BODY = ADVANCE_BODY
export const CARD_CW = 244
export const TITLE_LINES = 3

/** Characters that fit `TITLE_LINES` lines at the given advance. */
export const budgetAt = (adv, cw) => TITLE_LINES * Math.floor((cw == null ? CARD_CW : cw) / adv)

export const TITLE_BUDGET_MONO = budgetAt(ADV_MONO)
export const TITLE_BUDGET_BODY = budgetAt(ADV_BODY)

/** Titles that render in mono, and therefore take the narrower budget. */
export const MONO_KINDS = Object.freeze(['tool_call', 'exec', 'note'])

/* Below this an object is not worth showing at all; the verb keeps the line. */
const MIN_OBJECT = 24

const budgetForKind = (kind) => (MONO_KINDS.indexOf(kind) >= 0 ? TITLE_BUDGET_MONO : TITLE_BUDGET_BODY)

/* ── Marker-safe, word-boundary truncation (§1.6.2) ────────────────────────── */

const MASK_OPEN = '[masked: '

/**
 * Cut `text` to `max` characters without splitting a redaction marker and
 * without splitting a word.
 *
 * Ported from `crates/core/src/redact.rs:1735 truncate_respecting_markers`
 * rather than written fresh: a new truncator would have to re-earn the
 * A209-C5/B7-4 repair, and `[masked: high_entropy]` markers occur in this
 * store's real text — splitting one strands `[masked:` on the card.
 *
 * @param {string} text @param {number} max
 * @returns {{text:string, elided:boolean}}
 */
export function truncateWords(text, max) {
  const chars = Array.from(String(text == null ? '' : text))
  const source = chars.join('')
  if (max <= 0) return chars.length ? { text: ELLIPSIS, elided: true } : { text: '', elided: false }
  let cut = Math.min(max, chars.length)

  /* The word boundary first, so the cut lands between words rather than
   * mid-token. A boundary in the first 40% is worse than a hard cut, so it is
   * declined. */
  let stop = cut
  while (stop > 0 && !/\s/.test(chars[stop - 1])) stop -= 1
  if (cut < chars.length && stop >= Math.floor(cut * 0.4)) cut = stop

  /* Then the marker repair, which can only shorten further and is therefore
   * the LAST word on where the cut lands: never leave a `[masked: …]` opened
   * and unclosed. Indices are code-point indices into `chars`, not string
   * offsets, so a surrogate pair cannot shift the repair by one. */
  const mask = Array.from(MASK_OPEN)
  let idx = 0
  const opens = []
  while (idx + mask.length <= chars.length) {
    if (chars.slice(idx, idx + mask.length).join('') === MASK_OPEN) {
      opens.push(idx)
      idx += mask.length
    } else idx += 1
  }
  for (const start of opens) {
    if (start >= cut) break
    const rel = chars.slice(start).indexOf(']')
    const end = rel >= 0 ? start + rel + 1 : -1
    if (end < 0) cut = Math.min(cut, start)
    else if (end > cut) cut = start
  }

  if (cut >= chars.length) return { text: source, elided: false }

  const kept = chars.slice(0, cut).join('').replace(/\s+$/, '')
  return { text: kept + ELLIPSIS, elided: true }
}

/** The first physical line, and how many lines followed it. */
export function firstLine(value) {
  const s = String(value == null ? '' : value)
  const nl = s.indexOf('\n')
  if (nl < 0) return { line: s, more: 0 }
  return { line: s.slice(0, nl).replace(/\s+$/, ''), more: s.slice(nl + 1).split('\n').length }
}

/* ── Shortening values so the meaningful end survives (§1.7) ───────────────── */

/**
 * Abbreviate a hash or an opaque id to twelve characters.
 *
 * Matches `crates/cli/src/explain_cmd.rs:487 short_hash`, which is registered
 * in `redact::NON_RENDERING_TRUNCATIONS` precisely because the input is a
 * 40-character hex object id this product publishes. Do not pick another
 * prefix length; an algorithm prefix (`sha256:`) is dropped, not counted.
 */
export function shortHash(value) {
  const s = String(value == null ? '' : value)
  const i = s.indexOf(':')
  const body = i >= 0 ? s.slice(i + 1) : s
  return body.slice(0, 12)
}

/**
 * Keep the informative end of a path, elide the middle once.
 *
 * Rules, in order (§1.7): fold the home prefix to `~/`; **if the result fits,
 * stop** — an ellipsis that elides nothing is a lie about length; otherwise
 * keep the root marker plus the first segment, elide the middle with a single
 * `…`, and always keep the last two segments whole, because the filename plus
 * its parent is what distinguishes `…/mac/shell.jsx` from
 * `…/workspace/shell.jsx`, both of which exist in this repository.
 */
export function shortenPath(value, budget) {
  const raw = String(value == null ? '' : value)
  if (!raw) return { text: '', elided: false }

  /* The home fold is generic over the account name rather than pinned to one
   * machine's; it is deterministic and reversible to a reader who knows the
   * host. Nothing else is folded to a symbol. */
  const home = /^\/(?:Users|home)\/[^/]+\//.exec(raw)
  const folded = home ? '~/' + raw.slice(home[0].length) : raw
  if (folded.length <= budget) return { text: folded, elided: false }

  const rooted = folded.startsWith('~/') || folded.startsWith('/')
  const head = folded.startsWith('~/') ? '~' : ''
  const body = folded.replace(/^~\//, '').replace(/^\//, '')
  const segs = body.split('/').filter((s) => s.length > 0)

  if (segs.length <= 2) return leftTruncate(segs.join('/') || folded, budget, rooted && !head ? '/' : head ? '~/' : '')

  const first = segs[0]
  const tail = segs.slice(-2).join('/')
  const prefix = head ? head + '/' + first : rooted ? '/' + first : first
  const composed = prefix + '/' + ELLIPSIS + '/' + tail
  if (composed.length <= budget) return { text: composed, elided: true }

  /* The last two segments alone do not fit: keep the filename, and if even
   * that overruns, truncate it FROM THE LEFT — the extension and the
   * distinguishing suffix are the informative end. */
  return leftTruncate(segs[segs.length - 1], budget, '')
}

function leftTruncate(name, budget, prefix) {
  const whole = prefix + name
  if (whole.length <= budget) return { text: whole, elided: false }
  const room = Math.max(1, budget - 1)
  return { text: ELLIPSIS + name.slice(Math.max(0, name.length - room)), elided: true }
}

/**
 * Scheme-less host plus the last path segment.
 *
 * **The query string is dropped from the readable line entirely** and belongs
 * one disclosure deeper: query strings carry tokens, and doctrine forbids
 * personal or sensitive data in a surfaced URL parameter.
 */
export function shortenUrl(value, budget) {
  const raw = String(value == null ? '' : value).trim()
  if (!raw) return { text: '', elided: false }
  const stripped = raw.replace(/^[a-z][a-z0-9+.-]*:\/\//i, '')
  const noQuery = stripped.split('?')[0].split('#')[0]
  const cutQuery = noQuery.length < stripped.length
  const parts = noQuery.split('/').filter((s) => s.length > 0)
  const host = parts.length ? parts[0] : noQuery
  const tail = parts.length > 1 ? parts[parts.length - 1] : ''
  const composed = tail ? host + '/' + tail : host
  const elidedPath = parts.length > 2
  if (composed.length <= budget) return { text: composed, elided: cutQuery || elidedPath }
  const cut = truncateWords(composed, budget)
  return { text: cut.text, elided: true }
}

/* ── The tolerant prefix extractor (§1.1) ──────────────────────────────────── */

const ESCAPES = Object.freeze({ '"': '"', '\\': '\\', '/': '/', b: '\b', f: '\f', n: '\n', r: '\r', t: '\t' })

function unescapeAt(s, i) {
  const c = s[i + 1]
  if (c === undefined) return null
  if (c === 'u') {
    const hex = s.slice(i + 2, i + 6)
    if (hex.length < 4 || !/^[0-9a-fA-F]{4}$/.test(hex)) return null
    return { ch: String.fromCharCode(parseInt(hex, 16)), next: i + 6 }
  }
  if (Object.prototype.hasOwnProperty.call(ESCAPES, c)) return { ch: ESCAPES[c], next: i + 2 }
  return null
}

/** Read the JSON string starting at `s[i] === '"'`. `null` when it is cut. */
function readString(s, i) {
  let j = i + 1
  let out = ''
  while (j < s.length) {
    const c = s[j]
    if (c === '\\') {
      const r = unescapeAt(s, j)
      if (!r) return null
      out += r.ch
      j = r.next
      continue
    }
    if (c === '"') return { value: out, next: j + 1 }
    out += c
    j += 1
  }
  return null
}

const keyNeedle = (key) => new RegExp('"' + key.replace(/[^A-Za-z0-9_]/g, '') + '"\\s*:\\s*')

/**
 * Pull one string field out of a possibly-truncated JSON prefix.
 *
 * @returns {{value:string, cut:boolean}|null} `cut` is true when the value ran
 * off the end of what survived — the record or the transport already removed
 * characters this reading can never recover.
 */
export function extractString(source, key) {
  const src = String(source == null ? '' : source)
  const m = keyNeedle(key).exec(src)
  if (!m) return null
  let i = m.index + m[0].length
  if (src[i] !== '"') return null
  i += 1
  let out = ''
  while (i < src.length) {
    const c = src[i]
    if (c === '\\') {
      const r = unescapeAt(src, i)
      if (!r) return { value: out, cut: true }
      out += r.ch
      i = r.next
      continue
    }
    if (c === '"') return { value: out, cut: false }
    out += c
    i += 1
  }
  return { value: out, cut: true }
}

/** A number field, only when the record shows it terminated. */
export function extractNumber(source, key) {
  const src = String(source == null ? '' : source)
  const m = new RegExp(keyNeedle(key).source + '(-?[0-9]+(?:\\.[0-9]+)?)\\s*[,}]').exec(src)
  return m ? Number(m[1]) : null
}

/** A boolean field, only when the record shows it terminated. */
export function extractBool(source, key) {
  const src = String(source == null ? '' : source)
  const m = new RegExp(keyNeedle(key).source + '(true|false)\\s*[,}\\]]').exec(src)
  return m ? m[1] === 'true' : null
}

/** How many elements an array field holds, or `null` when it was cut. */
export function countArrayItems(source, key) {
  const src = String(source == null ? '' : source)
  const m = new RegExp(keyNeedle(key).source + '\\[').exec(src)
  if (!m) return null
  let i = m.index + m[0].length
  let depth = 0
  let commas = 0
  let seen = false
  while (i < src.length) {
    const c = src[i]
    if (c === '"') {
      const r = readString(src, i)
      if (!r) return null
      seen = true
      i = r.next
      continue
    }
    if (c === '{' || c === '[') {
      depth += 1
      seen = true
      i += 1
      continue
    }
    if (c === ']') {
      if (depth === 0) return seen ? commas + 1 : 0
      depth -= 1
      i += 1
      continue
    }
    if (c === '}') {
      depth -= 1
      i += 1
      continue
    }
    if (c === ',' && depth === 0) {
      commas += 1
      i += 1
      continue
    }
    if (!/\s/.test(c)) seen = true
    i += 1
  }
  return null
}

/** The top-level key names of a JSON object prefix, or `null` when it was cut. */
export function topLevelKeys(source) {
  const s = String(source == null ? '' : source).trim()
  if (s[0] !== '{') return null
  const keys = []
  let i = 1
  let depth = 0
  let expectKey = true
  while (i < s.length) {
    const c = s[i]
    if (c === '"') {
      const r = readString(s, i)
      if (!r) return null
      if (depth === 0 && expectKey) {
        keys.push(r.value)
        expectKey = false
      }
      i = r.next
      continue
    }
    if (c === '{' || c === '[') {
      depth += 1
      i += 1
      continue
    }
    if (c === '}') {
      if (depth === 0) return keys
      depth -= 1
      i += 1
      continue
    }
    if (c === ']') {
      depth -= 1
      i += 1
      continue
    }
    if (c === ',' && depth === 0) {
      expectKey = true
      i += 1
      continue
    }
    i += 1
  }
  return null
}

/* ── Field access over the view: extractor first, parsed object second ─────── */

function fieldStr(view, key) {
  const hit = extractString(view.argsText, key)
  if (hit) return hit
  const args = view.args
  if (args && typeof args === 'object' && typeof args[key] === 'string') return { value: args[key], cut: false }
  return null
}

function fieldNum(view, key) {
  const hit = extractNumber(view.argsText, key)
  if (hit != null) return hit
  const args = view.args
  if (args && typeof args === 'object' && typeof args[key] === 'number') return args[key]
  return null
}

function fieldBool(view, key) {
  const hit = extractBool(view.argsText, key)
  if (hit != null) return hit
  const args = view.args
  if (args && typeof args === 'object' && typeof args[key] === 'boolean') return args[key]
  return null
}

function fieldCount(view, key) {
  const hit = countArrayItems(view.argsText, key)
  if (hit != null) return hit
  const args = view.args
  if (args && typeof args === 'object' && Array.isArray(args[key])) return args[key].length
  return null
}

function argKeys(view) {
  const hit = topLevelKeys(view.argsText)
  if (hit) return hit
  const args = view.args
  if (args && typeof args === 'object') return Object.keys(args)
  return null
}

/* ── Composition primitives ────────────────────────────────────────────────── */

const quote = (s) => '“' + s + '”'

/** De-underscore and capitalise the first letter; the rest is left alone so
 *  `TaskUpdate` stays `TaskUpdate` and `preview_eval` becomes `Preview eval`. */
export function toolWords(name) {
  const s = String(name == null ? '' : name).replace(/_/g, ' ').trim()
  return s ? s[0].toUpperCase() + s.slice(1) : ''
}

/**
 * Render one identifying argument at a budget, by its type.
 *
 * @param {string} value @param {'path'|'url'|'command'|'text'|'quoted'|'plain'|'hash'} type
 * @param {number} budget @param {boolean} cut whether the record already lost characters
 */
function renderObject(value, type, budget, cut) {
  if (value == null || value === '') return { text: '', elided: false }
  const room = Math.max(MIN_OBJECT, budget)
  if (type === 'path') {
    const r = shortenPath(value, room)
    return { text: r.text + (cut && !r.elided ? ELLIPSIS : ''), elided: r.elided }
  }
  if (type === 'url') {
    const r = shortenUrl(value, room)
    return { text: r.text + (cut && !r.elided ? ELLIPSIS : ''), elided: r.elided }
  }
  if (type === 'hash') return { text: shortHash(value), elided: false }
  const head = type === 'command' || type === 'text' ? firstLine(value) : { line: String(value), more: 0 }
  const wrap = type === 'quoted' ? 2 : 0
  const r = truncateWords(head.line, Math.max(MIN_OBJECT, room - wrap))
  const elided = r.elided || head.more > 0
  const text = r.text + (cut && !r.elided ? ELLIPSIS : '')
  return { text: type === 'quoted' ? quote(text) : text, elided }
}

/* ── The per-tool table (§1.3) ─────────────────────────────────────────────── */

/* Each rule returns a plan: a CONSTANT verb, the one identifying argument and
 * how to render it, and a noun-bearing detail. The verb never comes from the
 * argument text. `cut` travels with the plan so the line can end `…` for a
 * value the record itself no longer holds in full. */

const plan = (verb, rest) => Object.assign({ verb, object: null, type: 'plain', detail: '', cut: false }, rest || {})

function bashPlan(view) {
  const cmd = fieldStr(view, 'command')
  if (!cmd) return null
  const head = firstLine(cmd.value)
  /* Three states, kept apart. A whole multi-line command can be counted; a cut
   * one cannot — the lines that survived the prefix are a LOWER bound, and
   * printing one as if it were the total would be an assertion the record does
   * not carry. 69% of this session's commands are multi-line. */
  const detail = !cmd.cut ? (head.more > 0 ? ' · ' + countPhrase(head.more, 'more line', 'more lines') : '') : head.more > 0 ? ' · more lines not in this reading' : ''
  return plan('Ran', { object: cmd.value, type: 'command', detail, cut: cmd.cut })
}

function pathPlan(verb, view, extra) {
  const fp = fieldStr(view, 'file_path')
  if (!fp) return null
  return plan(verb, { object: fp.value, type: 'path', detail: extra || '', cut: fp.cut })
}

function readDetail(view) {
  const pages = fieldNum(view, 'pages')
  if (pages != null) return ' · ' + countPhrase(pages, 'page', 'pages')
  const offset = fieldNum(view, 'offset')
  const limit = fieldNum(view, 'limit')
  if (offset != null && limit != null) return ' · lines ' + offset + '–' + (offset + limit - 1)
  if (limit != null) return ' · lines 1–' + limit
  if (offset != null) return ' · from line ' + offset
  return ''
}

const TOOL_RULES = Object.freeze({
  /* The four that are 84.7% of the corpus and 96.2% of the target session. */
  Bash: bashPlan,
  Edit: (v) => pathPlan('Edited', v, fieldBool(v, 'replace_all') === true ? ' · every occurrence' : ''),
  Read: (v) => pathPlan('Read', v, readDetail(v)),
  Write: (v) => {
    const body = fieldStr(v, 'content')
    const chars = body && !body.cut ? body.value.length : null
    return pathPlan('Wrote', v, chars != null ? ' · ' + countPhrase(chars, 'character', 'characters') : '')
  },

  /* The measured tail. Every row is grounded in a measured key set. */
  TaskUpdate: (v) => {
    const id = fieldStr(v, 'taskId')
    const status = fieldStr(v, 'status')
    if (!id || !status) return null
    return plan('Set task', { object: shortHash(id.value) + ' to ' + status.value, type: 'plain', cut: status.cut })
  },
  TaskCreate: (v) => {
    const s = fieldStr(v, 'subject')
    return s ? plan('Created task', { object: s.value, type: 'quoted', cut: s.cut }) : null
  },
  StructuredOutput: (v) => {
    const keys = argKeys(v)
    /* Key count only: the shape varies per caller, and naming a field the
     * schema does not fix would be an assertion the record cannot carry. */
    return plan('Returned structured output', { detail: keys ? ' · ' + countPhrase(keys.length, 'field', 'fields') : '' })
  },
  ToolSearch: (v) => {
    const q = fieldStr(v, 'query')
    return q ? plan('Looked up tools matching', { object: q.value, type: 'quoted', cut: q.cut }) : null
  },
  Workflow: (v) => {
    const d = fieldStr(v, 'description')
    /* `description` IS the identifying argument of a Workflow call, not a gloss
     * on a command that also exists — there is no truer field to displace. */
    if (d) return plan('Ran workflow:', { object: d.value, type: 'text', cut: d.cut })
    const s = fieldStr(v, 'script')
    if (!s || s.cut) return plan('Ran workflow')
    return plan('Ran workflow', { detail: ' · ' + countPhrase(firstLine(s.value).more + 1, 'line of script', 'lines of script') })
  },
  TodoWrite: (v) => {
    const n = fieldCount(v, 'todos')
    return plan('Updated the todo list', { detail: n != null ? ' · ' + countPhrase(n, 'item', 'items') : '' })
  },
  SendMessage: (v) => {
    const to = fieldStr(v, 'to') || fieldStr(v, 'recipient')
    return to ? plan('Sent a message to', { object: to.value, type: 'plain', cut: to.cut }) : null
  },
  Monitor: (v) => {
    const c = fieldStr(v, 'command')
    return c ? plan('Watched', { object: c.value, type: 'command', cut: c.cut }) : null
  },
  Agent: (v) => {
    const t = fieldStr(v, 'subagent_type')
    if (!t) return null
    const d = fieldStr(v, 'description')
    return plan('Delegated to', { object: t.value + (d ? ': ' + d.value : ''), type: 'text', cut: d ? d.cut : t.cut })
  },
  AskUserQuestion: (v) => {
    const n = fieldCount(v, 'questions')
    return plan('Asked the operator', { detail: n != null ? ' · ' + countPhrase(n, 'question', 'questions') : '' })
  },
  TaskStop: (v) => {
    const id = fieldStr(v, 'task_id')
    return id ? plan('Stopped task', { object: id.value, type: 'hash' }) : null
  },
  TaskOutput: (v) => {
    const id = fieldStr(v, 'task_id')
    return id ? plan('Read output of task', { object: id.value, type: 'hash' }) : null
  },
  SendUserFile: (v) => {
    const n = fieldCount(v, 'files')
    return n != null ? plan('Sent', { object: countPhrase(n, 'file', 'files'), type: 'plain' }) : plan('Sent files')
  },
  DesignSync: (v) => {
    const m = fieldStr(v, 'method')
    return m ? plan('DesignSync', { object: m.value, type: 'plain', cut: m.cut }) : null
  },
  WebFetch: (v) => {
    const u = fieldStr(v, 'url')
    return u ? plan('Fetched', { object: u.value, type: 'url', cut: u.cut }) : null
  },
  Artifact: (v) => {
    const l = fieldStr(v, 'label')
    return l ? plan('Published artifact', { object: l.value, type: 'quoted', cut: l.cut }) : null
  },
  /* Measured: the empty-object case is real. The verb stands alone. */
  ListAgents: () => plan('Listed agents'),
  ScheduleWakeup: (v) => {
    if (fieldBool(v, 'stop') === true) return plan('Cancelled the wake-up')
    const d = fieldNum(v, 'delaySeconds')
    return d != null ? plan('Scheduled a wake-up in', { object: countPhrase(d, 'second', 'seconds'), type: 'plain' }) : plan('Scheduled a wake-up')
  },
  Skill: (v) => {
    const s = fieldStr(v, 'skill')
    return s ? plan('Invoked skill', { object: s.value, type: 'plain', cut: s.cut }) : null
  },
  WebSearch: (v) => {
    const q = fieldStr(v, 'query')
    return q ? plan('Searched the web for', { object: q.value, type: 'quoted', cut: q.cut }) : null
  },
  /* No location: the record has none. The one recorded call carries
   * `{output_mode, pattern}` and nothing else. Do not render a `path` field
   * that does not exist. */
  Grep: (v) => {
    const p = fieldStr(v, 'pattern')
    return p ? plan('Searched for', { object: p.value, type: 'quoted', cut: p.cut }) : null
  },
})

/* MCP tools: ~2,500 calls over ~50 names. The eight measured high-frequency
 * rows are named; every other `mcp__<server>__<method>` falls to the generic
 * rule — method as the verb, server in the detail. */
const MCP_RULES = Object.freeze({
  mcp__Claude_in_Chrome__browser_batch: (v) => {
    const n = fieldCount(v, 'actions')
    return plan('Ran a browser batch', { detail: n != null ? ' · ' + countPhrase(n, 'action', 'actions') : '' })
  },
  mcp__Claude_Browser__javascript_tool: () => plan('Ran JavaScript in the browser'),
  mcp__Claude_Preview__preview_eval: (v) => {
    const e = fieldStr(v, 'expression')
    return e ? plan('Evaluated', { object: e.value, type: 'text', cut: e.cut }) : plan('Evaluated an expression')
  },
  mcp__Claude_Browser__computer: (v) => {
    const a = fieldStr(v, 'action')
    return a ? plan('Browser:', { object: a.value, type: 'plain', cut: a.cut }) : plan('Browser action')
  },
  mcp__Claude_Browser__navigate: (v) => {
    const u = fieldStr(v, 'url')
    return u ? plan('Navigated to', { object: u.value, type: 'url', cut: u.cut }) : plan('Navigated')
  },
  mcp__ccd_session__mark_chapter: (v) => {
    const t = fieldStr(v, 'title')
    return t ? plan('Marked chapter', { object: t.value, type: 'quoted', cut: t.cut }) : plan('Marked a chapter')
  },
  mcp__ccd_session_mgmt__send_message: (v) => {
    const id = fieldStr(v, 'session_id')
    return id ? plan('Sent a message to session', { object: id.value, type: 'hash' }) : plan('Sent a message to a session')
  },
  mcp__Claude_Preview__preview_resize: (v) => {
    const w = fieldNum(v, 'width')
    const h = fieldNum(v, 'height')
    return w != null && h != null ? plan('Resized the preview to', { object: w + '×' + h, type: 'plain' }) : plan('Resized the preview')
  },
})

/* ── The universal fallback (§1.4) ─────────────────────────────────────────── */

/* The FIRST key present in this fixed priority list becomes the object. Fixed
 * so that the same payload always yields the same line, whatever order the
 * keys were serialised in. */
const FALLBACK_KEYS = Object.freeze([
  'command',
  'file_path',
  'path',
  'url',
  'pattern',
  'query',
  'expression',
  'prompt',
  'subject',
  'title',
  'label',
  'skill',
  'name',
  'message',
  'text',
])

const FALLBACK_TYPE = Object.freeze({
  command: 'command',
  file_path: 'path',
  path: 'path',
  url: 'url',
  pattern: 'quoted',
  query: 'quoted',
  expression: 'text',
  prompt: 'text',
  subject: 'quoted',
  title: 'quoted',
  label: 'quoted',
  skill: 'plain',
  name: 'plain',
  message: 'text',
  text: 'text',
})

function mcpParts(tool) {
  const m = /^mcp__(.+?)__(.+)$/.exec(String(tool || ''))
  return m ? { server: m[1].replace(/_/g, ' '), method: m[2] } : null
}

function fallbackPlan(view) {
  const tool = String(view.tool == null ? '' : view.tool)
  const mcp = mcpParts(tool)
  const serverTail = mcp ? ' · ' + mcp.server : ''
  /* A verb about the CALL is always true; a verb about the WORK would be a
   * guess. `Mark chapter` occurs in this store with a space in its name, so
   * tool names are not assumed to be identifier-shaped. */
  const verb = mcp ? toolWords(mcp.method) : 'Called ' + toolWords(tool)

  for (const key of FALLBACK_KEYS) {
    const hit = fieldStr(view, key)
    if (hit) return plan(verb, { object: hit.value, type: FALLBACK_TYPE[key], detail: serverTail, cut: hit.cut })
  }

  const keys = argKeys(view)
  if (keys && keys.length === 0) return plan(verb, { detail: serverTail })
  if (keys) return plan(verb, { detail: ' · ' + countPhrase(keys.length, 'argument', 'arguments') + serverTail })

  const text = String(view.argsText == null ? '' : view.argsText).trim()
  if (!text) return plan(verb, { detail: serverTail })
  /* Fail loud (Rule 12): never a blank card, and never the broken JSON as the
   * headline. The raw payload stays one click away. */
  return plan(verb, { detail: ' · argument text could not be read' + serverTail, cut: true })
}

/* ── The outcome slot — three values, never two (§1.5) ─────────────────────── */

/* `crates/capture/src/fold.rs` defines `ToolOutcome` as
 * `Succeeded | Failed | NotStated`. Collapsing `NotStated` into `Succeeded`
 * already shipped once as a bug; this reuses the codebase's own string rather
 * than inventing a fourth word. */
export function outcomeDetail(view) {
  if (view.ok === true) return ''
  if (view.ok === false) return ' · did not complete'
  return ' · outcome not stated'
}

/* ── The per-kind table (§1.2) ─────────────────────────────────────────────── */

/* `op` is inflected from the record's own word, never replaced by a synonym. */
const FS_OP = Object.freeze({ modify: 'Modified', create: 'Created', delete: 'Deleted', rename: 'Renamed' })

function planFor(view) {
  const kind = String(view.kind == null ? '' : view.kind)
  const raw = String(view.rawText == null ? '' : view.rawText)

  if (kind === 'session_start') return plan('Session started')
  if (kind === 'session_end') return plan('Session ended')

  if (kind === 'user_prompt') {
    const text = raw.trim()
    /* Never an empty card: 120 and 157 empty prompts are measured in the
     * legacy sessions, and an empty card is indistinguishable from a bug. */
    if (!text) return plan("The operator's message is not in this record")
    return plan('The operator asked', { object: raw, type: 'text', cut: !!view.textCut })
  }

  if (kind === 'tool_call') {
    const tool = String(view.tool == null ? '' : view.tool)
    const rule = TOOL_RULES[tool] || MCP_RULES[tool]
    const built = rule ? rule(view) : null
    return built || fallbackPlan(view)
  }

  if (kind === 'tool_result') {
    /* Low cardinality in the name, everything else in attributes, and the
     * OUTCOME IS NEVER PART OF THE NAME — OpenTelemetry's span-naming rule,
     * which is what makes today's `Bash ok` wrong twice over: it replaces the
     * object with the outcome, and it shouts. `tool: "unknown"` is 64.9% of
     * result rows store-wide; it is neither hidden nor guessed. */
    const tool = String(view.tool == null ? '' : view.tool)
    const toolTail = !tool ? '' : tool === 'unknown' ? ' · tool not recorded' : ' · ' + tool
    return plan('Result of a tool call', { detail: toolTail + outcomeDetail(view) })
  }

  if (kind === 'snapshot') {
    /* `unchanged` is never printed here: this reading compares nothing. The
     * word is earned by an equality test over two tree hashes, which belongs
     * to the group card, or it is not printed at all (§1.9 rule 3). */
    const foldTail = view.contemporaneous === false ? ' · taken at fold' : ''
    if (!view.treeHash) return plan('Working tree recorded', { detail: foldTail })
    return plan('Working tree recorded as', { object: 'tree ' + shortHash(view.treeHash), type: 'plain', detail: foldTail })
  }

  if (kind === 'fs_change') {
    const op = String(view.op == null ? '' : view.op)
    const verb = FS_OP[op.toLowerCase()] || (op ? op[0].toUpperCase() + op.slice(1) : 'Changed')
    if (!view.path) return plan(verb)
    return plan(verb, { object: view.path, type: 'path' })
  }

  if (kind === 'exec') {
    const argv = String(view.argv == null ? '' : view.argv)
    const detail = view.exitCode == null ? '' : ' · exit ' + view.exitCode
    if (!argv) return plan('Ran a command', { detail })
    return plan('Ran', { object: argv, type: 'command', detail })
  }

  if (kind === 'note') {
    /* Stage B cannot recover a legacy note's text: the transport delivers the
     * literal four-character word `note` and nothing else (1,590 of 1,591 note
     * rows in a legacy-imported session). Say that, rather than drawing the kind name. */
    if (!raw || raw === kind) return plan('The agent noted', { detail: ' · the note text is not in this reading' })
    return plan('The agent noted', { object: raw, type: 'text', cut: !!view.textCut })
  }

  if (kind === 'stop') {
    const text = raw.replace(/^stopped:\s*/, '')
    if (!text.trim()) return plan('The agent stopped')
    return plan('The agent stopped', { object: text, type: 'text', cut: !!view.textCut })
  }

  /* When `text` is the kind name the record supplied nothing, and a kind name
   * is not a sentence. `subagent_stop` draws its own identifier 47 times in the
   * target session today. */
  if (kind === 'subagent_stop') return plan('A subagent stopped')

  if (raw && raw !== kind) return plan('Recorded event · ' + kind, { object: raw, type: 'text', cut: !!view.textCut })
  return plan('Recorded event · ' + kind)
}

/* ── The function (§1.1) ───────────────────────────────────────────────────── */

/**
 * @typedef {object} Line
 * @property {string} verb from the fixed table; never derived from argument text
 * @property {string} object the one identifying argument, already shortened; may be ''
 * @property {string} detail noun-bearing, or ''
 * @property {boolean} elided true iff THIS function removed characters
 * @property {boolean} cutUpstream true iff the record or the transport had already cut the value
 * @property {boolean} mono recorded values render mono; derived phrasing does not
 */

/**
 * One readable line for one recorded event.
 *
 * Returns a struct, not a string: the card composes `verb + object + detail`,
 * the inspector composes differently (§1.8), and find-in-trace searches
 * `object` alone. One function, three consumers, no second renderer.
 *
 * @param {object} view the normalised event view (see `viewFromEntry`)
 * @returns {Line}
 */
export function readableLine(view) {
  const v = view || {}
  const kind = String(v.kind == null ? '' : v.kind)
  const p = planFor(v)
  const mono = MONO_KINDS.indexOf(kind) >= 0
  const budget = budgetForKind(kind)
  const room = budget - p.verb.length - p.detail.length - 2
  const rendered = renderObject(p.object, p.type, room, p.cut)
  return {
    verb: p.verb,
    object: rendered.text,
    detail: p.detail,
    elided: rendered.elided,
    cutUpstream: !!p.cut || !!v.argsCut || !!v.textCut,
    mono,
  }
}

/** The card's string: `verb + ' ' + object + detail`. */
export function composeLine(line) {
  if (!line) return ''
  const head = line.object ? line.verb + ' ' + line.object : line.verb
  return head + (line.detail || '')
}

/* ── Stage B: the view, built from the timeline entry the server sends ─────── */

/**
 * Build the normalised view from one `explain.what_happened[]` entry.
 *
 * The entry is exactly `{seq, kind, text, risk}`
 * (`crates/explain/src/timeline.rs`), already flattened and cut. Everything
 * here is recovery of structure the transport removed; a widened transport
 * would fill the same view directly and the phrasing would not change.
 *
 * @param {{seq?:number, kind?:string, text?:string, risk?:string}} entry
 */
export function viewFromEntry(entry) {
  const e = entry || {}
  const kind = String(e.kind == null ? '' : e.kind)
  let raw = e.text == null ? '' : String(e.text)
  const view = { kind, seq: e.seq == null ? null : e.seq, rawText: raw, args: null, argsText: '' }
  if (e.risk) view.risk = e.risk

  /* A trailing `…` is the transport's own claim of elision
   * (`explain_cmd.rs:508`). It is a fact about the record, not about the text. */
  const cutMark = raw.endsWith(ELLIPSIS)
  if (cutMark) {
    raw = raw.slice(0, -1)
    view.rawText = raw
    view.textCut = true
  }

  if (kind === 'tool_call') {
    if (typeof e.tool === 'string' && typeof e.input === 'string') {
      view.tool = e.tool
      view.argsText = e.input
      view.textCut = e.input.endsWith(ELLIPSIS)
      view.argsCut = view.textCut
      return view
    }
    /* `summarize` emits `"{tool} {input}"`. Split at the space that precedes
     * the object, because a tool name may contain a space — `Mark chapter`
     * occurs once in this store. */
    const brace = raw.indexOf(' {')
    const at = brace >= 0 ? brace : raw.indexOf(' ')
    view.tool = at > 0 ? raw.slice(0, at) : raw
    view.argsText = at > 0 ? raw.slice(at + 1) : ''
    if (cutMark) view.argsCut = true
    return view
  }

  if (kind === 'tool_result') {
    /* `ToolOutcome::label()` — three values, and the third is its own word. */
    if (raw.endsWith(' outcome not stated')) {
      view.tool = raw.slice(0, -' outcome not stated'.length)
      view.okBasis = 'not_stated'
    } else if (raw.endsWith(' FAILED')) {
      view.tool = raw.slice(0, -' FAILED'.length)
      view.ok = false
    } else if (raw.endsWith(' ok')) {
      view.tool = raw.slice(0, -' ok'.length)
      view.ok = true
    }
    return view
  }

  if (kind === 'snapshot') {
    const m = /^tree ([0-9a-fA-F]+)/.exec(raw)
    if (m) view.treeHash = m[1]
    if (/\(taken at fold\)/.test(raw)) view.contemporaneous = false
    return view
  }

  if (kind === 'fs_change') {
    const m = /^(\S+)\s+(.+)$/.exec(raw)
    if (m) {
      view.op = m[1]
      view.path = m[2]
    }
    return view
  }

  if (kind === 'exec') {
    const m = /^(.*) → exit (-?\d+)$/.exec(raw)
    if (m) {
      view.argv = m[1]
      view.exitCode = Number(m[2])
    } else view.argv = raw
    return view
  }

  return view
}

/* ── The disclosure ladder's level-2 rows (§1.8) ───────────────────────────── */

/**
 * The agent's own one-line description of a tool call, when this reading holds
 * it whole.
 *
 * **Never the headline** (§0 K2, §1.9 rule 2): this is agent-authored text, and
 * the gap between what the agent SAID it was doing and what it RAN is the
 * artefact this product exists to surface. It renders one disclosure deeper,
 * under its own attribution, and never fills a missing command.
 *
 * @returns {string|null} null when the record or the transport did not deliver it
 */
export function describedAs(entry) {
  const v = viewFromEntry(entry)
  if (v.kind !== 'tool_call') return null
  const hit = extractString(v.argsText, 'description')
  if (!hit || hit.cut || !hit.value) return null
  return hit.value
}

/**
 * The outcome, in words, for the inspector's `Outcome` row.
 *
 * Three values, never two. `ok_basis="event_name"` — 11,794 rows — means the
 * outcome was inferred from which harness event fired rather than read from an
 * exit code; that weaker basis travels with the claim, but only here, because
 * doctrine keeps mechanism vocabulary off the first level.
 *
 * @returns {string|null} null when the event is not a tool result
 */
export function outcomeSentence(entry) {
  const v = viewFromEntry(entry)
  if (v.kind !== 'tool_result') return null
  const head = v.ok === true ? 'The record states it completed.' : v.ok === false ? 'The record states it did not complete.' : 'The record does not state an outcome.'
  const basis = v.okBasis === 'event_name' ? ' Outcome read from the event name.' : ''
  const tool = v.tool && v.tool !== 'unknown' ? '' : ' The tool name was not recorded.'
  return head + basis + tool
}

/** `readableLine(viewFromEntry(entry))`, for callers that want both. */
export const readableEntry = (entry) => readableLine(viewFromEntry(entry))

/** The composed card string for one timeline entry. */
export const readableText = (entry) => composeLine(readableEntry(entry))

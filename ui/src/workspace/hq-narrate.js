// HQ Verdict Panel — the deterministic template engine and the grounding
// post-check. Pure functions, no React, no model. Step 1 of
// docs/HQ-VERDICT-PANEL-PLAN.md; mirrors p3 `crates/narrate/src/ground.rs`.
//
//   fill(template, values)   placeholder substitution; a missing value throws,
//                            so `{x}` never reaches a screen. Own properties
//                            only: an inherited name is a missing name. One
//                            closing full stop of a value is dropped where the
//                            template puts a full stop right after the slot, so
//                            the value still reads verbatim. Where the template
//                            closes a quotation and then a full stop after the
//                            slot (`"{x}".`), a value that ends its own sentence
//                            takes the place of that stop instead.
//   compose(template, values) fill for a sentence a panel reads: an empty or
//                            blank slot throws too, and values are trimmed.
//   pick(families, keys)     specific → general lookup; throws when no key hits.
//   ground(sentence, ctx)    the §3.4 post-check: refs must exist in the
//                            record, numbers must be licensed, no banned token
//                            (inflections included), no path in the manager
//                            register, sentence budget per register.

/// The 13 banned tokens, matched whole-word, case-insensitive, with the
/// enumerated inflections below. The list is the addendum's; adding to it is
/// the operator's call, not this file's.
export const BANNED_TOKENS = Object.freeze([
  'lie', 'lied', 'lying', 'liar', 'fake', 'faked', 'dishonest',
  'cheat', 'cheated', 'cheating', 'hallucinated', 'lazy', 'deceptive',
])

/// The suffixes p3 ruling M4 enumerates — an auditable table, not a stemmer.
const BANNED_SUFFIXES = ['s', 'es', 'ed', 'ing', 'ly', 'ness', 'y', 'ity', 'er', 'ors', 'ers']

export const SENTENCE_BUDGET = Object.freeze({ operator: 3, manager: 5 })

const PLACEHOLDER = /\{([a-z_]+)\}/g

/* Own properties only: `{constructor}` or `{__proto__}` must not resolve
 * through the prototype and render "function Object() { [native code] }". */
const own = (values, name) => (Object.prototype.hasOwnProperty.call(values, name) ? values[name] : undefined)

/* A slot the template follows with a full stop closes the sentence there. The value's trailing blanks are dropped,
 * and so is ONE closing full stop, because the template's stop takes its place: the value still reads verbatim
 * ("`src/cli.rs`.", "cd ..", "ls -la .", "main.."), and the boundary never adds a second stop ("`src/cli.rs`..") or
 * leaves a blank before the template's ("exited 1. ."). No other character is removed. A value's stop stays wherever
 * the template does not put a full stop right after the slot. Linear: nothing rescans a long run of stops or blanks. */
const closesSentence = (template, placeholder, offset) => template[offset + placeholder.length] === '.'
const dropClosingStop = (text) => {
  const t = text.trimEnd()
  return t.endsWith('.') ? t.slice(0, -1) : t
}

/* A slot the template closes with its own quotation mark and then a full stop, `"{slot}".`: a value that ends its own
 * sentence (".", "?", "!" or "…", before any trailing blanks) takes the place of that full stop, so a line never reads a
 * stop after a quotation that already ended ("…in `src/cli.rs`.". What you asked…"). The value is kept whole; only the
 * template's stop gives way. A value that does not end its sentence keeps it: "fix the totals". */
const SLOT = /\{([a-z_]+)\}("\.)?/g
const endsSentence = (text) => /[.!?…]$/.test(text.trimEnd())
const closeQuotation = (text) => (endsSentence(text) ? '"' : '".')

export function fill(template, values = {}) {
  if (typeof template !== 'string') throw new TypeError('fill: template must be a string')
  return template.replace(SLOT, (slot, name, quoteStop, offset) => {
    const v = own(values, name)
    if (v === undefined || v === null) throw new Error(`fill: missing placeholder value for {${name}}`)
    const text = String(v)
    if (quoteStop) return text + closeQuotation(text)
    return closesSentence(template, slot, offset) ? dropClosingStop(text) : text
  })
}

/// A slot `compose` could not fill. `kind` is 'missing_slot' or 'empty_slot'.
export class SlotError extends Error {
  constructor(kind, slot) {
    super(`compose: ${kind === 'empty_slot' ? 'empty' : 'missing'} placeholder value for {${slot}}`)
    this.kind = kind
    this.slot = slot
  }
}

/// `fill` for a sentence a panel reads. A missing, empty or blank value throws
/// a SlotError, because a template filled with nothing leaves a stray space
/// before its punctuation ("exited 0 .") or a hole ("A test in  changed").
/// Values are trimmed, and, as in `fill`, one closing full stop of a value is
/// dropped where the template puts a full stop right after the slot. Emptiness
/// is judged after that, so a value of one stop is refused, never rendered " .".
export function compose(template, values = {}) {
  if (typeof template !== 'string') throw new TypeError('compose: template must be a string')
  return template.replace(SLOT, (slot, name, quoteStop, offset) => {
    const v = own(values, name)
    if (v === undefined || v === null) throw new SlotError('missing_slot', name)
    let text = String(v).trim()
    if (!quoteStop && closesSentence(template, slot, offset)) text = dropClosingStop(text)
    if (!text) throw new SlotError('empty_slot', name)
    return quoteStop ? text + closeQuotation(text) : text
  })
}

export function placeholders(template) {
  const out = []
  for (const m of String(template).matchAll(PLACEHOLDER)) if (!out.includes(m[1])) out.push(m[1])
  return out
}

/// The slots whose value is someone else's recorded words, and whose words they are: `agent` for a claim's or a
/// promise's text, `user` for the user's own message or ask. A template writes one as `"{slot}"`: the straight quotes
/// are the template's quotation marks. Its value is never composed into ThruLine's sentence (`composeQuoted`).
export const QUOTE_SLOTS = Object.freeze({ claim_quote: 'agent', prompt_quote: 'user' })

const QUOTATION = new RegExp(`"\\{(${Object.keys(QUOTE_SLOTS).join('|')})\\}"`)

/// `compose` for a template that may carry one quotation: a Said line, a clicked body.
///
///   quote       { slot, text, ref }: the words to quote, and the ref of the record they are quoted from
///   recordText  ref → the text of the record `ref` resolves to, or undefined when it resolves to none
///
/// The template's own words (everything but the quotation) compose as `compose` composes them, from `values` with
/// every quotation slot removed. So a quotation slot written without its quotation marks, or a second quotation,
/// throws a SlotError: a quotation's value is never authored. The quotation itself is not composed. It is carried
/// verbatim as a part of its own, and only when it is exactly the text of the record its ref resolves to; otherwise
/// the reason is returned as `{ violation }`:
///   missing_quote         the template quotes a slot the caller supplied no quote for
///   quote_ref_unresolved  the ref resolves to no record (`ref` is null when there was none)
///   quote_mismatch        the quote is not exactly the text of the record the ref resolves to
///   empty_quote           that record's text, and so the quote, is empty or blank
///
/// Otherwise returns { text, authored, parts, quote, slots, violation: null }:
///   text      the line as plain text, the quotation inside the template's quotation marks
///   authored  the template's own words alone: what `ground` reads
///   parts     in order, [{ text }] and at most one { quote, speaker }: what a panel renders
///   slots     the placeholders the template's own words filled: the only values that may license their numbers
export function composeQuoted(template, values = {}, quote = null, recordText = () => undefined) {
  if (typeof template !== 'string') throw new TypeError('composeQuoted: template must be a string')
  const authoredValues = {}
  for (const name of Object.keys(values)) if (!Object.prototype.hasOwnProperty.call(QUOTE_SLOTS, name)) authoredValues[name] = values[name]
  const m = QUOTATION.exec(template)
  if (!m) {
    const text = compose(template, authoredValues)
    return { text, authored: text, parts: [{ text }], quote: null, slots: placeholders(template), violation: null }
  }
  const slot = m[1]
  const beforeTemplate = template.slice(0, m.index)
  const afterTemplate = template.slice(m.index + m[0].length)
  const before = compose(beforeTemplate, authoredValues)
  const composedAfter = compose(afterTemplate, authoredValues)
  if (!quote || quote.slot !== slot) return { violation: { kind: 'missing_quote', slot } }
  const ref = quote.ref == null ? null : quote.ref
  const record = ref == null ? undefined : recordText(ref)
  if (typeof record !== 'string') return { violation: { kind: 'quote_ref_unresolved', slot, ref } }
  if (quote.text !== record) return { violation: { kind: 'quote_mismatch', slot, ref } }
  if (!record.trim()) return { violation: { kind: 'empty_quote', slot, ref } }
  /* As in `fill` and `compose`: a quotation that ends its own sentence takes the place of the full stop the template
   * puts right after its closing quotation mark. That stop is the template's own first character, never a value's. */
  const after = afterTemplate.startsWith('.') && endsSentence(record) ? composedAfter.slice(1) : composedAfter
  return {
    text: `${before}"${record}"${after}`,
    authored: `${before}${after}`,
    parts: [...(before ? [{ text: before }] : []), { quote: record, speaker: QUOTE_SLOTS[slot] }, ...(after ? [{ text: after }] : [])],
    quote: record,
    slots: placeholders(beforeTemplate + afterTemplate),
    violation: null,
  }
}

export function pick(families, keys) {
  if (!Array.isArray(keys) || keys.length === 0) throw new Error('pick: keys must be a non-empty array')
  for (const key of keys) {
    if (Object.prototype.hasOwnProperty.call(families, key)) return { key, family: families[key] }
  }
  throw new Error(`pick: no family for any of ${keys.join(', ')}`)
}

/// The specific → general chain for a claim card.
export function verdictKeys(kind, state) {
  const chain = []
  if (kind) chain.push(`verdict/${kind}/${state}`)
  chain.push(`verdict/*/${state}`, 'verdict/*')
  return chain
}

// ── banned tokens ──────────────────────────────────────────────────────

function stemForms(out, stem) {
  out.add(stem)
  if (!stem.endsWith('e')) out.add(stem + 'e')
  if (stem.endsWith('i')) out.add(stem.slice(0, -1) + 'y')
  if (stem.endsWith('y')) out.add(stem.slice(0, -1) + 'i')
}

function bannedStems(word) {
  const lower = word.toLowerCase()
  const out = new Set()
  stemForms(out, lower)
  for (const suffix of BANNED_SUFFIXES) {
    if (lower.endsWith(suffix)) {
      const stem = lower.slice(0, -suffix.length)
      if (stem.length >= 3) stemForms(out, stem)
    }
  }
  return out
}

const BANNED_STEMS = BANNED_TOKENS.map((t) => [t, bannedStems(t)])

/// The banned token a single word is, or inflects, or null.
export function bannedWord(word) {
  for (const part of String(word).split(/[-/]/)) {
    const bare = part.replace(/[^A-Za-z0-9']/g, '').toLowerCase()
    if (!bare) continue
    if (BANNED_TOKENS.includes(bare)) return bare
    const candidate = bannedStems(bare)
    for (const [token, stems] of BANNED_STEMS) {
      for (const s of stems) if (candidate.has(s)) return token
    }
  }
  return null
}

/// First banned token in a text, as `{word, token}`, or null.
export function findBannedToken(text) {
  for (const word of String(text).split(/\s+/)) {
    const token = bannedWord(word)
    if (token) return { word, token }
  }
  return null
}

// ── paths, refs, numbers, sentences ────────────────────────────────────

/// A token reads as a file path when it carries a separator and an extension.
/// Deliberately generous: this keeps paths OUT of the manager register.
export function looksLikePath(token) {
  const t = String(token).replace(/^[("'`[]+|[)"'`\],.;:!?]+$/g, '')
  return /[/\\]/.test(t) && /\.[A-Za-z0-9]{1,8}$/.test(t)
}

const REF_CHIP = /\[([^\[\]\s]+)\]/g

export function refsIn(text) {
  return [...String(text).matchAll(REF_CHIP)].map((m) => m[1])
}

export function sentenceCount(text) {
  const t = String(text).trim()
  if (!t) return 0
  const ends = t.match(/[.!?]+["'”’)\]]*(?=\s|$)/g)
  return Math.max(1, ends ? ends.length : 0)
}

function digitRuns(text) {
  return String(text).match(/\d+/g) || []
}

/// The §3.4 post-check for one rendered sentence.
///
/// ctx.refs     record ids that exist (with or without brackets)
/// ctx.numbers  numbers the record licenses (numbers or strings; every digit
///              run inside each is licensed, so "14:28" licenses 14 and 28)
/// ctx.register 'manager' | 'operator'
/// ctx.chips    optional: the refs the caller appends as chips. When given,
///              they are the sentence's only refs and the whole text is prose,
///              so a bracket inside it (a route directory, a reason) is never
///              read as a ref. Without it, refs are read out of the text.
///
/// Returns { ok, role, text, violations }. A sentence with no ref is an
/// `inference`, which is a role, not a violation.
export function ground(sentence, ctx = {}) {
  const text = String(sentence).trim()
  const register = ctx.register === 'operator' ? 'operator' : 'manager'
  const known = new Set((ctx.refs || []).map((r) => String(r).replace(/^\[|\]$/g, '')))
  const licensed = new Set()
  for (const n of ctx.numbers || []) for (const run of digitRuns(n)) licensed.add(run)
  const violations = []

  const supplied = Array.isArray(ctx.chips)
  const refs = supplied ? ctx.chips.map((r) => String(r).replace(/^\[|\]$/g, '')) : refsIn(text)
  for (const id of refs) if (!known.has(id)) violations.push({ kind: 'unknown_ref', id })

  const prose = supplied ? text : text.replace(REF_CHIP, ' ')
  for (const run of digitRuns(prose)) {
    if (!licensed.has(run)) violations.push({ kind: 'unlicensed_number', number: run })
  }

  const banned = findBannedToken(prose)
  if (banned) violations.push({ kind: 'banned_token', token: banned.token, word: banned.word })

  if (register === 'manager') {
    for (const token of prose.split(/\s+/)) {
      if (looksLikePath(token)) { violations.push({ kind: 'path_in_manager_register', token }); break }
    }
  }

  const count = sentenceCount(text)
  const budget = SENTENCE_BUDGET[register]
  if (count > budget) violations.push({ kind: 'too_many_sentences', count, budget })

  return {
    ok: violations.length === 0,
    role: refs.length > 0 ? 'grounded' : 'inference',
    text,
    violations,
  }
}

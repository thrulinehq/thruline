/* §5v — the one voice, UI side.
 *
 * `voice.data.js` is GENERATED from crates/core/src/voice.rs and checked in.
 * Do not hand-edit it: `the_ui_mirror_has_not_drifted` fails the Rust suite the
 * moment it diverges, and the fix is always to change the Rust table and
 * regenerate, never the other way round. Doctrine (docs/UX-DOCTRINE.md) is the
 * source, Rust is the authority, this file is the mirror.
 *
 * Everything here is presentation. No verdict is decided in this file. */

import table from './voice.data.js'

const bySlug = (rows) => Object.fromEntries(rows.map((r) => [r.slug, r]))

export const VERDICT = bySlug(table.verdicts)
export const CURRENCY = bySlug(table.currencies)
export const LEDGER_STATE = bySlug(table.ledger_states)
export const PROOF_CURRENTNESS = bySlug(table.proof_currentness)
export const ABSTENTION = bySlug(table.abstentions)
/* Level two of the `why` disclosure, keyed by attention kind. R2 renders these
 * under the item sentence; exported here so no part of the authority sits in
 * the mirror unread — an unexported table is the first step of a drift. */
export const DISCLOSURE = bySlug(table.disclosures)
/* The same level, asked of a claim rather than an attention item — L2's
 * "what happened" line, keyed by state. */
export const CLAIM_STORY = bySlug(table.claim_stories)
export const READINESS = bySlug(table.readinesses)
export const JARGON = table.jargon
export const ENUMS = table.enums

/* The composition word for a state — the ONLY form allowed in a chip, badge,
 * or count. An unknown slug falls back to the "unclear" word rather than
 * printing the slug, which is how engine vocabulary reaches a screen. */
export const word = (slug) => (VERDICT[slug] || VERDICT.unknown).word

/** The first-level sentence for a state. */
export const sentence = (slug) => (VERDICT[slug] || VERDICT.unknown).sentence

/** What happened, one level behind the sentence. Mirrors `voice::claim_story`. */
export const claimStory = (slug) => (CLAIM_STORY[slug] || CLAIM_STORY.unknown).sentence

/** Chip label: the composition word, capitalized. */
export const chipLabel = (w) => (w ? w[0].toUpperCase() + w.slice(1) : '')

/* UX-DOCTRINE §4: a number alone is a bug. `n` may arrive already formatted
 * for the locale ("1,094"), so singularity is decided on the digits rather
 * than on the type — otherwise thousands separators would silently pluralize
 * wrong or force every call site to choose between a noun and a comma. */
export const countPhrase = (n, singular, plural) => {
  const digits = String(n).replace(/[^0-9]/g, '')
  return digits === '1' ? `1 ${singular}` : `${n} ${plural}`
}

/* A node badge: "3 done · 1 unproven". Zero-count elements are dropped here,
 * once, rather than at every call site — `Refused 0` shipped because the rule
 * lived at the call sites. */
export const composition = (parts) =>
  parts
    .filter(([n]) => n > 0)
    .map(([n, w]) => `${n} ${w}`)
    .join(' · ')

/** One sentence above the items. Calm is a sentence, never a blank screen. */
export const headline = (project, needingAttention) =>
  needingAttention === 0
    ? `${project} — everything checks out.`
    : `${project} — ${countPhrase(needingAttention, 'thing worth a look', 'things worth a look')}. Everything else checks out.`

/* L3 Trace's own copy, mirroring `voice::trace_caption` / `trace_foot` /
 * `trace_scope`. Free functions rather than table rows, for the same reason
 * `headline` is: they take an argument. The Rust side is the authority and the
 * engine-wide walk checks all three against the ban. */

/** F6: even Trace, once demoted, must always say what it is. */
export const traceCaption = (subject, day) =>
  day
    ? `You are looking at the recorded events around “${subject}”, ${day}.`
    : `You are looking at the recorded events around “${subject}”.`

/** What the window does not show, said out loud rather than left to assumption. */
export const traceFoot = (atWidest) =>
  atWidest
    ? 'This is as far as the maze opens. The rest of the run stays unread until you ask for it.'
    : 'Everything else in this run is out of frame. Widen to open one more corridor.'

export const traceScope = () => 'The maze only ever opens around what you asked about.'

/* The ban, mirroring crates/core/src/voice.rs::banned_in. Two shapes: jargon
 * anywhere, and a verdict enum used as the name of a state — either a whole
 * label slot that IS the enum, or the enum capitalized mid-sentence. Ordinary
 * English that happens to use the word ("Installation not verified") passes;
 * see the doctrine's note on why banning the substring makes copy worse. */
const labelSlots = (text) =>
  text.split(/[·,:;\n()]/).map((seg) => seg.replace(/^[^\p{L}]+/u, '').replace(/[^\p{L}]+$/u, ''))

export function bannedIn(text) {
  const hay = String(text).toLowerCase()
  const jargon = JARGON.find((term) => hay.includes(term))
  if (jargon) return jargon

  for (const slot of labelSlots(String(text))) {
    const hit = ENUMS.find((e) => e === slot.toLowerCase())
    if (hit) return hit
  }

  const s = String(text)
  for (const m of s.matchAll(/\p{Lu}[\p{L}]*/gu)) {
    const before = s.slice(0, m.index).trimEnd()
    const sentenceStart = before === '' || /[.!?:;—–\n]$/.test(before)
    if (sentenceStart) continue
    const hit = ENUMS.find((e) => e === m[0].toLowerCase())
    if (hit) return hit
  }
  return null
}

/* What people said was wrong with a statement, as the screens draw it (C30 R-LEDGER).
 *
 *   objectionsOf(servedClaim.objections) → [{id, who, at, when, reason, matches, line, meta}]
 *   objectedStatements(statements)       → [{id, step, plain, verdict, verdictWord, objections}]
 *
 * PURE: no fetch, no clock. The input is a claim's `objections[]` as the HQ
 * thread view serves it (`GET /api/hq/projects/<id>/threads/<tid>`,
 * `crates/core/src/statement_objection.rs`): who, when, the reason as typed
 * (already masked by the server) and `matches_sentence`. The key is absent when
 * nobody objected, and then this returns [] — nothing is made up.
 *
 * AN OBJECTION IS WORDS BESIDE A STATEMENT, NEVER A RESULT. Nothing here reads
 * or writes a verdict, a count, readiness or an approval; `objectedStatements`
 * carries the statement's own verdict through untouched so a screen can draw it
 * next to the objection, and does not compute one. When `matches_sentence` is
 * false the sentence is no longer the one the person objected to, and the line
 * says so instead of attaching the objection to the new wording.
 *
 * Writing an objection is not here. Its one address is declared beside its one
 * control (screens/Change-Brief/objection-actions.js).
 */
import { whenOf } from './notes.js'

const list = (x) => (Array.isArray(x) ? x : [])
const str = (x) => (typeof x === 'string' ? x : '')

/** The longest reason the record keeps, in characters (`statement_objection::MAX_REASON_CHARS`). */
export const OBJECTION_MAX_CHARS = 2000

/** The sentence the form and the answer say beside the box (`KEPT_BESIDE` in statement_objection.rs;
 *  objections.test.js holds the two together). */
export const KEPT_BESIDE = 'Your objection is kept beside this statement. It does not change what ThruLine says about it.'

/** What the line says when the sentence is no longer the one the person objected to. */
export const SENTENCE_CHANGED = 'The sentence changed since, so this is not carried over to the new wording.'

/** The verdict of a statement in the words the screens already use for it, or '' when unknown. */
export const VERDICT_WORDS = Object.freeze({
  proven: 'Proven',
  not_proven: 'Not proven',
  contradicted: 'Not proven',
  needs_recheck: 'Needs a re-check',
  recorded: 'Recorded',
})

/**
 * The objections a served claim carries, oldest first (the order the record
 * appended them). An entry with no words is not drawn.
 * @param {Array|null|undefined} served  the claim's `objections[]`
 */
export function objectionsOf(served) {
  return list(served)
    .filter((o) => o && typeof o === 'object' && str(o.reason).trim())
    .map((o) => {
      const who = str(o.actor).trim() || 'someone'
      const when = whenOf(o.at)
      const matches = o.matches_sentence !== false
      return {
        id: str(o.id),
        who,
        at: str(o.at),
        when,
        reason: str(o.reason),
        matches,
        /* "You said this is wrong: …" beside the statement; an earlier wording is said to be one */
        lead: matches ? 'You said this is wrong:' : 'You said an earlier wording of this was wrong:',
        /* who and when, the same way a note says it */
        meta: who + (when ? ' · ' + when : ''),
      }
    })
}

/**
 * The statements people objected to, each with the verdict it already had.
 * @param {Array} statements  the Change snapshot's `statements`
 */
export function objectedStatements(statements) {
  return list(statements)
    .filter((s) => s && list(s.objections).length)
    .map((s) => ({
      id: str(s.id),
      step: s.step == null ? null : s.step,
      plain: str(s.plain) || str(s.text),
      verdict: s.proof && s.proof.verdict ? String(s.proof.verdict) : null,
      verdictWord: (s.proof && VERDICT_WORDS[s.proof.verdict]) || '',
      objections: s.objections,
    }))
}

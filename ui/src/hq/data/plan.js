/* The plan rule, once (C21-b, C30 P0-6): Pro is a valid licence that allows
 * approving. Settings, Approve and Team-View all call this, and the plan
 * chip and every "Free / Pro" word on them come from it.
 *
 * It reads `GET /api/licence` (`thruline.licence.status` v4, served by
 * `crates/cli/src/licence_cmd.rs` `status_body`) and decides from two served
 * fields only:
 *   - `entitlements.approve_version.allowed`: the server's own explicit answer
 *     to "may this machine approve a version?" — the same `decide` the approve
 *     route runs;
 *   - `status`: the installed licence's verdict ("valid", "absent",
 *     "expired", …).
 * `plan` (the licence's plan field), any leftover v0.1.0 fields, and every other field are
 * not read: a valid Team licence that the server lets approve is Pro, and a
 * licence that says "pro" but may not approve is not.
 *
 * PURE: no fetch, no React.
 */

/** The only two plan words on any screen (C20). */
export const PLAN_WORDS = Object.freeze({ pro: 'Pro', free: 'Free' })

/** The server's explicit approving answer, or null when the document carries none. */
function approvingAnswer(doc) {
  const e = doc.entitlements && doc.entitlements.approve_version
  return e && typeof e === 'object' && typeof e.allowed === 'boolean' ? e.allowed : null
}

/**
 * `'pro'`, `'free'`, or `null` when the answer is not a licence document this
 * rule can read (absent, failed, malformed, or missing the explicit approving
 * answer). Null is never drawn as Free or as Pro.
 * @param {unknown} doc the /api/licence body
 */
export function planOf(doc) {
  if (!doc || typeof doc !== 'object' || Array.isArray(doc)) return null
  if (typeof doc.status !== 'string' || !doc.status) return null
  const allowed = approvingAnswer(doc)
  if (allowed == null) return null
  return allowed && doc.status === 'valid' ? 'pro' : 'free'
}

/**
 * The plan, its word, and whether approving is open, from one licence document.
 *
 * `approving` follows the server's answer: true only when it says allowed AND
 * either the licence is valid or the server says its paid gate is off (a build
 * where nothing needs a licence, so the plan stays Free while approving is
 * open). An answer that says "allowed" with the gate on and no valid licence
 * behind it is a v0.1.0-era document, and is not a grant.
 * @param {unknown} doc the /api/licence body
 * @returns {{plan: 'pro'|'free'|null, word: 'Pro'|'Free'|null, approving: boolean}}
 */
export function licencePlan(doc) {
  const plan = planOf(doc)
  const approving = plan != null && approvingAnswer(doc) === true && (doc.status === 'valid' || doc.paid_gate === false)
  return { plan, word: plan ? PLAN_WORDS[plan] : null, approving }
}

export default licencePlan

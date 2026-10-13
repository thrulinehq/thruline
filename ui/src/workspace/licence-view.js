/* The licence document as the screen reads it.
 *
 * PURE: no fetch and no React. The page, sidebar and disabled Approve share
 * the same words from HQ_COPY. A missing or partial answer never opens the
 * paid approval act. Data left by an older version cannot grant it either.
 */
import { HQ_COPY } from './hq-copy.js'
import { fill } from './hq-narrate.js'

const L = HQ_COPY.licence
const KNOWN = new Set(Object.keys(L.status))
const DORMANT_PLANS = new Set(['team'])

/**
 * @param {object|null} doc the /api/licence body, or null if it did not arrive
 */
export function licenceView(doc) {
  const status = doc && typeof doc.status === 'string' && KNOWN.has(doc.status) ? doc.status : 'unknown'
  const plan = doc && doc.plan && L.plans[doc.plan] ? L.plans[doc.plan] : null
  /* A previously issued plan can remain valid without advertising a plan
   * that nobody can buy now. */
  const dormant = Boolean(doc && !plan && DORMANT_PLANS.has(doc.plan))
  const day = doc && typeof doc.expires_on === 'string' && doc.expires_on ? doc.expires_on : null
  const gateOff = Boolean(doc && doc.paid_gate === false)
  const keyless = status === 'no_key' && gateOff

  let sentence
  let fix = null
  let line
  if (status === 'valid' && plan && day) {
    sentence = fill(L.status.valid.sentence, { plan, date: day })
    line = fill(L.line.valid, { plan, date: day })
  } else if (status === 'valid' && dormant && day) {
    sentence = fill(L.status.valid.sentence_unnamed, { date: day })
    line = fill(L.line.valid_unnamed, { date: day })
  } else if (status === 'valid') {
    /* A partial valid answer is unknown, never a grant. */
    return licenceView(null)
  } else if (status === 'absent') {
    /* C20: no licence is the Free plan, stated as a plan. The Pro rule is the
     * page's "what needs a licence" line, and the Approve refusal's reason;
     * with the paid gate off nothing needs Pro, so nothing points at it. */
    sentence = L.status.absent.plan
    fix = gateOff ? null : L.status.absent.fix
    line = L.line.absent
  } else {
    const copy = L.status[status]
    sentence = status === 'expired' ? fill(copy.problem, { date: day || '' }) : copy.problem
    fix = copy.fix
    line = status === 'unknown' ? L.line.unknown
      : keyless ? L.line.not_needed
        : L.line.problem
  }

  const entitlement = doc && doc.entitlements && doc.entitlements.approve_version
  let approve
  if (entitlement && entitlement.allowed === true && (gateOff || status === 'valid')) {
    approve = { allowed: true }
  } else {
    const slug = entitlement && typeof entitlement.status === 'string' ? entitlement.status : ''
    let pair
    let byLicence = true
    if (status === 'unknown' || !entitlement) {
      pair = [L.status.unknown.problem, L.status.unknown.fix]
      byLicence = false
    } else if ((slug === status || entitlement.allowed === true) && status !== 'valid') {
      /* The server names this licence state as the refusal, or an older
       * answer still claims approving is allowed with no licence behind it
       * (data a v0.1.0 server served). Either way the licence's own pair is
       * the reason.
       * An older server that refuses without naming why is not a licence
       * refusal the screen can vouch for: it falls to `unknown` below. */
      pair = status === 'absent' ? [L.status.absent.problem, L.status.absent.fix] : [sentence, fix]
      if (status === 'no_key') byLicence = false
    } else {
      pair = [L.status.unknown.problem, L.status.unknown.fix]
      byLicence = false
    }
    approve = { allowed: false, reason: pair[0], enabler: pair[1], plans: byLicence && !gateOff }
  }

  const technicalDoc = (doc && doc.technical) || {}
  const T = L.technical
  const technical = [
    [T.licence_id, technicalDoc.licence_id || T.none],
    [T.key_id, technicalDoc.key_id || T.none],
    [T.keys_built_in, typeof technicalDoc.keys_built_in === 'number' ? String(technicalDoc.keys_built_in) : T.none],
    [T.machine_bound, technicalDoc.machine_bound === true ? T.yes : technicalDoc.machine_bound === false ? T.no : T.none],
  ]

  return { status, sentence, fix, line, approve, technical, gateOff, offersInstall: !keyless }
}

/** Where the licence page lives. Every door uses one address. */
export const LICENCE_HASH = '#/licence'

/** The pricing page is a click-only browser link. */
export const PLANS_URL = 'https://thrulinehq.com/pricing'

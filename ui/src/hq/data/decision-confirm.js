/* "Confirm on this Mac": the Decisions lane's native confirmation, asked for
 * from HQ (C35, docs/decisions/API.md "C35 native review transport").
 *
 * A click in this page is NOT the person's confirmation. This file prepares an
 * act, reads back its exact review, and asks the server to open a separate
 * window on this Mac. The confirmation is the Confirm control in THAT window,
 * followed by the Mac's own presence check; only then can a receipt exist, and
 * only the job's final answer (`receipt_id` named) says it does.
 *
 * This is the one file under ui/src/hq that reaches these addresses
 * (`decision-confirm.test.js` holds the census):
 *
 *   GET  /api/hq/decisions/confirmation-token                         this launch's transport token
 *   POST /api/hq/projects/<p>/decisions/<d>/prepare                   (a) one inert act line
 *   POST /api/hq/projects/<p>/decisions/<d>/review                    (b) the exact review, read only
 *   POST /api/hq/projects/<p>/decisions/<d>/native-confirm            (c) register the native window
 *   GET  /api/hq/decisions/check?job_id=<id>                          (d) follow that job
 *
 * The three POSTs carry the Decisions token in `X-ThruLine-Run-Token`. It is
 * the Decisions lane's own token (the token route above), not the run route's
 * token, and it authorizes transport only: never presence, authorship or an
 * acceptance. The review and native-confirm bodies are exactly
 * `{schema_version, action, act_row_id[, expected_review_sha256]}`; the
 * browser never sends presence, uid, nonce, result or actor fields there
 * (the server refuses them), and `prepareBody` builds from a whitelist so no
 * other field can leave this page.
 *
 * Preparing appends an inert line the engine records as pending; nothing is
 * accepted, retired or set aside by it. Nothing in this file calls an act done
 * until the job names a receipt. */

const enc = encodeURIComponent

/** Header the Decisions token travels in (the name the server reads). */
export const TOKEN_HEADER = 'X-ThruLine-Run-Token'

/** GET: this launch's Decisions transport token. */
export const DECISION_TOKEN = '/api/hq/decisions/confirmation-token'
/** POST (a): prepare one inert act line. */
export const DECISION_PREPARE = (project, decision) => `/api/hq/projects/${enc(project)}/decisions/${enc(decision)}/prepare`
/** POST (b): the exact review of that line, and its SHA-256. Read only. */
export const DECISION_REVIEW = (project, decision) => `/api/hq/projects/${enc(project)}/decisions/${enc(decision)}/review`
/** POST (c): ask the server to open the native window for that review. */
export const DECISION_NATIVE_CONFIRM = (project, decision) => `/api/hq/projects/${enc(project)}/decisions/${enc(decision)}/native-confirm`
/** GET (d): one retained job, running or final. */
export const DECISION_JOB = (job) => `/api/hq/decisions/check?job_id=${enc(job)}`

/** How often a running confirmation job is read. */
export const POLL_MS = 1500

const SHA256 = /^[0-9a-f]{64}$/
const text = (v) => (typeof v === 'string' ? v.trim() : '')

/** The fields each action may send beyond `actor` and `because` (API.md, prepare). */
const OWN_FIELDS = Object.freeze({
  accept: ['revision', 'severity', 'applies_from_version'],
  retire: [],
  waive: ['version_id'],
})

/** What each action reads like once it is recorded (said only after a receipt). */
const DONE = Object.freeze({
  accept: 'You accepted this rule',
  retire: 'You retired this rule',
  waive: 'You set this rule aside for that version',
})

/**
 * The closed body for `prepare`, or the reason it cannot be sent. PURE. Built
 * from a whitelist: a field the action does not own is never copied, whatever
 * the caller passes. There is no default for the name or the reason.
 * @returns {{ok: true, body: object} | {ok: false, code: string}}
 */
export function prepareBody(action, input) {
  if (!Object.prototype.hasOwnProperty.call(OWN_FIELDS, action)) return { ok: false, code: 'unknown_action' }
  const given = input && typeof input === 'object' ? input : {}
  const actor = text(given.actor)
  const because = text(given.because)
  if (!actor) return { ok: false, code: 'name_required' }
  if (!because) return { ok: false, code: 'because_required' }
  const body = { schema_version: 1, action, actor, because }
  if (action === 'accept') {
    if (!Number.isInteger(given.revision) || given.revision < 1) return { ok: false, code: 'revision_required' }
    if (given.severity !== 'blocks' && given.severity !== 'warns') return { ok: false, code: 'severity_required' }
    if (!text(given.applies_from_version)) return { ok: false, code: 'version_required' }
    body.revision = given.revision
    body.severity = given.severity
    body.applies_from_version = text(given.applies_from_version)
  }
  if (action === 'waive') {
    if (!text(given.version_id)) return { ok: false, code: 'version_required' }
    body.version_id = text(given.version_id)
  }
  return { ok: true, body }
}

/**
 * The closed body for `review` (no digest) and `native-confirm` (with the
 * digest of the review the person was shown). PURE. Never carries presence,
 * uid, nonce, result or actor fields.
 * @returns {{ok: true, body: object} | {ok: false, code: string}}
 */
export function actBody(action, actRowId, sha) {
  if (!Object.prototype.hasOwnProperty.call(OWN_FIELDS, action)) return { ok: false, code: 'unknown_action' }
  if (!text(actRowId)) return { ok: false, code: 'act_required' }
  const body = { schema_version: 1, action, act_row_id: actRowId }
  if (sha !== undefined) {
    if (typeof sha !== 'string' || !SHA256.test(sha)) return { ok: false, code: 'digest_unreadable' }
    body.expected_review_sha256 = sha
  }
  return { ok: true, body }
}

const UNREACHABLE = Object.freeze({ ok: false, status: 0, doc: { error: 'unreachable' } })

async function send(f, method, url, { body, token } = {}) {
  if (typeof f !== 'function') return UNREACHABLE
  const headers = { Accept: 'application/json' }
  if (body !== undefined) headers['Content-Type'] = 'application/json'
  if (token) headers[TOKEN_HEADER] = token
  try {
    const res = await f(url, { method, cache: 'no-store', headers, body: body === undefined ? undefined : JSON.stringify(body) })
    const doc = await res.json().catch(() => null)
    return { ok: !!res.ok, status: res.status, doc }
  } catch (_e) {
    return UNREACHABLE
  }
}

const local = (code) => ({ ok: false, status: 0, doc: { error: code }, local: true })

/**
 * The four requests, one at a time. Every method resolves to
 * `{ok, status, doc}` and never throws; a request that got no answer is
 * status 0. The token is read once and kept until the server says it is not
 * ours (then the next request reads it again).
 */
export function confirmClient(f = typeof fetch === 'function' ? fetch : null) {
  let token = null
  async function post(url, built) {
    if (!built.ok) return local(built.code)
    if (!token) {
      const t = await send(f, 'GET', DECISION_TOKEN)
      const given = t.doc && typeof t.doc.token === 'string' ? t.doc.token : ''
      if (!t.ok || !given) return t.ok ? local('token_unreadable') : t
      token = given
    }
    const res = await send(f, 'POST', url, { body: built.body, token })
    if (res.status === 403 && res.doc && res.doc.error === 'confirmation_token_required') token = null
    return res
  }
  return {
    prepare: (project, decision, action, input) => post(DECISION_PREPARE(project, decision), prepareBody(action, input)),
    review: (project, decision, act) => post(DECISION_REVIEW(project, decision), actBody(act && act.action, act && act.actRowId)),
    nativeConfirm: (project, decision, act) => post(DECISION_NATIVE_CONFIRM(project, decision), actBody(act && act.action, act && act.actRowId, act && act.sha)),
    job: (id) => (text(id) ? send(f, 'GET', DECISION_JOB(id)) : Promise.resolve(local('job_required'))),
  }
}

/* ---- what a refusal says ------------------------------------------------- */

/** Plain words for each refusal the Decisions routes send, or this file makes
 *  before sending. The server's own `detail` is appended as given. */
const WORDS = Object.freeze({
  // made here, before anything is sent
  name_required: 'Type your name first.',
  because_required: 'Say why first. There is no default reason.',
  severity_required: 'Choose whether this rule blocks approval or only warns.',
  revision_required: 'ThruLine did not say which revision of the rule this is.',
  version_required: 'ThruLine has no exact commit to name for this.',
  unknown_action: 'ThruLine does not know that action.',
  act_required: 'There is no prepared line to look at yet.',
  digest_unreadable: 'The review’s digest came back in a form this page cannot send.',
  token_unreadable: 'ThruLine’s answer about this page’s permission could not be read.',
  job_required: 'There is no confirmation to follow.',
  // the server's
  confirmation_token_required: 'ThruLine did not accept this page’s permission to ask for a confirmation. Reload this page and try again.',
  confirmation_token_unavailable: 'ThruLine could not give this page its permission to ask for a confirmation.',
  same_origin_required: 'ThruLine takes this only from its own page. Reload this page and try again.',
  decision_not_found: 'This rule is no longer in the record ThruLine is reading. Reload this page.',
  act_not_found: 'The prepared line is no longer in the record ThruLine is reading.',
  malformed_request: 'ThruLine could not read what this page sent.',
  invalid_json_body: 'ThruLine could not read what this page sent.',
  invalid_prepare_body: 'ThruLine could not use what this page sent for this action.',
  invalid_native_body: 'ThruLine could not use what this page sent for this action.',
  review_sha256_required: 'ThruLine did not get the review’s digest.',
  exact_version_required: 'ThruLine could not resolve that version to an exact commit.',
  same_member_successor_required: 'The rule that replaces this one must be in the same project record.',
  preparation_conflict: 'ThruLine’s record refused to prepare this.',
  review_unavailable: 'ThruLine could not produce the review for this line. A review that holds values the masker would hide is not shown.',
  review_moved: 'The record changed since you read the review. Read the review again before you confirm.',
  native_confirmation_unavailable: 'Confirming on this Mac is not available right now.',
  native_confirmation_busy: 'Another confirmation window is already open on this Mac. Finish or close it, then try again.',
  job_store_unavailable: 'ThruLine could not register the confirmation, so no window was opened.',
  store_unavailable: 'ThruLine could not read its record just now.',
  upgrade_pending: 'ThruLine needs an upgrade before it can record anything. Run thruline upgrade in a terminal, then try again.',
  body_too_large: 'What you typed is too long to send.',
  unreachable: 'ThruLine did not answer, so it may not have received this.',
})

const TAIL = Object.freeze({ prepare: ' Nothing was prepared.', review: ' Nothing was confirmed.', confirm: ' Nothing was confirmed.' })

/**
 * One refusal, in words. PURE. Every refusal the server sends is shown: its
 * plain sentence when this file has one, otherwise its code, and always its
 * own `detail` as given.
 * @param {{ok?: boolean, status?: number, doc?: any}} res
 * @param {'prepare'|'review'|'confirm'} step
 * @returns {{code: string, status: number, text: string}}
 */
export function refusalOf(res, step) {
  const r = res && typeof res === 'object' ? res : {}
  const doc = r.doc && typeof r.doc === 'object' ? r.doc : {}
  const status = typeof r.status === 'number' ? r.status : 0
  let code = typeof doc.error === 'string' ? doc.error : ''
  if (!code && status === 413) code = 'body_too_large'
  if (!code && r.ok) code = 'unreadable'
  const detail = typeof doc.detail === 'string' && doc.detail ? doc.detail : null
  const known = code === 'unreadable'
    ? 'ThruLine answered in a form this page cannot read.'
    : WORDS[code] || `ThruLine refused this${status ? ` (it answered ${status})` : ''}${code ? `: ${code}` : ''}.`
  const tail = code === 'unreachable' ? '' : TAIL[step] || ''
  return { code: code || 'other', status, text: known + (detail ? ' ' + detail : '') + tail }
}

/* ---- what the job says --------------------------------------------------- */

/**
 * What one `NativeConfirmationJob` means for the screen. PURE. Only a
 * `complete` job that names a `receipt_id` is a recorded act; a running job is
 * never presented as one, and a finished job without a receipt says so.
 * @returns {{kind: 'running'|'recorded'|'recorded_file_stale'|'no_receipt'|'failed'|'unreadable', final: boolean, receiptId: string|null, errors: string[], sentence: string}}
 */
export function jobOutcome(job, action) {
  const j = job && typeof job === 'object' ? job : {}
  const errors = Array.isArray(j.errors) ? j.errors.filter((e) => typeof e === 'string' && e) : []
  const receiptId = typeof j.receipt_id === 'string' && j.receipt_id ? j.receipt_id : null
  const label = j.progress && typeof j.progress.label === 'string' && j.progress.label ? j.progress.label : null
  const done = DONE[action] || 'Your confirmation'
  const said = errors.join(' ')
  if (j.state === 'running') {
    return {
      kind: 'running',
      final: false,
      receiptId: null,
      errors,
      sentence: `${label ? label + '. ' : ''}Waiting for you in the window on this Mac. Nothing is recorded until you choose Confirm there and pass the Mac’s own presence check.`,
    }
  }
  if (j.state === 'complete') {
    if (!receiptId) {
      return { kind: 'no_receipt', final: true, receiptId: null, errors, sentence: 'The confirmation finished but names no receipt, so nothing is recorded as confirmed.' + (said ? ' ' + said : '') }
    }
    if (j.derived_written === false) {
      return {
        kind: 'recorded_file_stale',
        final: true,
        receiptId,
        errors,
        sentence: `${done}, and the receipt is recorded (${receiptId}). The decisions file in the project did not refresh${said ? ': ' + said : '.'} The record has the receipt; the copy in the project needs repairing.`,
      }
    }
    return {
      kind: 'recorded',
      final: true,
      receiptId,
      errors,
      sentence: `${done}. It is recorded as receipt ${receiptId}.${j.derived_written === true ? ' The decisions file in the project was refreshed.' : ''}`,
    }
  }
  if (j.state === 'failed') {
    return { kind: 'failed', final: true, receiptId: null, errors, sentence: `Not confirmed. ${said || 'ThruLine gave no reason.'} No receipt was recorded.` }
  }
  return { kind: 'unreadable', final: true, receiptId: null, errors, sentence: 'ThruLine answered about this confirmation in a form this page cannot read, so nothing is shown as confirmed.' }
}

/** The review as the person is shown it: the facts first, then every byte. PURE. */
export function reviewFacts(review) {
  const r = review && typeof review === 'object' ? review : {}
  const d = r.decision && typeof r.decision === 'object' ? r.decision : {}
  const rows = []
  const put = (label, value) => {
    if (value !== undefined && value !== null && value !== '') rows.push([label, String(value)])
  }
  put('Action', r.action)
  put('The rule', d.text)
  put('Kind', d.kind)
  put('Revision', d.revision != null ? d.revision + (d.revision_basis ? ' (' + String(d.revision_basis).replace(/_/g, ' ') + ')' : '') : null)
  put('Parameters SHA-256', d.params_sha256)
  return { summary: typeof r.summary === 'string' ? r.summary : null, rows, json: JSON.stringify(r, null, 2) }
}

/* ---- the flow ------------------------------------------------------------ */

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))
const LOST = 'ThruLine no longer holds this confirmation (the server may have restarted), so it cannot say how it ended. Nothing is shown as confirmed. Reload this page to see what the record holds.'

/**
 * One person's walk through one action, as a small state machine the screen
 * subscribes to. Each method is one step and one request; none runs the next
 * step except `prepare`, which reads the review back at once (that read is
 * harmless: no prompt, no person act). `confirm` runs only after a review has
 * been read, and sends that review's digest.
 *
 *   step   'form' → 'prepared' → 'review' → 'following' → 'ended'
 *          (with 'reviewing' / 'starting' while a request is out)
 *   act    {action, actRowId} the prepared line, once there is one
 *   review {sha, doc} the exact review as served
 *   job    the last job document read
 *   outcome  jobOutcome(...) of that job, once there is one
 *   refusal  {code, status, text} the last refusal, shown with its text
 *
 * @param {{client: object, project: string, decision: string, action: string, wait?: (ms: number) => Promise<void>, onRecorded?: (job: object) => void}} o
 */
export function confirmFlow({ client, project, decision, action, wait = sleep, onRecorded = () => {} }) {
  let state = { step: 'form', busy: false, act: null, review: null, job: null, outcome: null, refusal: null }
  const subs = new Set()
  let stopped = false
  let following = false
  const set = (patch) => {
    state = { ...state, ...patch }
    subs.forEach((fn) => fn(state))
  }
  const api = {
    get state() {
      return state
    },
    subscribe(fn) {
      subs.add(fn)
      return () => subs.delete(fn)
    },
    /** Stop reading the job (the screen has gone). The window on the Mac is not affected. */
    stop() {
      stopped = true
    },
    /** (a) prepare one inert line, then (b) read its review back. */
    async prepare(input) {
      if (state.busy || stopped) return
      set({ busy: true, refusal: null })
      const res = await client.prepare(project, decision, action, input)
      if (stopped) return
      const doc = res.doc && typeof res.doc === 'object' ? res.doc : {}
      if (!(res.ok && typeof doc.act_row_id === 'string' && doc.act_row_id && doc.state === 'pending_bridge')) {
        set({ busy: false, refusal: refusalOf(res, 'prepare') })
        return
      }
      set({ busy: false, step: 'prepared', act: { action: ['accept', 'retire', 'waive'].includes(doc.action) ? doc.action : action, actRowId: doc.act_row_id } })
      await api.review()
    },
    /** (b) read the exact review of the prepared line. Read only. */
    async review() {
      if (state.busy || stopped || !state.act) return
      set({ busy: true, refusal: null, review: null, step: 'reviewing' })
      const res = await client.review(project, decision, state.act)
      if (stopped) return
      const doc = res.doc && typeof res.doc === 'object' ? res.doc : {}
      if (res.ok && SHA256.test(doc.review_sha256 || '') && doc.review && typeof doc.review === 'object') {
        set({ busy: false, step: 'review', review: { sha: doc.review_sha256, doc: doc.review } })
      } else {
        set({ busy: false, step: 'prepared', refusal: refusalOf(res, 'review') })
      }
    },
    /** (c) ask for the native window, for the review that was read; then follow the job. */
    async confirm() {
      if (state.busy || stopped || !state.act || !state.review) return
      const shown = state.review
      set({ busy: true, refusal: null, step: 'starting' })
      const res = await client.nativeConfirm(project, decision, { ...state.act, sha: shown.sha })
      if (stopped) return
      const job = res.doc && typeof res.doc === 'object' ? res.doc : {}
      if (res.ok && typeof job.job_id === 'string' && job.job_id && job.kind === 'native_confirmation') {
        set({ busy: false, step: 'following', job, outcome: jobOutcome(job, state.act.action) })
        await api.follow()
        return
      }
      const refusal = refusalOf(res, 'confirm')
      const moved = refusal.code === 'review_moved'
      set({ busy: false, step: moved ? 'prepared' : 'review', review: moved ? null : shown, refusal })
    },
    /** (d) read the job until it is final. Safe to call again after it gave up. */
    async follow() {
      if (stopped || following || !state.job) return
      following = true
      let silent = 0
      const acted = state.act ? state.act.action : action
      try {
        for (;;) {
          const id = state.job.job_id
          const res = await client.job(id)
          if (stopped) return
          const doc = res.doc && typeof res.doc === 'object' ? res.doc : null
          if (res.ok && doc && doc.kind === 'native_confirmation' && doc.job_id === id) {
            silent = 0
            const outcome = jobOutcome(doc, acted)
            set({ job: doc, outcome, step: outcome.final ? 'ended' : 'following' })
            if (outcome.final) {
              if (outcome.receiptId) onRecorded(doc)
              return
            }
          } else if (res.status === 404) {
            set({ step: 'ended', outcome: { kind: 'lost', final: true, receiptId: null, errors: [], sentence: LOST } })
            return
          } else if (res.status === 0) {
            silent += 1
            if (silent >= 3) {
              set({ step: 'ended', outcome: { kind: 'unreachable', final: false, receiptId: null, errors: [], sentence: 'ThruLine stopped answering while the confirmation was open. Nothing is shown as confirmed.' } })
              return
            }
          } else {
            set({ step: 'ended', outcome: { kind: 'unreadable', final: true, receiptId: null, errors: [], sentence: `ThruLine answered about this confirmation in a form this page cannot read (it answered ${res.status}), so nothing is shown as confirmed.` } })
            return
          }
          await wait(POLL_MS)
          if (stopped) return
        }
      } finally {
        following = false
      }
    },
    /** Back to the form for a fresh line. The old line stays pending in the record; it does nothing. */
    reset() {
      if (state.busy || stopped) return
      set({ step: 'form', act: null, review: null, job: null, outcome: null, refusal: null })
    },
  }
  return api
}

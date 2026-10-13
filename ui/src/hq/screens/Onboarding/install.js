/* First run's Install (C30 R-ROUTES "Install"; census §3.11 rows 5, 6, 10, 11).
 *
 *   GET  /api/hq/projects/<id>/install   the plan: for each part, the one file
 *                                        it would write, what it would add,
 *                                        where it is already installed, or
 *                                        why it is refused. Writes nothing.
 *   POST /api/hq/projects/<id>/install   {hooks, mcp, speaker, run_capture,
 *                                        plan}: exactly the boxes the person
 *                                        ticked, against the plan the page
 *                                        showed. The click is the consent.
 *
 * The POST carries this launch's run token (`GET /api/run/token`), read and
 * sent exactly as the run routes' clients send it (Repair-Loop/actions.js),
 * and the browser's own same-origin `Origin`. This is the one write address
 * this screen reaches besides the load (CLAUDE.md, the write-address table).
 *
 *   POST /api/hq/projects/<id>/uninstall {part: "speaker", plan}: write
 *                                        address 30 (C46 §1.2(2)), the same
 *                                        guards; reached by Settings' speaker
 *                                        switch, never by First run.
 *
 * Rules (C30 G3): "Installed" only when the plan read the entries in a file;
 * a part the plan could not read is said as such, never as installed or not.
 */
import { useEffect, useRef, useState } from 'react'

/** The plan's address, and the click's. */
export const INSTALL_ADDRESS = (id) => `/api/hq/projects/${encodeURIComponent(id)}/install`
/** This launch's token, as the run routes read it. */
export const RUN_TOKEN = '/api/run/token'
/** The shape this client reads (the route's `schema_version`). */
export const INSTALL_SCHEMA_VERSION = 1

const UNREADABLE_SHAPE = 'ThruLine answered in a form this page cannot read, so nothing could be checked.'
const PARTS = ['hooks', 'speaker', 'mcp', 'run_capture']

/**
 * One read of the plan. Resolves to the served document; rejects with a sentence.
 * @param {string} id the HQ project id
 */
export async function readInstall(id, { fetchImpl = typeof fetch === 'function' ? fetch : null, signal } = {}) {
  if (!fetchImpl) throw new Error('This browser cannot reach ThruLine’s local server.')
  let res
  try {
    res = await fetchImpl(INSTALL_ADDRESS(id), { method: 'GET', signal, cache: 'no-store', credentials: 'same-origin', headers: { accept: 'application/json' } })
  } catch (e) {
    if (e && e.name === 'AbortError') throw e
    throw new Error('ThruLine’s local server did not answer. Is thruline ui still running?')
  }
  if (res.status === 404) throw new Error('ThruLine no longer knows this folder. Reload the page and choose it again.')
  if (res.status === 503) throw new Error('ThruLine could not open its record, so nothing could be checked.')
  if (!res.ok) throw new Error(`ThruLine could not read what Install would add (it answered ${res.status}).`)
  let doc
  try {
    doc = JSON.parse(await res.text())
  } catch {
    throw new Error(UNREADABLE_SHAPE)
  }
  if (!validPlan(doc, id)) throw new Error(UNREADABLE_SHAPE)
  return doc
}

function validPlan(doc, id) {
  return !!doc && doc.schema_version === INSTALL_SCHEMA_VERSION && !!doc.project && doc.project.id === id
    && typeof doc.plan === 'string' && !!doc.parts && PARTS.every((p) => doc.parts[p] && typeof doc.parts[p].state === 'string')
}

async function answer(res) {
  const doc = await res.json().catch(() => null)
  return { ok: res.ok, status: res.status, doc }
}

/**
 * The click: exactly the ticked boxes, against the plan on screen.
 * @param {string} id
 * @param {{mcp: boolean, speaker: boolean, runcap: boolean}} choices
 * @param {string} plan the plan digest the page showed
 * @returns {Promise<{ok: boolean, status: number, doc: object|null}>}
 */
export async function postInstall(id, choices, plan, f = typeof fetch === 'function' ? fetch : null) {
  if (!f) return { ok: false, status: 0, doc: { error: 'unreachable' } }
  let token
  try {
    token = await answer(await f(RUN_TOKEN, { cache: 'no-store', headers: { Accept: 'application/json' } }))
  } catch (_e) {
    return { ok: false, status: 0, doc: { error: 'unreachable' } }
  }
  if (!token.ok || !token.doc || typeof token.doc.token !== 'string') {
    return { ok: false, status: token.status, doc: token.doc || { error: 'run_token_required' } }
  }
  const c = choices || {}
  const body = { hooks: true, mcp: !!c.mcp, speaker: !!c.speaker, run_capture: !!c.runcap, plan: String(plan || '') }
  try {
    return await answer(await f(INSTALL_ADDRESS(id), {
      method: 'POST',
      cache: 'no-store',
      headers: { Accept: 'application/json', 'Content-Type': 'application/json', 'X-ThruLine-Run-Token': token.doc.token },
      body: JSON.stringify(body),
    }))
  } catch (_e) {
    return { ok: false, status: 0, doc: { error: 'unreachable' } }
  }
}

/** Taking one part back (C46 §1.2(2), write address 30): Settings' speaker switch, Off. */
export const UNINSTALL_ADDRESS = (id) => `/api/hq/projects/${encodeURIComponent(id)}/uninstall`

/**
 * Take one installed part back, against the plan on screen: `{part, plan}` and nothing else, with
 * this launch's run token and the page's own Origin, exactly as the install click is sent. In this
 * version the one part is `speaker`, and only from the project's own `.claude/settings.local.json`.
 * @returns {Promise<{ok: boolean, status: number, doc: object|null}>}
 */
export async function postUninstall(id, part, plan, f = typeof fetch === 'function' ? fetch : null) {
  if (!f || !id) return { ok: false, status: 0, doc: { error: 'unreachable' } }
  let token
  try {
    token = await answer(await f(RUN_TOKEN, { cache: 'no-store', headers: { Accept: 'application/json' } }))
  } catch (_e) {
    return { ok: false, status: 0, doc: { error: 'unreachable' } }
  }
  if (!token.ok || !token.doc || typeof token.doc.token !== 'string') {
    return { ok: false, status: token.status, doc: token.doc || { error: 'run_token_required' } }
  }
  try {
    return await answer(await f(UNINSTALL_ADDRESS(id), {
      method: 'POST',
      cache: 'no-store',
      headers: { Accept: 'application/json', 'Content-Type': 'application/json', 'X-ThruLine-Run-Token': token.doc.token },
      body: JSON.stringify({ part: String(part || ''), plan: String(plan || '') }),
    }))
  } catch (_e) {
    return { ok: false, status: 0, doc: { error: 'unreachable' } }
  }
}

/** What a removal's answer means for a switch: kept, or refused in words that say nothing changed. */
export function uninstallAnswer(res) {
  const r = res || {}
  const doc = r.doc && typeof r.doc === 'object' ? r.doc : {}
  if (r.ok) return { kind: 'kept' }
  const code = typeof doc.error === 'string' ? doc.error : ''
  // the route's own sentence, which already says nothing changed
  if ((code === 'elsewhere' || code === 'not_installed' || code === 'refused' || code === 'write_failed') && typeof doc.plain === 'string' && doc.plain) {
    return { kind: 'refused', code, detail: doc.plain }
  }
  const words = {
    plan_changed: 'What is installed in this folder changed since this page read it, so nothing changed. The switch shows it again; press it again if you still want this.',
    install_in_progress: 'Another install is being written for this HQ right now, so nothing changed. Press the switch again in a moment.',
    run_token_required: 'ThruLine only changes this from its own page, and this request did not come from it, so nothing changed. Reload the page and try again.',
    same_origin_required: 'ThruLine only changes this from its own page, and this request did not come from it, so nothing changed. Reload the page and try again.',
    no_such_project: 'ThruLine no longer knows this folder, so nothing changed. Reload the page.',
    store_unavailable: 'ThruLine could not open its record, so nothing changed.',
    unreachable: 'ThruLine’s local server did not answer, so this may not have changed. Reload the page to see where it stands.',
  }
  return { kind: 'refused', code: code || 'other', detail: words[code] || words[r.status ? '' : 'unreachable'] || `ThruLine did not change anything (it answered ${r.status || 'nothing'}).` }
}

const str = (x) => (typeof x === 'string' && x ? x : null)
const list = (x) => (Array.isArray(x) ? x.filter((s) => typeof s === 'string') : [])

/** One part of the plan as typed facts. */
function partFacts(p) {
  const x = p || {}
  const r = x.refusal && typeof x.refusal === 'object' ? x.refusal : null
  return {
    state: x.state === 'will_add' || x.state === 'installed' || x.state === 'refused' ? x.state : 'unknown',
    file: str(x.file),
    level: str(x.level),
    adds: list(x.adds),
    where: list(x.where),
    refusal: r ? { code: str(r.code), plain: str(r.plain) } : null,
  }
}

/** The served plan as facts; null without a plan. */
export function installFacts(doc) {
  if (!doc || typeof doc !== 'object' || !doc.parts) return null
  const run = doc.parts.run_capture || {}
  const u = doc.undo || {}
  return {
    project: doc.project ? doc.project.id : null,
    plan: str(doc.plan),
    hooks: partFacts(doc.parts.hooks),
    speaker: partFacts(doc.parts.speaker),
    mcp: partFacts(doc.parts.mcp),
    run: Object.assign(partFacts(run), {
      wrappers: list(run.wrappers),
      pathLine: str(run.path_line),
      onPath: run.on_path === true ? true : run.on_path === false ? false : null,
    }),
    undo: { project: str(u.project), user: str(u.user), shim: str(u.shim) },
  }
}

/** One part of a click's answer as typed facts. */
function outcomeFacts(p) {
  const x = p || {}
  const r = x.refusal && typeof x.refusal === 'object' ? x.refusal : null
  return { outcome: str(x.outcome) || 'unknown', file: str(x.file), entries: list(x.entries), where: list(x.where), refusal: r ? { code: str(r.code), plain: str(r.plain) } : null }
}

/**
 * A click's answer as facts: what each part did, or why the click was refused.
 *   {ok: true, changed, parts: {hooks, speaker, mcp, run}, plain}
 *   {ok: false, error, plain}
 */
export function resultFacts(res) {
  if (!res) return null
  const doc = res.doc || {}
  if (res.ok && doc.parts) {
    return {
      ok: true,
      changed: doc.changed === true,
      parts: { hooks: outcomeFacts(doc.parts.hooks), speaker: outcomeFacts(doc.parts.speaker), mcp: outcomeFacts(doc.parts.mcp), run: outcomeFacts(doc.parts.run_capture) },
      plain: str(doc.plain),
    }
  }
  return { ok: false, error: str(doc.error) || (res.status ? 'other' : 'unreachable'), status: res.status || 0, plain: refusedWords(str(doc.error), res.status) }
}

/** A refused click, in words. */
export function refusedWords(code, status) {
  switch (code) {
    case 'plan_changed': return 'What is installed in this folder changed since this page read it, so nothing was installed. The list above is read again; check it, then press Install again.'
    case 'install_in_progress': return 'Another install is being written for this HQ right now, so this one was not started. Press Install again in a moment.'
    case 'run_token_required': case 'same_origin_required': return 'ThruLine only installs from its own page, and this request did not come from it, so nothing was installed. Reload the page and press Install again.'
    case 'no_such_project': return 'ThruLine no longer knows this folder, so nothing was installed. Reload the page and choose it again.'
    case 'store_unavailable': return 'ThruLine could not open its record, so nothing was installed.'
    case 'unreachable': return 'ThruLine’s local server did not answer, so nothing was installed. Is thruline ui still running?'
    default: return `ThruLine did not install anything (it answered ${status || 'nothing'}).`
  }
}

/**
 * First run's data with the chosen folder's install plan folded in, as
 * `folders[].install` = {state: 'reading'|'read'|'failed', facts, error, busy, result}.
 */
export function withInstall(data, install) {
  if (!data || !Array.isArray(data.folders) || !install || !install.project) return data
  const facts = installFacts(install.doc)
  const state = install.error ? 'failed' : facts ? 'read' : install.reading ? 'reading' : null
  if (!state) return data
  const folders = data.folders.map((f) => (f && f.id === install.project
    ? Object.assign({}, f, { install: { state, facts: state === 'read' ? facts : null, error: install.error || null, busy: !!install.busy, result: install.result || null } })
    : f))
  return Object.assign({}, data, { folders })
}

/**
 * The plan for the folder in front of the person, and the one click.
 * Read when the folder is chosen or changes, and again on reload(). A click
 * replaces the plan with the one the route read after writing (`after`), or,
 * when the plan had changed, with the fresh one it sent back.
 *
 * @returns {{project, doc, error, reading, busy, result, reload, run(choices): Promise<object>}}
 */
export function useInstall(projectId, { read = readInstall, post = postInstall } = {}) {
  const id = projectId || null
  const [st, setSt] = useState(() => ({ project: id, doc: null, error: null, reading: !!id, busy: false, result: null }))
  const [tick, setTick] = useState(0)
  const reader = useRef(read)
  reader.current = read
  const poster = useRef(post)
  poster.current = post
  useEffect(() => {
    if (!id) {
      setSt({ project: null, doc: null, error: null, reading: false, busy: false, result: null })
      return undefined
    }
    let live = true
    const ctl = typeof AbortController === 'function' ? new AbortController() : null
    setSt((s) => (s.project === id ? Object.assign({}, s, { error: null, reading: true }) : { project: id, doc: null, error: null, reading: true, busy: false, result: null }))
    Promise.resolve()
      .then(() => reader.current(id, { signal: ctl ? ctl.signal : undefined }))
      .then(
        (doc) => { if (live) setSt((s) => Object.assign({}, s, { project: id, doc, error: null, reading: false })) },
        (e) => {
          if (!live || (e && e.name === 'AbortError')) return
          setSt((s) => Object.assign({}, s, { project: id, doc: null, error: (e && e.message) || 'ThruLine could not read what Install would add.', reading: false }))
        },
      )
    return () => {
      live = false
      if (ctl) ctl.abort()
    }
  }, [id, tick])
  const mine = st.project === id
  const run = async (choices) => {
    const plan = mine && st.doc ? st.doc.plan : null
    if (!id || !plan) return resultFacts({ ok: false, status: 0, doc: { error: 'unreachable' } })
    setSt((s) => Object.assign({}, s, { busy: true, result: null }))
    const res = await poster.current(id, choices, plan)
    const result = resultFacts(res)
    const next = res && res.doc ? (res.ok ? res.doc.after : res.doc.plan) : null
    setSt((s) => Object.assign({}, s, { busy: false, result, doc: next && validPlan(next, id) ? next : s.doc }))
    return result
  }
  return {
    project: id,
    doc: mine ? st.doc : null,
    error: mine ? st.error : null,
    reading: mine ? st.reading : !!id,
    busy: mine && st.busy,
    result: mine ? st.result : null,
    reload: () => setTick((n) => n + 1),
    run,
  }
}

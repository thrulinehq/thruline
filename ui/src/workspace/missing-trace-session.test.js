import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { JSDOM } from 'jsdom'

/* A Change can still name a confirmed conversation after its execution record
 * has gone missing. Follow the Change's rendered trace link through the real
 * workspace read path. All responses are synthetic and supplied in memory. */
const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-missing-trace-'))
after(() => rm(temporary, { recursive: true, force: true }))

const entry = `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import Workspace from './shell.jsx';
  import { ChangeScreen } from './change-workspace.jsx';
  import { buildChangeView } from './change-view.js';
  export { Workspace, buildChangeView };
  export const renderChange = props => renderToStaticMarkup(React.createElement(ChangeScreen, props));
  export const renderWorkspace = workspace => renderToStaticMarkup(workspace.render());
`
const bundle = await build({
  stdin: { contents: entry, resolveDir: here }, bundle: true, write: false,
  format: 'esm', platform: 'node', loader: { '.css': 'empty' },
  define: { 'import.meta.env': '{}' }, logLevel: 'silent',
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
})
const compiled = join(temporary, 'missing-trace.mjs')
await writeFile(compiled, bundle.outputFiles[0].text)
const M = await import(pathToFileURL(compiled).href)

const MISSING = 'session-confirmed-but-absent'
const change = {
  id: 'change-missing-trace', project_id: 'project-synthetic', opened_at: '2026-10-06T09:00:00Z',
  request: { title: 'Keep uploads after a retry', purpose: 'A retry should keep the upload.' },
  accepted_scope: {
    version: 1, criteria: ['An upload survives one failed attempt.'], exclusions: [],
    allowed_scope: [], constraints: [], check_profile: [],
    acceptance: { actor: 'sam', accepted_at: '2026-10-06T09:01:00Z' },
  },
  associations: [{ id: 'link-one', session_id: MISSING, provenance: 'confirmed', basis: 'user', actor: 'sam', asserted_at: '2026-10-06T09:02:00Z' }],
  examination: 'none_yet', decision: 'none_yet',
}
const model = M.buildChangeView({
  change,
  work: [{ id: MISSING, findings: [], focus: {
    summary: { total: 1, shown: 1, merged: 0 },
    labels: [{ id: 'linked-step', text: 'One linked step' }], groups: [],
  } }],
})

const response = (body, status = 200) => ({
  ok: status === 200, status, headers: { get: () => null },
  text: async () => JSON.stringify(body),
})

test('a missing linked session opens a refused trace, never a clean verdict or findings', async (t) => {
  const changePage = new JSDOM(M.renderChange({ model, stage: 'verify' }), { url: 'http://127.0.0.1/' })
  const link = [...changePage.window.document.querySelectorAll('a')]
    .find((a) => a.textContent === 'Open the full trace')
  assert.ok(link, 'the Change exposes the full trace link for its confirmed work')
  const target = new URL(link.href)
  assert.equal(target.searchParams.get('session'), MISSING)
  assert.equal(target.hash, '#/trace')
  changePage.window.close()

  const browser = new JSDOM('<!doctype html><html><body></body></html>', { url: target.href })
  const before = { window: globalThis.window, document: globalThis.document }
  globalThis.window = browser.window
  globalThis.document = browser.window.document
  const calls = []
  t.mock.method(globalThis, 'fetch', async (url) => {
    calls.push(String(url))
    if (url === '/api/sessions') return response({ shape: 'thruline.sessions', schema_version: 1, total: 0, sessions: [] })
    if (String(url).startsWith('/api/explain') || String(url).startsWith('/api/claims')) return response({ error: 'no_session' }, 502)
    throw new Error('Unexpected read: ' + url)
  })

  try {
    const workspace = new M.Workspace({ liveOnly: true })
    workspace.setState = (update, then) => {
      workspace.state = { ...workspace.state, ...(typeof update === 'function' ? update(workspace.state) : update) }
      if (then) then()
    }
    workspace.live = true
    workspace.log = () => {}
    workspace.state.altitude = 'pro'
    workspace.state.panels = { ...workspace.state.panels, right: true, bottom: true }
    assert.equal(workspace.deepSession, MISSING)
    assert.equal(M.Workspace.routeFromHash(), 'trace')
    await workspace.loadData()
    assert.equal(workspace.state.dataStatus, 'refused', 'the absent execution record is a failed read')
    assert.ok(calls.includes('/api/sessions'), 'the trace checks the execution store before deciding what session it can read')

    const page = new JSDOM(M.renderWorkspace(workspace)).window.document
    assert.match(page.body.textContent, /This reading could not be opened/, 'the trace names the failed read')
    const verdict = page.querySelector('.ws-verdict')
    if (verdict) {
      assert.match(verdict.textContent, /unknown|unavailable|could not|not read|no session/i,
        'a verdict over an unread session must say that its outcome is unknown')
      const answers = new Map([...verdict.querySelectorAll('.ws-verdict-question')]
        .map((row) => [row.querySelector('dt')?.textContent, row.querySelector('dd')?.textContent.trim()]))
      for (const question of [
        'Which commands or tool calls failed?',
        'Does the evidence cover the latest changes?',
        'What should I do next?',
      ]) {
        if (answers.has(question)) assert.match(answers.get(question), /unknown|unavailable|could not|not read|no session/i, question)
      }
    }
    const findings = page.querySelector('#ws-dock-pane-findings')
    if (findings) assert.doesNotMatch(findings.textContent, /No findings — no completion claim was made/i,
      'an unread session cannot have a finding-free result')
  } finally {
    if (before.window === undefined) delete globalThis.window
    else globalThis.window = before.window
    if (before.document === undefined) delete globalThis.document
    else globalThis.document = before.document
    browser.window.close()
  }
})

/* TRACE-MISSING-SESSION-TRUE (D-332) — the rule, pinned for every way the read
 * can fail and at every altitude. When the session the trace names cannot be
 * read (it is not in the listing, or `/api/explain` answers non-zero, 404,
 * something that is not JSON, or nothing), the page states nothing about what
 * the session holds and offers exactly one honest next step:
 *   - no verdict panel and none of its answers, no "Nothing to do";
 *   - no dock tab, dock note or saved-view count that says "none";
 *   - no replay-strip sentence sending the reader to another session, no
 *     primary action, no rail line about an earlier read and no "Skip";
 *   - one command, `thruline doctor`, and no other command block or primary.
 * A deep-linked id the listing does not hold is refused by name even when
 * other sessions are listed: none of them is read in its place and the
 * address keeps the id it was given. */
const LISTED = 'cccccccc-0009-4000-8000-00000000000a'
const OTHER = 'cccccccc-0003-4000-8000-000000000000'
const listing = (ids) => ({ shape: 'thruline.sessions', schema_version: 1, total: ids.length, sessions: ids.map((id) => ({ id, event_count: 3 })) })
const raw = (text, status = 200) => ({ ok: status === 200, status, headers: { get: () => null }, text: async () => text })
const ALTITUDES = ['pro', 'workbench', 'explore', 'brief']

async function openTrace(t, session, altitude, answer, { early = false } = {}) {
  const address = 'http://127.0.0.1/sessions?session=' + encodeURIComponent(session) + '#/trace'
  const browser = new JSDOM('<!doctype html><html><body></body></html>', { url: address })
  const before = { window: globalThis.window, document: globalThis.document }
  globalThis.window = browser.window
  globalThis.document = browser.window.document
  const calls = []
  t.mock.method(globalThis, 'fetch', async (url) => {
    calls.push(String(url))
    return answer(String(url))
  })
  try {
    const workspace = new M.Workspace({ liveOnly: true })
    workspace.setState = (update, then) => {
      workspace.state = { ...workspace.state, ...(typeof update === 'function' ? update(workspace.state) : update) }
      if (then) then()
    }
    workspace.live = true
    workspace.log = () => {}
    workspace.state.altitude = altitude
    workspace.state.panels = { ...workspace.state.panels, right: true, bottom: true, left: true }
    /* `early`: the page also renders while the read is in flight, as the real
     * app does, so anything memoised then must not outlive the refusal. */
    const reading = workspace.loadData()
    const during = early ? new JSDOM(M.renderWorkspace(workspace)).window.document : null
    await reading
    const page = new JSDOM(M.renderWorkspace(workspace)).window.document
    return { workspace, page, during, calls, address, after: browser.window.location.href }
  } finally {
    if (before.window === undefined) delete globalThis.window
    else globalThis.window = before.window
    if (before.document === undefined) delete globalThis.document
    else globalThis.document = before.document
    browser.window.close()
  }
}

function assertUnreadTrace({ workspace, page }, label) {
  assert.equal(workspace.state.dataStatus, 'refused', label + ': the read failed')
  const text = page.body.textContent
  assert.match(text, /This session could not be read/, label + ': the page says it could not read this session')
  assert.equal(page.querySelector('.ws-verdict'), null, label + ': no verdict panel over an unread session')
  for (const claim of [
    /No findings — no completion claim was made/,
    /Nothing to do/,
    /Which commands or tool calls failed\?/,
    /Does the evidence cover the latest changes\?/,
    /What should I do next\?/,
    /No checks are recorded in this session/,
    /No promises are open in this session/,
    /none in this record/,
    /This record holds no recorded steps/,
    /Pick one of the sessions the session button/,
    /Line the claim up against the check/,
    /Read earlier at/,
    /recorded steps compared/,
    /Skip the steps/,
  ]) assert.doesNotMatch(text, claim, label + ': ' + claim)
  assert.equal(page.querySelector('.ws-read-rail'), null, label + ': no rail beside a failed read')

  /* Dock tabs and notes: what the session holds is unknown, never "none". */
  for (const tab of page.querySelectorAll('[role=tab][id^="ws-dock-tab-"]')) {
    assert.doesNotMatch(tab.textContent, /\bnone\b/, label + ': dock tab ' + tab.id)
    assert.match(tab.textContent, /not read/, label + ': dock tab ' + tab.id + ' says it was not read')
  }
  for (const pane of page.querySelectorAll('[role=tabpanel][id^="ws-dock-pane-"]')) {
    assert.match(pane.textContent, /could not be read/, label + ': ' + pane.id + ' says what it holds is unknown')
  }

  /* Next steps, counted by structure: a primary action, and every command
   * block in an alert or status (a refusal, a notice). Exactly one, and it is
   * the honest one. */
  assert.equal(page.querySelectorAll('[data-primary-action]').length, 0, label + ': no primary action over an unread session')
  const commands = [...page.querySelectorAll('[role=alert] pre, [role=status] pre')].map((pre) => pre.textContent.trim())
  assert.deepEqual(commands, ['thruline doctor'], label + ': exactly one next step, and it is honest')
  /* No command block sends a reader already in the UI back to `thruline ui` or
   * to an export (pinned by the deepEqual above). The words naming WHAT failed
   * may still ask whether `thruline ui` is serving the page when the server did
   * not answer at all: that is the cause, not a second step. */
  assert.doesNotMatch(text, /thruline export --all/, label + ': no export step')
}

for (const altitude of ALTITUDES) {
  test('[' + altitude + '] a linked session absent from an empty listing is refused by name', async (t) => {
    const seen = await openTrace(t, MISSING, altitude, (url) => {
      if (url === '/api/sessions') return response(listing([]))
      if (url.startsWith('/api/explain') || url.startsWith('/api/claims')) return response({ error: 'no_session' }, 502)
      throw new Error('Unexpected read: ' + url)
    })
    assertUnreadTrace(seen, altitude + ' absent')
    assert.ok(!seen.calls.some((url) => url.startsWith('/api/explain')), 'no other session is read in its place')
    assert.match(seen.page.body.textContent, new RegExp(MISSING), 'the refusal names the session the link asked for')
    assert.equal(seen.workspace.state.notice, null, 'no notice claims a first-ranked session was opened instead')
  })

  test('[' + altitude + '] a linked session absent while others are listed is refused by name, never swapped', async (t) => {
    const seen = await openTrace(t, MISSING, altitude, (url) => {
      if (url === '/api/sessions') return response(listing([OTHER, LISTED]))
      if (url.startsWith('/api/explain') || url.startsWith('/api/claims')) throw new Error('a listed session was read in place of the link: ' + url)
      throw new Error('Unexpected read: ' + url)
    })
    assertUnreadTrace(seen, altitude + ' absent-listed')
    assert.ok(!seen.calls.some((url) => url.startsWith('/api/explain') || url.startsWith('/api/claims')), 'no listed session is read in its place')
    assert.match(seen.page.body.textContent, new RegExp(MISSING), 'the refusal names the session the link asked for')
    assert.equal(seen.workspace.state.notice, null, 'no notice claims a first-ranked session was opened instead')
    assert.equal(seen.after, seen.address, 'the address keeps the id it was given')
    assert.ok(!seen.workspace.state.session || seen.workspace.state.session === MISSING, 'no other session is open')
  })

  for (const [label, explain] of [
    ['explain exits non-zero', () => response({ error: 'projection_failed', command: 'thruline explain --json', exit_code: 1, stderr: 'no session in the store' }, 502)],
    ['explain answers 404', () => response({ error: 'not_found' }, 404)],
    ['explain answers something that is not JSON', () => raw('<html>not a projection</html>')],
    ['explain does not answer', () => { throw new TypeError('fetch failed') }],
  ]) {
    test('[' + altitude + '] a listed session whose read fails shows nothing it holds: ' + label, async (t) => {
      const seen = await openTrace(t, LISTED, altitude, (url) => {
        if (url === '/api/sessions') return response(listing([LISTED]))
        if (url.startsWith('/api/explain')) return explain()
        if (url.startsWith('/api/claims')) return response({ error: 'no_session' }, 502)
        throw new Error('Unexpected read: ' + url)
      })
      assertUnreadTrace(seen, altitude + ' ' + label)
    })
  }
}

/* Verifier try 2: the saved-view counts were memoised by session and chips
 * only. A render while the read was in flight cached "none in this record",
 * the refusal arrived under the same key, and the unread rule never ran. */
for (const altitude of ALTITUDES) {
  for (const [label, session, answer] of [
    ['explain exits non-zero', LISTED, (url) => {
      if (url === '/api/sessions') return response(listing([LISTED]))
      if (url.startsWith('/api/explain')) return response({ error: 'projection_failed', command: 'thruline explain --json', exit_code: 1, stderr: 'no session in the store' }, 502)
      if (url.startsWith('/api/claims')) return response({ error: 'no_session' }, 502)
      throw new Error('Unexpected read: ' + url)
    }],
    ['absent from the listing', MISSING, (url) => {
      if (url === '/api/sessions') return response(listing([]))
      throw new Error('Unexpected read: ' + url)
    }],
  ]) {
    test('[' + altitude + '] a render while the read is in flight leaves nothing behind once it is refused: ' + label, async (t) => {
      const seen = await openTrace(t, session, altitude, answer, { early: true })
      assert.ok(seen.during, 'the page rendered while the read was in flight')
      assertUnreadTrace(seen, altitude + ' early ' + label)
      if (altitude === 'workbench') {
        const text = seen.page.body.textContent
        assert.match(text, /Not proven\s*not read/, 'the saved views say they were not read')
        assert.doesNotMatch(text, /none in this record/, 'no saved view says none')
      }
    })
  }
}

/* Verifier try 3: on-demand actions. Chosen on an unread session, an action
 * that reads the session (its claims, its stop, its findings, its events, its
 * thread) said what the session holds: "No completion claim in this session —
 * nothing to reconcile.", "No ending recorded". The palette and the toolbar no
 * longer offer the two named actions, and every such action, if called
 * anyway, says only that the session could not be read. */
const SESSION_FACT = /No completion claim|nothing to reconcile|No ending recorded|No findings in this record|No recorded events|nothing to walk|no claim to check against|No claim to check against/i

for (const [label, session, answer] of [
  ['explain exits non-zero', LISTED, (url) => {
    if (url === '/api/sessions') return response(listing([LISTED]))
    if (url.startsWith('/api/explain')) return response({ error: 'projection_failed', command: 'thruline explain --json', exit_code: 1, stderr: 'no session in the store' }, 502)
    if (url.startsWith('/api/claims')) return response({ error: 'no_session' }, 502)
    throw new Error('Unexpected read: ' + url)
  }],
  ['absent from the listing', MISSING, (url) => {
    if (url === '/api/sessions') return response(listing([]))
    throw new Error('Unexpected read: ' + url)
  }],
]) {
  test('[workbench] on-demand actions over an unread session state nothing it holds: ' + label, async (t) => {
    const seen = await openTrace(t, session, 'workbench', answer, { early: true })
    assertUnreadTrace(seen, 'workbench actions ' + label)
    const { workspace, page } = seen

    const offered = workspace.palRaw().map((item) => item.label)
    for (const name of ['Line the claim up against the check', 'Jump to stop']) {
      assert.ok(!offered.includes(name), 'the palette does not offer ' + name)
    }
    assert.ok(![...page.querySelectorAll('button')].some((b) => b.textContent.trim() === 'Jump to stop'), 'the toolbar does not offer Jump to stop')
    for (const item of workspace.palRaw()) assert.doesNotMatch(item.label + ' ' + item.hint, SESSION_FACT, 'palette: ' + item.label)

    const toasts = []
    workspace.toast = (text) => toasts.push(String(text))
    workspace.log = () => {}
    const actions = {
      lineUp: () => workspace.lineUp(),
      jumpStop: () => workspace.jumpStop(),
      stepFinding: () => workspace.stepFinding(1),
      step: () => workspace.step(1),
      toggleReplay: () => workspace.toggleReplay(),
      startWalk: () => workspace.startWalk(),
      focusThread: () => workspace.toast(workspace.focusThread()),
      copyNext: () => workspace.copyNext(),
    }
    for (const [name, act] of Object.entries(actions)) {
      toasts.length = 0
      act()
      assert.ok(toasts.length > 0, name + ' says something')
      for (const text of toasts) {
        assert.doesNotMatch(text, SESSION_FACT, name + ' states a fact about an unread session')
        assert.match(text, /could not be read/, name + ' says the session could not be read')
      }
    }
  })
}

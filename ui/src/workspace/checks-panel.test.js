import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

/* ── ChecksPanel is quiet while the Checks surface is not on screen ─────────────────────────────────────
 *
 * The defect (STEP0-AUDIT corrections table, row "narrator-path c1"; commit G ruling "ChecksPanel polling
 * under Verdict"): `HqWorkspace` mounts `ChecksPanel` in every mode (`hq.jsx:52`) because the panel holds
 * state a tab round trip must not lose — a running check sequence, the chosen sort and filter. The panel's
 * two reading hooks did not know that: `useCheckReading` fetched `/api/verification` on mount and polled it
 * every 3 s, and `useToolReading` fetched `/api/integrations` on mount, all while the Verdict or Trace tab
 * was the one being read. A hidden panel was asking the server for evidence nobody had asked to see, and
 * `/api/integrations` spawns `thruline integration workspace` on the server to answer.
 *
 * These tests mount the real `HqWorkspace` with `react-dom/client` over the smallest DOM it needs, count
 * every request the mount makes, and drive the poll with a fake `setInterval`. A count is the only honest
 * instrument here: a request that is not made leaves no other trace.
 */

const temporary = await mkdtemp(join(tmpdir(), 'thruline-checks-panel-'))
after(() => rm(temporary, { recursive: true, force: true }))
const built = await build({
  stdin: { contents: `
    import React from 'react';
    import { createRoot } from 'react-dom/client';
    import { HqWorkspace } from './hq.jsx';
    export { React, createRoot, HqWorkspace };
  `, resolveDir: dirname(fileURLToPath(import.meta.url)) },
  bundle: true, write: false, format: 'esm', platform: 'node',
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
})
const compiled = join(temporary, 'checks-panel.mjs')
await writeFile(compiled, built.outputFiles[0].text)
const L = await import(pathToFileURL(compiled).href)

/** A synthetic captured session: one recorded check, one claim, enough for the panel to render results. */
const SESSION = {
  id: 'ses-checks-quiet', key: 'ses-checks-quiet', label: 'live-0001', harness: 'Claude', finalTree: 'T2',
  project: '/work/project',
  events: [{ seq: 1, kind: 'exec', text: 'unit suite → exit 1' }],
  claims: [{ id: 'c1', node: 'clm', kind: 'task_complete', text: 'Suite green.', verdict: 'unsupported', verifier: 't@1', evidence: [], reasons: [], findings: [] }],
  promises: [],
  checks: [{ id: 'chk-1', node: 'chk', runner: 'unit suite', exit: 1, inputTree: 'T1' }],
  nodes: [{ id: 'ev1', objType: 'event', kind: 'exec', seq: 1, title: 'unit suite', lines: ['unit suite → exit 1'] }],
}
const INTEGRATIONS_URL = `/api/integrations?session=${SESSION.id}`
const VERIFICATION_URL = '/api/verification'
const PHASES_URL = `/api/phases?session=${SESSION.id}`

/** The least DOM `react-dom/client` needs. No mutation recording: these tests count requests, not paint. */
function domShim() {
  class Nd {
    constructor(nodeType, nodeName) { this.nodeType = nodeType; this.nodeName = nodeName; this.childNodes = []; this.parentNode = null; this.ownerDocument = null }
    get firstChild() { return this.childNodes[0] || null }
    get lastChild() { return this.childNodes[this.childNodes.length - 1] || null }
    get nextSibling() { const p = this.parentNode; if (!p) return null; return p.childNodes[p.childNodes.indexOf(this) + 1] || null }
    appendChild(c) { return this.insertBefore(c, null) }
    insertBefore(c, ref) {
      if (c.parentNode) c.parentNode.removeChild(c)
      const i = ref ? this.childNodes.indexOf(ref) : this.childNodes.length
      this.childNodes.splice(i < 0 ? this.childNodes.length : i, 0, c)
      c.parentNode = this
      return c
    }
    removeChild(c) {
      const i = this.childNodes.indexOf(c)
      if (i >= 0) this.childNodes.splice(i, 1)
      c.parentNode = null
      return c
    }
    contains(o) { let c = o; while (c) { if (c === this) return true; c = c.parentNode } return false }
    addEventListener() {}
    removeEventListener() {}
    get textContent() { return this.childNodes.map((c) => c.textContent).join('') }
    set textContent(v) {
      this.childNodes.forEach((c) => { c.parentNode = null })
      this.childNodes = []
      if (v !== '') this.appendChild(this.ownerDocument.createTextNode(String(v)))
    }
  }
  class Tx extends Nd {
    constructor(data, doc) { super(3, '#text'); this.d = String(data); this.ownerDocument = doc }
    get nodeValue() { return this.d }
    set nodeValue(v) { this.d = String(v) }
    get data() { return this.d }
    get textContent() { return this.d }
    set textContent(v) { this.nodeValue = v }
  }
  class El extends Nd {
    constructor(tag, doc) { super(1, tag.toUpperCase()); this.tagName = tag.toUpperCase(); this.at = new Map(); this.style = {}; this.ownerDocument = doc; this.namespaceURI = 'http://www.w3.org/1999/xhtml' }
    setAttribute(k, v) { this.at.set(k, String(v)) }
    setAttributeNS(_ns, k, v) { this.setAttribute(k, v) }
    removeAttribute(k) { this.at.delete(k) }
    getAttribute(k) { return this.at.has(k) ? this.at.get(k) : null }
    hasAttribute(k) { return this.at.has(k) }
    /* `react-dom` reads `select.options` when it mounts the results filter. Its own children, as a browser gives them. */
    get options() { return this.childNodes.filter((c) => c.tagName === 'OPTION') }
    html() { return `<${this.tagName.toLowerCase()}${[...this.at].map(([k, v]) => ` ${k}="${v}"`).join('')}>${this.childNodes.map((c) => (c.nodeType === 3 ? c.nodeValue : c.html ? c.html() : '')).join('')}</${this.tagName.toLowerCase()}>` }
  }
  class Doc extends Nd {
    constructor() { super(9, '#document') }
    createElement(tag) { return new El(tag, this) }
    createElementNS(_ns, tag) { return new El(tag, this) }
    createTextNode(t) { return new Tx(t, this) }
    createComment() { const c = new Nd(8, '#comment'); c.ownerDocument = this; return c }
    get activeElement() { return this.body }
  }
  const document = new Doc()
  document.documentElement = document.createElement('html')
  document.body = document.createElement('body')
  document.appendChild(document.documentElement)
  document.documentElement.appendChild(document.body)
  const ROOT = document.createElement('div')
  document.body.appendChild(ROOT)
  const walk = (n, out = []) => { out.push(n); n.childNodes.forEach((c) => walk(c, out)); return out }
  return { document, root: ROOT, roots: [], html: () => ROOT.html(), nodes: () => walk(ROOT) }
}

/**
 * A fake `setInterval`: nothing fires until a test advances the clock, and every live interval is visible.
 * Real time would make "no poll for 30 s" a 30-second test and "the poll stopped" unprovable.
 */
function fakeIntervals() {
  const real = { setInterval: globalThis.setInterval, clearInterval: globalThis.clearInterval }
  const timers = new Map()
  let now = 0
  let next = 1
  globalThis.setInterval = (fn, ms) => { const id = next++; timers.set(id, { fn, ms: Math.max(1, Number(ms) || 0), due: now + Math.max(1, Number(ms) || 0) }); return id }
  globalThis.clearInterval = (id) => { timers.delete(id) }
  return {
    live: () => timers.size,
    restore: () => { globalThis.setInterval = real.setInterval; globalThis.clearInterval = real.clearInterval },
    /** Fire every callback due within `ms`, in due order, exactly as a clock would. */
    tick(ms) {
      const end = now + ms
      for (;;) {
        let due = null
        for (const t of timers.values()) if (t.due <= end && (!due || t.due < due.due)) due = t
        if (!due) break
        now = due.due
        due.due = now + due.ms
        due.fn()
      }
      now = end
    },
  }
}

/** The mount environment: the shim, the stores, a fake clock and a fetch that answers from memory. */
async function withDom(fn) {
  const dom = domShim()
  const clock = fakeIntervals()
  const prior = { document: globalThis.document, window: globalThis.window, session: globalThis.sessionStorage, local: globalThis.localStorage, act: globalThis.IS_REACT_ACT_ENVIRONMENT, fetch: globalThis.fetch, MessageChannel: globalThis.MessageChannel }
  /* React's `act` opens a MessageChannel per flush and never closes it; node would hold the handle and
   * `npm test`, which runs every file in one process, would never exit. Tracked and closed below. */
  const channels = []
  class TrackedMessageChannel extends MessageChannel { constructor() { super(); channels.push(this) } }
  globalThis.MessageChannel = TrackedMessageChannel
  const store = new Map()
  const storage = { getItem: (k) => (store.has(k) ? store.get(k) : null), setItem: (k, v) => store.set(k, String(v)), removeItem: (k) => store.delete(k) }
  class HTMLIFrameElement {}
  globalThis.document = dom.document
  /* A real listener registry, not a pair of noops: the panel takes the shell's shared
   * `thruline-verification-update` broadcasts through `window`, and whether it is still listening from
   * behind another tab is itself under test below. */
  const listeners = new Map()
  globalThis.window = {
    document: dom.document, HTMLIFrameElement, event: undefined, location: { search: '', hash: '', pathname: '/' }, innerWidth: 1440, innerHeight: 900,
    addEventListener(type, fn) { if (!listeners.has(type)) listeners.set(type, new Set()); listeners.get(type).add(fn) },
    removeEventListener(type, fn) { const set = listeners.get(type); if (set) set.delete(fn) },
    dispatchEvent(event) { for (const fn of [...(listeners.get(event.type) || [])]) fn(event); return true },
  }
  globalThis.sessionStorage = storage
  globalThis.localStorage = storage
  globalThis.IS_REACT_ACT_ENVIRONMENT = true
  const calls = []
  globalThis.fetch = async (url) => {
    const address = String(url)
    calls.push(address)
    if (address === VERIFICATION_URL) return { ok: true, status: 200, json: async () => ({ enabled: false, detail: 'Local verification is not configured for this project.' }) }
    if (address === INTEGRATIONS_URL) return { ok: true, status: 200, json: async () => ({ schema: 'thruline.integration-workspace.v1', session: SESSION.id, catalog: { projects: [] } }) }
    return { ok: false, status: 404, json: async () => ({}) }
  }
  try {
    return await fn(dom, {
      calls,
      of: (address) => calls.filter((c) => c === address).length,
      checksCalls: () => calls.filter((c) => c === VERIFICATION_URL || c === INTEGRATIONS_URL),
      async advance(ms) {
        await L.React.act(async () => { clock.tick(ms) })
        await L.React.act(async () => {})
      },
      /** One broadcast from the shell's shared verification poller, as `shell.jsx` sends it. */
      async push(detail) {
        await L.React.act(async () => { globalThis.window.dispatchEvent({ type: 'thruline-verification-update', detail }) })
        await L.React.act(async () => {})
      },
      intervals: clock.live,
    })
  } finally {
    for (const root of dom.roots) { try { await L.React.act(async () => { root.unmount() }) } catch { /* the test's own failure is the report */ } }
    clock.restore()
    for (const c of channels) { try { c.port1.close(); c.port2.close() } catch { /* already closed */ } }
    for (const [k, v] of [['document', prior.document], ['window', prior.window], ['sessionStorage', prior.session], ['localStorage', prior.local], ['IS_REACT_ACT_ENVIRONMENT', prior.act], ['fetch', prior.fetch], ['MessageChannel', prior.MessageChannel]]) {
      if (v === undefined) delete globalThis[k]
      else globalThis[k] = v
    }
  }
}

/** `HqWorkspace` mounted once, with the tab it is showing changeable — the mount survives every switch. */
async function mounted(dom, mode) {
  const root = L.createRoot(dom.root)
  dom.roots.push(root)
  const noop = () => {}
  let current = mode
  const show = async (next) => {
    current = next
    await L.React.act(async () => {
      root.render(L.React.createElement(L.HqWorkspace, {
        session: SESSION, mode: current, onMode: noop, selected: null, reading: false,
        onInspect: noop, onCopy: noop, inspector: null,
      }))
    })
    await L.React.act(async () => {})
  }
  await show(mode)
  return { show }
}

const panelNode = (dom) => dom.nodes().find((n) => n.nodeType === 1 && (n.getAttribute('class') || '').includes('ws-checks-panel'))
const buttonLabelled = (dom, label) => dom.nodes().find((n) => n.tagName === 'BUTTON' && n.textContent === label)
const reactProps = (node) => node[Object.keys(node).find((k) => k.startsWith('__reactProps$'))]
async function click(dom, label) {
  const button = buttonLabelled(dom, label)
  assert.ok(button, `no button labelled "${label}" on screen`)
  await L.React.act(async () => { reactProps(button).onClick({ preventDefault() {}, stopPropagation() {}, target: {} }) })
  await L.React.act(async () => {})
}

test('the Verdict tab makes no ChecksPanel request, and no poll however long it is left open (commit G p6-checks-quiet)', async () => {
  await withDom(async (dom, T) => {
    await mounted(dom, 'verdict')
    /* The mount is live and the fetch stub is wired: the Verdict tab's own one request was made. */
    assert.equal(T.of(PHASES_URL), 1, 'the Verdict tab did not make its own phase request: this harness would prove nothing')
    /* And the panel is mounted, hidden, exactly as `hq.jsx:52` mounts it. */
    assert.ok(panelNode(dom), 'ChecksPanel is not mounted under the Verdict tab')
    assert.equal(panelNode(dom).getAttribute('hidden'), '', 'the mounted ChecksPanel is not hidden under Verdict')

    assert.deepEqual(T.checksCalls(), [], 'a hidden ChecksPanel asked the server for evidence on mount')
    for (let i = 0; i < 10; i++) await T.advance(3000)
    assert.deepEqual(T.checksCalls(), [], 'a hidden ChecksPanel polled the server')
    assert.equal(T.of(PHASES_URL), 1, 'the Verdict tab repeated its own request')
  })
})

test('opening Checks reads once per source, polls while it is the surface being read, and stops on the way out (commit G p6-checks-quiet)', async () => {
  await withDom(async (dom, T) => {
    const app = await mounted(dom, 'verdict')
    assert.deepEqual(T.checksCalls(), [])

    await app.show('checks')
    assert.equal(panelNode(dom).getAttribute('hidden'), null, 'the Checks surface is not on screen')
    assert.equal(T.of(VERIFICATION_URL), 1, 'opening Checks reads the local verification state exactly once')
    assert.equal(T.of(INTEGRATIONS_URL), 1, 'opening Checks reads the tool evidence exactly once')

    /* The poll is the captured-session workspace's own; it runs while Checks is the surface being read. */
    await T.advance(3000)
    assert.equal(T.of(VERIFICATION_URL), 2, 'the 3 s poll did not run while Checks was on screen')
    await T.advance(3000)
    await T.advance(3000)
    assert.equal(T.of(VERIFICATION_URL), 4, 'the poll is not once per interval while Checks is on screen')
    assert.equal(T.of(INTEGRATIONS_URL), 1, 'the tool evidence is read on opening, not polled')

    await app.show('verdict')
    const settled = T.calls.length
    for (let i = 0; i < 10; i++) await T.advance(3000)
    assert.equal(T.calls.length, settled, 'leaving Checks did not stop the poll')

    /* Returning reads again — a surface that is opened shows current evidence, not a stale reading. */
    await app.show('checks')
    assert.equal(T.of(VERIFICATION_URL), 5, 'returning to Checks did not re-read the local verification state, or read it twice')
    assert.equal(T.of(INTEGRATIONS_URL), 2, 'returning to Checks did not re-read the tool evidence, or read it twice')
  })
})

test('the panel that stays mounted keeps its state across a tab round trip (commit G p6-checks-quiet)', async () => {
  /* Why `hq.jsx:52` mounts ChecksPanel in every mode: a running check sequence and the reader's chosen
   * ordering must survive Checks → Verdict → Checks. Quieting the reads must not quietly unmount it. */
  await withDom(async (dom, T) => {
    const app = await mounted(dom, 'verdict')
    await app.show('checks')
    const before = panelNode(dom)
    assert.ok(buttonLabelled(dom, 'Attention first'), 'the result ordering control is not on screen')
    await click(dom, 'Attention first')
    assert.ok(buttonLabelled(dom, 'Check order'), 'the ordering did not change')

    await app.show('verdict')
    await app.show('checks')
    assert.equal(panelNode(dom), before, 'ChecksPanel was remounted by the tab round trip; its running state would be lost')
    assert.ok(buttonLabelled(dom, 'Check order'), 'the chosen ordering was lost across the tab round trip')
    assert.equal(T.of(INTEGRATIONS_URL), 2, 'the round trip read the tool evidence more than once per opening')
  })
})

test('an update pushed by the shared poller still reaches the panel from behind another tab, and costs no request (commit G p6-checks-quiet)', async () => {
  /* Quieting the reads must not make the panel deaf. The reading it does not ask for, it still accepts:
   * `shell.jsx` broadcasts `thruline-verification-update` from the one shared poller the project workspace
   * runs, and that costs this panel nothing. Gating the whole effect — instead of only the reading it
   * performs — would unregister the listener with the tab. */
  await withDom(async (dom, T) => {
    await mounted(dom, 'verdict')
    assert.deepEqual(T.checksCalls(), [])
    await T.push({ enabled: true, location: { repo: '/work/other-project' }, contract: { checks: [], criteria: [] }, pack_results: [], history: [], job: { running: false } })
    assert.match(dom.html(), /\/work\/other-project/, 'a pushed update did not reach the mounted panel')
    assert.deepEqual(T.checksCalls(), [], 'taking a pushed update made a request of its own')
  })
})

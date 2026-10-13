import React from 'react'
import { createRoot } from 'react-dom/client'
import Workspace from './workspace/shell.jsx'
import { chooseScreen, watchAddress } from './routes.js'

/* ThruLine boots into the evidence workspace. The attached Live Trace design
 * is the interface reference; the superseded OS shell is not mounted. The index is
 * rendered from the §9 summary index ALONE; opening a
 * project loads exactly one fixture. A missing or unreadable index is a
 * FAILURE, and says so — a product that draws its calm screen when it could
 * not read its own data is the exact lie this engine exists to catch
 * (Rule 12). */

const INDEX = '/fixtures/index.json'
const REQUIRED = ['contract_version', 'kind', 'generated_at', 'totals', 'projects']

function fail(message, detail) {
  const el = document.createElement('pre')
  el.style.cssText =
    'margin:0;padding:24px;font:12px/1.6 ui-monospace,monospace;color:#ff8a80;background:#1a1012;min-height:100vh;white-space:pre-wrap'
  el.textContent = `ThruLine — cannot render.\n\n${message}\n\n${detail || ''}\nIndex: ${INDEX}\n\nBuild it with:\n  thruline export --all`
  document.getElementById('root').replaceChildren(el)
  console.error('[thruline]', message, detail)
}

class Boundary extends React.Component {
  constructor(p) {
    super(p)
    this.state = { err: null }
  }
  static getDerivedStateFromError(err) {
    return { err }
  }
  componentDidCatch(err, info) {
    console.error('[thruline] render failed:', err, info)
  }
  render() {
    if (!this.state.err) return this.props.children
    return (
      <pre
        style={{
          margin: 0,
          padding: '24px',
          font: '12px/1.6 ui-monospace,monospace',
          color: '#ff8a80',
          background: '#1a1012',
          minHeight: '100vh',
          whiteSpace: 'pre-wrap',
        }}
      >
        {`ThruLine — render failed.\n\n${this.state.err?.message}\n\n${this.state.err?.stack || ''}`}
      </pre>
    )
  }
}

async function boot() {
  /* HQ Next: the founder's canvas, ported (docs/hq-next/PLAN.md). The
     artboards at `#/design/<Artboard>` run each ported artboard on its own
     canvas logic and demo data, for the pixel side-by-side with
     docs/hq-next/reference/. Development only: a shipped build carries them
     only when it is built with VITE_HQ_DESIGN=1 for that comparison. */
  const DESIGN = /^#\/design(?:\/([A-Za-z0-9_-]+))?$/
  const designBuild = import.meta.env.DEV || import.meta.env.VITE_HQ_DESIGN === '1'
  if (designBuild && DESIGN.test(location.hash)) {
    const mod = await import('./hq/artboards.jsx')
    const root = createRoot(document.getElementById('root'))
    const draw = () => {
      const m = location.hash.match(DESIGN)
      if (!m) return location.reload()
      root.render(<Boundary><mod.default name={m[1] || ''} /></Boundary>)
    }
    draw()
    window.addEventListener('hashchange', draw)
    return
  }

  /* HQ Next is the product: the app at `/` is the founder's canvas on the real
     store. The superseded workspace stays reachable at `/?legacy` while the
     port is compared with it; the other paths (/sessions, /verification,
     /acceptance) are unchanged. */
  const legacy = new URLSearchParams(location.search).has('legacy')
  if (!legacy && location.pathname === '/') {
    const mod = await import('./hq/app.jsx')
    createRoot(document.getElementById('root')).render(<Boundary><mod.default /></Boundary>)
    return
  }

  /* Which screen this address names is decided in ONE place, `routes.js`, so
     the app's routing table is a function a test can ask questions of. The
     order of the branches below is the order that file documents. */
  const at = chooseScreen(
    { pathname: location.pathname, hash: location.hash, search: location.search },
    { mockTooling: import.meta.env.VITE_MOCK_TOOLING === '1' },
  )

  /* ONE address watcher, installed here, before anything is mounted, because
     this is the one place that knows a root was chosen. Every screen used to
     bring its own, written for the one pair of screens its card cared about,
     and the gap between the pairs was a real defect: Back from a Change put
     `#/inbox` in the address bar and left the Change on the glass (T-2-8-F4).
     Any address that names a different screen now boots that screen. */
  watchAddress(window, { mockTooling: import.meta.env.VITE_MOCK_TOOLING === '1' })

  if (at.screen === 'sessions') {
    // Layout bootstrap only. Evidence comes from live projections, without
    // requiring a prior transcript export or substituting demo records.
    createRoot(document.getElementById('root')).render(<Boundary><Workspace liveOnly index={{ projects: [], totals: {}, kind: 'index', contract_version: 1 }} /></Boundary>)
    return
  }
  if (at.screen === 'verification') {
    createRoot(document.getElementById('root')).render(<Boundary><Workspace localVerification index={{ projects: [], totals: {}, kind: 'index', contract_version: 1 }} /></Boundary>)
    return
  }
  /* One Change, at `#/changes/<id>` (T-2-4). It is mounted BEFORE the index is
     read because it needs no index: it reads the Change routes the local
     server serves and nothing else, so the screen opens on a store with no
     export in it. The module is dynamically imported, so an ordinary session
     that never opens a Change never loads it, and this file keeps its one
     one and only index read (pinned, lib/arcade.test.js:228-229). */
  /* The whole story on one page (T-2-5), at `#/changes/<id>/packet`. It is
     matched BEFORE the Change screen because the Change screen's address
     grammar reads a trailing segment as a stage and falls back to `define`
     for one it does not know — so an unmatched packet address would silently
     open a different screen. */
  if (at.screen === 'packet') {
    const mod = await import('./workspace/packet.jsx')
    if (at.id) {
      createRoot(document.getElementById('root')).render(
        <React.StrictMode>
          <Boundary>
            <mod.default id={at.id} />
          </Boundary>
        </React.StrictMode>,
      )
      return
    }
    return fail('That is not a Change address.', location.hash)
  }

  if (at.screen === 'change') {
    const mod = await import('./workspace/change-workspace.jsx')
    if (at.id) {
      createRoot(document.getElementById('root')).render(
        <React.StrictMode>
          <Boundary>
            <mod.default id={at.id} stage={at.stage} />
          </Boundary>
        </React.StrictMode>,
      )
      return
    }
    return fail('That is not a Change address.', location.hash)
  }

  /* The licence page (T-DIST-4). It needs no index: it reads `GET
     /api/licence` and nothing else, and it writes nothing. */
  if (at.screen === 'licence') {
    const { default: LicenceScreen } = await import('./workspace/licence.jsx')
    createRoot(document.getElementById('root')).render(
      <React.StrictMode>
        <Boundary>
          <LicenceScreen />
        </Boundary>
      </React.StrictMode>,
    )
    return
  }

  if (at.screen === 'acceptance') {
    const { default: Acceptance } = await import('./acceptance.jsx')
    createRoot(document.getElementById('root')).render(<Boundary><Acceptance /></Boundary>)
    return
  }

  /* The cast exhibit (RA6 amendment B): in-product, deep-linkable, marked.
     It ships — unlike the mock gallery — but it renders only on its own
     route and no real-data surface imports it (pinned). This branch and the
     gallery one below keep their own address test rather than reading
     `at.screen`, because `lib/arcade.test.js` pins the literal gate in THIS
     file; `chooseScreen` returns `cast` and `gallery` for the same two
     addresses, and a test asserts the two agree. */
  if (new URLSearchParams(location.search).has('cast')) {
    const { default: CastExhibit } = await import('./cast.jsx')
    createRoot(document.getElementById('root')).render(
      <React.StrictMode>
        <Boundary>
          <CastExhibit />
        </Boundary>
      </React.StrictMode>,
    )
    return
  }

  /* Mock tooling. `VITE_MOCK_TOOLING` is undefined in an ordinary build, so the
     bundler removes this branch and the gallery module with it. */
  if (import.meta.env.VITE_MOCK_TOOLING === '1' && new URLSearchParams(location.search).has('gallery')) {
    const { default: Gallery } = await import('./gallery.jsx')
    createRoot(document.getElementById('root')).render(
      <React.StrictMode>
        <Boundary>
          <Gallery />
        </Boundary>
      </React.StrictMode>,
    )
    return
  }

  /* The landing (T-2-6/T-2-7): the Inbox on a machine that has picked a
     project, and the first-use path on one that has not. `Landing` makes that
     choice from the one read it takes; this file keeps its single index read. It is mounted
     BEFORE the index is read because it needs no index — it reads the Change
     routes the local server serves — so the first screen opens on a store
     with no export in it, and this file keeps its one and only index read
     (pinned, lib/arcade.test.js:228-229). */
  /* `#/start` (T-2-7) COMES THROUGH THE SAME `Landing`, carrying which address
     it was, and that is the fix for a real defect: this branch used to mount the
     pick screen unconditionally, so a bookmark, a typed `#/start` or a second
     tab re-opened "pick a project" on a machine that had already picked one and
     walked a person into writing the same work down again. The store decides
     what either address opens, in one place, from one read (`landingScreen`). */
  if (at.screen === 'inbox' || at.screen === 'first-use') {
    const { Landing } = await import('./workspace/first-use.jsx')
    createRoot(document.getElementById('root')).render(
      <React.StrictMode>
        <Boundary>
          <Landing address={at.screen} />
        </Boundary>
      </React.StrictMode>,
    )
    return
  }

  let index
  try {
    const res = await fetch(INDEX, { cache: 'no-store' })
    if (!res.ok) return fail(`Index request failed: HTTP ${res.status} ${res.statusText}`)
    index = await res.json()
  } catch (e) {
    return fail('The index could not be loaded or parsed as JSON.', String(e))
  }

  const missing = REQUIRED.filter((k) => index[k] === undefined)
  if (missing.length) return fail(`Index is missing required keys: ${missing.join(', ')}`)
  if (index.kind !== 'index') return fail(`Not a summary index: kind = ${JSON.stringify(index.kind)}`)
  if (index.diagnostics) {
    return fail('This is a diagnostics export. The product surface renders graduated classes only.')
  }

  createRoot(document.getElementById('root')).render(
    <React.StrictMode>
      <Boundary>
        <Workspace index={index} />
      </Boundary>
    </React.StrictMode>,
  )
}

boot()

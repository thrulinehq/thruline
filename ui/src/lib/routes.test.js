import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { chooseScreen, goToInbox, watchAddress, screenKey, INBOX_HASH, START_HASH, SCREENS } from '../routes.js'
import { RAIL } from '../workspace/logic.js'

/* T-2-6 — which screen an address names.
 *
 * The landing moved (`B8.1`): the bare address and `#/inbox` open the Inbox.
 * Everything else must land exactly where it did, which is what most of this
 * file is about — a new front door that quietly breaks the rest of the house
 * is not an improvement (`B8.3`).
 */

const here = dirname(fileURLToPath(import.meta.url))
const src = (rel) => readFileSync(join(here, '..', rel), 'utf8')
const at = (hash, search = '', pathname = '/') => chooseScreen({ pathname, hash, search })

test('the bare address is the Inbox, and so is #/inbox — the landing is no longer the technical view', () => {
  assert.deepEqual(at(''), { screen: 'inbox' })
  assert.deepEqual(at('#'), { screen: 'inbox' })
  assert.deepEqual(at('#/'), { screen: 'inbox' })
  assert.deepEqual(at('#/inbox'), { screen: 'inbox' })
  assert.equal(INBOX_HASH, '#/inbox')
})

test('the first-use path has its own address, and it is the one the CLI opens (T-2-7)', () => {
  assert.deepEqual(at('#/start'), { screen: 'first-use' })
  assert.deepEqual(at('#/licence'), { screen: 'licence' })
  assert.equal(START_HASH, '#/start')
  /* The CLI writes the same address, in Rust, when the store holds no
   * project. Two places, one string — pinned here so a rename in either
   * language cannot quietly send `thruline ui` to a screen that is not there. */
  const rust = readFileSync(join(here, '..', '..', '..', 'crates', 'cli', 'src', 'uiserve', 'first_use.rs'), 'utf8')
  assert.match(rust, /pub\(super\) const START_ADDRESS: &str = "\/#\/start";/, 'the CLI no longer writes the address this table resolves')
  /* And it is not reachable by accident from the landing. */
  assert.equal(at('#/startle').screen, 'index')
})

test('every rail destination that is not the Inbox still opens the workspace shell, which reads the key itself', () => {
  const keys = []
  RAIL.forEach((g) => g.items.forEach((it) => keys.push(it[0])))
  assert.ok(keys.length >= 15, `expected the full rail, found ${keys.length}`)
  for (const key of keys) {
    const screen = at('#/' + key).screen
    assert.equal(screen, key === 'inbox' ? 'inbox' : 'index', `#/${key} went to ${screen}`)
  }
  /* And the shell still resolves each of those keys out of the hash itself,
   * so "opens the shell" is not the same as "opens the shell on Trace". */
  const shell = src('workspace/shell.jsx')
  assert.match(shell, /static routeFromHash\(\)/)
  assert.match(shell, /\^#\\\/\(\[a-z\]\+\)\$/)
})

test('the pathname routes and the two query routes are untouched', () => {
  assert.equal(at('', '', '/sessions').screen, 'sessions')
  assert.equal(at('#/trace', '', '/sessions').screen, 'sessions')
  assert.equal(at('', '', '/verification').screen, 'verification')
  assert.equal(at('', '', '/acceptance').screen, 'acceptance')
  assert.equal(at('', '?cast').screen, 'cast')
  assert.equal(at('', '?gallery').screen, 'inbox', 'without the mock-tooling flag the gallery address is not a screen')
  assert.equal(chooseScreen({ pathname: '/', hash: '', search: '?gallery' }, { mockTooling: true }).screen, 'gallery')
})

test('a deep link to one piece of recorded work still opens the shell, hash or no hash', () => {
  /* `shell.jsx` writes `?session=<id>` and keeps the hash for the screen, so a
   * bare address carrying a session is somebody's saved link into the shell.
   * Sending it to the Inbox would break a link that already exists. */
  assert.equal(at('', '?session=cccccccc-0000-4000-8000-00000000s001').screen, 'index')
  assert.equal(at('#/trace', '?session=cccccccc-0000-4000-8000-00000000s001').screen, 'index')
  assert.equal(at('#/inbox', '?session=cccccccc-0000-4000-8000-00000000s001').screen, 'index')
})

test('the Change screen and the whole story on one page keep their addresses, and the packet is matched first', () => {
  assert.deepEqual(at('#/changes/abc'), { screen: 'change', id: 'abc', stage: 'define' })
  assert.deepEqual(at('#/changes/abc/verify'), { screen: 'change', id: 'abc', stage: 'verify' })
  assert.deepEqual(at('#/changes/abc/packet'), { screen: 'packet', id: 'abc' })
  /* An address that starts like a Change and is not one is a Change screen
   * with no id, which `main.jsx` turns into "that is not a Change address" —
   * never a silent fall-through to some other screen. */
  assert.deepEqual(at('#/changes/'), { screen: 'change' })
  assert.deepEqual(at('#/changes/a b'), { screen: 'change' })
})

test('every screen the table can name is one main.jsx mounts', () => {
  const main = src('main.jsx')
  for (const screen of SCREENS) {
    if (screen === 'cast' || screen === 'gallery' || screen === 'index') continue
    assert.ok(main.includes(`at.screen === '${screen}'`), `main.jsx mounts nothing for ${screen}`)
  }
  /* The two exhibit routes keep their own literal gate in main.jsx, which
   * `lib/arcade.test.js` pins; the table must agree with them rather than
   * contradict them. */
  assert.match(main, /has\('cast'\)/)
  assert.equal(at('', '?cast').screen, 'cast')
  assert.match(main, /import\.meta\.env\.VITE_MOCK_TOOLING === '1'/)
})

test('the control labelled Inbox goes to the Inbox and nowhere else', () => {
  const shell = src('workspace/shell.jsx')
  /* The baseline's H2T-3: clicking Inbox opened the captured-session picker
   * over the technical canvas. Neither door may do that any more. */
  assert.ok(
    !/if \(key === 'inbox'\) \{\s*this\.setState\(\{ drawer: 'sessions'/.test(shell),
    'the Inbox control still opens the session picker',
  )
  assert.match(shell, /import \{ goToInbox \} from '\.\.\/routes\.js'/)
  assert.equal([...shell.matchAll(/goToInbox\(/g)].length, 2, 'both doors — the rail click and the restored address — go through the one helper')

  const win = { location: { hash: '', reloads: 0, reload() { this.reloads += 1 } } }
  assert.deepEqual(goToInbox(win), { hash: '#/inbox', reloaded: true })
  assert.equal(win.location.hash, '#/inbox')
  assert.equal(win.location.reloads, 1)
  assert.equal(chooseScreen({ pathname: '/', hash: win.location.hash, search: '' }).screen, 'inbox', 'the address it writes is one the table sends to the Inbox')
})

test('an item’s link is a click: the address moves, and the screen it names is booted', () => {
  /* Without this the old screen stays on the glass and the click looks like it
   * did nothing — the failure mode the T-2-5 walk hit between the Change
   * screen and the whole story on one page. */
  const listeners = {}
  const win = {
    location: { pathname: '/', hash: '#/inbox', search: '', reloads: 0, reload() { this.reloads += 1 } },
    addEventListener(name, fn) { listeners[name] = fn },
    removeEventListener(name) { delete listeners[name] },
  }
  const stop = watchAddress(win)
  assert.equal(typeof listeners.hashchange, 'function')

  win.location.hash = '#/changes/cccccccc-0000-4000-8000-00000000c001/verify'
  listeners.hashchange()
  assert.equal(win.location.reloads, 1)

  /* And an address that is still the Inbox is not a boot: a reload on every
   * hash write would throw away a screen the person is reading. */
  win.location.hash = '#/inbox'
  listeners.hashchange()
  assert.equal(win.location.reloads, 1, 'the address returned to the booted screen and it booted anyway')

  stop()
  assert.equal(listeners.hashchange, undefined)
  assert.equal(typeof watchAddress(null), 'function')
  assert.equal(typeof watchAddress({}), 'function')
})

/* ── T-2-8-F4: every pair, not one ─────────────────────────────────────────
 *
 * The watcher this replaced covered exactly two pairs, each written for the
 * walk its own card had done: the Change screen ↔ the whole story on one page,
 * and leaving the Inbox. Arriving at the Inbox was covered by nobody, and that
 * is what shipped: Back from a Change put `#/inbox` in the address bar and
 * left the Change on the glass. So this drives ALL of them.
 */
const ADDRESSES = [
  { screen: 'inbox', pathname: '/', hash: '#/inbox', search: '' },
  { screen: 'inbox', pathname: '/', hash: '', search: '' },
  { screen: 'first-use', pathname: '/', hash: '#/start', search: '' },
  { screen: 'licence', pathname: '/', hash: '#/licence', search: '' },
  { screen: 'change', pathname: '/', hash: '#/changes/cccccccc-0000-4000-8000-00000000c001', search: '' },
  { screen: 'change', pathname: '/', hash: '#/changes/cccccccc-0000-4000-8000-00000000c001/verify', search: '' },
  { screen: 'change', pathname: '/', hash: '#/changes/cccccccc-0000-4000-8000-00000000c002', search: '' },
  { screen: 'packet', pathname: '/', hash: '#/changes/cccccccc-0000-4000-8000-00000000c001/packet', search: '' },
  { screen: 'index', pathname: '/', hash: '#/trace', search: '' },
  { screen: 'sessions', pathname: '/sessions', hash: '', search: '' },
  { screen: 'verification', pathname: '/verification', hash: '', search: '' },
  { screen: 'acceptance', pathname: '/acceptance', hash: '', search: '' },
]

test('from every screen, an address that names a different screen boots it — all pairs', () => {
  for (const from of ADDRESSES) {
    for (const to of ADDRESSES) {
      const listeners = {}
      const win = {
        location: { pathname: from.pathname, hash: from.hash, search: from.search, reloads: 0, reload() { this.reloads += 1 } },
        addEventListener(name, fn) { listeners[name] = fn },
        removeEventListener(name) { delete listeners[name] },
      }
      watchAddress(win)
      /* The address moves the only way a hash change can move it: the hash.
         A pair that differs only in the path is not reachable by `hashchange`
         and is skipped rather than pretended. */
      if (to.pathname !== from.pathname) continue
      win.location.hash = to.hash
      win.location.search = to.search
      listeners.hashchange()
      const different = screenKey(chooseScreen(from)) !== screenKey(chooseScreen(to))
      assert.equal(
        win.location.reloads, different ? 1 : 0,
        `${from.hash || from.pathname} → ${to.hash || to.pathname}: ` +
          (different ? 'the address named another screen and it stayed on the glass' : 'the same screen booted itself away'),
      )
    }
  }
})

test('the stage inside the Change screen is that screen’s own business', () => {
  const at = (hash) => screenKey(chooseScreen({ pathname: '/', hash, search: '' }))
  assert.equal(at('#/changes/abc'), at('#/changes/abc/verify'))
  assert.equal(at('#/changes/abc'), at('#/changes/abc/define'))
  assert.notEqual(at('#/changes/abc'), at('#/changes/abc/packet'))
  assert.notEqual(at('#/changes/abc'), at('#/changes/abd'))
  assert.notEqual(at('#/changes/abc'), at('#/inbox'))
})

test('the one watcher is installed in the one place that chooses a root', () => {
  const main = readFileSync(join(here, '..', 'main.jsx'), 'utf8')
  assert.match(main, /import \{ chooseScreen, watchAddress \} from '\.\/routes\.js'/)
  assert.equal([...main.matchAll(/watchAddress\(/g)].length, 1, 'the watcher is installed once, or it is not one watcher')
  /* And no screen brings its own any more. */
  for (const file of ['workspace/inbox.jsx', 'workspace/packet.jsx', 'workspace/change-workspace.jsx']) {
    const source = readFileSync(join(here, '..', file), 'utf8')
    assert.ok(!/addEventListener\('hashchange'/.test(source), `${file} watches the address itself`)
  }
})

test('a window that refuses address writes leaves the person where they are, and says nothing it cannot back up', () => {
  const sealed = { location: { get hash() { return '' }, set hash(_v) { throw new Error('sandboxed') }, reload() { assert.fail('reloaded after a refused write') } } }
  assert.equal(goToInbox(sealed), null)
  assert.equal(goToInbox(null), null)
  assert.equal(goToInbox({}), null)
})

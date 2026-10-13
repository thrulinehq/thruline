/* Which screen an address names (T-2-6).
 *
 * PURE. No window, no fetch, no React. `main.jsx` mounts what this decides, so
 * the app's routing table is a function a test can ask questions of instead of
 * a ladder of `if`s inside a boot sequence nothing can call twice.
 *
 * WHAT CHANGED IN T-2-6. The bare address used to open the workspace shell —
 * the dense technical view (the baseline's H2T-2). It now opens the INBOX
 * (`B8.1`), and so does `#/inbox`, which is where the control labelled Inbox
 * goes and nowhere else (H2T-3).
 *
 * WHAT DID NOT CHANGE. Every other address resolves exactly where it did:
 * `/sessions`, `/verification`, `/acceptance`, `?cast`, `?gallery`, the Change
 * screen and the whole story on one page, and every `#/<key>` the workspace
 * rail writes — `#/trace`, `#/sessions`, `#/receipts`, `#/capture`, `#/policy`
 * and the rest — still open the workspace shell, which reads the key out of
 * the hash itself (`shell.jsx` `routeFromHash`). Those are `B8.3`'s supporting
 * surfaces and a test asserts each one still lands on the shell.
 *
 * ONE DELIBERATE EXCEPTION, and it is the reason `search` is a parameter here:
 * a bare address carrying `?session=<id>` is a deep link to one piece of
 * recorded work in the shell (`shell.jsx` `writeSession`). Sending it to the
 * Inbox would break a link somebody already has, so it still opens the shell.
 */

/** Every screen this app can open. A caller that matches on a string not in here is wrong. */
export const SCREENS = Object.freeze([
  'sessions',
  'verification',
  'packet',
  'change',
  'acceptance',
  'cast',
  'gallery',
  'inbox',
  'first-use',
  'licence',
  'index',
])

const CHANGE_AT = /^#\/changes\/([A-Za-z0-9_-]{1,128})(?:\/([a-z]+))?$/
const PACKET_AT = /^#\/changes\/([A-Za-z0-9_-]{1,128})\/packet$/

/**
 * The screen an address names.
 *
 * @param {{pathname: string, hash: string, search: string}} at
 * @param {{mockTooling?: boolean}} [env] `VITE_MOCK_TOOLING`, which is
 *   undefined in an ordinary build and removes the gallery with it
 * @returns {{screen: string, id?: string, stage?: string}}
 */
export function chooseScreen(at, env) {
  const pathname = (at && at.pathname) || '/'
  const hash = (at && at.hash) || ''
  const search = (at && at.search) || ''

  if (pathname === '/sessions') return { screen: 'sessions' }
  if (pathname === '/verification') return { screen: 'verification' }

  /* The packet is matched BEFORE the Change screen, because the Change
   * screen's address grammar reads a trailing segment as a stage and falls
   * back to `define` for one it does not know — an unmatched packet address
   * would silently open a different screen. */
  const packet = PACKET_AT.exec(hash)
  if (packet) return { screen: 'packet', id: packet[1] }
  if (hash.startsWith('#/changes/')) {
    const m = CHANGE_AT.exec(hash)
    return m ? { screen: 'change', id: m[1], stage: m[2] || 'define' } : { screen: 'change' }
  }

  if (pathname === '/acceptance') return { screen: 'acceptance' }

  const params = new URLSearchParams(search)
  if (params.has('cast')) return { screen: 'cast' }
  if (env && env.mockTooling && params.has('gallery')) return { screen: 'gallery' }

  /* The first-use path (T-2-7), at `#/start`. `thruline ui` appends it when the
   * store holds no project.
   *
   * THIS NAMES AN ADDRESS; IT DOES NOT DECIDE THE SCREEN ON ITS OWN, and the
   * distinction was a defect before it was a comment. `main.jsx` mounts the same
   * `Landing` for this as for the bare address, telling it which address it was,
   * and `Landing` asks `GET /api/first-use`: `#/start` opens the pick screen
   * while this machine can see a repository that is not already picked and
   * already tracked, and the Inbox once it cannot (`first-use.jsx`
   * `landingScreen`). `#/start` is the address `thruline ui` prints on a first
   * run, so it is the one people bookmark; it used to re-open "pick a project"
   * unconditionally, and each visit wrote the same recorded work down again —
   * three visits, three identical Inbox items, nothing on screen to say why. */
  if (hash === '#/start') return { screen: 'first-use' }

  /* The licence page (T-DIST-4), at `#/licence`. A read-only page: the status
   * in one sentence, what needs a licence, and the command that installs one. */
  if (hash === '#/licence') return { screen: 'licence' }

  if (!params.has('session') && (hash === '' || hash === '#' || hash === '#/' || hash === '#/inbox')) {
    return { screen: 'inbox' }
  }
  return { screen: 'index' }
}

/** The address the first-use path lives at, and the one `thruline ui` opens on a store with no project in it. */
export const START_HASH = '#/start'

/** The one address the control labelled Inbox goes to. */
export const INBOX_HASH = '#/inbox'

/**
 * Go to the Inbox from a screen that is not it.
 *
 * `main.jsx` chooses the screen ONCE, at boot, and mounts one root; a root
 * cannot become another one. So this changes the address and reloads, which
 * is the same thing `change-workspace.jsx`'s `reboot` does for the link
 * between the Change screen and the whole story on one page. The window is a
 * parameter so a test can watch what it was asked to do.
 *
 * @param {Window} win
 * @returns {{hash: string, reloaded: boolean}|null}
 */
export function goToInbox(win) {
  if (!win || !win.location) return null
  try {
    win.location.hash = INBOX_HASH
    if (typeof win.location.reload === 'function') win.location.reload()
    return { hash: INBOX_HASH, reloaded: true }
  } catch (_navigationError) {
    /* a sandboxed document refuses address writes; the app says nothing it
       cannot back up and stays where it is */
    return null
  }
}

/**
 * The identity of a screen: which one, and which thing it is showing.
 *
 * The stage inside the Change screen is deliberately NOT part of it. Moving
 * from Define to Verify is that screen's own business and must not throw the
 * screen away; moving to the Inbox, to another Change, or to the whole story
 * on one page is a different root, and `main.jsx` mounts one root per boot.
 *
 * @param {{screen: string, id?: string}} at
 * @returns {string}
 */
export function screenKey(at) {
  return `${(at && at.screen) || ''}:${(at && at.id) || ''}`
}

/**
 * The ONE address watcher. It owns the choice of root.
 *
 * `main.jsx` decides which screen an address names once, at boot, and mounts
 * one root; a root cannot become another one. So every way the address can
 * move — a link, a Back, a typed address, a bookmark — has to be answered by
 * booting the screen the new address names, or the old screen stays on the
 * glass and the reader has done something that looks like it did nothing
 * (`03-USER-LENS.md` §2, *gives up*).
 *
 * This used to be three watchers, each written for the one pair of screens its
 * own card cared about: the Change screen and the whole story on one page
 * watched for each other, the Inbox watched for leaving itself, and nothing at
 * all watched for ARRIVING at the Inbox. `T-2-8-F4` is what that cost — Back
 * from a Change put `#/inbox` in the address bar and left the Change on the
 * screen. One watcher over the routing table has no pairs in it: any address
 * that names a different screen boots that screen, including one added later.
 *
 * @param {Window} win
 * @param {{mockTooling?: boolean}} [env]
 * @returns {() => void} the cleanup a React effect returns
 */
export function watchAddress(win, env) {
  if (!win || typeof win.addEventListener !== 'function' || !win.location) return () => {}
  const at = () =>
    screenKey(chooseScreen({ pathname: win.location.pathname, hash: win.location.hash, search: win.location.search }, env))
  const booted = at()
  const onHash = () => {
    if (at() !== booted && typeof win.location.reload === 'function') win.location.reload()
  }
  win.addEventListener('hashchange', onHash)
  return () => win.removeEventListener('hashchange', onHash)
}

export default chooseScreen

/* The one place a Chrome launch is described, for every `.mjs` under
 * `scripts/review-integration/` and for the two CDP drives under
 * `docs/loop/p6b/`.
 *
 * R-2026-09-23-15: this file is canonical and lives at
 * `scripts/testing/chrome-launch.mjs`. It was `scripts/review-integration/
 * chrome-launch.mjs` until that ruling; lanes repoint at the new path once,
 * and a lane-local copy is deleted at its merge rather than kept in step.
 *
 * R-2026-09-23-12: headless Chrome under a fresh HOME raises a macOS
 * "Keychain Not Found" dialog on the founder's screen, once per run, and a
 * dialog on the founder's screen is a defect in the harness, not in the
 * product. `--use-mock-keychain` is the flag that stops it; the other three
 * stop the first-run and default-browser prompts that a fresh profile also
 * invites. They live here so that a new harness cannot forget them, and
 * `chrome-launch.test.mjs` fails if any `.mjs` under
 * `scripts/review-integration/` launches Chrome without going through
 * `launchChromium`.
 *
 * This module deliberately does not import playwright: the caller passes the
 * `chromium` namespace it already resolved, so the test can read the flags on
 * a machine with no browser runtime installed.
 */

import { spawn as nodeSpawn } from 'node:child_process'
import { readFile } from 'node:fs/promises'
import { join } from 'node:path'

export const CHROME_EXECUTABLE = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'

/** The four flags R-2026-09-23-12 requires of every Chrome launch. */
export const KEYCHAIN_FLAGS = Object.freeze([
  '--use-mock-keychain',
  '--password-store=basic',
  '--no-first-run',
  '--no-default-browser-check',
])

/* ── R-2026-09-24-28 — no fixed DevTools port, anywhere ────────────────
 *
 * On 2026-09-24 one lane's harness attached to ANOTHER lane's Chrome on the
 * fixed DevTools port 9337 and drove it for four seconds, while a walk in
 * that other lane recorded a passing grant a minute later. Evidence whose
 * origin is not what the record says is the failure this product exists to
 * detect, so: every Chrome launch here asks for `--remote-debugging-port=0`
 * (the kernel picks a free port, and no second process can guess it) and
 * reads the port Chrome actually took from `<user-data-dir>/DevToolsActivePort`
 * — the file Chrome writes into its own profile. A literal port is refused
 * rather than honoured, and `chrome-launch.test.mjs` fails if one is passed.
 */

/** The DevTools flag every launch carries: port 0 means "pick a free one". */
export const DEVTOOLS_ARGS = Object.freeze(['--remote-debugging-port=0'])

/** Every flag a review-integration Chrome carries, required four first. */
export const CHROME_ARGS = Object.freeze([
  ...KEYCHAIN_FLAGS,
  '--disable-background-networking',
  ...DEVTOOLS_ARGS,
])

/** Any `--remote-debugging-port=` whose value is not exactly 0. */
const LITERAL_PORT = /^--remote-debugging-port=(?!0$)/

/** The one sentence a refused launch says, named so the test can assert it. */
export const LITERAL_PORT_REFUSAL = 'a literal DevTools port is refused, R-2026-09-24-28'

/**
 * Throw if any caller-supplied arg pins the DevTools port.
 * @param args  the args a caller wants appended to the shared ones
 * @param where the name of the entry point, for the message
 */
export function refuseLiteralPort(args = [], where = 'this launch') {
  for (const arg of args) {
    if (typeof arg === 'string' && LITERAL_PORT.test(arg)) {
      throw new Error(
        `${LITERAL_PORT_REFUSAL}: ${where} was given \`${arg}\`. ` +
          'Use --remote-debugging-port=0 and read the port from DevToolsActivePort.',
      )
    }
  }
  return args
}

/**
 * Read `<userDataDir>/DevToolsActivePort` once Chrome has written it.
 * The file is two lines: the port Chrome took, then the browser socket path.
 */
async function readDevToolsActivePort(userDataDir, timeoutMs, proc) {
  const file = join(userDataDir, 'DevToolsActivePort')
  const end = Date.now() + timeoutMs
  let last = null
  while (Date.now() < end) {
    if (proc && proc.exitCode !== null && proc.exitCode !== undefined) {
      throw new Error(`Chrome exited (code ${proc.exitCode}) before it wrote ${file}`)
    }
    try {
      const [port, socketPath] = (await readFile(file, 'utf8')).split('\n')
      if (port && socketPath && Number(port) > 0) {
        return { port: Number(port), path: socketPath.trim(), file }
      }
    } catch (e) {
      last = e
    }
    await new Promise((r) => setTimeout(r, 100))
  }
  throw new Error(
    `Chrome did not write ${file} within ${timeoutMs} ms${last ? ': ' + last.message : ''}`,
  )
}

/**
 * Spawn Chrome with an EPHEMERAL DevTools port and report the port it took.
 *
 * Throws synchronously — before anything is spawned — if `extraArgs` pins the
 * port, so a harness cannot talk itself onto another lane's browser.
 *
 * @param {object}   o
 * @param {string}   o.userDataDir  a profile directory this launch owns (required)
 * @param {string[]} [o.extraArgs]  further flags, appended after the shared ones
 * @param {string}   [o.url]        the first page ('about:blank' by default)
 * @param {boolean}  [o.headless]   `--headless=new` (true by default)
 * @param {number}   [o.timeoutMs]  how long to wait for DevToolsActivePort (10 s)
 * @param {string}   [o.executable] the Chrome binary
 * @param {Function} [o.spawnFn]    child_process.spawn, injectable for the test
 * @returns {Promise<{proc: object, port: number, wsEndpoint: string, args: string[], userDataDir: string}>}
 */
export function spawnChromeWithDevTools(o = {}) {
  const {
    userDataDir,
    extraArgs = [],
    url = 'about:blank',
    headless = true,
    timeoutMs = 10000,
    executable = CHROME_EXECUTABLE,
    spawnFn = nodeSpawn,
  } = o
  refuseLiteralPort(extraArgs, 'spawnChromeWithDevTools')
  if (!userDataDir) {
    throw new Error('spawnChromeWithDevTools needs its own userDataDir (a fresh profile)')
  }
  const args = [
    ...(headless ? ['--headless=new'] : []),
    ...CHROME_ARGS,
    `--user-data-dir=${userDataDir}`,
    ...extraArgs,
    url,
  ]
  const proc = spawnFn(executable, args, { stdio: 'ignore' })
  return readDevToolsActivePort(userDataDir, timeoutMs, proc).then(
    ({ port, path }) => ({ proc, port, wsEndpoint: `ws://127.0.0.1:${port}${path}`, args, userDataDir }),
    (e) => {
      try {
        proc.kill('SIGKILL')
      } catch {
        /* already gone */
      }
      throw e
    },
  )
}

/**
 * Launch Chrome through playwright with the shared flags.
 * @param chromium the `chromium` namespace from playwright (caller-resolved)
 * @param options  extra playwright launch options; `args` are appended after
 *                 CHROME_ARGS and can never remove one of them
 *
 * Playwright drives its browser over its own ephemeral pipe, so nothing here
 * needs a port — and under R-2026-09-24-28 a caller that passes a literal
 * `--remote-debugging-port=<n>` is refused rather than launched.
 */
export function launchChromium(chromium, options = {}) {
  const { args = [], ...rest } = options
  refuseLiteralPort(args, 'launchChromium')
  return chromium.launch({
    executablePath: CHROME_EXECUTABLE,
    headless: true,
    ...rest,
    args: [...CHROME_ARGS, ...args],
  })
}

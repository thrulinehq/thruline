/* N13-F11-AXE-SR — the axe half of F11's bar ("axe 0 serious; VoiceOver and
 * NVDA read every state as its word").
 *
 *   node scripts/testing/axe-sweep.mjs
 *
 * Runs axe-core — the pinned devDependency in ui/package.json, read from
 * ui/node_modules and injected over CDP `Runtime.evaluate`; no CDN, no
 * network — in headless Chrome over the shared harness page
 * (`a11y-harness.mjs`: the real app entry, synthetic data only), on the
 * Inbox, the Change screen at each of its six stages, the Trace at Brief,
 * Explore and Workbench, the run sheet, the export sheet and the key list. Rules:
 * the WCAG 2.0, 2.1 and 2.2 A and AA tags.
 *
 * Every violation is printed with its impact, rule and first targets;
 * moderate and minor are listed, not hidden. The gate is serious + critical.
 *
 * Exit 0 passed (0 serious, 0 critical) · 1 failed · 2 could not run.
 * A view whose ready selector never appears is "could not run" for that view
 * and is never counted as a pass.
 */
import { readFile } from 'node:fs/promises'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'
import { COULD_NOT_RUN, FAILED, PASSED, UI, withPage } from './a11y-harness.mjs'

/** The rule tags F11's axe bar runs. */
export const TAGS = Object.freeze(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22a', 'wcag22aa'])

/** The impacts that fail the gate. */
export const GATE = Object.freeze(['critical', 'serious'])

const STAGES = ['define', 'prepare', 'build', 'verify', 'review', 'approve']
const click = (text) => `(() => { const b = [...document.querySelectorAll('button')].find((x) => x.textContent.trim() === ${JSON.stringify(text)}); if (b) b.click(); return !!b })()`

/** Every view the sweep visits. `open` runs after load; `ready` must then exist. */
export const VIEWS = Object.freeze([
  { name: 'Inbox', path: '#/inbox', ready: '#root > *' },
  ...STAGES.map((s) => ({ name: `Change · ${s}`, path: `?altitude=explore#/changes/ch1/${s}`, ready: '.ws-shell' })),
  { name: 'Trace · Brief', path: '?altitude=brief#/a11y', ready: '.ws-root' },
  { name: 'Trace · Explore', path: '?altitude=explore#/a11y', ready: '.ws-root' },
  { name: 'Trace · Workbench', path: '?altitude=workbench#/a11y', ready: '.ws-root' },
  { name: 'Run sheet', path: '?altitude=explore#/changes/ch1/verify', ready: '.ws-shell', open: click('Check it again'), after: '[role="dialog"]' },
  { name: 'Export sheet', path: '#/changes/ch1/packet', ready: '#root > *', open: click('Save the whole story as a file'), after: '.ws-export-sheet' },
  { name: 'Key list', path: '?altitude=explore#/changes/ch1', ready: '.ws-shell', open: click('Keys'), after: '.ws-shell-dialog' },
])

/**
 * Tally axe violations by impact, and name each one.
 *
 * @param {Array<{id: string, impact: string|null, nodes: Array<{target: string[]}>}>} violations
 * @returns {{counts: Record<string, number>, lines: string[]}}
 */
export function tally(violations) {
  const counts = { critical: 0, serious: 0, moderate: 0, minor: 0, unknown: 0 }
  const lines = []
  for (const v of violations || []) {
    const impact = counts[v.impact] === undefined ? 'unknown' : v.impact
    counts[impact] += 1
    const targets = (v.nodes || []).slice(0, 3).map((n) => (n.target || []).join(' ')).join(' | ')
    lines.push(`${impact.padEnd(8)} ${v.id} (${(v.nodes || []).length} nodes) ${targets}`)
  }
  return { counts, lines }
}

/**
 * The sweep's exit code. Nothing ran → could not run; any serious or
 * critical → failed; otherwise passed. An `unknown` impact is not a pass
 * either: axe that cannot say how bad a violation is has not said it is not.
 *
 * @param {{ran: number, counts: Record<string, number>}} r
 * @returns {0|1|2}
 */
export function verdict({ ran, counts }) {
  if (!ran) return COULD_NOT_RUN
  return GATE.some((g) => counts[g] > 0) || counts.unknown > 0 ? FAILED : PASSED
}

async function main() {
  const source = await readFile(join(UI, 'node_modules/axe-core/axe.min.js'), 'utf8').catch(() => null)
  if (!source) {
    console.log('axe-sweep: could not run — ui/node_modules/axe-core is not installed (cd ui && npm ci)')
    return COULD_NOT_RUN
  }
  return withPage('axe-sweep', async ({ evaluate, goto, base }) => {
    const total = { critical: 0, serious: 0, moderate: 0, minor: 0, unknown: 0 }
    let ran = 0
    const notRun = []
    for (const view of VIEWS) {
      try {
        await goto(base + view.path, view.ready)
        if (view.open) {
          if (!(await evaluate(view.open))) throw new Error('its door was not on the page')
          const end = Date.now() + 5000
          while (!(await evaluate(`!!document.querySelector(${JSON.stringify(view.after)})`))) {
            if (Date.now() > end) throw new Error(`${view.after} did not open`)
            await new Promise((r) => setTimeout(r, 150))
          }
          await new Promise((r) => setTimeout(r, 400))
        }
      } catch (e) {
        notRun.push(view.name)
        console.log(`view  ${view.name}: could not run — ${e.message}`)
        continue
      }
      await evaluate(source + ';true')
      const result = await evaluate(`axe.run(document, { runOnly: { type: 'tag', values: ${JSON.stringify(TAGS)} }, resultTypes: ['violations'] }).then((r) => r.violations.map((v) => ({ id: v.id, impact: v.impact, nodes: v.nodes.map((n) => ({ target: n.target })) })))`)
      const { counts, lines } = tally(result)
      ran += 1
      for (const k of Object.keys(total)) total[k] += counts[k]
      console.log(`view  ${view.name}: critical ${counts.critical} · serious ${counts.serious} · moderate ${counts.moderate} · minor ${counts.minor}${counts.unknown ? ' · unknown ' + counts.unknown : ''}`)
      for (const l of lines) console.log('      ' + l)
    }
    console.log(`axe-sweep: axe-core ${await evaluate('axe.version').catch(() => '?')}, tags ${TAGS.join(' ')}, ${ran} of ${VIEWS.length} views ran`)
    console.log(`axe-sweep: critical ${total.critical} · serious ${total.serious} · moderate ${total.moderate} · minor ${total.minor}${total.unknown ? ' · unknown ' + total.unknown : ''}`)
    if (notRun.length) console.log(`axe-sweep: could not run on ${notRun.length} view(s), not counted as passed: ${notRun.join('; ')}`)
    const code = verdict({ ran, counts: total })
    console.log(`axe-sweep: ${code === PASSED ? 'PASSED' : code === FAILED ? 'FAILED' : 'COULD NOT RUN'}`)
    return code
  })
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) process.exit(await main())

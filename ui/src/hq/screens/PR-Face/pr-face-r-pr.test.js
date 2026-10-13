/* C30 R-PR — PR-Face is honest and live (FEATURE-CENSUS §3.14 rows 2–7).
 *
 *   row 2  "The approval it waits on →" is drawn live, to this Change's
 *          Approve sheet, and says so when an approval already stands;
 *   row 3  the comment has one row per line of what counts as done, and the
 *          branch comes from git (the route's answer);
 *   row 4  the port's "Post to the pull request" is gone;
 *   row 5  the check run, the commit trailer (change_decisions), the README
 *          badge words (the CLI's generator, served) and the edited-in-place
 *          panel are drawn from the record, or say plainly that it holds none;
 *   row 6  the six contrasts stay out of the product (gallery only);
 *   row 7  The whole story and the Approve sheet link in.
 * Plus: Copy as Markdown copies exactly the comment shown and the Action setup
 * line, the setup line is the Action's own command (pinned to
 * packaging/action/action.yml), and work nobody tracked gets the track offer,
 * never a note. Synthetic fixtures only (pr-fixture.js).
 */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { prFaceFrom, noteFacts, changesSince, TRAILER_KEY, UNTRACKED } from './from-change.js'
import { setupOf, verifyCommand, inlineMd, markdownOf, LINE_MARK } from './markdown.js'
import { PR_PREVIEW } from './read.js'
import { NOW, CHANGE_ID, TARGET, tracked, snapshot, prDoc, STANDING } from './pr-fixture.js'
import { wholeStoryFrom } from '../Whole-Story/from-change.js'
import { selectApproveSheet } from '../Approve-Sheet/from-change.js'

const here = dirname(fileURLToPath(import.meta.url))
const repo = join(here, '../../../../..')

async function bundle(dir, tag) {
  const ENTRY = `
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import Template from './template.jsx'
import Logic from './logic.js'
export function instance(props) { const inst = new Logic(props || {}); inst.props = props || {}; return inst }
export function render(props, state) {
  const inst = instance(props)
  if (state) inst.state = { ...inst.state, ...state }
  return renderToStaticMarkup(React.createElement(Template, { v: inst.renderVals(), link: (h) => h }))
}`
  const result = await build({
    bundle: true, write: false, format: 'esm', platform: 'node', jsx: 'automatic',
    banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
    loader: { '.css': 'empty', '.js': 'jsx' }, logLevel: 'silent',
    stdin: { contents: ENTRY, resolveDir: dir, loader: 'jsx' },
  })
  const file = join(await mkdtemp(join(tmpdir(), tag)), 'screen.mjs')
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
}
const PR = await bundle(here, 'pr-rpr-')
const WS = await bundle(join(here, '../Whole-Story'), 'pr-rpr-ws-')
const AS = await bundle(join(here, '../Approve-Sheet'), 'pr-rpr-as-')

const unescape = (s) => s.replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
const html = (M, props, state) => unescape(M.render(props, state))
const words = (h) => h.replace(/<!-- -->/g, '').replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ')

const T = tracked()
const ctx = (over = {}) => ({ now: NOW, tracked: T, pr: { doc: prDoc() }, ...over })
const page = (snap = snapshot(T), c = ctx()) => prFaceFrom(snap, c)
/** A tracked Change with a standing approval in `change_decisions`. */
const approvedT = (over = {}) => tracked({ ready: true, states: ['proven', 'proven', 'proven', 'proven'], standing: { ...STANDING, ...over } })

/* ---------------------------------------------------------- row 3: one row per line */

test('row 3: the comment has one row per line of what counts as done, in the model’s order, state and words', () => {
  const d = page()
  const rows = d.note.table.rows
  assert.equal(rows.length, T.model.criteria.length)
  rows.forEach((r, i) => {
    const c = T.model.criteria[i]
    assert.deepEqual(r[0], [{ glyph: LINE_MARK[c.state] }])
    assert.deepEqual(r[1], [{ rec: c.text }])
    assert.deepEqual(r[2], [c.word])
  })
  /* the agent's statements are folded below, not rows of the table */
  assert.ok(!rows.some((r) => JSON.stringify(r).includes('Tests pass.')))
  assert.match(inlineMd(d.note.layers[0].items.flat()), /Tests pass\./)
  /* the head names the version the lines are measured on */
  assert.deepEqual(d.note.table.head[2], ['On ', { code: T.model.version.short }])
})

test('row 3: the branch on the host line is git’s, from the route — and says so when git could not', () => {
  const meta = (doc) => inlineMd(page(snapshot(T), ctx({ pr: { doc } })).host.meta)
  assert.equal(meta(prDoc()), 'shop · branch `fix-total` · head `abcdef0`')
  assert.equal(meta(prDoc({ git: { branch: null, head: 'abcdef0123456789abcdef0123456789abcdef01', detached: true, reason: null } })), 'shop · no branch, head `abcdef0`')
  assert.equal(meta(prDoc({ git: { branch: null, head: null, detached: null, reason: 'the folder is not a git repository' } })), 'shop · branch not read: the folder is not a git repository')
  assert.equal(inlineMd(page(snapshot(T), ctx({ pr: { doc: null, loading: true } })).host.meta), 'shop · reading the branch…')
  assert.equal(inlineMd(page(snapshot(T), ctx({ pr: { doc: null, error: 'x' } })).host.meta), 'shop · the branch could not be read')
  const h = html(PR, { data: page() })
  assert.match(words(h), /shop · branch fix-total · head abcdef0/)
})

/* ---------------------------------------------------------- row 2: the approval it waits on */

test('row 2: “The approval it waits on →” is live, to this Change’s Approve sheet', () => {
  const d = page()
  assert.deepEqual(d.approval, { href: '#/changes/' + CHANGE_ID + '/approve?project=p1', label: 'The approval it waits on', stands: false })
  const h = html(PR, { data: d })
  assert.match(h, new RegExp('<a href="#/changes/' + CHANGE_ID + '/approve\\?project=p1" class="tl-btn"[^>]*>The approval it waits on<span aria-hidden="true">→</span></a>'))
})

test('row 2: when an approval stands, the link says so, and the note’s first line is it', () => {
  const t = approvedT()
  const d = page(snapshot(t), ctx({ tracked: t }))
  assert.equal(d.approval.label, 'Approved on bbbbbbb')
  assert.equal(d.approval.stands, true)
  assert.match(d.markdown, /\*\*◆ Approved on `bbbbbbb` by sam in ThruLine\.\*\*/)
  assert.match(d.markdown, /\*\*For approval:\*\* approved in ThruLine by sam at 2026-09-29 19:00Z\. ThruLine never approves for you\./)
  assert.match(html(PR, { data: d }), /class="tl-btn"[^>]*>Approved on bbbbbbb<span aria-hidden="true">→<\/span><\/a>/)
  /* an approval of an earlier version: the code changed since, and the link says that too */
  const moved = tracked({ ready: true, states: ['proven', 'proven', 'proven', 'proven'], standing: { ...STANDING, target_revision: 'aaaaaaa1c0ffee11' } })
  const dm = page(snapshot(moved), ctx({ tracked: moved }))
  assert.equal(dm.approval.label, 'Approved on aaaaaaa · code changed since')
  assert.match(dm.markdown, /\*\*◭ Needs a re-check — approved on `aaaaaaa`, and the code changed since\.\*\*/)
  assert.doesNotMatch(dm.markdown, /◆ Approved/)
})

/* ---------------------------------------------------------- row 4: no post button */

test('row 4: the live page has no post button and no “not available” sentence, tracked or not', () => {
  for (const d of [page(), prFaceFrom(snapshot(null), { now: NOW })]) {
    const h = html(PR, { data: d })
    assert.doesNotMatch(h, /Post to the pull request|pr-post-why|Not available in this version|disabled="" aria-describedby/)
  }
  const src = readFileSync(join(here, 'logic.js'), 'utf8') + readFileSync(join(here, 'template.jsx'), 'utf8')
  assert.doesNotMatch(src, /Post to the pull request|POST_OFF|Not available in this version/)
})

/* ---------------------------------------------------------- row 5: the check run */

const section3 = (d) => { const h = html(PR, { data: d }); const a = h.indexOf('aria-labelledby="s3"'); assert.ok(a > 0, 'section 3 is drawn'); return h.slice(a, h.indexOf('aria-labelledby="s4"')) }

test('row 5: with no receipt in the folder, the check run says the Action would post nothing', () => {
  const d = page()
  const s = words(section3(d))
  assert.match(s, /thruline\/receipt/)
  assert.match(s, /no receipt/)
  assert.match(s, /No check run: this folder holds no receipt\./)
  assert.match(s, /would stop with “No receipt” and post no check run, no comment and no badge\./)
  /* no conclusion is drawn as this Change's: the conclusion words appear only in the table of what it can conclude */
  const fig = section3(d).slice(0, section3(d).indexOf('</figure>'))
  assert.doesNotMatch(words(fig), /\b(success|neutral|failure)\b/)
  for (const [c, l] of [['success', 'verified'], ['neutral', 'partial'], ['failure', 'not verified']]) assert.match(s, new RegExp(c + ' The badge reads “' + l + '”\\.'))
})

test('row 5: with receipts, it names the newest one the Action would check, and the command to check it here', () => {
  const d = page(snapshot(T), ctx({ pr: { doc: prDoc({ receipts: { dir: '.thruline/receipts', count: 2, newest: 'r-2026.json', reason: null } }) } }))
  assert.equal(d.checkRun.tag, 'not checked here')
  assert.equal(d.checkRun.command, 'thruline verify-receipt .thruline/receipts/r-2026.json --offline --format check-run')
  const s = words(section3(d))
  assert.match(s, /Not checked here: \.thruline\/receipts\/r-2026\.json \./)
  assert.match(s, /2 receipts are here; it takes the newest\./)
  assert.match(s, /Opening this page does not check a receipt\. Press Check this receipt to use the CLI verifier with temporary Git storage outside your repository/)
})

test('row 5: the check run’s name and the badge’s words are the served ones, never a copy', () => {
  const doc = prDoc({ check_run: { name: 'thruline/other' }, badge: { states: [{ conclusion: 'success', label: 'all good', colour: '#000' }] } })
  const d = page(snapshot(T), ctx({ pr: { doc } }))
  const s = words(section3(d))
  assert.match(s, /thruline\/other/)
  assert.match(s, /success The badge reads “all good”\./)
  assert.doesNotMatch(s, /partial|not verified/)
  assert.match(inlineMd(d.setup), /prints the `thruline\/other` check run/)
})

test('row 5: a folder that is gone, an unreadable route and a read still out are each said, never drawn as a result', () => {
  const gone = page(snapshot(T), ctx({ pr: { doc: prDoc({ folder: { recorded: true, present: false, name: 'shop', reason: 'the folder recorded for this Change is not on this machine' }, receipts: { dir: '.thruline/receipts', count: null, newest: null, reason: 'x' } }) } }))
  assert.deepEqual(gone.checkRun.title, ['The folder recorded for this Change is not on this machine, so this page cannot say what the Action would find there.'])
  assert.equal(gone.checkRun.tag, null)
  const err = page(snapshot(T), ctx({ pr: { doc: null, error: 'down' } }))
  assert.match(err.checkRun.title[0], /could not read this Change’s folder/)
  assert.deepEqual(err.checkRun.conclusions, [])
  const out = page(snapshot(T), ctx({ pr: { doc: null, loading: true } }))
  assert.deepEqual(out.checkRun.title, ['Reading this Change’s folder…'])
})

/* ---------------------------------------------------------- row 5: trailer and badge */

const section4 = (d) => { const h = html(PR, { data: d }); const a = h.indexOf('aria-labelledby="s4"'); assert.ok(a > 0, 'section 4 is drawn'); return h.slice(a, h.indexOf('</main>')) }

test('row 5: the commit trailer comes from the standing approval in change_decisions, and only then', () => {
  const none = page()
  assert.equal(none.trailer.text, null)
  const s0 = section4(none)
  assert.doesNotMatch(s0, /ThruLine-Approved/)
  assert.match(words(s0), /No approval stands for this Change, so there is no trailer yet\./)
  const t = approvedT()
  const d = page(snapshot(t), ctx({ tracked: t }))
  assert.equal(d.trailer.text, TRAILER_KEY + 'bbbbbbb')
  const s = section4(d)
  assert.match(s, /aria-label="Commit message"/)
  assert.match(words(s), /ThruLine-Approved: bbbbbbb/)
  assert.match(words(s), /It comes with the approval given in ThruLine by sam at 2026-09-29 19:00Z, and names the one version approved\. ThruLine never commits\./)
  /* withdrawn: the record holds no standing row, so no trailer */
  const w = tracked({ ready: true, states: ['proven', 'proven', 'proven', 'proven'], standing: null, history: [STANDING, { id: 'dec2', act: 'withdrawn', target_revision: TARGET, actor: 'sam', decided_at: '2026-09-29T19:05:00Z' }] })
  assert.equal(page(snapshot(w), ctx({ tracked: w })).trailer.text, null)
})

test('row 5: the trailer is written exactly as the Approve sheet writes it, for the same version', () => {
  const t = approvedT()
  const s = snapshot(t)
  const as = selectApproveSheet(s, { id: CHANGE_ID, change: { ...t.change, decisions: { ...t.change.decisions, history: [STANDING] } }, model: t.model }, { licence: { status: 'valid', plan: 'pro', entitlements: { approve_version: { allowed: true } } } })
  const v = AS.instance({ data: as }).renderVals()
  assert.equal(v.trailer, page(s, ctx({ tracked: t })).trailer.text)
})

test('row 5: no badge is drawn as if one were made; its words are the CLI’s, served', () => {
  const d = page()
  const s = section4(d)
  assert.doesNotMatch(s, /<svg role="img"/)
  assert.equal(inlineMd(d.badge.lines[0]), 'No badge is drawn here. The CLI draws it from a checked receipt: `thruline verify-receipt <receipt> --offline --format badge`, and it reads verified, partial or not verified.')
  assert.match(s, /No badge is drawn here\. The CLI draws it from a checked receipt: <code[^>]*>thruline verify-receipt <receipt> --offline --format badge<\/code>, and it reads verified, partial or not verified\./)
  assert.match(words(s), /This folder holds no receipt, so there is no badge at all\./)
})

/* ---------------------------------------------------------- row 5: edited in place */

test('row 5: the edited-in-place panel compares the first read on this page with now, from typed fields only', () => {
  const s0 = snapshot(T)
  const first = { facts: noteFacts(s0, { now: NOW, tracked: T }), readAt: new Date(NOW).toISOString() }
  assert.equal(page(s0, ctx()).edited, null, 'no first read, no panel')
  const same = page(s0, ctx({ first }))
  assert.equal(same.edited.title, '07:00Z · nothing to edit yet')
  assert.deepEqual(same.edited.lines, [])
  assert.match(same.edited.delta, /^Nothing in the record has changed since 07:00Z, when this page first read it\./)
  /* a passing run on this version, and line 2 re-checked */
  const t2 = tracked({ states: ['proven', 'proven', 'none', 'not_proven'] })
  const s2 = snapshot(t2, { commands: [{ id: 'c1', session_id: 's1', ts: '2026-09-29T18:10:00Z', command: 'pytest -q tests', is_test: true, outcome: 'succeeded', exit_code: 0, tally: { passed: 7, failed: 0 }, version_id: 'aaaaaaa1' }, { id: 'c2', session_id: 's1', ts: '2026-09-29T18:25:00Z', command: 'cd /Users/someone/code/shop && pytest -q', is_test: true, outcome: 'unknown', exit_code: null, version_id: 'bbbbbbb2' }, { id: 'c3', session_id: 's1', ts: '2026-09-29T18:45:00Z', command: 'pytest -q tests', is_test: true, outcome: 'succeeded', exit_code: 0, tally: { passed: 8, failed: 0 }, version_id: 'bbbbbbb2' }] })
  const later = page(s2, ctx({ tracked: t2, first, now: NOW + 60000 }))
  assert.equal(later.edited.title, '07:01Z · the same note, edited in place')
  assert.equal(later.edited.lines.length, 3)
  assert.deepEqual(later.edited.lines[0], later.note.lines[0])
  assert.match(later.edited.delta, /Since 07:00Z, when this page first read the record: 1 test run added · line 2 now reads Proven \(was Needs a re-check\)\./)
  assert.deepEqual(changesSince(first.facts, first.facts), [])
  const h = html(PR, { data: later })
  assert.match(words(h), /07:01Z · the same note, edited in place/)
  assert.ok(!h.includes('16:09'))
})

/* ---------------------------------------------------------- Copy, and the setup line */

test('Copy as Markdown copies exactly the comment shown, then the Action setup line drawn under it', () => {
  const d = page()
  assert.equal(d.copy, markdownOf(d.note) + '\n' + inlineMd(d.setup) + '\n')
  assert.ok(d.copy.startsWith(d.markdown))
  const h = html(PR, { data: d })
  const u = h.indexOf('<p id="pr-setup"')
  assert.ok(u > h.indexOf('</figure>'), 'the setup line is drawn under the comment')
  assert.equal(words(h.slice(u, h.indexOf('</p>', u))).trim(), inlineMd(d.setup).replace(/`/g, ''))
})

test('the setup line is the Action’s own command, with the Action’s own arguments (packaging/action/action.yml)', () => {
  const action = readFileSync(join(repo, 'packaging/action/action.yml'), 'utf8')
  /* the Action runs exactly these flags to make the check run it posts */
  assert.match(action, /thruline verify-receipt "\$receipt" \$\{key_arg\[@\]\+"\$\{key_arg\[@\]\}"\} --offline \\\n\s+--format check-run --head-sha "\$\{GITHUB_SHA\}" > check-run\.json/)
  assert.match(action, /ls -t \.thruline\/receipts\/\*\.json/)
  assert.equal(verifyCommand(null), 'thruline verify-receipt .thruline/receipts/latest.json --offline --format check-run --head-sha "$GITHUB_SHA"')
  /* the book's own receipt name, when the folder holds none */
  assert.match(readFileSync(join(repo, 'docs/book/src/ci.md'), 'utf8'), /thruline receipt --out \.thruline\/receipts\/latest\.json/)
  assert.match(readFileSync(join(repo, 'docs/book/src/ci.md'), 'utf8'), /The Action does not exist yet/)
  assert.equal(verifyCommand('r-1.json'), 'thruline verify-receipt .thruline/receipts/r-1.json --offline --format check-run --head-sha "$GITHUB_SHA"')
  /* a name that cannot be written into a command as it is falls back, never pasted raw */
  assert.equal(verifyCommand('a b;rm -rf.json'), verifyCommand(null))
  assert.equal(inlineMd(setupOf({ checkName: 'thruline/receipt' })), 'In CI: the ThruLine Action is not published yet, so add its check as a step. `' + verifyCommand(null) + '` prints the `thruline/receipt` check run the Action would post.')
})

/* ---------------------------------------------------------- row 6: work nobody tracked */

test('work nobody tracked gets no note — the page says what a tracked Change shows, and offers to track it', () => {
  const d = prFaceFrom(snapshot(null), { now: NOW })
  for (const k of ['note', 'markdown', 'setup', 'copy', 'approval', 'checkRun', 'trailer', 'badge', 'edited']) assert.equal(d[k], null, k)
  assert.deepEqual(d.untracked, { why: UNTRACKED.why, what: UNTRACKED.what, next: UNTRACKED.next, busy: false, said: null })
  const h = html(PR, { data: d })
  const w = words(h)
  assert.match(w, /This work is not tracked as a Change, so ThruLine has no note for its pull request\./)
  assert.match(w, /one row for each line of what counts as done/)
  assert.equal((h.match(/class="tl-btn"/g) || []).length, 1)
  assert.match(h, /<button type="button" class="tl-btn"[^>]*>Track this work as a Change<\/button>/)
  assert.doesNotMatch(h, /<table|thruline:note|Copy as Markdown|The approval it waits on|aria-labelledby="s3"|ThruLine-Approved/)
  let tracks = 0
  PR.instance({ data: d, actions: { track: () => { tracks += 1 } } }).renderVals().onTrack()
  assert.equal(tracks, 1)
  const busy = prFaceFrom(snapshot(null), { now: NOW, track: { busy: true } })
  assert.match(html(PR, { data: busy }), /disabled=""[^>]*>Track this work as a Change/)
  let again = 0
  PR.instance({ data: busy, actions: { track: () => { again += 1 } } }).renderVals().onTrack()
  assert.equal(again, 0, 'a press while busy sends nothing')
  const said = prFaceFrom(snapshot(null), { now: NOW, track: { said: 'ThruLine could not track this work, so nothing was written. Reload this page and try again.' } })
  assert.match(html(PR, { data: said }), /role="alert"[^>]*>ThruLine could not track this work/)
})

test('row 6: the six contrasts stay in the gallery and out of the product', () => {
  assert.match(html(PR, {}), /Why this beats a walkthrough/)
  for (const d of [page(), prFaceFrom(snapshot(null), { now: NOW })]) assert.doesNotMatch(html(PR, { data: d }), /Why this beats a walkthrough|aria-labelledby="s5"|coderabbit/)
})

/* ---------------------------------------------------------- row 7: links in */

test('row 7: The whole story links in to this Change’s pull-request page; its gallery does not', () => {
  const at = { screen: 'story', change: CHANGE_ID, project: 'p1' }
  const ws = wholeStoryFrom(snapshot(T), { change: T.change, hq: T.model, proposals: null }, { now: NOW })
  const h = html(WS, { data: ws, at })
  assert.match(h, new RegExp('<a href="#/changes/' + CHANGE_ID + '/pr\\?project=p1" class="tl-ghost"[^>]*>On the pull request<span aria-hidden="true">→</span></a>'))
  assert.doesNotMatch(html(WS, {}), /On the pull request/)
})

test('row 7: the Approve sheet links in beside the trailer an approval gives; its gallery does not', () => {
  const at = { screen: 'approve', change: CHANGE_ID, project: 'p1' }
  const t = approvedT()
  const as = selectApproveSheet(snapshot(t), { id: CHANGE_ID, change: { ...t.change, decisions: { ...t.change.decisions, history: [STANDING] } }, model: t.model }, { licence: { status: 'valid', plan: 'pro', entitlements: { approve_version: { allowed: true } } } })
  const h = html(AS, { data: as, at }, { open: true })
  assert.match(h, /ThruLine-Approved: bbbbbbb/)
  assert.match(h, new RegExp('What goes on the pull request: <a href="#/changes/' + CHANGE_ID + '/pr\\?project=p1">the note, and this trailer<span aria-hidden="true"> →</span></a>'))
  for (const st of [undefined, { open: true }, { open: true, history: [{ kind: 'approved', time: '16:10', name: 'Sam', note: '' }] }]) {
    assert.doesNotMatch(html(AS, {}, st), /What goes on the pull request/)
  }
})

/* ---------------------------------------------------------- the words */

test('the live page uses none of the words the copy rules refuse, and no home folder', () => {
  const t = approvedT()
  for (const d of [page(), page(snapshot(t), ctx({ tracked: t })), prFaceFrom(snapshot(null), { now: NOW })]) {
    const w = words(html(PR, { data: d }))
    assert.doesNotMatch(w, /\b(lie|lies|lied|cheat|violate|ignore|ignored|hallucinat\w*|trial)\b/i)
    assert.doesNotMatch(w, /Not available in this version|\d\s?%|\+00:00|\bIST\b|~\/|\/Users\//)
  }
})

test('the passive preview stays a GET; the explicit check reuses it with a token', () => {
  assert.equal(PR_PREVIEW(CHANGE_ID), '/api/changes/' + CHANGE_ID + '/pr')
  const src = readFileSync(join(here, 'read.js'), 'utf8')
  assert.match(src, /useResource\(trackedId \? PR_PREVIEW\(trackedId\) : null/)
  assert.match(src, /PR_PREVIEW\(id\) \+ '\?check=1'/)
  assert.match(src, /X-ThruLine-Run-Token/)
  assert.doesNotMatch(src, /method:\s*['"]POST/)
})

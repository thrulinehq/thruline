/* P6-4 — Ask, a scoped command bar, never a chat (design record §3.5).
 *
 * One test per acceptance line on the card:
 *  - the five questions, each answered from the ONE derived view-model, on the
 *    agreement fixture and on every state a criterion can be in;
 *  - "The record doesn't say" for each of the five, with the case that makes
 *    it say so named, and the three parts §3.5 requires present every time;
 *  - free text lands on one of the five or on that same card with the five
 *    offered — and on no third path;
 *  - the fence: every string in a rendered answer is either a string of the
 *    copy file or a value of the model. Nothing in the answer path is written
 *    anywhere else, and no model produces any of it;
 *  - the evidence doors move the Change screen to the line they name;
 *  - the keyboard: `/`, ⌘K, Escape, and focus back where it was.
 *
 * Every fixture is synthetic. No string here came off a real machine.
 */

import test, { after } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'
import { JSDOM } from 'jsdom'
import { HQ_COPY } from './hq-copy.js'
import { QUESTIONS, matchQuestion } from './ask-hq.js'

const here = dirname(fileURLToPath(import.meta.url))
const temporary = await mkdtemp(join(tmpdir(), 'thruline-ask-'))
after(() => rm(temporary, { recursive: true, force: true }))

const COMMON = {
  bundle: true, write: false, format: 'esm', platform: 'node',
  banner: { js: "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" },
  loader: { '.css': 'empty' }, define: { 'import.meta.env': '{}' }, logLevel: 'silent',
}
async function load(name, contents) {
  const result = await build({ ...COMMON, stdin: { contents, resolveDir: here } })
  const file = join(temporary, name)
  await writeFile(file, result.outputFiles[0].text)
  return import(pathToFileURL(file).href)
}

const M = await load('ask.mjs', `
  import React from 'react';
  import { renderToStaticMarkup } from 'react-dom/server';
  import { AskPanel } from './ask-hq.jsx';
  import { answer, ask, matchQuestion, openDoor, scopeChip, suggestions, QUESTIONS } from './ask-hq.js';
  import { deriveChangeModel } from './change-model.js';
  export { answer, ask, matchQuestion, openDoor, scopeChip, suggestions, QUESTIONS, deriveChangeModel };
  export const renderAsk = (props) => renderToStaticMarkup(React.createElement(AskPanel, props));
`)

const A = HQ_COPY.ask
const doc = (html) => new JSDOM('<!doctype html><body>' + html + '</body>').window.document

/* ── one Change, and every state one accepted line can be in ──────────── */

const CHANGE = {
  id: 'ch1',
  project_id: 'p1',
  request: { title: 'Totals add up on the checkout page' },
  implementation: { snapshot_id: 'e71c5b0aaaa', bound_at: '2026-09-22T10:00:00Z', repository_identity: 'shop' },
  accepted_scope: {
    version: 1,
    criteria: ['The totals add up'],
    allowed_scope: ['checkout/'],
    check_profile: [],
    acceptance: { actor: 'sam', accepted_at: '2026-09-22T09:00:00Z' },
  },
  examination: 'none_yet',
}

const item = (over) => ({
  link_id: 'l1', evidence_id: 'e1', session_id: 's1', basis: 'declared', kind: 'check_run', name: 'tests pass',
  observed: 'proven', bears: true, applies: true, applies_slug: 'inputs_applies', fresh: true,
  fresh_slug: 'applicability_still_applies', ...over,
})

const coverageWith = (items) => ({
  scope_version: 1,
  link_rows: items.length,
  criteria: [{ index: 1, text: 'The totals add up', state: items.length ? 'evidence' : 'none', items }],
})

const APPLICABILITY = {
  items: [{
    link_id: 'l1', criterion_index: 1, version: 'e71c5b0',
    reason_slug: 'applicability_reason_source_changed', reason_paths: ['checkout/totals.py'],
  }],
}

/** The agreement fixture, and the four states beside the proven one. */
const FIXTURES = {
  /* Proven — the agreement fixture of `change-agreement.test.js`, same shape. */
  proven: () => M.deriveChangeModel(CHANGE, coverageWith([item({})]), null, null, []),
  /* Needs a re-check — the same record after the code moved under it. */
  needs_recheck: () => M.deriveChangeModel(CHANGE, coverageWith([item({ fresh: false, fresh_slug: 'applicability_needs_a_recheck' })]), APPLICABILITY, null, []),
  /* Not proven — the record holds the opposite. */
  not_proven: () => M.deriveChangeModel(CHANGE, coverageWith([item({ observed: 'not_proven' })]), null, null, []),
  /* Recorded — a run that settled nothing. */
  recorded: () => M.deriveChangeModel(CHANGE, coverageWith([item({ observed: 'recorded', bears: false })]), null, null, []),
  /* In progress — nothing behind the line at all. */
  in_progress: () => M.deriveChangeModel(CHANGE, coverageWith([]), null, null, []),
}

const IDS = QUESTIONS.map((q) => q.id)

/* ── 1. the five questions ─────────────────────────────────────────────── */

test('there are exactly five questions, they are the five the design record names, and each is offered by its own copy string', () => {
  assert.equal(IDS.length, 5)
  assert.deepEqual(IDS, ['needs_evidence', 'changed_since', 'why_not_approve', 'said_vs_record', 'not_recorded'])
  assert.deepEqual(M.suggestions().map((q) => q.id), IDS)
  for (const q of M.suggestions()) {
    assert.equal(q.question, A.questions[q.id].question)
    assert.match(q.question, /\?$/, `${q.id} is not a question`)
  }
  assert.equal(M.answer('anything_else', FIXTURES.proven()), null, 'there is no sixth question')
})

test('every question answers on every state a line can be in, or says the record doesn\'t say — and never a blank either way', () => {
  for (const [state, make] of Object.entries(FIXTURES)) {
    const hq = make()
    for (const id of IDS) {
      const r = M.answer(id, hq)
      assert.ok(r, `${state}/${id}: no answer object at all`)
      assert.equal(r.question, A.questions[id].question)
      if (r.answered) {
        assert.ok(r.sentences.length, `${state}/${id}: answered with nothing said`)
        assert.ok(r.action, `${state}/${id}: an answer with nothing to do next`)
      } else {
        /* The three parts, every time, never one of them missing. */
        for (const part of ['lookedFor', 'missing', 'wouldAnswer']) {
          assert.ok(r[part] && r[part].length > 20, `${state}/${id}: ${part} teaches nothing`)
        }
        assert.deepEqual(r.sentences, [], `${state}/${id}: a "doesn't say" card with sentences in it`)
      }
    }
  }
})

test('what still needs evidence: the waiting lines on this record, and the proven one says so with its version and scope', () => {
  const waiting = M.answer('needs_evidence', FIXTURES.in_progress())
  assert.equal(waiting.answered, true)
  assert.equal(waiting.sentences[0], A.questions.needs_evidence.lead_one)
  assert.equal(waiting.evidence[0].door.anchor, 'ws-cov-line-1')

  const settled = M.answer('needs_evidence', FIXTURES.proven())
  assert.equal(settled.sentences[0], A.questions.needs_evidence.lead_none)
  /* Never a bare all-clear: the lines it rests on are printed under it, each
   * with its own version and scope. */
  assert.ok(settled.sentences[1].includes('e71c5b0') && settled.sentences[1].includes('checkout/'), settled.sentences[1])
})

test('what changed since the last check: the model\'s own freshness answer, and nothing said where nothing was assessed', () => {
  const moved = M.answer('changed_since', FIXTURES.needs_recheck())
  assert.equal(moved.answered, true)
  assert.equal(moved.sentences[0], FIXTURES.needs_recheck().changedSince.text)
  assert.ok(moved.sentences.some((s) => s.includes('needs a re-check')), moved.sentences.join(' | '))

  const none = M.answer('changed_since', FIXTURES.in_progress())
  assert.equal(none.answered, false, 'a line nobody assessed cannot be said to have moved or held')
  assert.equal(none.missing, A.questions.changed_since.missing)
})

test('why can\'t I approve: with no readiness answer in the record, Ask says so rather than inventing a blocker', () => {
  const r = M.answer('why_not_approve', FIXTURES.proven())
  assert.equal(r.answered, false, 'this fixture carries no readiness block')
  assert.equal(r.lookedFor, A.questions.why_not_approve.looked_for)

  /* With one, the answer is the readiness computation's own blocker and the
   * obligations it lists — no second opinion about the same record.
   * D-050 (RUN-LINK-TRUTH-R5): the answer used to open with the readiness
   * count sentence and then each line's own reason, which is a second account
   * of why approval is off beside the one Brief, the Approve stage, the
   * Inspector and What we checked give. It now leads with that one blocker. */
  const coverage = { ...coverageWith([item({})]), readiness: {
    ready: false,
    target_version: 'e71c5b0aaaa',
    headline_slug: 'readiness_needs_evidence',
    obligations: [{ id: 'o1', criterion_index: 1, text: 'The totals add up', outcome: 'needs_evidence', unsatisfied_slug: 'no_evidence' }],
    rests_on: [],
  } }
  const hq = M.deriveChangeModel(CHANGE, coverage, null, null, [])
  const with_ = M.answer('why_not_approve', hq)
  assert.equal(with_.answered, true)
  assert.equal(with_.sentences[0], hq.readiness.blocker.reason + ' ' + hq.readiness.blocker.enabler) // D-050: was hq.readiness.sentence
  assert.equal(with_.sentences.length, 1, 'one thing missing, so the blocker is the whole answer') // D-050: was sentences[1] starting with the line's words
  assert.equal(with_.evidence[0].label, 'The totals add up', 'the line it concerns is named beside it')
  assert.equal(with_.evidence[0].door.anchor, 'ws-cov-line-1', 'the door goes to the line that is missing evidence')
})

test('what the agent claimed that the record doesn\'t back: nothing written down is said as nothing written down, never as a clean bill', () => {
  const r = M.answer('said_vs_record', FIXTURES.proven())
  assert.equal(r.answered, false)
  assert.equal(r.missing, A.questions.said_vs_record.missing)
  /* And the card carries no word that could be read as a pass. */
  const dom = doc(M.renderAsk({ hq: FIXTURES.proven(), onGo() {} }))
  assert.doesNotMatch(dom.body.textContent, /all clear|nothing wrong|looks good/i)
})

test('what was not recorded: the gaps the model already derived, said as a list', () => {
  const r = M.answer('not_recorded', FIXTURES.in_progress())
  assert.equal(r.answered, true)
  const gaps = FIXTURES.in_progress().packet.notRecorded
  assert.ok(gaps.length, 'this fixture has a line with nothing behind it')
  for (const g of gaps) assert.ok(r.sentences.includes(g), g)
  /* Nothing accepted at all is a different fact, and it says so. */
  const bare = M.deriveChangeModel({ ...CHANGE, accepted_scope: null }, { scope_version: 1, link_rows: 0, criteria: [] }, null, null, [])
  assert.equal(M.answer('not_recorded', bare).answered, false)
})

/* ── 2. free text: one of the five, or the card, and no third path ────── */

test('typed words land on one of the five, or on "The record doesn\'t say" with the five offered — and nowhere else', () => {
  for (const q of M.suggestions()) assert.equal(matchQuestion(q.question), q.id, q.question)
  assert.equal(matchQuestion('what still needs evidence'), 'needs_evidence')
  assert.equal(matchQuestion('anything changed since we last checked?'), 'changed_since')
  assert.equal(matchQuestion('why is approve greyed out'), 'why_not_approve')
  assert.equal(matchQuestion('did the agent claim something'), 'said_vs_record')
  assert.equal(matchQuestion(''), null)
  assert.equal(matchQuestion('what is the weather in Lisbon'), null)

  const lost = M.ask('what is the weather in Lisbon', FIXTURES.proven())
  assert.equal(lost.answered, false)
  assert.equal(lost.id, null)
  assert.equal(lost.lookedFor, A.unmatched.looked_for)
  assert.equal(lost.wouldAnswer, A.unmatched.would_answer)
  const dom = doc(M.renderAsk({ hq: FIXTURES.proven(), onGo() {} }))
  assert.equal(dom.querySelectorAll('.ws-ask-suggest-btn').length, 5, 'the five stay offered')
})

/* ── 3. the fence: copy file and model, and no third source ───────────── */

/** Every string the copy file holds, flattened. */
function copyStrings(obj, out = []) {
  if (Array.isArray(obj)) obj.forEach((v) => copyStrings(v, out))
  else if (obj && typeof obj === 'object') Object.values(obj).forEach((v) => copyStrings(v, out))
  else if (typeof obj === 'string') out.push(obj)
  return out
}
const COPY = copyStrings(HQ_COPY)

/**
 * Whether a sentence could have come from the copy file: it is a copy string,
 * or a copy string with `{slots}` filled (the model's own templates), or it is
 * built out of copy strings with record values between them. The test below
 * pairs this with the model check, so a sentence has to be one or the other.
 */
/* A template made of slots and nothing else (the review lane's `finding_in`
 * is `{path}`) carries no word of the copy file, so it vouches for nothing:
 * whatever fills it is a record value, and the model check below is what has
 * to find it. Without this, one bare slot would match every sentence and the
 * fence would stop being an instrument (the A2 merge of `p4/review-repair`). */
const onlySlots = (c) => c.replace(/\{[a-z_]+\}/g, '').trim() === ''

function fromCopy(text) {
  return COPY.some((c) => c === text || (c.includes('{') && !onlySlots(c) && new RegExp('^' + c.replace(/[.*+?^${}()|[\]\\]/g, '\\$&').replace(/\\\{[a-z_]+\\\}/g, '.+') + '$').test(text)))
}

test('THE FENCE: every string in an answer is a string of the copy file or a value of the model — there is no third source, and no model in the path', () => {
  for (const [state, make] of Object.entries(FIXTURES)) {
    const hq = make()
    const inModel = JSON.stringify(hq)
    for (const id of IDS) {
      const r = M.answer(id, hq)
      const said = [
        ...r.sentences,
        ...r.evidence.flatMap((e) => [e.kindName, e.label]),
        r.action ? r.action.label : null,
        r.lookedFor, r.missing, r.wouldAnswer, r.question,
      ].filter(Boolean)
      assert.ok(said.length, `${state}/${id}: nothing said at all`)
      for (const s of said) {
        /* A sentence built out of two record values with a copy separator is
         * neither a whole copy string nor a whole model value; each PART is
         * checked, which is the same rule applied at the seam. */
        const parts = String(s).split(/ — |: /)
        for (const part of parts) {
          assert.ok(
            fromCopy(part) || inModel.includes(JSON.stringify(part).slice(1, -1)),
            `${state}/${id}: "${part}" is in neither the copy file nor the model`,
          )
        }
      }
    }
  }
})

test('the fence is a live instrument: a sentence nobody wrote fails it', () => {
  assert.equal(fromCopy(A.permanent), true)
  assert.equal(fromCopy('Everything looks fine to me.'), false)
  assert.equal(JSON.stringify(FIXTURES.proven()).includes('Everything looks fine to me.'), false)
})

test('neither Ask file carries a sentence of its own, and neither reaches an address', () => {
  for (const file of ['ask-hq.js', 'ask-hq.jsx']) {
    const whole = readFileSync(join(here, file), 'utf8')
    /* The comments are this file's own explanation of itself, and they quote
     * the design record's words on purpose. The rule is about what RENDERS,
     * so they are set aside before the scan rather than silently matched. */
    const src = whole.replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, '')
    /* No fetch, no address, no storage: Ask reads the model it was handed. */
    for (const re of [/\bfetch\s*\(/, /['"`]\/api\//, /localStorage/, /XMLHttpRequest/]) {
      assert.doesNotMatch(whole, re, `${file} reaches outside the model it was given`)
    }
    /* No natural-language literal: a string of two or more words, once the
     * class names (every word of which starts `ws-`) are set aside. Its reach
     * is exactly that; a one-word literal is not caught, and neither is text
     * assembled at runtime — the fence above is what holds those. */
    for (const m of src.matchAll(/'([^'\\\n]{6,})'|"([^"\\\n]{6,})"/g)) {
      const text = m[1] || m[2]
      if (!/[A-Za-z]{2,}\s+[A-Za-z]{2,}/.test(text)) continue
      if (text.split(/\s+/).every((w) => w.startsWith('ws-'))) continue
      assert.fail(`${file} carries literal copy instead of HQ_COPY.ask: ${text}`)
    }
  }
})

/* ── 4. what a person meets ───────────────────────────────────────────── */

test('the panel: the permanent line, a removable scope chip, the five questions before typing, and one action after an answer', () => {
  const hq = FIXTURES.in_progress()
  const d = doc(M.renderAsk({ hq, onGo() {} }))
  assert.equal(d.querySelector('.ws-ask-permanent').textContent, A.permanent)
  assert.ok(d.querySelector('.ws-ask-chip').textContent.includes(hq.title), 'the chip names this Change by its own title')
  assert.ok(d.querySelector('.ws-ask-chip-x') !== null, 'the chip can be taken out of scope')
  assert.equal(d.querySelectorAll('.ws-ask-suggest-btn').length, 5)
  assert.equal(d.querySelector('.ws-ask-answer'), null, 'nothing is answered before anything is asked')
  /* The box is off with both halves of why while there is nothing to ask. */
  const off = d.querySelector('.ws-change-blocked button')
  assert.equal(off.disabled, true)
  assert.ok(d.getElementById('ws-ask-why').textContent.includes(A.box.submit_why.enabler))
  /* No avatar, no typing indicator, no personality, no accusation. */
  assert.doesNotMatch(d.body.innerHTML, /avatar|typing|thinking|assistant|img /i)
})

test('an answer card names the evidence it used, and every door points at an element the Change screen renders', async () => {
  const hq = FIXTURES.needs_recheck()
  for (const id of IDS) {
    const r = M.answer(id, hq)
    if (!r.answered) continue
    for (const e of r.evidence) {
      assert.ok(['verify', 'approve'].includes(e.door.stage), `${id}: a door to a stage that is not on this screen`)
      assert.match(e.door.anchor, /^ws-(cov-line-\d+|change-readiness|change-decision|change-cards)$/, `${id}: ${e.door.anchor}`)
    }
  }
  /* And the anchors are real: the Change screen renders each of them. */
  const src = readFileSync(join(here, 'change-workspace.jsx'), 'utf8')
  for (const anchor of ['ws-cov-line-', 'ws-change-readiness', 'ws-change-cards']) {
    assert.ok(src.includes(anchor), `the Change screen renders no ${anchor}`)
  }
})

test('a door moves the screen to the stage that holds the evidence, and puts focus on the line itself', () => {
  const dom = new JSDOM('<!doctype html><body><li id="ws-cov-line-1">The totals add up</li></body>')
  const went = []
  const ran = []
  const opened = M.openDoor({ stage: 'verify', anchor: 'ws-cov-line-1' }, (s) => went.push(s), dom.window.document, (fn) => ran.push(fn))
  assert.equal(opened, true)
  assert.deepEqual(went, ['verify'], 'the stage moved first')
  ran.forEach((fn) => fn())
  const line = dom.window.document.getElementById('ws-cov-line-1')
  assert.equal(line.getAttribute('tabindex'), '-1', 'the line takes focus from script and stays out of the Tab order')
  assert.equal(dom.window.document.activeElement, line)
  /* No door is not an act. */
  assert.equal(M.openDoor(null, () => assert.fail('moved the screen with no door'), dom.window.document), false)
})

test('the scope chip is removable, and with nothing in scope Ask answers nothing and says why', () => {
  /* Rendered with no Change at all — the same state removing the chip leaves. */
  const d = doc(M.renderAsk({ hq: null, onGo() {} }))
  assert.equal(d.querySelector('.ws-ask-chip').textContent.includes(A.scope.chip_untitled), true)
  assert.equal(M.answer('needs_evidence', null), null, 'out of scope, there is nothing to look in')
})

/* ── 5. the keyboard ──────────────────────────────────────────────────── */

test('/ and ⌘K both open Ask; Escape closes it and focus goes back to what opened it', async () => {
  const drive = await load('ask-drive.mjs', `
    import React from 'react';
    import { createRoot } from 'react-dom/client';
    import { flushSync } from 'react-dom';
    import { ShellTopBar } from './shell-chrome.jsx';
    import { AskPanel } from './ask-hq.jsx';
    import { deriveChangeModel } from './change-model.js';
    export { React, createRoot, flushSync, ShellTopBar, AskPanel, deriveChangeModel };
  `)
  const dom = new JSDOM('<!doctype html><html><body><div id="root"></div></body></html>', { url: 'http://127.0.0.1/' })
  const w = dom.window
  const names = ['window', 'document', 'navigator', 'Node', 'Element', 'HTMLElement', 'Event', 'KeyboardEvent', 'MouseEvent', 'getComputedStyle', 'requestAnimationFrame', 'cancelAnimationFrame', 'localStorage']
  const held = names.map((n) => [n, Object.getOwnPropertyDescriptor(globalThis, n)])
  for (const n of names) if (w[n] !== undefined) Object.defineProperty(globalThis, n, { value: n === 'window' ? w : w[n], configurable: true, writable: true })
  const settle = () => new Promise((r) => setTimeout(r, 5))
  try {
    const hq = drive.deriveChangeModel(CHANGE, coverageWith([item({})]), null, null, [])
    const root = drive.createRoot(w.document.getElementById('root'))
    drive.flushSync(() => root.render(drive.React.createElement(drive.ShellTopBar, {
      crumbs: [], altitude: 'brief', register: 'manager', onAltitude() {}, onRegister() {},
      askPanel: ({ onClose }) => drive.React.createElement(drive.AskPanel, { hq, onGo() {}, onClose }),
    })))
    await settle()
    const d = w.document
    const press = async (key, extra) => {
      d.body.dispatchEvent(new w.KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...extra }))
      await settle(); await settle()
    }
    const door = d.querySelector('.ws-shell-ask-btn')
    door.focus()
    assert.equal(d.activeElement, door, 'the Ask door has focus before the key is pressed')

    for (const [key, extra] of [['/', {}], ['k', { metaKey: true }], ['k', { ctrlKey: true }]]) {
      await press(key, extra)
      assert.ok(d.querySelector('.ws-ask') !== null, `${extra.metaKey ? 'Cmd+' : extra.ctrlKey ? 'Ctrl+' : ''}${key} opened Ask`)
      assert.ok(d.querySelector('[role="dialog"]').contains(d.activeElement), 'focus is inside the sheet')
      await press('Escape')
      assert.equal(d.querySelector('.ws-ask'), null, 'Escape closed it')
      assert.equal(d.activeElement, door, 'focus went back to what opened it')
    }
    root.unmount()
  } finally {
    for (const [n, dsc] of held) {
      if (dsc) Object.defineProperty(globalThis, n, dsc)
      else delete globalThis[n]
    }
    w.close()
  }
})

/* ── STEP-7-LIMITS — "what changed since" is never an all-clear ─────────
 *
 * PW-F12: with every assessed line still applying, the answer opened with
 * "Nothing the record watches has changed since these were checked." —
 * which a person reads as "the code has not moved", when ThruLine has not
 * looked at anything committed since it last recorded the code. It now states
 * that limit, from the same field the Change screen prints. */
test('STEP-7-LIMITS · the changed-since answer states the record\'s limit instead of the all-clear', () => {
  const settled = FIXTURES.proven()
  const r = M.answer('changed_since', settled)
  assert.equal(r.answered, true, 'the fixture assesses the line, so Ask answers')
  assert.ok(!r.sentences.includes(HQ_COPY.change.model.changed_since_none), `the all-clear is still printed: ${r.sentences.join(' | ')}`)
  assert.ok(settled.codeLimit, 'the model derives no limit')
  assert.equal(r.sentences[0], settled.codeLimit.text)

  /* And where a line does need a re-check, the moved answer leads and the
   * limit is still said: what the record watched is not all that may have moved. */
  const moved = M.answer('changed_since', FIXTURES.needs_recheck())
  assert.equal(moved.sentences[0], FIXTURES.needs_recheck().changedSince.text)
  assert.ok(moved.sentences.includes(FIXTURES.needs_recheck().codeLimit.text), moved.sentences.join(' | '))
})

/* ── STEP-7-GIT-LINE — Ask's changed-since answer leads with the move ──── */

test('STEP-7-GIT-LINE · while the code has moved, the changed-since answer opens with that sentence and never the all-clear', () => {
  const moved = { recorded: 'a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0', now: 'f0e1d2c3b4a5968778695a4b3c2d1e0f9a8b7c6d', changed_paths: ['src/lib.rs'], commits: 1, uncommitted_compared: false }
  const withMove = (c) => ({ ...c, code_moved: moved })
  const settled = M.deriveChangeModel(withMove(CHANGE), coverageWith([item({})]), null, null, [])
  assert.ok(settled.codeMoved, 'no moved line derived')
  const r = M.answer('changed_since', settled)
  assert.equal(r.answered, true)
  assert.equal(r.sentences[0], settled.codeMoved.sentence)
  assert.ok(!r.sentences.includes(HQ_COPY.change.model.changed_since_none), r.sentences.join(' | '))

  const stale = M.deriveChangeModel(withMove(CHANGE), coverageWith([item({ fresh: false, fresh_slug: 'applicability_needs_a_recheck' })]), APPLICABILITY, null, [])
  assert.equal(M.answer('changed_since', stale).sentences[0], stale.codeMoved.sentence)

  /* With no line assessed, the move is still a fact the record can state. */
  const unassessed = M.deriveChangeModel(withMove(CHANGE), coverageWith([]), null, null, [])
  const u = M.answer('changed_since', unassessed)
  assert.equal(u.answered, true)
  assert.equal(u.sentences[0], unassessed.codeMoved.sentence)
})

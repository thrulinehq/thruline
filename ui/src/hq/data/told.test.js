/* C30 R-LEDGER: what a person told the agent, as the screens read it.
 *
 * The reader is pure and the record is the only source: a thread with no act
 * has no `told_agent` and yields null; an unknown "later work" never files a
 * thread as waiting; nothing here says a message was delivered. Synthetic
 * fixtures only. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { toldOf, toldWhen, toldRung, NOTHING_SENT, TOLD_MESSAGE_MAX_CHARS } from './told.js'
import { changeSnapshot } from './change.js'
import { loadChange } from './use-change.js'

const here = dirname(fileURLToPath(import.meta.url))
const NOW = new Date(2026, 9, 8, 17, 0)
const at = (day, h, m) => new Date(2026, 9, day, h, m).toISOString()
const served = (over) => ({ last_act: 'copied', at: at(8, 16, 2), version_id: 'wt-1', act_id: 'cam_1', later_work: false, ...over })

test('no act, nothing: a thread without told_agent reads as null, never as a made-up copy', () => {
  assert.equal(toldOf(undefined, NOW), null)
  assert.equal(toldOf(null, NOW), null)
  assert.equal(toldOf({}, NOW), null)
  assert.equal(toldOf({ last_act: 'sent', at: at(8, 16, 2), later_work: false }, NOW), null, 'there is no act that means sent')
  assert.equal(toldOf('copied', NOW), null)
})

test('a copy and a pasted mark with no work since are waiting on the agent; nothing else is', () => {
  assert.equal(toldOf(served(), NOW).waiting, true)
  assert.equal(toldOf(served({ last_act: 'marked_pasted' }), NOW).waiting, true)
  assert.equal(toldOf(served({ later_work: true }), NOW).waiting, false, 'work came after the act')
  assert.equal(toldOf(served({ last_act: 'unmarked_pasted' }), NOW).waiting, false, 'a mark taken back is not a hand-off')
  assert.equal(toldOf(served({ later_work: undefined }), NOW).waiting, false, 'unknown later work never files a thread as waiting')
  assert.equal(toldOf(served({ later_work: 'false' }), NOW).waiting, false, 'only a real boolean counts')
  const t = toldOf(served({ last_act: 'unmarked_pasted' }), NOW)
  assert.equal(t.standing, false)
  assert.equal(t.act, 'unmarked_pasted')
})

test('the time is the act’s own, in this machine’s time, and never the word "yesterday"', () => {
  assert.equal(toldWhen(at(8, 16, 2), NOW), '16:02')
  assert.equal(toldWhen(at(7, 9, 5), NOW), '09:05 on 7 Oct')
  assert.equal(toldWhen(new Date(2025, 11, 31, 23, 59).toISOString(), NOW), '23:59 on 31 Dec 2025')
  assert.equal(toldWhen('not a time', NOW), '')
  assert.equal(toldWhen(undefined, NOW), '')
  assert.ok(!/yesterday/i.test(toldWhen(at(7, 9, 5), NOW)))
  const t = toldOf(served(), NOW)
  assert.equal(t.when, '16:02')
  assert.equal(t.actId, 'cam_1')
  assert.equal(t.versionId, 'wt-1')
})

test('the first rung reads the act, and says nothing was sent', () => {
  assert.deepEqual(toldRung(null), { reached: false, value: '' })
  assert.equal(toldRung(toldOf(served(), NOW)).value, 'Copied — not sent · 16:02')
  assert.equal(toldRung(toldOf(served({ last_act: 'marked_pasted', at: at(8, 16, 12) }), NOW)).value, 'You said you pasted it · 16:12')
  const undone = toldRung(toldOf(served({ last_act: 'unmarked_pasted' }), NOW))
  assert.match(undone.value, /took back “pasted”/)
  assert.match(undone.value, /not sent/)
  for (const act of ['copied', 'marked_pasted', 'unmarked_pasted']) {
    const r = toldRung(toldOf(served({ last_act: act }), NOW))
    assert.ok(r.reached)
    assert.ok(!/\bdelivered\b|\bsent to\b|\bsend\b/i.test(r.value), r.value)
  }
})

test('the sentence every surface says is the one the server says, and no forbidden word is in it', () => {
  const rust = readFileSync(join(here, '../../../../crates/core/src/change_agent_message.rs'), 'utf8')
  const m = rust.match(/pub const NOTHING_SENT: &str = "((?:[^"\\]|\\.|\\\n)*)"/s)
  assert.ok(m, 'the Rust constant is found')
  const said = m[1].replace(/\\\n\s*/g, '')
  assert.equal(said, NOTHING_SENT)
  assert.match(NOTHING_SENT, /^Nothing was sent\./)
  for (const w of ['lie', 'cheat', 'violate', 'ignore', 'hallucinat', 'trial']) assert.ok(!NOTHING_SENT.toLowerCase().includes(w), w)
  const max = rust.match(/pub const MAX_MESSAGE_CHARS: usize = (\d+);/)
  assert.equal(Number(max[1]), TOLD_MESSAGE_MAX_CHARS)
})

test('the thread’s act reaches the snapshot from the project view, and only from there', async () => {
  const thread = { id: 'abcdef0123456789', title: 'Stop the double posting', whose_move: 'you', current_version_id: 'wt-1', proof: {} }
  const view = {
    schema_version: 1,
    project: { id: 'p1', name: 'shop', root: '/r/shop' },
    thread,
    prompts: [], edits: [], commands: [], claims: [], versions: [], sessions: [], files: [],
    current_version_id: 'wt-1',
    masking: { masked: 0, families: [], complete: true },
  }
  const projectView = (extra) => ({ project: { id: 'p1', members: [{ root: '/r/shop' }] }, threads: [{ id: thread.id, ...extra }, { id: 'other', told_agent: served({ act_id: 'cam_other' }) }] })
  const run = async (pv) => {
    const get = async (url) => {
      if (url.includes('/threads/')) return view
      if (url.endsWith('/api/hq/projects/p1')) return pv
      if (url.includes('/api/storage')) return { bytes: 1 }
      throw new Error('unexpected ' + url + ' 404')
    }
    return (await loadChange({ change: thread.id, project: 'p1' }, { get, readTracked: null, now: NOW.getTime() })).snapshot
  }
  const withAct = await run(projectView({ told_agent: served() }))
  assert.equal(withAct.change.told.act, 'copied')
  assert.equal(withAct.change.told.actId, 'cam_1', 'this thread’s act, not the other thread’s')
  assert.equal(withAct.change.told.waiting, true)
  const without = await run(projectView({}))
  assert.equal(without.change.told, null)
  const unread = await run(null)
  assert.equal(unread.change.told, null, 'a project view that could not be read says nothing was told')
  assert.equal(changeSnapshot(view, null, NOW, {}).change.told, null)
})

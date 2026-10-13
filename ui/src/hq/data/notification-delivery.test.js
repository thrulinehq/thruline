import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createNotificationDelivery } from './notification-delivery.js'
import { allowBrowserNotifications } from './browser-notification-permission.js'
import { localWeekKey, summaryMarkdown } from './weekly-summary.js'

function memory() {
  const map = new Map()
  return { getItem: (key) => map.get(key) || null, setItem: (key, value) => { map.set(key, value) } }
}
const off = { ready: false, not_backed: false, agent_waiting: false, recording_stopped: false }
const event = (key) => ({ key, kind: 'ready', title: 'Your move', body: 'Ready', url: '#/changes/c1/approve' })
const read = (events, complete = { ready: true, agent_waiting: true }) => ({ projectId: 'p1', complete, events })

test('first enable baselines current items; later item announces once across reloads', () => {
  const storage = memory()
  const sent = []
  class Note { static permission = 'granted'; constructor(title, opts) { sent.push([title, opts.tag]) } }
  const on = { ...off, ready: true }
  const first = createNotificationDelivery({ storage, NotificationClass: Note })
  assert.equal(first.deliver(read([event('ready:c1:v1')]), on).sent, 0)
  assert.equal(first.deliver(read([event('ready:c1:v1'), event('ready:c2:v1')]), on).sent, 1)
  assert.equal(first.deliver(read([event('ready:c2:v1')]), on).sent, 0)
  const reopened = createNotificationDelivery({ storage, NotificationClass: Note })
  assert.equal(reopened.deliver(read([event('ready:c2:v1')]), on).sent, 0)
  assert.deepEqual(sent, [['Your move', 'ready:c2:v1']])
  reopened.disableOff(off)
  assert.equal(reopened.deliver(read([event('ready:c3:v1')]), on).sent, 0, 'switching back on baselines items from while Off')
})

test('a reopened page baselines unseen ready and waiting items from while HQ was closed', () => {
  const storage = memory()
  const sent = []
  class Note { static permission = 'granted'; constructor(_, opts) { sent.push(opts.tag) } }
  const on = { ...off, ready: true, agent_waiting: true }
  const waiting = (id) => ({ key: `agent_waiting:p1:${id}`, kind: 'agent_waiting', title: 'Agent waiting', body: 'Answer', url: '#/inbox/p1' })
  const first = createNotificationDelivery({ storage, NotificationClass: Note })
  first.deliver(read([event('ready:c1:v1'), waiting('q1')]), on)
  first.deliver(read([event('ready:c1:v1'), event('ready:c2:v1'), waiting('q1'), waiting('q2')]), on)
  const reopened = createNotificationDelivery({ storage, NotificationClass: Note })
  assert.equal(reopened.deliver(read([event('ready:c2:v1'), event('ready:c3:v1'), waiting('q2'), waiting('q3')]), on).sent, 0)
  assert.equal(reopened.deliver(read([event('ready:c3:v1'), event('ready:c4:v1'), waiting('q3'), waiting('q4')]), on).sent, 2)
  assert.deepEqual(sent, ['ready:c2:v1', 'agent_waiting:p1:q2', 'ready:c4:v1', 'agent_waiting:p1:q4'])
})

test('a failed first project read cannot establish the page baseline', () => {
  const storage = memory()
  const sent = []
  class Note { static permission = 'granted'; constructor(_, opts) { sent.push(opts.tag) } }
  const on = { ...off, ready: true }
  const delivery = createNotificationDelivery({ storage, NotificationClass: Note })
  assert.equal(delivery.deliver(read([event('ready:c1:v1')], { ready: false }), on).sent, 0)
  assert.equal(delivery.deliver(read([event('ready:c1:v1')]), on).sent, 0)
  assert.equal(delivery.deliver(read([event('ready:c1:v1'), event('ready:c2:v1')]), on).sent, 1)
  assert.deepEqual(sent, ['ready:c2:v1'])
})

test('returning to a project baselines items and recorder changes from while another project was open', () => {
  const sent = []
  class Note { static permission = 'granted'; constructor(_, opts) { sent.push(opts.tag) } }
  const delivery = createNotificationDelivery({ storage: memory(), NotificationClass: Note })
  const on = { ...off, ready: true, recording_stopped: true }
  const facts = (projectId, events, recordings = []) => ({ projectId, complete: { ready: true }, events, recordings })
  delivery.deliver(facts('p1', [event('ready:p1:c1')], [{ project: 'p1', active: true, stopped: false }]), on)
  assert.equal(delivery.deliver(facts('p1', [event('ready:p1:c1'), event('ready:p1:c2')]), on).sent, 1)
  delivery.deliver(facts('p2', [event('ready:p2:c1')]), on)
  assert.equal(delivery.deliver(facts('p1', [event('ready:p1:c1'), event('ready:p1:c2'), event('ready:p1:c3')], [{ project: 'p1', active: false, stopped: true }]), on).sent, 0)
  assert.equal(delivery.deliver(facts('p1', [event('ready:p1:c3'), event('ready:p1:c4')]), on).sent, 1)
  assert.deepEqual(sent, ['ready:p1:c2', 'ready:p1:c4'])
})

test('a stopped recorder announces only after active and stopped are both observed in this open page', () => {
  const sent = []
  class Note { static permission = 'granted'; constructor(title) { sent.push(title) } }
  const storage = memory()
  const on = { ...off, recording_stopped: true }
  const first = createNotificationDelivery({ storage, NotificationClass: Note })
  const active = { project: 'p1', active: true, stopped: false }
  const stopped = { project: 'p1', active: false, stopped: true }
  first.deliver({ recordings: [active] }, on)
  first.deliver({ recordings: [stopped] }, on)
  first.deliver({ recordings: [stopped] }, on)
  assert.deepEqual(sent, ['Recording stopped'])
  const reopened = createNotificationDelivery({ storage, NotificationClass: Note })
  reopened.deliver({ recordings: [stopped] }, on)
  assert.deepEqual(sent, ['Recording stopped'], 'closed-page stale activity cannot announce')
})

test('storage unavailable forbids delivery because once-only cannot be guaranteed', () => {
  const d = createNotificationDelivery({ storage: null, NotificationClass: class Note { static permission = 'granted' } })
  assert.equal(d.deliver(read([event('ready:c1:v1')]), { ...off, ready: true }).sent, 0)
})

test('revoked browser permission is visible and does not consume an unseen item', () => {
  const storage = memory()
  const sent = []
  class Note { static permission = 'granted'; constructor(_, opts) { sent.push(opts.tag) } }
  const on = { ...off, ready: true }
  const delivery = createNotificationDelivery({ storage, NotificationClass: Note })
  delivery.deliver(read([]), on)
  Note.permission = 'denied'
  const blocked = delivery.deliver(read([event('ready:c1:v1')]), on)
  assert.equal(blocked.sent, 0)
  assert.match(blocked.reason, /not allowed now/)
  Note.permission = 'granted'
  assert.equal(delivery.deliver(read([event('ready:c1:v1')]), on).sent, 1)
  assert.deepEqual(sent, ['ready:c1:v1'])
})

test('first switch-on asks browser permission; denial or no API keeps it Off', async () => {
  let asks = 0
  class Ask { static permission = 'default'; static async requestPermission() { asks++; return 'granted' } }
  assert.deepEqual(await allowBrowserNotifications(Ask), { allowed: true, reason: null })
  assert.equal(asks, 1)
  class Denied { static permission = 'denied'; static async requestPermission() { asks++ } }
  assert.equal((await allowBrowserNotifications(Denied)).allowed, false)
  assert.equal((await allowBrowserNotifications(null)).allowed, false)
  assert.equal(asks, 1)
})

test('weekly card key follows local Monday and Markdown names exact record counts', () => {
  assert.equal(localWeekKey(new Date(2026, 9, 4, 12)), '2026-09-28')
  assert.equal(localWeekKey(new Date(2026, 9, 5, 0)), '2026-10-05')
  const text = summaryMarkdown({ schema_version: 1, since: '2026-10-02', through: '2026-10-09', approvals: 2, not_backed: 3, rechecks: 4, projects_recorded: 5 })
  assert.match(text, /Changes approved: 2/)
  assert.match(text, /Not backed findings: 3/)
  assert.match(text, /Re-checks run: 4/)
  assert.match(text, /Projects recorded: 5/)
  assert.equal(summaryMarkdown({ schema_version: 2 }), null)
  assert.equal(summaryMarkdown({ schema_version: 1, since: '2026-10-02', through: '2026-10-09', approvals: 2, not_backed: 3, rechecks: 4 }), null)
  assert.equal(summaryMarkdown({ schema_version: 1, since: 'invalid', through: '2026-10-09', approvals: 2, not_backed: 3, rechecks: 4, projects_recorded: 5 }), null)
})

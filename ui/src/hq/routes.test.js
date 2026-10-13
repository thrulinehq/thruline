/* HQ Next routing: the addresses each screen lives at, and where the canvas's
 * prototype links go. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { parse, href, linker, nextRoute, SCREENS, LINK_TARGETS } from './routes.js'

test('#/changes/<id>/pr opens On the pull request for that Change', () => {
  assert.equal(SCREENS.pr, 'PR-Face')
  assert.deepEqual(parse('#/changes/654c1e5fdb4b52c6/pr?project=dfbfd6089e4bb388'), { screen: 'pr', change: '654c1e5fdb4b52c6', project: 'dfbfd6089e4bb388' })
  assert.equal(href('pr', { change: 'abc', project: 'p1' }), '#/changes/abc/pr?project=p1')
  assert.deepEqual(parse(href('pr', { change: 'abc' })), { screen: 'pr', change: 'abc', project: undefined })
})

test('the canvas’s link to PR-Face goes to the Change in front of the person, or is no link (C30 R-LINKS: never the Inbox)', () => {
  assert.equal(linker({ change: 'abc', project: 'p1' })('PR-Face.dc.html'), '#/changes/abc/pr?project=p1')
  assert.equal(linker({ project: 'p1' })('PR-Face.dc.html'), undefined)
})

test('the other Change screens are where they were', () => {
  for (const sub of ['statements', 'changed', 'done', 'check', 'approve', 'story']) {
    assert.equal(parse('#/changes/abc/' + sub).screen, sub)
  }
  assert.equal(parse('#/changes/abc').screen, 'brief')
  assert.equal(parse('#/changes/abc/nonsense').screen, 'brief')
})

test('C27: the default address is the Inbox (of the most recently active project with records); #/start is First run, for adding a folder', () => {
  for (const h of ['', '#', '#/', '#/inbox']) assert.deepEqual(parse(h), { screen: 'inbox', project: undefined }, JSON.stringify(h))
  assert.deepEqual(parse('#/start'), { screen: 'start', project: undefined })
  assert.equal(href('start'), '#/start')
  assert.equal(SCREENS.start, 'Onboarding')
})

/* C30 R-ROUTES "Step view": `#/changes/<id>/step/<n>` */

test('step view · the step address round-trips through parse and href, with its project and an encoded Change id', () => {
  assert.equal(SCREENS.step, 'Step-View')
  assert.equal(href('step', { change: 'abc', step: 12, project: 'p1' }), '#/changes/abc/step/12?project=p1')
  assert.deepEqual(parse('#/changes/abc/step/12?project=p1'), { screen: 'step', change: 'abc', project: 'p1', step: 12 })
  assert.deepEqual(parse(href('step', { change: 'abc', step: 7 })), { screen: 'step', change: 'abc', project: undefined, step: 7 })
  const odd = 'chg_a b/c'
  assert.deepEqual(parse(href('step', { change: odd, step: 3, project: 'p 1' })), { screen: 'step', change: odd, project: 'p 1', step: 3 })
})

test('step view · an address that names no step is still the step page, with no number: never the Brief, never the Inbox', () => {
  for (const [h, asked] of [['#/changes/abc/step', undefined], ['#/changes/abc/step/', undefined], ['#/changes/abc/step/0', '0'], ['#/changes/abc/step/-3', '-3'], ['#/changes/abc/step/1.5', '1.5'], ['#/changes/abc/step/abc', 'abc'], ['#/changes/abc/step/%E0%A4%A', '%E0%A4%A']]) {
    const r = parse(h)
    assert.equal(r.screen, 'step', h)
    assert.equal(r.change, 'abc', h)
    assert.equal(r.step, null, h)
    assert.equal(r.asked, asked, h)
  }
  assert.equal(href('step', { change: 'abc' }), '#/changes/abc/step', 'no number, no segment')
})

test('step view · the other Change screens are where they were; a step carries no statement', () => {
  for (const sub of ['statements', 'changed', 'done', 'check', 'approve', 'story', 'pr']) assert.equal(parse('#/changes/abc/' + sub).screen, sub)
  assert.equal(parse('#/changes/abc').screen, 'brief')
  assert.equal('statement' in parse('#/changes/abc/step/4?statement=s'), false)
})

test('step view · a place on the page does not leave the step; Find keeps the step it opened over', () => {
  const at = parse('#/changes/abc/step/9?project=p1')
  assert.deepEqual(nextRoute(at, '#ws-done'), at)
  assert.equal(linker({ ...at, screen: 'step' })('Command-Ask.dc.html'), '#/changes/abc/step/9?project=p1&find=1')
})

test('step view · no canvas link names the step page (a step needs its number); a screen reaches one with href(step)', () => {
  assert.equal(linker({ change: 'abc', project: 'p1' })('Step-View.dc.html'), undefined)
  assert.equal(LINK_TARGETS['Step-View'], undefined)
})

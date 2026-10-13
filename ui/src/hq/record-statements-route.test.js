/* Record KPI lists are read-only routes with the slice that the person clicked. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { href, parse, resolveLink, SCREENS } from './routes.js'

test('a Record statement-list address keeps range, agent, project slice and subset', () => {
  const where = { project: 'current', range: '7', agent: 'cx', scope: 'project:2', subset: 'moved' }
  const address = href('recordStatements', where)
  assert.equal(address, '#/record/statements?project=current&range=7&agent=cx&scope=project%3A2&subset=moved')
  assert.deepEqual(parse(address), { screen: 'recordStatements', ...where })
  assert.equal(SCREENS.recordStatements, 'Record-Statements')
})

test('the cross-Change list is a named route, not a fictitious canvas page', () => {
  assert.deepEqual(parse('#/record/statements'), {
    screen: 'recordStatements', project: undefined, range: '30', agent: 'all', scope: 'all', subset: 'all',
  })
  assert.equal(resolveLink('Record-Statements.dc.html'), undefined)
  assert.match(readFileSync(new URL('./app.jsx', import.meta.url), 'utf8'), /'Record-Statements': lazy\(\(\) => import\('\.\/screens\/Record-Statements\/index\.jsx'\)\)/)
  assert.equal(parse(href('recordStatements', { subset: 'recorded' })).subset, 'recorded')
})

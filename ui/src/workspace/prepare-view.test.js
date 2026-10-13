/* T-3-2 — what exactly will be examined, read as Sam reads it.
 *
 * The model is pure, so every one of these is a function call with a document
 * in and sentences out. The document here is the shape the engine serves; the
 * route test in `crates/cli/src/uiserve.rs` is what keeps the two in step.
 */

import test from 'node:test'
import assert from 'node:assert/strict'

import {
  buildPrepareView,
  isolationNote,
  fileCount,
  inputValue,
  lineSentence,
  prepareLines,
  recordsLine,
  shortRevision,
  technicalRows,
} from './prepare-view.js'

const known = (value) => ({ state: 'known', value })
const unknown = { state: 'unknown' }

function served(overrides = {}) {
  return {
    prepare: {
      schema_version: 1,
      change_id: 'c1',
      record_count: 1,
      app_port: 45781,
      app_program: 'serve',
      missing_slug: 'prepare_not_written_down',
      inputs: {
        snapshot_id: known('ws_0001'),
        source_identity: known('wsid1:aaaa'),
        base_revision: known('a1b2c3d4e5f6a7b8c9d0'),
        uncommitted: { state: 'present', files: 2, diff_sha256: known('dd') },
        worktree_path: known('/Users/example/projects/retry-delivery'),
        lockfiles_sha256: known('cccc'),
        config_sha256: unknown,
        config_files: [],
        tools: [
          { name: 'cargo', version: known('cargo 1.97.1'), present: true },
          { name: 'ruff', version: unknown, present: false },
        ],
        target: {
          kind: 'running_app',
          port: 45781,
          owner_pid: known('101'),
          owner_exe: known('/Users/example/projects/retry-delivery/target/debug/serve'),
          database_path: known('/Users/example/projects/retry-delivery/data/outbox.sqlite3'),
        },
      },
      lines: [
        { slug: 'prepare_repository', value: '/Users/example/projects/retry-delivery', tool: null },
        { slug: 'prepare_base', value: 'a1b2c3d4e5f6a7b8c9d0', tool: null },
        { slug: 'prepare_uncommitted_included', value: '2', tool: null },
        { slug: 'prepare_running_app', value: '45781', tool: null },
        { slug: 'prepare_tool_present', value: 'cargo 1.97.1', tool: 'cargo' },
        { slug: 'prepare_tool_absent', value: null, tool: 'ruff' },
      ],
      ...overrides,
    },
  }
}

test('unknown is not a value: it renders as "not written down", never blank', () => {
  assert.equal(inputValue(unknown), null)
  assert.equal(inputValue(known('x')), 'x')
  assert.equal(inputValue(null), null)
  assert.equal(inputValue({ state: 'known', value: '   ' }), null)

  const rows = technicalRows(served().prepare)
  const config = rows.find((r) => r.label === 'config_sha256')
  assert.equal(config.value, 'not written down')
})

test('a revision is shortened by the surface, not by the engine', () => {
  assert.equal(shortRevision('a1b2c3d4e5f6a7b8c9d0'), 'a1b2c3d4e5f6')
  assert.equal(shortRevision('short'), 'short')
  assert.equal(shortRevision(null), null)
})

test('a count of files reaches a person with its noun', () => {
  assert.equal(fileCount(1), '1 file')
  assert.equal(fileCount(2), '2 files')
  assert.equal(fileCount(0), '0 files')
})

test('the lines read as sentences a person can say back', () => {
  const lines = prepareLines(served().prepare)
  assert.deepEqual(lines.map((l) => l.text), [
    'The project folder: /Users/example/projects/retry-delivery',
    'Starting from: a1b2c3d4e5f6',
    'Your unsaved changes are included: 2 files.',
    'The app running on port 45781, started by serve.',
    'cargo will run. It is version cargo 1.97.1.',
    'ruff is not on this machine, so it will not run.',
  ])
})

test('a clean tree and an unreadable one say different things', () => {
  const clean = prepareLines(
    served({ lines: [{ slug: 'prepare_uncommitted_none', value: null, tool: null }] }).prepare,
  )
  assert.deepEqual(clean.map((l) => l.text), [
    'There are no unsaved changes: this is exactly what is committed.',
  ])
  const unreadable = prepareLines(
    served({ lines: [{ slug: 'prepare_uncommitted_unknown', value: null, tool: null }] }).prepare,
  )
  assert.deepEqual(unreadable.map((l) => l.text), [
    'We could not read whether you have unsaved changes.',
  ])
})

test('a port with nothing identifiable on it says so and names no owner', () => {
  const lines = prepareLines(
    served({
      app_program: null,
      lines: [{ slug: 'prepare_app_unknown', value: '45999', tool: null }],
    }).prepare,
  )
  assert.deepEqual(lines.map((l) => l.text), [
    'Nothing is running on port 45999 that we could identify.',
  ])
})

test('a slug with no sentence renders nothing rather than half of one', () => {
  assert.equal(lineSentence({ slug: 'prepare_invented' }, {}), null)
  const lines = prepareLines({ lines: [{ slug: 'prepare_invented', value: 'x' }] })
  assert.deepEqual(lines, [])
})

test('the plain lines never say snapshot id, tree hash, pid or worktree', () => {
  const text = prepareLines(served().prepare).map((l) => l.text).join(' ').toLowerCase()
  for (const word of ['snapshot', 'tree hash', 'pid', 'worktree', 'wsid', 'sha256']) {
    assert.ok(!text.includes(word), `the plain lines said ${word}: ${text}`)
  }
})

test('the technical layer says all of it, one click deeper', () => {
  const rows = technicalRows(served().prepare)
  const labels = rows.map((r) => r.label)
  assert.deepEqual(labels, [
    'snapshot_id',
    'source_identity',
    'base_revision',
    'worktree_path',
    'lockfiles_sha256',
    'config_sha256',
    'port',
    'port_owner_pid',
    'port_owner_exe',
    'database_path',
  ])
  assert.equal(rows.find((r) => r.label === 'port_owner_pid').value, '101')
  assert.equal(
    rows.find((r) => r.label === 'base_revision').value,
    'a1b2c3d4e5f6a7b8c9d0',
    'the technical layer shows the whole revision, not the short one',
  )
})

test('an examination with no running app carries no rows about one', () => {
  const doc = served().prepare
  doc.inputs.target = { kind: 'none' }
  const labels = technicalRows(doc).map((r) => r.label)
  assert.ok(!labels.includes('port_owner_exe'))
})

test('nothing written down is an answer, not an empty box', () => {
  const view = buildPrepareView({
    prepare: {
      inputs: null,
      lines: [{ slug: 'prepare_nothing_recorded', value: null, tool: null }],
      record_count: 0,
      tools: [],
    },
  })
  assert.equal(view.recorded, false)
  assert.deepEqual(view.technical, [])
  assert.match(view.lines[0].text, /^Nothing is written down yet/)
  assert.match(view.lines[0].text, /thruline change prepare/)
})

test('the note stops claiming nothing about databases once a database is on screen', () => {
  const withApp = served().prepare
  assert.match(isolationNote(withApp), /^Only what was actually written down is shown here\. The app and the database above/)
  const doc = served().prepare
  doc.inputs.target = { kind: 'none' }
  assert.match(isolationNote(doc), /Nothing about databases, ports, secrets or outside services is claimed\./)
  assert.equal(buildPrepareView({ prepare: withApp }).note, isolationNote(withApp))
})

test('a document that never arrived is not a claim about a version', () => {
  const view = buildPrepareView(null)
  assert.deepEqual(view, {
    recorded: false,
    lines: [],
    technical: [],
    records: null,
    note: isolationNote(null),
  })
})

test('how many records there are is said with its noun', () => {
  assert.equal(recordsLine({ record_count: 0 }), null)
  assert.equal(recordsLine({ record_count: 1 }), '1 record of what will be looked at.')
  assert.match(recordsLine({ record_count: 3 }), /^3 records/)
})

/* "Was anything destructive?" (census §3.08 row 6) reads one list, and only
 * one: the phrases `thruline explain` flags as a "destructive command"
 * (crates/explain/src/timeline.rs DESTRUCTIVE). This test reads that Rust
 * list and fails when the palette's copy of it differs, so the two cannot
 * drift apart. The match is the explain crate's: the command's text,
 * lower-cased, contains the phrase. */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { DESTRUCTIVE, destructivePhrase } from './live.js'

const RUST = new URL('../../../../../crates/explain/src/timeline.rs', import.meta.url)

test('the palette’s list is the explain crate’s list, phrase for phrase, in its order', () => {
  const src = readFileSync(RUST, 'utf8')
  const block = src.match(/const DESTRUCTIVE: &\[&str\] = &\[([\s\S]*?)\];/)
  assert.ok(block, 'crates/explain/src/timeline.rs still holds const DESTRUCTIVE')
  const rust = [...block[1].matchAll(/"((?:[^"\\]|\\.)*)"/g)].map((m) => m[1])
  assert.ok(rust.length >= 10, rust.join())
  assert.deepEqual([...DESTRUCTIVE], rust)
})

test('a command is on the list when its lower-cased text contains a phrase, as explain reads it', () => {
  assert.equal(destructivePhrase('rm -rf .pytest_cache && pytest -q'), 'rm -rf')
  assert.equal(destructivePhrase('git push --force origin main'), 'git push --force')
  assert.equal(destructivePhrase('psql -c "DROP TABLE users"'), 'drop table')
  assert.equal(destructivePhrase('chmod -R 777 /srv'), 'chmod -r 777')
  assert.equal(destructivePhrase('git push origin main'), null)
  assert.equal(destructivePhrase('rm build.log'), null, 'only what the list names: no guess beyond it')
  assert.equal(destructivePhrase(''), null)
  assert.equal(destructivePhrase(null), null)
})

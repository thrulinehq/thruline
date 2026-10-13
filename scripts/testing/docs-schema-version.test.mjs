// DOCS-SCHEMA-VERSION: the current Change-view inventory must describe the
// served shape. Historical version notes elsewhere in docs/ remain history.
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..')
const doc = readFileSync(join(root, 'docs/THRULINE.md'), 'utf8')
const source = readFileSync(join(root, 'crates/core/src/change.rs'), 'utf8')

test('DOCS-SCHEMA-VERSION: the inventory matches the served Change-view version', () => {
  const served = source.match(/^pub const CHANGE_VIEW_SCHEMA_VERSION: u32 = (\d+);$/m)
  const documented = doc.match(/^\| `crates\/core\/src\/change\.rs` \| `CHANGE_VIEW_SCHEMA_VERSION` \| `(\d+)` \|$/m)
  assert.ok(served, 'the source must name the served version')
  assert.ok(documented, 'the document must list the Change-view version')
  assert.equal(documented[1], served[1])
})

test('DOCS-SCHEMA-VERSION: the QUAL-F2-R2 note names every served Change-view key', () => {
  const shape = source.match(/pub struct ChangeView \{([\s\S]*?)\n\}/)
  const note = doc.match(/^The Change view serves these schema \d+ keys \(QUAL-F2-R2\): (.+) \(`code: crates\/core\/src\/change\.rs ChangeView`\)\.$/m)
  assert.ok(shape, 'the source must declare ChangeView')
  assert.ok(note, 'the document must name the QUAL-F2-R2 served keys')
  const servedKeys = [...shape[1].matchAll(/^\s*pub ([a-z_]+):/gm)].map((m) => m[1]).sort()
  const documentedKeys = [...note[1].matchAll(/`([a-z_]+)`/g)].map((m) => m[1]).sort()
  assert.ok(servedKeys.includes('retrospective'), 'QUAL-F2-R2 serves retrospective')
  assert.deepEqual(documentedKeys, servedKeys)
})

/* C20 (2026-10-07) renamed the row "Licence and trial" to "Licence and Pro
 * approval": there is no trial any more. The rule this test holds — adjacent
 * citations in the row are separated — is unchanged; only the row's name moved. */
test('DOCS-SCHEMA-VERSION: the Licence and Pro approval row separates adjacent citations', () => {
  const row = doc.match(/^\| Licence and Pro approval \|.*$/m)
  assert.ok(row, 'the capability table must include Licence and Pro approval')
  assert.doesNotMatch(row[0], /``/, 'adjacent backtick spans need a space')
})

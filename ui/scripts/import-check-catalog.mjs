import { readFileSync, writeFileSync } from 'node:fs'
import { createHash } from 'node:crypto'

const source = readFileSync(new URL('../../docs/reference/ThruLine-Check-Catalog.md', import.meta.url), 'utf8')
const families = [...source.matchAll(/^\| ([A-Z]+) \| (\d+) \| ([^|]+) \|$/gm)].map(([, id, count, name]) => ({ id, name: name.trim(), count: Number(count) }))
const fields = { 'Priority': 'priority', 'Method': 'method', 'Cost': 'cost', 'Gate': 'gate', 'Applies / trigger': 'trigger', 'Required evidence': 'evidence', 'Pass': 'passCriterion', 'Fail': 'failCriterion', 'Unknown / limits': 'limits', 'Validation pair': 'validation' }
const checks = [...source.matchAll(/^### ([A-Z]+-\d+) - (.+)\n([\s\S]*?)(?=^#{2,3} |$(?![\s\S]))/gm)].map(([, id, name, body]) => {
  const check = { id, family: id.split('-')[0], name }
  for (const [, label, text] of body.matchAll(/\*\*([^*]+):\*\*\s*([\s\S]*?)(?=\s*·\s*\*\*|\n\n\*\*|$(?![\s\S]))/gm)) {
    if (fields[label]) check[fields[label]] = text.trim()
  }
  for (const key of Object.values(fields)) if (!check[key]) throw new Error(`${id}: missing ${key}`)
  return check
})
if (families.length !== 15 || checks.length !== 134) throw new Error('Catalog census changed; review the source and importer')
for (const family of families) if (checks.filter(c => c.family === family.id).length !== family.count) throw new Error(`Wrong count for ${family.id}`)
const contract = source.split('## Common evidence contract\n')[1]?.split('## Family index\n')[0]?.trim()
const references = source.split('## Sources\n')[1]?.trim()
if (!contract || !references?.includes('[44]')) throw new Error('Common contract or references were lost')
const catalog = { version: '2026-09-08', source: 'ThruLine-Check-Catalog.md', sourceSha256: createHash('sha256').update(source).digest('hex'), families, checks, contract, references }
writeFileSync(new URL('../src/workspace/check-catalog.json', import.meta.url), JSON.stringify(catalog, null, 2) + '\n')
console.log(`Imported ${checks.length} checks, ${families.length} families, and 44 references`)

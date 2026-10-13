/* "Start from a template": what this project's .thruline/policy.yml holds for
 * templates of what counts as done, read through the one GET this screen adds
 * (`GET /api/hq/projects/<id>/done-templates`, docs/hq-next/API.md). READS
 * ONLY: picking a template adds its lines to this screen's draft as
 * proposals, and the file is never written.
 *
 * The read uses the project's base-branch policy. The sentence says which
 * case it is: no file, a file with no templates, or a file ThruLine could not
 * read. PURE: a served
 * document (or its read) in, words out. */

/** The one read: a project's templates for what counts as done. */
export const DONE_TEMPLATES = (project) => '/api/hq/projects/' + encodeURIComponent(project) + '/done-templates'

const FILE = '.thruline/policy.yml'
const list = (v) => (Array.isArray(v) ? v : [])

export const TEMPLATE_WORDS = Object.freeze({
  reading: 'Reading this project’s base-branch ' + FILE + '…',
  failed: 'ThruLine could not read this project’s base-branch policy file just now, so no template is offered. Reload this page to try again.',
  absent: 'This project’s base branch has no ' + FILE + ', so it has no templates for what counts as done. To add the file, run thruline policy --init in the project folder; it starts from the documented example, which holds no templates.',
  none: 'This project’s base-branch ' + FILE + ' holds no templates for what counts as done, so none can be picked here.',
  notRead: 'ThruLine could not read this project’s base-branch ' + FILE + ', so no template is offered. Check the policy file and base branch.',
  some: 'A template’s lines arrive as proposals, never accepted for you.',
})

/**
 * `read` is a `useResource` answer ({data, error, loading}) or null.
 * Returns `{templates: [{name, from, lines}], why, detail}`.
 */
export function templatesOf(read) {
  const doc = read && read.data
  if (!doc) {
    if (read && read.error) return { templates: [], why: TEMPLATE_WORDS.failed, detail: null }
    return { templates: [], why: TEMPLATE_WORDS.reading, detail: null }
  }
  const templates = list(doc.templates)
    .filter((t) => t && typeof t.name === 'string' && t.name.trim())
    .map((t) => ({ name: t.name.trim(), from: FILE, lines: list(t.lines).map((l) => String(l).trim()).filter(Boolean) }))
    .filter((t) => t.lines.length)
  if (templates.length) return { templates, why: TEMPLATE_WORDS.some, detail: null }
  const state = doc.policy && doc.policy.state
  const detail = doc.policy && typeof doc.policy.detail === 'string' && doc.policy.detail ? doc.policy.detail : null
  if (state === 'absent' || doc.reason === 'no_policy_file') return { templates, why: TEMPLATE_WORDS.absent, detail: null }
  if (state === 'read' || doc.reason === 'no_templates_section') return { templates, why: TEMPLATE_WORDS.none, detail: null }
  return { templates, why: TEMPLATE_WORDS.notRead, detail }
}

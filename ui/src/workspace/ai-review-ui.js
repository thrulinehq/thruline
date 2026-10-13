/* AI-REVIEW-OUT-OF-LAUNCH-R2 (founder rulings R-2026-09-28-20 and -37).
 *
 * Every PR-Agent word, id and label the screens know lives in this module and
 * in ai-review-facts.jsx, and nowhere else in ui/src. That is the whole
 * mechanism: `vite build` (ui/vite.config.js, plugin
 * `thruline-launch-without-ai-review`) resolves both modules to their
 * `.launch` twins, which carry none of it, and drops the PR-Agent row from
 * spec/integrations/catalog.json as the bundle imports it. So the committed
 * ui/dist — the launch build's embedded, unconditionally served assets —
 * holds no PR-Agent entry point, label, capability or catalog row, and the
 * build fails loud if one reaches an output chunk anyway.
 *
 * This full module is what the tests (esbuild, unmodified by that plugin)
 * and `npm run dev` load, so the round-1 gate — shown only when the server
 * says `ai_review: true` — is still exercised against the real words.
 *
 * The ai-review cargo build embeds the same committed ui/dist. Its screens
 * get the PR-Agent source row back from the server (`served_catalog()` keeps
 * it when the feature is on, and the screen prefers the served catalog), and
 * name its recipe by id; the titled labels below are not in the launch UI. */
export const AI_REVIEW_IN_UI = true
export const AI_REVIEW_PACKS = ['pr-agent']
export const isAiReviewPack = (id) => AI_REVIEW_PACKS.includes(id)
/* Agent-repair recipes that are AI review; added to the deterministic set. */
export const AI_REVIEW_AGENT_RECIPES = ['pr-agent']
export const AI_REVIEW_TITLES = { 'pr-agent': 'Review a change with PR-Agent' }
export const AI_REVIEW_NAMES = { 'pr-agent': 'PR-Agent' }
export const AI_REVIEW_SCOPE_NOTE = '; PR-Agent uses its configured committed diff'
export const AI_REVIEW_SUGGESTION_SUMMARY = 'Original PR-Agent replacement and source'

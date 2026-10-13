/* AI-REVIEW-OUT-OF-LAUNCH-R2 — the launch build's twin of ai-review-ui.js.
 * `vite build` resolves ai-review-ui.js to this file (ui/vite.config.js), so
 * the committed ui/dist carries no AI-review word. See ai-review-ui.js.
 *
 * Nothing here names the withheld capability. The launch screens show only
 * the sources this build's own catalog ships — the ids of the catalog the
 * plugin has already filtered, which is the catalog the launch server serves,
 * put here by the build as `__THRULINE_LAUNCH_SOURCE_IDS__` (a short list, so
 * the catalog itself stays in the lazy tool-evidence chunk) — so a reading
 * from any server that still lists an AI reviewer shows it nowhere, as round
 * 1 promised, without this file having to say its name. */
/* global __THRULINE_LAUNCH_SOURCE_IDS__ */
const SHIPPED = new Set(__THRULINE_LAUNCH_SOURCE_IDS__)
export const AI_REVIEW_IN_UI = false
export const AI_REVIEW_PACKS = []
export const isAiReviewPack = (id) => !SHIPPED.has(id)
export const AI_REVIEW_AGENT_RECIPES = []
export const AI_REVIEW_TITLES = {}
export const AI_REVIEW_NAMES = {}
export const AI_REVIEW_SCOPE_NOTE = ''
export const AI_REVIEW_SUGGESTION_SUMMARY = null

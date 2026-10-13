/* The state colours that change with a choice in Settings (C30 R-PREFS).
 *
 * PROVEN is drawn from ONE token. The standard green is the fallback written
 * into the value, so a screen with nothing around it (the design gallery, a
 * test) draws exactly the canvas's colour; "Proven is teal" (DESIGN-SPEC §1.11)
 * re-points the token for the whole app with one rule in appearance.css, on the
 * app root, and nothing else moves: shapes and glyphs stay as they were.
 *
 *   proven        the glyph, the word, the line
 *   provenA(a)    the same colour at an opacity: a border (.5-.6), a fill (.08-.28)
 *
 * No screen writes the green as a hex or as rgba(61,214,140,…) any more
 * (palette.test.js reads every source under hq/ to keep it so). Settings' own
 * preview pill shows BOTH variants by name, so it states them (STANDARD, TEAL).
 *
 * PURE: no window, no React.
 */

export const STANDARD = Object.freeze({ hex: '#3DD68C', rgb: '61, 214, 140' })
export const TEAL = Object.freeze({ hex: '#2DD4BF', rgb: '45, 212, 191' })

/** The token with its fallback: what a glyph, a word or a line is coloured with. */
export const PROVEN = 'var(--tl-proven, ' + STANDARD.hex + ')'

/** The token at an opacity (a number or a string like '.6'). */
export const provenA = (a) => 'rgba(var(--tl-proven-rgb, ' + STANDARD.rgb + '), ' + String(a).replace(/^0\./, '.') + ')'

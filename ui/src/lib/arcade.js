/* The cast and the cabinet, extracted from the approved design.
 *
 * Provenance: docs/design/ThruLine Arcade.dc.html (RECON PACK v2, RA3)
 *   sha256 6bcbdb2b18afd4253ac74842c00d07f3f8ff908366ac835ab7ba79ccc1a9eff0
 *   verified byte-identical to the live claude.ai/design project at import.
 *   The sprite maps, the two palettes, sprite() and CHOMP below are copied
 *   from its `pixel sprite engine` block verbatim. Mock fixture data from the
 *   same block is NOT copied — it does not cross into a product route (§5w,
 *   A83 ruling 2). The v1 NIB (a 14-wide wedge, two frames) is superseded by
 *   this file's 13×13 round-bodied chomper with a third, half-open frame.
 *
 * §5w: the skin may gamify the look, never the information. Every sprite here
 * is reachable only from a record. HOLLOW ships as an asset and renders on no
 * product path until the `unsupported` class graduates — asserted by
 * `the benched ghost has no product render path` in arcade.test.js. */

/* NIB, 13×13 — closed / half / open. Round-bodied chomper; the mouth is a
   pen nib's slit + vent, so the trail line threads visibly through the slit. */
export const NIB_A = [
  '....HHHHH....',
  '..HHCCCCCHH..',
  '.HCCCCCCCCCH.',
  '.HCCCCCCCCCC.',
  'HCCCCCCCCCCCC',
  'HCCCKKCCCCCCC',
  'HCCCKKKKKKKKK',
  'HCCCCCCCCCCCC',
  'HCCCCCCCCCCCC',
  '.HCCCCCCCCCC.',
  '.HCCCCCCCCCH.',
  '..HHCCCCCHH..',
  '....HHHHH....',
]

export const NIB_MID = [
  '....HHHHH....',
  '..HHCCCCCHH..',
  '.HCCCCCCCCCH.',
  '.HCCCCCCCCC..',
  'HCCCCCCCCCC..',
  'HCCCKKCCCC...',
  'HCCCKKKK.....',
  'HCCCCCCCCC...',
  'HCCCCCCCCCC..',
  '.HCCCCCCCCC..',
  '.HCCCCCCCCCH.',
  '..HHCCCCCHH..',
  '....HHHHH....',
]

export const NIB_B = [
  '....HHHHH....',
  '..HHCCCCCHH..',
  '.HCCCCCCCC...',
  '.HCCCCCCC....',
  'HCCCCCCCC....',
  'HCCCKKCC.....',
  'HCCCKK.......',
  'HCCCCCCC.....',
  'HCCCCCCCC....',
  '.HCCCCCCC....',
  '.HCCCCCCCC...',
  '..HHCCCCCHH..',
  '....HHHHH....',
]

/* The chomp cycle: mouth fully open exactly at phase 0 — pellet contact.
   Indices into [NIB_A, NIB_MID, NIB_B]. */
export const NIB_FRAMES = [NIB_A, NIB_MID, NIB_B]
export const CHOMP = [2, 1, 0, 1]

export const DUSK = [
  '.....VV.....',
  '....VVVV....',
  '...VVVVVV...',
  '.WVVKVVKVVW.',
  'WWVVVVVVVVWW',
  'WWWVVVVVVWWW',
  'WWWWVVVVWWWW',
  '.WWWWVVWWWW.',
  '..WWWVVWWW..',
  '...WW.VWW...',
  '....W.VW....',
  '......V.....',
]

export const SNAG = [
  '.....RR.....',
  '....RRRR....',
  '...RRRRRR...',
  '..RRRRRRRR..',
  '.RRKRRRRKRR.',
  'RRRKRRRRKRRR',
  'RRRRRRRRRRRR',
  'RRRRRRRRRRRR',
  'RRRRRRRRRRRR',
  'RRRRRRRRRRRR',
  '.RR.RRRR.RR.',
  '..R..RR..R..',
]

export const HOLLOW = [
  '...OOOOOO...',
  '..O......O..',
  '.O........O.',
  'O..........O',
  'O..O....O..O',
  'O..O....O..O',
  'O..........O',
  'O..........O',
  'O..........O',
  'O..........O',
  'O.O.O..O.O.O',
  '..O...O...O.',
]

export const COIN = [
  '..AAAA..',
  '.AYYYYA.',
  'AYYAAYYA',
  'AYAAAAYA',
  'AYAAAAYA',
  'AYYAAYYA',
  '.AYYYYA.',
  '..AAAA..',
]

/* Colour reinforces identity; it never carries it alone. GRAY is the same
 * geometry in a grayscale print — the judging rule from the art direction. */
export const PAL = {
  C: '#6fe0ff',
  H: '#b6f0ff',
  K: '#0a0c18',
  V: '#c58bff',
  W: '#c58bff55',
  R: '#ff5f4d',
  O: '#8a93b0',
  A: '#ffc24d',
  Y: '#8a5f10',
}
export const GRAY = {
  C: '#c9c9c9',
  H: '#f0f0f0',
  K: '#111111',
  V: '#a6a6a6',
  W: '#a6a6a655',
  R: '#e2e2e2',
  O: '#8a8a8a',
  A: '#d4d4d4',
  Y: '#4a4a4a',
}

/* A sprite is a single element whose box-shadow paints one rectangle per lit
 * pixel — chunky by construction, and it survives at any integer scale. */
export function sprite(rows, s, pal = PAL) {
  const out = []
  for (let y = 0; y < rows.length; y++) {
    for (let x = 0; x < rows[y].length; x++) {
      const c = rows[y][x]
      if (c !== '.' && pal[c]) out.push(`${x * s}px ${y * s}px 0 0 ${pal[c]}`)
    }
  }
  return { shadow: out.join(','), w: `${rows[0].length * s}px`, h: `${rows.length * s}px`, px: `${s}px` }
}

/* The cast. Glyph + word travel with every appearance so the state survives a
 * grayscale print and a screen reader alike (art direction §3, doctrine §"cast").
 * `bench: true` means designed, not appearing: no product path may reach it. */
export const CAST = {
  nib:    { name: 'NIB',    word: 'worker',      glyph: '\u25B8', frames: [NIB_A, NIB_B], bench: false },
  dusk:   { name: 'DUSK',   word: 'out of date', glyph: '\u25CC', frames: [DUSK],         bench: false },
  snag:   { name: 'SNAG',   word: 'contradicted',   glyph: '\u25C6', frames: [SNAG],   bench: false },
  hollow: { name: 'HOLLOW', word: 'no proof found', glyph: '\u25CB', frames: [HOLLOW], bench: true },
}

/* Which cast member an attention kind may show. `contradiction` is the only
 * kind that summons SNAG; nothing summons HOLLOW. A kind with no entry renders
 * no sprite at all — no sprite without a receipt. */
export const CAST_FOR_KIND = {
  contradiction: 'snag',
  staleness: 'dusk',
}

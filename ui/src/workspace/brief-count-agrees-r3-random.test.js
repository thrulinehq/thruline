/* BRIEF-COUNT-AGREES-R3 — a seeded random sweep over all nine outcomes (the
 * round-2 eight and a line proven on another version), n = 1..12 lines, the
 * identity confirmed and not, the review obligation in each of its five states.
 * The harness, and what it holds, is `brief-count-agrees-r3-harness.mjs`. */
import { runShard, random } from './brief-count-agrees-r3-harness.mjs'

runShard('the random sweep, over nine outcomes and five review states', random(500, 20260930), 0, 1)

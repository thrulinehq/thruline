/* BRIEF-COUNT-AGREES-R3 — shard 3 of 12 of the round-2 adversary's systematic
 * sweep (n = 1..12 lines, every ordered pair of six outcomes, the identity
 * confirmed and not) with the review obligation in each of its five states. The
 * harness, and what it holds, is `brief-count-agrees-r3-harness.mjs`. */
import { runShard, systematic } from './brief-count-agrees-r3-harness.mjs'

runShard('the systematic sweep, with the review in each of its five states', systematic(), 2, 12)

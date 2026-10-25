# ThruLine v0.3 build log

ThruLine checks what a coding agent claims against what actually ran. It reads the session logs Claude Code and Codex already write, keeps an append-only, hash-chained record on your machine, and judges each claim with deterministic rules, with no LLM in the verdict. v0.2 is public in this repository (FSL-1.1-MIT).

v0.3 is being built in a private branch. This log is where we share, every day, what changed, what broke and what we measured, including results that don't flatter us.

How we write it:
- Every number comes from a file we can re-run, and the method sits next to it.
- No accuracy claim we haven't measured.
- Nothing private: no session text, no keys, no personal paths.

| Day | Date | Headline |
| --- | --- | --- |
| 1 | 19 Oct 2026 | [Ground truth first: 357 agent "tests pass" claims, labelled](2026-10-19.md) |
| 2 | 20 Oct 2026 | [The pass was real. It just wasn't about the code anymore.](2026-10-20.md) |
| 3 | 21 Oct 2026 | [From 0 of 9 to 3 of 9, and why the other 6 still slip](2026-10-21.md) |
| 4 | 22 Oct 2026 | [Receipts went from 0 of 9 to 8 of 9, and the gate nearly accused 5 honest sessions](2026-10-22.md) |
| 5 | 23 Oct 2026 | [The release check failed 6 of 10,406 tests, and two were a pipe hiding a failure](2026-10-23.md) |
| 6 | 24 Oct 2026 | [Our own warning would have blocked 2 honest coding loops, and the release candidate is green](2026-10-24.md) |
| 7 | 25 Oct 2026 | [9 of 9 lies caught while the agent was still working, 0 false alarms, and the release candidate is done](2026-10-25.md) |

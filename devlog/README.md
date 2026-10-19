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

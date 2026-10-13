# Codex

Codex has a hook pack, and it is installed by a different command than Claude
Code's — `thruline init` is Claude Code only.

```sh
thruline hook-install --harness codex
thruline shim install --harness codex
```

`hook-install` **refuses rather than appends** when ThruLine hooks are already
present. Two recorders writing two records of one session is worse than none,
because nothing afterwards says which record a claim was checked against.
`--undo` removes exactly what it wrote and leaves other packs alone.

Pass `--harness codex` to `shim install` as well. One session's hook lines and
its execs must share one spool file, or the fold's resume guard skips the
second — and you get a chain with a `Stop` event and none of the commands it is
supposed to bound.

## Two things Codex does differently

**Its `PostToolUse` fires on failure and carries no outcome.** Claude Code has a
separate failure event; Gemini puts the error in the tool response. Codex has
neither, so a Codex tool result cannot be read for success on its own.

**Its `SessionEnd` timeout is 1 second**, capped at 3 — tighter than a cold
binary start. The hook pack writes an explicit timeout for that event rather
than inheriting the default.

## Importing history you already have

Codex writes rollout files. ThruLine can read them:

```sh
thruline import-codex
```

**Every imported session is marked `capture_degraded`, and no claim in one can
reach Proven.** A rollout is the harness's own log of what it believes happened,
not evidence that it happened — so imported history is there for
`thruline explain` and for continuity, and it is never treated as proof. This is
not a limitation to be worked around; it is the distinction the whole product
rests on.

Two record types survive the import faithfully: `patch_apply_end` becomes a file
change, and `context_compacted` becomes a compaction boundary. A patch that
*failed* to apply becomes a note, not a file change — because it did not change
a file.

Anything the importer does not recognise is counted and named in its report
rather than dropped.

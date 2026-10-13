# Claude Code

Claude Code is the only harness `thruline init` sets up, and the only one with a
first-class install path.

```sh
thruline init
```

It asks one question — consent for the speaker hooks, which put receipt-carrying
lines into your session — and prints exactly which files it touched and which
entries it wrote. Nothing else changes.

## What that writes

Two files. In `~/.claude/settings.json`, the sensor hooks: `SessionStart`,
`UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `PreCompact`, `Stop`,
`SessionEnd`, and `StopFailure` — registered dormant, never observed on this
machine, recorded if it ever arrives. Each one runs `thruline sensor append
<Event>`, which appends one hash-chained row to the ledger at
`~/.thruline/ledger.db` and exits 0 with nothing on stdout; the entries carry a
5-second timeout. The spool `thruline fold` reads is a different file, written
by the shim below and by `thruline hook-install`'s pack — not by these.

In `~/.claude.json`, one `mcpServers.thruline` entry: the context server,
`thruline mcp --agent claude --db <ledger>`.

Point the settings file somewhere else with `--settings` and the config file
with `--claude-json`, and reverse everything with `thruline init --undo`.

## The other half

```sh
thruline shim install
export PATH="<the directory it prints>:$PATH"
```

The shim's candidates are `pytest`, `cargo`, `go`, `npm`, `pnpm`, `yarn`, `npx`,
`make`, `mvn`, `gradle`, `dotnet`, `uv`, `uvx`, `python` and `python3`, and by
default a wrapper is written only for the names that already resolve to a real
program on your `PATH` — `shim install` prints how many wrappers it wrote and
where, and names every candidate it skipped and why. What it wrote into that
directory, not this list, is what is being captured. For a wrapped command, the
command line, exit code, output digests and a tree snapshot on both sides of a
real check are captured as they happen — rather than reconstructed from what
the agent said about them afterwards.

The hooks say what the agent did. The shim says what actually ran. A claim is
decided from the second.

## Then

```sh
thruline fold
thruline claims
```

## The stop gate, if you want it

```sh
thruline init --gate
```

On `Stop`, ThruLine extracts the turn's completion claims and blocks the stop
when one is `unsupported` — the word the binary prints for a claim that was
made and for which no proof was found in the record — with a reason naming the
claim and the command that would settle it.

An `unverifiable` claim — one the record does not hold enough of to decide
either way — never blocks. Blocking there would accuse the agent exactly where
ThruLine could not observe enough to decide, which is the verdict model
inverted. The gate also allows the stop after 5 seconds whatever it is doing,
and blocks at most twice per session: a gate that hangs is worse than one that
abstains.

## For an organisation

```sh
thruline hook-install --managed-settings-snippet
```

prints the enterprise managed-settings block and writes nothing, so hooks can be
enforced where a user cannot remove them.

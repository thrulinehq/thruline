# Any agent, under `thruline run`

Not every agent has hooks. For those:

```sh
thruline run -- <your agent command>
```

No hooks, no vendor contract, no settings file. The agent runs under a
pseudo-terminal, so it behaves exactly as it does in your terminal — colours,
progress bars, interactive prompts — while ThruLine records the session around
it. The PTY is not cosmetic: an agent that believes it is not on a terminal
behaves differently, and recording a different agent from the one you run is not
a recording.

`thruline run` exits with the child's own status, unchanged. Add `--no-pty` to
give the child pipes anyway, and `--report <path>` to write the JSON report to a
file — the command is a pass-through, so the child owns stdout and stderr and a
report printed to either would share the stream with the agent's own output.

Then:

```sh
thruline fold
thruline claims
```

## One honest wrinkle: TTY versus pipe

A program's output differs depending on whether it is talking to a terminal.
When ThruLine re-executes a check later to confirm it, that re-run is not on a
terminal, so the bytes will not match even though nothing was tampered with.

ThruLine records the difference explicitly — `stdio: "tty"` versus
`stdio: "pipe"` — and **does not compare output across it**. Exit codes are
compared; output is not. A verifier that reported a mismatch there would be
reporting tampering where there is none.

## Cursor, Windsurf, Copilot CLI, OpenCode

No first-class adapters, and none is claimed. This is the path for all of them.
When a stable hook API exists for one, it gets a page like Claude Code's and a
row in the capability table — not before.

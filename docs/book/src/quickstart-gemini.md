# Gemini CLI

**The hook pack is not installable yet.** `thruline hook-install --harness
gemini` refuses, in as many words:

```text
no hook pack for `gemini` yet: its event contract is mapped, but the
installer cannot write its registration file
```

That refusal is the honest state, and this page exists to say so rather than to
let you find out by running it.

## What is built

The event contract is mapped and tested against Gemini's documentation: which
events exist, what they are called, which ThruLine event kind each becomes, and
where Gemini differs from the other two harnesses.

| | Gemini |
|---|---|
| tool-post event | `AfterTool`, with failure in `tool_response.error` |
| stop analogue | `AfterAgent` — Gemini has **no** `Stop` event |
| compaction | `PreCompress` only; there is no post-compaction event |
| hook timeout unit | **milliseconds**, where the others are seconds |

Each of those rows is a way to be silently wrong, and each is checked against
the vendor's documentation rather than assumed.

## What is missing

The registration file. Nothing writes ThruLine's hooks into
`.gemini/settings.json`, so nothing fires and nothing is captured.

*What would prove it:* a hook pack that writes the registration, a session
captured through it, and a claim in that session reaching a verdict from
evidence the hooks delivered.

## What to do meanwhile

Use the harness-agnostic path:

```sh
thruline run -- gemini
```

See [Any agent, under `thruline run`](./quickstart-run.md). It needs no hooks
and no vendor contract, and it records what the agent executes.

Where capture is degraded for any reason — a hook that could not fire, a field
that was not delivered — the session is marked and every claim in it is capped
at **Couldn't check**. Silence about a gap is the one thing an evidence system
must never do.

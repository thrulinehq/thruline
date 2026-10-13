# Connecting your coding agent to ThruLine

This guide describes only what this version of ThruLine does. Where something is planned and not built, it says so in these exact words: arrives in a later version. It is checked by `ui/src/workspace/integration-guide.test.js`: every command, screen address and agent tool named here is one the product has, and a sentence about something not built fails that test unless it says it arrives in a later version.

ThruLine reads the records your coding agent already writes while you work: Claude Code and Codex. It keeps what it reads in one folder on your machine. Nothing leaves the machine.

## Start here

In the project you are working in, run:

```
thruline init
```

Then open ThruLine:

```
thruline ui
```

`thruline ui` opens ThruLine in your browser. It runs on your machine only, and Ctrl+C stops it. The first screen asks you to pick the project you are working on. ThruLine then proposes a Change from the work it can already see, and writes nothing down until you say so.

If the first screen says this machine has nothing recorded yet, `thruline init` is the command that changes that: work with your agent the way you normally do, and the next session appears.

## What `thruline init` installs

`thruline init` writes the capture hooks and the tool server entry below without asking, asks before installing anything optional, prints every file it touched, and `thruline init --undo` takes out exactly what it put in. What it recorded stays recorded.

It installs three things into Claude Code's own configuration.

**The capture hooks.** While Claude Code works, these write down, word for word and as it happens, each session start, each request you type, each tool the agent uses, each stop, and each time the conversation is compacted. They only write down; they never talk to the agent. For Codex, ThruLine reads its records when they are brought in (see "Bringing in older work" below); live capture for Codex arrives in a later version.

**Three short hooks that speak, behind one question.** `thruline init` asks once before installing these. A plain Enter says yes, and `thruline init --yes` answers yes without asking.

- When a session starts, `thruline brief --hook session-start` puts at most three lines into the new session, each one pointing at where it came from in the record.
- When the agent stops, `thruline audit --hook stop` looks for a claim that the work is finished which the record holds no proof of, and writes it down in these words: claimed, and no proof found in the record.
- When a session ends, `thruline digest precompute --hook session-end` prepares those lines for the next session.

None of the three can hold your agent up. If anything goes wrong they stay silent, and each gives up on its own after a few seconds.

`thruline init --gate` also installs a stop gate. It blocks the agent from stopping when its claim that the work is finished has no proof in the record. It is off unless you ask for it.

**A tool server for the agent.** `thruline init` adds ThruLine to Claude Code's list of tool servers, so the agent can call ThruLine directly. The server is `thruline mcp --agent claude`, and it runs only when Claude Code starts it. It offers the agent three tools, and the agent's own tool list shows them:

- `claim_completion` — the agent says it finished something, and names what would prove it. ThruLine writes the claim down beside the claiming session's own record. `thruline verify-claims` later checks each one against that record and writes one answer per claim; "no proof found" means the record it searched held no match, never that the claim is false.

## What the agent can ask for while it is working

The tool server offers the agent three tools. `claim_completion`, above, is the one it uses when it thinks it is finished. The other two are how it finds out where the work stands before it starts, and whether what it was told still holds before it claims anything.

- `get_context_packet` — the agent asks for a written summary of where the work stands in this project, and says in one line what it has been asked to do. The summary comes back in fixed sections, always the same sections in the same order: what the task is, what is not in scope, where the work currently stands, what the record backs, what it does not back, what is stale or contradicts something else, what has already been tried and did not work, what a person still has to decide, what would have to be true for the work to count as done, and where to pick up. Every line in it says where it came from and why it is there. It comes back with a chain position as well: one long hexadecimal word naming exactly the state of the record the summary was written from.
- `check_context` — the agent hands that chain position back and asks whether it is still the current one. Current means the summary it is holding still describes the record. Not current means something has been written down since, and the agent should ask for the summary again.

Used together, they are four steps:

1. Ask for the summary once, at the start, saying what the task is.
2. Do the work.
3. Before claiming anything is finished, ask whether the chain position it started from is still current.
4. If the answer is not current, it comes back in one of two ways. Either it hands back the lines that have changed since — read those, and it says in as many words that the whole summary need not be asked for again — or it says no summary was ever handed out at that point, in which case ask for a fresh one and read it before claiming anything.

Both tools read what ThruLine has recorded on this machine, in the folder ThruLine was pointed at, and neither one changes a word of it. Each writes down that the agent asked, so that what the agent was handed can be read back afterwards. Nothing leaves the machine.

**No section is ever left blank without saying why.** On a machine where ThruLine has only just started recording, most sections will carry one of two sentences instead of content, and they do not mean the same thing:

- `the source was queried and is empty` — ThruLine looked in the place that would hold this, and that place held nothing. It also says what it measured, for example that there is no recorded home folder for any project in scope yet.
- `no source object exists` — there is nothing in this version that could have filled the section at all, for example that nobody is holding a piece of work, because this version has no way for anyone to hold one.

An empty section is never an answer about the work. "Nothing is recorded here" and "there is nothing wrong here" are different sentences, and ThruLine only ever writes the first.

The summary is written fresh each time from what has been recorded, and asking for it twice at the same chain position gives back the same words.

## The words ThruLine uses about one piece of evidence

ThruLine uses these words about one piece of evidence. They describe what one piece of evidence showed. None of them answers "is this done?".

- **Proven** — the record backs it.
- **Not proven** — the record does not back it.
- **Recorded** — something ran, and it settles nothing.
- **Loose thread** — promised, and the record shows no follow-through.
- **Fog** — a gap in the recording.

## Changes: the work you are asked to accept

A Change is one piece of work you are asked to accept, judged against what counts as done. The screen is where you read one, and approve one version of it. These commands do the same things from a terminal.

Open a Change:

```
thruline change new --title "what the work is" --requester "your name"
```

See what ThruLine suggests should count as done, then accept it. Accepting what counts as done is not approving the work.

```
thruline change propose <change>
thruline change accept <change> --all --actor "your name"
```

Link the work that was done to a Change. There are two ways. Run a check for it: put the command you would normally run after the two dashes, and what it does, and how it ends, is recorded against that Change.

```
thruline check --for-change <change> -- <the command you would run>
```

Or link a session your agent already recorded. `thruline change suggest` lists the recorded work that might belong to the Change; nothing it lists counts until a person confirms it.

```
thruline change suggest <change>
thruline change associate <change> <work> --confirm --actor "your name"
```

Write down exactly what will be checked for a Change (the project folder and where it starts from):

```
thruline change prepare <change>
```

See what the linked work shows for a Change:

```
thruline change results <change>
```

Stop a check that is still running:

```
thruline change cancel-run --run <run>
```

Approving a version is done on the screen, with your name on it. It does not merge, deploy or tell the agent anything.

Asking questions of the record from the screen arrives in a later version. Asking for a review from the screen arrives in a later version; until then, ask the person you would have asked, and link what they send back.

## Bringing in older work, and checking the record

Bring in sessions that were recorded before `thruline init` was installed. Without `--apply`, it only shows what it would read and how much room it needs, and writes nothing:

```
thruline ingest <path>
thruline ingest --apply <path>
```

Check that nothing in the record has changed since it was written, by reading every file again:

```
thruline verify
```

See what the recorder holds on this machine:

```
thruline doctor
```

After you install a new version of ThruLine, it may ask you to run one command before anything else. Until you do, every other command stops and says so, having changed nothing:

```
thruline upgrade
```

## Not in this version

- Handing a fresh session what the last one knew, at the moment it starts, arrives in a later version.
- Capturing Codex live, as it works, arrives in a later version.
- Asking questions of the record from the screen arrives in a later version.
- Coordinating two agents working on the same project at the same time arrives in a later version.

## Technical: the addresses the screen reads and writes, and what the agent's two state tools return

### The addresses the screen reads and writes

`thruline ui` serves the screen from `127.0.0.1` only. The Change screen and its sheets read these addresses for Change-related work:

- `GET /api/changes` — the list of open Changes.
- `GET /api/changes/<id>` — one Change.
- `GET /api/changes/<id>/proposals` — what ThruLine suggests should count as done.
- `GET /api/changes/<id>/coverage` — what the linked work shows for each accepted line.
- `GET /api/changes/<id>/applicability` — whether each piece of linked evidence still applies to the version on screen.
- `GET /api/changes/<id>/inputs` — what exactly will be examined.
- `GET /api/explain` and `GET /api/claims` — the story of one recorded session, and what it claimed; the Change screen reads these only for work that was confirmed as part of the Change.
- `GET /api/run/token` — the run sheet's token for starting a check.
- `GET /api/changes/<id>/run` — what the run sheet can start and the latest run outcome.
- `GET /api/changes/<id>/findings` — recorded review findings and where they stand.
- `GET /api/changes/<id>/reviews/<review>/raw` — a masked inspection copy of a recorded review's report.
- `GET /api/changes/<id>/reviews/<review>/parsed` — a masked inspection copy of that review's parsed record.
- `GET /api/changes/<id>/export` — whether the records behind the saved Change file can be read now.
- `GET /api/licence` — whether this machine has a licence that works for Approve.

It writes to these Change addresses, each only on a click that says what it writes:

- `POST /api/changes/<id>/decisions` — approve exactly one version.
- `POST /api/changes/<id>/decisions/withdraw` — take that approval back.
- `POST /api/changes/<id>/runs/<run>/cancel` — stop one running check.
- `POST /api/changes/<id>/links` — add or remove one link between a line of what counts as done and one piece of evidence. Removing a link never deletes it; the removed link stays on the screen under its own heading.
- `POST /api/changes/<id>/run/permission` — record a person's permission to run checks for this project.
- `POST /api/changes/<id>/run/permission/revoke` — record that permission being taken back.
- `POST /api/changes/<id>/run` — start an allowed check from the Change's accepted check profile.
- `POST /api/changes/<id>/findings/<finding>/standing` — record where one review finding stands.

The first screen, where you pick a project, reads `GET /api/first-use` and `GET /api/first-use/proposal?project=<id>`, and writes, one click each, `POST /api/first-use/register`, `POST /api/first-use/change` and, only if you accept what counts as done there, `POST /api/changes/<id>/scope/accept`.

The page a Change's "The whole story" door opens is called the packet in the engine's own vocabulary. It is the same record, read top to bottom. It is a view, not a file ThruLine writes: its own page offers to copy it as text.

### The two state tools: what they read, and what they return

Both tools are served by `thruline mcp --agent claude` over stdio. Both read the ledger at the configured `--db` and derive from it; neither mutates what it read. Each records that it was called — the call, its scope, its request hash and the chain position it answered with — so that what an agent was served can be read back later. There is no network call in either path.

**`get_context_packet`** compiles the Context Packet for a scope. Arguments: `project` or `repo` (a repo path, or a repo id from `identity groups` — it spans every working root of that repo, across vendors, and an id nothing carries is an error, not an empty answer), `task`, `budget_tokens`, and `handoff_of`. Its own description states that the returned chain position **is** the snapshot id for the other two tools, and that `handoff_of` with a frozen issue id returns that issue's retained content verbatim as `handoff` and records this packet as its successor.

Observed against this repository on 2026-09-23 with `repo: <repo path>`, a one-line `task` and `budget_tokens: 1200`:

```json
{
  "chain_position": "<64 hex characters>",
  "continues_from": null,
  "excluded": 0,
  "handoff": null,
  "tokens": 119,
  "packet_md": "## TASK\n\n- …"
}
```

`packet_md` is Markdown. Its headings are fixed, and are always all present in this order — fourteen from the section order, plus `EXCLUSIONS`, which `render` appends:

```
## TASK
## NON-GOALS
## CURRENT STATE
## WORKSPACE STATE
## OWNERSHIP
## OTHER ACTIVE WORK
## VERIFIED AND CURRENT
## SUPPORTED, NOT VERIFIED
## STALE OR CONFLICTING
## FAILED APPROACHES
## OPEN HUMAN DECISIONS
## REQUIRED COMPLETION EVIDENCE
## RESUME
## SNAPSHOT BINDING
## EXCLUSIONS
```

Every line carries a bracketed provenance tail, `[<origin> · evidence=… · currency=… · applicability=… · because: …]`. A caller-supplied line is rendered as such and never as a fact the engine established — the `TASK` line reads `[caller · evidence=n/a · currency=n/a · applicability=unknown · because: supplied by the caller…]`. `SNAPSHOT BINDING` carries the chain position and, as its reason, `recompiling at this chain position reproduces this packet byte for byte`.

A section with no lines renders one of two sentences, and they are not interchangeable:

```
(nothing — the source was queried and is empty: <what was measured>)
(nothing — no source object exists: <why no object could exist>)
```

On the run above — a store with no identity backfill and no leases — four of them read:

```
## WORKSPACE STATE
(nothing — the source was queried and is empty: no workspace_identities row for
any project in scope; run `thruline identity backfill`)

## OWNERSHIP
(nothing — no source object exists: no lease or per-agent ownership object
exists; owner_key is a single constant)

## OTHER ACTIVE WORK
(nothing — no source object exists: no lease object exists; parallel-work
coordination is M8)

## VERIFIED AND CURRENT
(nothing — the source was queried and is empty: 0 of 0 claim(s) are verified and
current. This section is NOT filled from `supported`)
```

The fourth is the one to read twice: a verified-and-current section is never filled from what is merely supported, so an empty one means no claim has been verified — not that nothing was claimed, and never that everything passed.

**`check_context`** takes `chain_position` (required) and the same `project`/`repo`, and answers whether that position is still the current one. It computes the binding only, not a full compile. Observed with the chain position returned above:

```json
{
  "current": true,
  "deltas": [],
  "latest_chain_position": "<the same 64 hex characters>",
  "note": "the packet you hold is still bound to the current chain position",
  "status": "current"
}
```

There are three answers, and `deltas` is a different thing in each. All three carry `status`, `current` and `latest_chain_position`.

- `status: "current"` — the position is the latest. `deltas` is `[]`, and the note is the one above.
- `status: "changed"` — the position is stale, and a packet was served at it, so the difference can be computed. `deltas` is the list of lines that differ, from `packet::diff_lines` by way of `delta_json`: each entry carries `change` (`added`, `removed` or `axis-changed`), `section`, `text`, `origin`, `evidence`, `currency`, `applicability` and `reason`, and an `axis-changed` entry also carries `was` with the previous `evidence`, `currency` and `applicability`. The answer also carries `served_by` (`project` and `path` per project), and its note reads: `these are the lines that differ from the packet you were served; you do not need to refetch the whole packet`.
- `status: "unknown_position"` — no packet was served at that position, so there is nothing to diff against. `deltas` is `null` — not an empty list, because empty would read as "nothing changed", and the note reads: `no packet was served at that position, so the change set cannot be computed; refetch the packet`.

So an agent that gets a not-current answer has two different moves. On `changed`, read `deltas` and act on the lines that moved; the answer says in as many words that the whole packet need not be fetched again. On `unknown_position`, `deltas` tells you nothing and `get_context_packet` must be called again before anything is claimed.

`get_context_packet`'s own description says "Deltas are not in v1 — a not-current answer means refetch the packet." That sentence is on `check_context` in `crates/core/src/mcp.rs`, and the `Changed` arm four lines below it returns a computed list. Where the two disagree, the code is what runs; this guide describes the code.

**What is not here, read from the compiler rather than from a description string.** There is no lease or per-agent ownership object, so `OWNERSHIP` and `OTHER ACTIVE WORK` can only ever be empty, and each says so in its own words; coordinating two agents at once arrives in a later version. There is no failed-approach object either, so `FAILED APPROACHES` is filled only from what the record already shows. `deltas` is **not** in that list: it is computed and returned whenever the position is stale and a packet was served at it.

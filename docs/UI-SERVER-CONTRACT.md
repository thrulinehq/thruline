# The `thruline ui` server contract — v1.1

**Audience: Track A** (the web UI and HTML report), and anyone else pinning fixtures against this
surface. **Authority:** subordinate to `docs/BUILD-SPEC.md`. Where this document and the spec
disagree, the spec wins; where this document and the code disagree, **the code is the defect** —
every behaviour below is held by an acceptance test in
`crates/cli/tests/ui_server_contract.rs`, run against the shipping binary.

**Status of the numbers in here:** the endpoint set, the refusals and the shapes were measured by
running `thruline ui` on 2026-09-05, not transcribed from an earlier document. The one thing this
contract **cannot** promise is stated in *Versioning* below, and it is stated rather than papered
over.

---

## 0. What `thruline ui` was before this contract

Recorded so the change is legible.

`thruline ui [--port] [--no-open] [--refresh] [--dir] [--diagnostics]` bound `127.0.0.1:7777`
(falling back to any free port when 7777 was taken and `--port` was not given), exported the ledger
into `~/.thruline/export` if no export was there, printed the URL, opened a browser, and served:

| what | from |
|---|---|
| the app | **only** the `ui/dist` build embedded in the binary at compile time (`crates/cli/build.rs`) |
| `GET /fixtures/…json` | the export directory |
| `GET /api/state`, `POST /api/refresh`, `GET /api/lanes` | in-process |
| `GET /api/mind/status`, `POST /api/mind/interpret` | the local Ollama model |

**There was no way for another team to supply a UI**, and **there was no JSON API over the
`explain` / `claims` shapes** — the app's only project data was the export fixtures. Both of those
are what this contract adds. Everything else above is unchanged and still true.

---

## 1. The static directory

```
thruline ui --static-dir <DIR>
```

**What Track A supplies:** a built directory whose root contains `index.html`. Any layout beneath
that; assets may be nested arbitrarily. Nothing is required to be named `assets/`.

**Left off, the embedded UI is served** — today's behaviour, unchanged, and the default.

### The refusals — loud, and before anything binds

`--static-dir` is validated **before the socket is created, before the export runs, and before a
single line is printed.** Every failure exits **1** with the path named on stderr:

| condition | message contains | exit |
|---|---|---|
| the path does not exist | `no static directory at <path>` | 1 |
| the path is a file | `<path> is not a directory` | 1 |
| the directory has no `index.html` | `<path> has no index.html` | 1 |

**There is deliberately no fallback to the embedded build.** A silent fallback would show a
working UI serving the wrong bytes, which is worse than not starting. And a missing directory is
never answered with an empty page: *a UI that renders nothing is indistinguishable from a UI
reporting no findings*, and this product exists to stop absence of a signal reading as absence of a
problem.

### How it is served

- `GET /` → `index.html`.
- `GET /<path>` resolving to a file inside the root → that file, with a content type from its
  extension (`html js mjs css json map svg png jpg jpeg webp ico woff2 woff ttf txt wasm`,
  otherwise `application/octet-stream`).
- **A path with no file extension is a client route and gets `index.html`** (200) — this is a
  single-page app host.
- **A named file (anything with a `.` in its last segment) that is absent is a 404, never the
  shell.** Answering a missing bundle with HTML is precisely how a blank page happens.
- **Escapes are refused with 403.** `..`, `%2e%2e`, backslashes, and empty segments are rejected on
  the *decoded* path; the resolved path is then canonicalized and must still be under the root, so
  a symlink out of the directory is refused too.
- Every response carries `Cache-Control: no-store`. Do not rely on browser caching; the server is
  ephemeral.

**Not promised:** directory listings (there are none), `index.html` in subdirectories, range
requests, compression, ETags, HTTP/2. All are absent today and none is planned.

---

## 2. The JSON API

### 2.1 The projections

| endpoint | is exactly | selects |
|---|---|---|
| `GET /api/explain` | the stdout of `thruline explain --json` | the last session |
| `GET /api/explain?session=<id>` | the stdout of `thruline explain --json --session <id>` | that session |
| `GET /api/claims` | the stdout of `thruline claims --json` | the last session |
| `GET /api/claims?session=<id>` | the stdout of `thruline claims --json --session <id>` | that session |
| `GET /api/phases` / `?session=<id>` | the **stderr** phase lines of `thruline claims --json --phases [--last \| --session <id>]`, wrapped (see §2.1.1) | the last session / that session |
| `GET /api/sessions` | the stdout of `thruline sessions --per-project --json` | ranked sessions from every recorded project, bounded per project |

The `thruline.sessions` envelope is **schema version 2**. It adds `limit_scope`:
`"store"` means `limit` bounds the entire returned list; `"project"` means it bounds
each recorded project's rows, so `returned` can exceed `limit`. `total` remains
the whole-store session count. HQ uses the project scope. Consumers pinning
version 1 must accept this explicit version change before relying on the new field;
the per-session row fields are unchanged.

**"the last session" is the lexicographically last session id**, not the most recent one:
`sessions.started_at` and `ended_at` are NULL on every row of a real store, so there is no recency
for that default to use. On the operator's 44,107-session store it lands on a two-event session and
the app opens on nothing. **That default is unchanged by this version** — `/api/sessions` adds a way
to *choose*, and a consumer that wants a specific session passes `?session=<id>`, which is what the
list exists to supply.

**"Exactly" means byte-for-byte.** These endpoints re-enter the same `thruline` binary and serve
what it writes to stdout. Nothing is added to the body — no wrapper, no timestamp, no
`schema_version` the CLI does not itself emit. This is the design, not an implementation detail:
*a second representation of the same fact is a second thing to keep in sync*, and the only way two
shapes can be guaranteed never to diverge is for there to be one shape. The acceptance test asserts
byte equality against the CLI run in the same environment.

Consequences you can rely on:

- When the CLI's shape changes, the API's changes with it, in the same release, with no
  coordination.
- When the CLI's shape gains a field, the API carries it without a server change.
- The API can never claim a field the CLI does not emit.

**Method:** `GET` only. Anything else is `405`.

#### 2.1.1 `/api/phases` — the verdict pipeline's phase log

`/api/phases` is the one projection that is **not** a stdout body. It runs the same
`thruline claims --json` the `/api/claims` projection runs, with `--phases` added, and serves
what that flag writes to **stderr**: one JSON object per pipeline phase, emitted when that
phase's real work completes, carrying the pipeline's own count and its measured wall time.
Stdout is not served here (only its `session` id is read, so a `--last` request can name the
session it reports on); `/api/claims` remains the byte-for-byte home of that body, and adding
`--phases` changes none of its bytes.

The body:

```json
{"contract":"thruline.phases.v1","session":"<id>","phases":[
  {"count":3,"detail":"session <id> · 3 events","elapsed_ms":1,"phase":"read_record"},
  {"count":1,"detail":"clm_<id>","elapsed_ms":0,"phase":"find_claims"},
  {"count":0,"detail":"no check cited","elapsed_ms":0,"phase":"match_proof"},
  {"count":0,"detail":"0 edits · 0 trees","elapsed_ms":0,"phase":"workspace_since"},
  {"count":1,"detail":"tests_pass@1","elapsed_ms":0,"phase":"run_verifiers"},
  {"count":0,"detail":"0 findings","elapsed_ms":0,"phase":"write_up"}
]}
```

(That is the shape of a real run over the contract test's three-event fixture session, with the
session and claim ids shown as placeholders; key order within an object is serializer order and
not contractual.)

- The six phases arrive **in that order, once each**, on every exit-0/exit-3 run. `count` is,
  respectively: events read, claims extracted, checks cited by the session, snapshot trees seen,
  claims judged, and findings across all claims. `detail` is a short human line built from those
  same numbers (`find_claims` names each extracted claim by its `claims[].id`, joined with ` · `,
  or says `no completion claim in this session`; `workspace_since` appends
  `· changed after the last check: <n> path(s)` when a cited check's input tree differs from the
  final tree).
- **A `detail` carries ids and counts, never recorded text.** The phase objects are served
  verbatim, outside the masking applied to `unparsed` and to a 502's `stderr`, so no claim text
  and no path is placed in them: claim text is read from `/api/claims`, which **masks it on the
  way out** (see §2.2), and paths changed since a check are counted, not named.
  `crates/cli/tests/hq_phases_masking.rs` records every string of the three case lists of the
  A209 masking corpus (`crates/core/tests/a209_masking_corpus.rs`: must-mask, must-not-mask and
  the named exceptions) and fails when a phase line carries one of them whole or any line of
  one, a `TLCANARY` fragment, or a sentinel sentence of claim prose. The bare credential prefixes
  in that file's live-gate test are gate inputs, not corpus cases, and are not checked.
- `elapsed_ms` is measured per phase with a monotonic clock and is never padded. It is a
  non-negative integer, or **`null` when the measured time could not be represented — unknown**,
  never a stand-in number. A consumer must treat `null` as unknown; the HQ Verdict Panel refuses a
  log with any untimed row rather than play it on an assumed time.
- A stderr line that is **not** a phase object (a warning the CLI printed) is returned under an
  `unparsed` array — masked, then bounded, exactly as a 502's `stderr` is — rather than dropped.
  The key is absent when there were none.
- Status codes and `session` validation are those of §2.3: exit 0 and 3 are `200`, anything else is
  `502` with the same `projection_failed` body, a bad `session` is `400`, any other query parameter
  is `400`. Headers: `X-ThruLine-Projection: claims --json --phases` and `X-ThruLine-Exit`.
- **Not live.** The engine has no streaming transport; the log is the completed run's phases, so a
  consumer that animates it is replaying a record, not watching one. Server-sent streaming is an
  interface request (`docs/INTERFACE-REQUESTS.md`), not a promise of this contract.

### 2.2 The shapes themselves

Owned by the CLI verbs, not by this server. Reproduce them with `thruline explain --json` and
`thruline claims --json` — that is the authoritative source, and pinning a fixture taken from the
CLI is exactly as valid as pinning one taken from the API. As of 2026-09-05 their top-level keys
are:

- `explain --json`: `session`, `what_happened`, `why_it_stopped`, `what_it_forgot`, `promises`,
  `claims_vs_proof`, `risk_flags`, `narrative` — BUILD-SPEC §10's six questions — plus whatever
  envelope fields the CLI's `--json` registry adds (see *Versioning*).
- `claims --json`: `session`, `final_tree_hash`, `checks`, `claims`, `promises`, `negated`, same
  proviso. `capture_degraded` was added on 2026-09-16 (commit G). It is the session view's own
  flag, the one that caps every claim in the session at `unverifiable`, and the HQ panel reads it
  to show Fog rather than Couldn't check. It is not a receipt's `capture_degraded`, which also
  counts content filters in scope at the moment the receipt is cut.
- `sessions --json`: `store`, `total`, `returned`, `limit`, `limit_scope`, `sessions`, same proviso. `total` is
  every session in the store and `returned` is how many of them this body carries — both are
  present because the list is bounded. The CLI defaults to 50 rows with `limit_scope: "store"`.
  HQ passes `--per-project`, giving `limit_scope: "project"` and up to 50 ranked sessions for
  every recorded repository; activity in one repository cannot hide another. This enumerates
  captured repositories, not folders on disk. Each row of `sessions`: `id`, `repo_key`, `repo_root`, `harness`,
  `event_count`, `user_prompts`, `stop_events`, `file_change_events`, `final_messages`,
  `first_event_at`, `last_event_at`, `head_hash`, `capture_degraded`.

  Three things about those rows are contract, not incident:
  - **`repo_key` is often the lossy dash form** (`-Users-a-b`), which is ambiguous between
    `/Users/a/b` and `/Users/a-b`. Render it verbatim; un-mangling it fabricates a receipt. Use
    `repo_root` when it is non-null — it is the real path, and it is null on most rows.
  - **`first_event_at` / `last_event_at` are an event's *recorded* time**, not a session's start or
    end, and they are `null` rather than `1970-01-01T00:00:00Z` when the record carries only the
    epoch sentinel. Label them "last recorded event"; "last active" is a fact the store does not
    have.
  - **`stop_events` is the gate on the other two projections.** A session with no `stop` event has
    no final message, so `explain` and `claims` have nothing to extract from it. `final_messages`
    counts the `stop` events that carry a non-empty message — what the extractor reads. Neither is
    a promise that a claim comes out; only opening the session answers that.

  `started_at`, `ended_at`, `stop_reason` and `model` are deliberately **not** in a row: they are
  NULL on every row of a real store, and four permanently-null fields would teach a consumer that
  the store has no timestamps rather than that these columns were never written.

**Claim and promise text is masked on the way out, and this changed on 2026-09-16.** The four
fields are `claims[].text` and `promises[].text` on `/api/claims`, and `claims_vs_proof[].text`
and `promises[].text` on `/api/explain`. Each is passed through `redact::mask` as the body is
built (`claims_cmd::served_text`, `explain_cmd::served_text`, both enumerated in
`redact::MASK_CALL_SITES`), so a value that matches a known credential shape is replaced by a
`[masked: <family>]` marker naming the detector that fired.

**This field was previously served exactly as the record held it**, and this document said so — it
read "every producer the product ships masks it at capture; nothing masks it again on the way out".
That was true of the producers, and it was the wrong thing to rely on: masking at capture is a
property of whoever wrote the record, not of this surface, and `fold` does not re-mask a final
assistant message. A record written by a producer that did not mask — an import, a foreign hook, a
spool line written by hand — reached this API raw. The change is recorded here rather than shipped
silently, because it alters the bytes a consumer receives (interface requests 11 and 16,
`docs/INTERFACE-REQUESTS.md`).

What a consumer can rely on, stated as narrowly as it is true:

- **A mask is best-effort, never a guarantee of absence.** `redact`'s own contract: a secret in a
  shape the detector does not know is still in these bytes. Do not present this text as scrubbed.
- **The mask is a fixpoint.** Masking a served value again changes nothing, so a consumer that
  re-masks for its own surface neither double-marks nor cuts further.
- **Only these four fields changed.** Ids, spans, kinds, verdicts, evidence, reasons, findings and
  every other field are untouched, and the mask cannot change which claims exist: it is applied to
  the body being written, after extraction and after judgement. The claim ids in a body are the
  ids the pipeline reports on `/api/phases` for the same run.
- **The two projections serve the same words for the same records**, so a consumer that reads
  promise text from `/api/explain` when a claims read fails gets a value equal to the one
  `/api/claims` would have served. The Verdict Panel's Said row depends on exactly this: it shows
  the agent's words only when they equal the text of the record it cites.

Held by `crates/cli/tests/hq_phases_masking.rs`
(`the_claims_and_explain_projections_mask_the_text_they_serve` and
`masking_changes_the_words_served_and_not_which_claims_exist_or_how_they_were_judged`), over a
session whose record deliberately holds every string of the A209 masking corpus unmasked.

### 2.3 Status codes and headers

| situation | status | notes |
|---|---|---|
| CLI exited **0** | `200` | |
| CLI exited **3** | `200` | §14: 3 is a **policy** answer (an unsupported claim), not a failure. An unsupported claim is what the reader came to see. |
| CLI exited anything else | `502` | body: `{"error":"projection_failed","command":…,"exit_code":…,"stderr":…}` (stderr **masked, then** bounded to 2000 chars — in that order; see below) |
| `session` is not an identifier | `400` | body: `{"error":"invalid_session"}` |
| an unknown query parameter | `400` | `{"error":"unknown_query_parameter"}` |
| `session` given twice | `400` | `{"error":"session_given_twice"}` |
| **any** query parameter on `/api/sessions` | `400` | `{"error":"unknown_query_parameter"}` — that endpoint takes none, and refuses **before a process is spawned**. A `?limit=` that was silently ignored would be a UI showing 50 rows and believing it asked for 200. |

**`sessions --json` never exits 3.** Exit 3 is §14's *policy* answer — an unsupported claim — and
this command adjudicates nothing. The server's `3 → 200` rule still applies to it unchanged, which
is the correct fail-safe; a 3 out of `sessions` would be a defect in the command, not a policy
answer.

**The `stderr` in a 502 is masked before it is bounded, and the order is part of the contract.**
These bytes come off a child process's stderr and have passed through nothing that redacts them, so
this is the only mask they ever see. Cutting first can slice a credential's body below its
detector's threshold, and the cut string is all a later masker would get — so a consumer that
re-bounds this field for display must not treat the value as safe to cut and then scan. Held by
`uiserve::bounded_stderr`, enumerated in `redact::TRUNCATION_SITES`, and asserted behaviourally by
`a_credential_straddling_the_stderr_cut_is_masked_before_it_is_cut`. **This field was previously cut
at 2,000 characters and not masked at all**; the change is recorded here rather than shipped
silently, because it alters the bytes Track A receives.

Two response headers on both projections:

- `X-ThruLine-Projection: explain --json` — the exact command this body is the output of. If your
  fixture disagrees with the server, this names the command to run to see the difference.
- `X-ThruLine-Exit: 0|3|…` — **the CLI's exit code, so a policy answer is distinguishable from a
  success without re-deriving the verdicts client-side.** Do not infer "unsupported claims exist"
  from a `3` alone; read `claims_vs_proof`. The header is for telemetry and for deciding whether to
  re-fetch, not a verdict.

`session` accepts `[A-Za-z0-9._-]{1,128}`, not `.`, not leading `-`. Everything else is refused
**before a process is spawned** — the value becomes an argv element and the accepted set is narrow
by construction rather than escaped on the way past.

### 2.4 Two stores, and they are not the same store

This bites, so it is stated first rather than discovered:

| surface | reads |
|---|---|
| `/fixtures/…` (and `POST /api/refresh`) | `~/.thruline/ledger.db` → exported to `~/.thruline/export` |
| `/api/explain`, `/api/claims`, `/api/sessions` | `~/.thruline/exec.db` (the execution ledger; `$THRULINE_HOME` overrides) |

A session id that appears in one does not necessarily appear in the other. **`/api/explain` takes
execution-ledger session ids**, which are the ids `thruline explain` and `thruline claims` print —
and which `/api/sessions` enumerates. The `id` of any row it returns is a valid `?session=` value
for the other two.

---

## 3. The rest of the surface

Pre-existing, documented so Track A knows what is there and what it may lean on.

| endpoint | method | answer | may Track A rely on it? |
|---|---|---|---|
| `/fixtures/index.json` | GET | the export summary index | **yes** — the §9 fixture contract |
| `/fixtures/projects/<id>.json` | GET | one project | **yes** |
| `/api/state` | GET | `{have_export, refreshing, last_refresh, generated_at}` | **yes** |
| `/api/refresh` | POST | `202 {"ok":true,"started":true}`, or `409 {"ok":false,"reason":"already_refreshing"}`. Asynchronous: poll `/api/state`. | **yes** |
| `/api/lanes` | GET | per-vendor count of recordings written in the last 5 minutes, with its provenance in the payload | **yes** |
| `/api/mind/status`, `/api/mind/interpret` | GET / POST | the local shadow-interpretation endpoints, backed by a fixed loopback model | **no** — subject to change without notice |

Only `.json` names are reachable under `/fixtures/`, at the root or under `projects/`; anything
else is a 404. That guard predates this contract and is unchanged.

---

## 4. Loopback, and the refusal to be anywhere else

**Zero network egress beyond `127.0.0.1`.** This server hands out real transcript content, so the
invariant is not a preference:

- The listener binds an **IPv4 loopback address only**. `--address` accepts `127.0.0.1`,
  `localhost` (a spelling of `127.0.0.1` — no hostname is ever resolved, because a DNS answer must
  not get to decide where a local-only server listens), and any other `127.0.0.0/8` address.
- **Anything else exits 1 before a socket exists**, with the address named:
  `refusing to bind <addr>: thruline ui serves your transcripts and answers only on the IPv4
  loopback.` Witnessed for `0.0.0.0`, `::`, `192.168.1.10` and `example.com`.
- A connection from a non-loopback peer is answered `403 loopback only`.
- `Host` must be the address the server printed (or `localhost:<port>`); `Origin`, when present,
  must match. Otherwise `403`. This is the **browser** half: without it, any page the user has open
  could point a request at this port through a hostname resolving to 127.0.0.1 and read the answer.
- Nothing survives the process. No daemon, no watcher, no scheduled work: Ctrl+C closes the socket
  and that is the end of it.

---

## 5. Versioning

**`schema_version` is the CLI's, not this server's. The server neither adds it nor drops it.**

This is the one place a projection could have lied, so the rule is stated as a rule rather than as
a current fact:

- The server **never invents** a `schema_version`. Adding one at the API layer would mean the API
  states a version the CLI does not — a number nothing stands behind, pinned by four teams.
- The server **never strips** one. Whatever the `--json` shape carries, the endpoint carries.

An acceptance test asserts that **equivalence** rather than a presence: the API's body carries
`schema_version` if and only if the CLI's does. That is what makes this section unable to go stale
— it holds in both directions and through any change to the emitters.

**Read the current answer, do not take it from here.** `thruline explain --json | jq
'.schema_version, .shape'` is the derivation; this document is a memory. As measured on 2026-09-05
the answer was `1` and `thruline.explain` (`thruline.claims` for the other), supplied by the CLI's
own `--json` registry (`crates/cli/src/json_out.rs`) — a shape owned by another track, which is
exactly why this server projects it rather than restating it.

> **History, because it is the reason this section is written this way.** When this contract was
> drafted these two shapes carried **no** `schema_version` at all — `schema_version` appeared 77
> times across `crates/` and not one of them versioned a CLI output shape. The contract shipped
> emitting the shapes exactly as they were and saying so, rather than minting a version the CLI
> could not honour. The registry landed separately, and the endpoints carried it through **with no
> change to this server** — which is the property, demonstrated rather than asserted.

**What else is versioned:**

- **This document is v1.1.** The endpoint set, the status codes, the headers and the refusals above
  are the contract. Changing any of them is a version bump here and a change to
  `crates/cli/tests/ui_server_contract.rs`, which is what makes the bump visible.

  **What v1.1 changed:** `GET /api/sessions` was added, projecting `thruline sessions --json` on the
  same mechanism, with the same two headers and the same 200-on-exit-3 / 502 rule. Nothing that was
  in v1 changed — `/api/explain` with no `session` still opens the lexicographically last id, and
  the `X-ThruLine-Projection` values of the two existing endpoints are byte-identical to what they
  were. A minor bump rather than a major one because **adding** an endpoint breaks no pinned
  consumer; §"What will not change without a major version" governs *changing* a path or method,
  and none was changed.
- **The bodies of `/api/explain` and `/api/claims` are versioned by whatever versions the CLI
  verbs.** Pin fixtures from `thruline explain --json`; if the CLI's output changes under you, the
  API changed too, by construction.

### What may change without a version bump

Named, so nothing here is a surprise:

- `/api/mind/*` in any way.
- The **content** of `/api/state.last_refresh` (it is the export report, which grows fields).
- The exact wording of any human-readable message, including the refusals — **the exit codes and
  the named path/address in them are the contract; the sentence around them is not.**
- Content types for extensions not listed in §1.

### What will not change without a major version

- Endpoint paths and methods.
- The rule that `/api/explain` and `/api/claims` are byte-identical to the CLI.
- 200-on-exit-3.
- Loopback-only binding, and the refusal to start on a bad `--static-dir`.

---

## 6. Known limits, stated rather than found

- **One process per projection request.** `/api/explain` spawns `thruline explain`. A local
  single-user server can afford it and it is what makes drift impossible; it is not free. There is
  no request timeout — a hung child holds one thread, not the server.
- **No CI has ever run on this repository** (`docs/STATUS.md`). Every behaviour above was witnessed
  on one macOS host, which is **case-insensitive where Linux is not**. The static-file path is the
  obvious place for that to matter, so it is closed rather than noted: `/Assets/App.js` against a
  directory containing `assets/app.js` would have been served here and 404'd on a Linux runner, so
  **the server compares the name on disk against the name requested and 404s on either platform**
  (witnessed on macOS for `/Assets/app.js`, `/assets/App.js`, `/ASSETS/APP.JS`). Asset paths are
  case-sensitive by the server's decision, not the filesystem's.
  The rest of this surface has still never met a case-sensitive runner.
- `/api/lanes` reads `$HOME/.claude/projects` and `$HOME/.codex/sessions` by file mtime. It counts
  files; it is not a claim about health, and the payload says so.
- The static server does no compression and sends `Cache-Control: no-store` on everything.

## Integration evidence (2026-09-09 additive route)

`GET /api/integrations?session=<recorded-id>` returns `thruline.integration-workspace.v1`: the selected session ID, its recorded repository path, manifest-based check suggestions, the 21-source catalog, and that repository's latest 100 integration bindings with current assessments. These are project checks; they are not attributed to the selected historical agent session. The response is derived by `thruline integration workspace --session <id>` against the UI's evidence store. It executes no check or model.

Exactly one valid session parameter is required. Extra parameters, duplicate parameters, malformed/framed GET bodies and duplicate headers fail closed; non-GET returns 405, foreign Host/Origin returns 403. Acceptance-only mode returns 404. A busy reading returns 503; an unavailable/oversized/timed-out CLI projection returns 502. The shared structured-projection budget is 30 seconds and 1 MiB. A result-artifact integrity or freshness-read failure is shown explicitly, with no previous green result retained. Native integration-route unit tests cover these boundaries; the main UI-server regression suite remains unchanged in authority.

The UI opens receipt details in place and can copy an exact `thruline evidence` command. Starting a tool remains an explicitly accepted CLI plan; no browser-supplied command or filesystem path is executed by this endpoint.

### Explicit integration actions

`POST /api/integrations/action` accepts at most 512 bytes of strict JSON with exactly
`session`, `action`, `recipe`, and optional `digest`. It requires the matching local Origin,
valid Host, one JSON Content-Type and Content-Length, and no query/transfer encoding. Actions
are `preview`, `accept`, `run`, `cancel`; recipe selection is native, and browser-supplied argv,
root paths and executable paths are rejected. The session resolves its stored repository.
Acceptance compares the review digest and appends the existing accepted-contract chain; running
requires the current acceptance hash. One HQ check runs at a time; cancellation only addresses
the session that owns that foreground check. Execution is bounded separately from HTTP transport;
an interrupted/oversized response tells the user to inspect receipts before retrying.

//! R2 — detect-and-mask (A177, corrected by rulings D1–D4 at A178).
//!
//! **This module never promises absence.** "Secrets excluded by default" is a
//! denylist promise, and denylists fail silently toward disclosure: the one
//! secret whose shape nobody anticipated is exactly the one that ships. So the
//! contract is inverted — the artifact records the **treatment it received**,
//! never the property it has:
//!
//!   detector v2 · 3 masked · patterns: aws, email · best-effort, not a guarantee
//!
//! A reader can act on that. "Scrubbed: true" would invite them not to look.
//!
//! Three rules give the module its shape:
//!
//! 1. **Every mask is stated where it happened.** A silently altered line would
//!    be a lie with better manners than the leak. The replacement names the
//!    detector that fired.
//! 2. **A detector reports what it matched, never what it decided.** `aws`
//!    means "this has the shape of an AWS key", not "this is a live
//!    credential" — the RB7 proxy discipline applied to secrets.
//! 3. **Entropy fires only in value positions** (D1). Known-format detectors
//!    fire in any position, at any **word boundary** within a token; the
//!    entropy heuristic does not, because a filename like
//!    `NotificationDigestV2ComponentWrapper.tsx` is high-entropy by every
//!    measure and is *exactly* the context recovery exists to preserve.
//!
//!    **This sentence used to read "Known-format detectors are global", and
//!    that was false** — corrected at A209 after a probe refuted it. They
//!    were segment-boundary-scoped: the scan split each token on `/@=:` and
//!    required `starts_with`, so `_` and `.` — both token characters —
//!    concealed a known prefix outright. The claim mattered beyond this
//!    module: a Recovery Capsule's redaction declaration scopes residual
//!    risk to shapes "the detector does not know", and the detector knew
//!    this shape and masked it elsewhere in the same document. Detectors are
//!    now boundary-scoped *by design and by name* ([`known_format_prefix`]):
//!    a prefix after any separator fires, a prefix buried inside an
//!    alphanumeric run does not.
//!
//! # Known limitations
//!
//! Enumerated in two kinds, and **both kinds are mandatory** (D4). A section
//! that lists only what was designed, while the record already measured a
//! miss, is the false-calm family wearing documentation clothes.
//!
//! **Designed exemptions** — accepted deliberately:
//!
//! * *Entropy is value-scoped.* A naked high-entropy blob pasted as bare prose
//!   — no `=`, no quotes, no URL, no flag — is not masked by entropy. The
//!   trade-off, argued in full: masking it globally destroyed a real filename
//!   in `objective` and `in_flight`, the two sections a fresh session reads
//!   first, on **every** capsule. That is certain harm to the artifact's core
//!   purpose. The miss is possible, not certain, and is backstopped by the
//!   preview-and-consent gate that blocks injection
//!   ([`Treatment::needs_acknowledgement`]). Certain destruction of working
//!   context loses to a possible miss a human is shown before it travels.
//! * *Published identifiers are exempt from entropy.* Hex digests, UUIDs and
//!   ULIDs are the product's receipts; masking them removed a chain position
//!   in the first live run. Prefix detectors still fire on them.
//! * *A known prefix buried mid-alphanumeric-run is not matched* (A209).
//!   `myrepoghp_<key>` with no separator before the prefix leaks. This is
//!   the deliberate price of the boundary rule, and the thing it buys is
//!   `task-board.ts`, `disk-usage.txt` and `monkeyJump.tsx` surviving —
//!   `sk-` and `eyJ` sit inside ordinary English, and a `contains` scan
//!   masks all three. Certain destruction of the paths a fresh session
//!   reads first loses to a possible miss on a shape no tool emits (a
//!   secret pasted with no delimiter before it). Stated here rather than
//!   left silent.
//! * *A known prefix with no plausible body is not matched* (A209).
//!   `sk-`, `AKIA`, `ghp_` and `eyJ` alone are words or fragments, not
//!   credentials; each detector requires a minimum body length and charset
//!   before it fires. `-----BEGIN` is exempt — the banner IS the signal.
//!   The residual: a real credential truncated below the body minimum is
//!   missed, which is the correct trade, because a truncated credential is
//!   not usable and the alternative masks 38 real strings per corpus.
//!
//! **Measured misses** — specimens the record has actually produced:
//!
//! * *A real email address travelled in a live capsule* (measured at A174, in
//!   a `git commit` author flag). Addressed by the `email` detector below;
//!   listed here because it was measured, not because it was predicted.
//! * *A real service project-ref travelled in a live capsule* (measured at
//!   A174, `supabase functions deploy … --project-ref <ref>`). Addressed by
//!   the flag-value rule; same reason.
//! * *A known-format secret inside a path or underscore-joined name was not
//!   masked at all* (measured at A209 by an independent verifier's probe,
//!   reproduced first-hand before the repair). Specimens, verbatim, with the
//!   pre-repair result:
//!
//!   ```text
//!   ghp_TLCANARYaaZZ11bbYY22ccXX33ddWW44e            → masked=1 {github}
//!   /tmp/repo/secrets_ghp_TLCANARYaaZZ11…e.txt       → masked=0 {}   ← MISS
//!   AKIAIOSFODNN7EXAMPLE                             → masked=1 {aws}
//!   /tmp/key_AKIAIOSFODNN7EXAMPLE.txt                → masked=0 {}   ← MISS
//!   ```
//!
//!   This was not a heuristic gap but a scanning defect, and it reached a
//!   persisted artifact: a Recovery Capsule builds its unresolved-work lines
//!   from edit FILE PATHS, so the missed shape is the shape that category
//!   emits. The v1 and v2 capsules both leaked it. Addressed by
//!   [`known_format_prefix`]'s boundary scan and pinned by
//!   `the_a209_measured_miss_is_masked_at_every_boundary` below, by the
//!   committed adversarial corpus in `tests/a209_masking_corpus.rs`, and
//!   end-to-end through a sealed capsule in `a209_capsule_v2.rs`.
//!   (This line previously pointed at `c3_redaction_corpus.rs`, which has
//!   not changed since A178 and contains none of these specimens — a false
//!   pointer that sent reviewers to the wrong file while the corpus itself
//!   lived only in a scratchpad. Corrected at A209-C round 3, and the
//!   corpus committed so it is a standing gate rather than a one-time
//!   measurement.)
//! * *The A209 repair's own first attempt over-masked, and it was measured*
//!   — D4 cuts both ways, so the over-masking is enumerated here exactly as
//!   the under-masking above. Widening known-format detection to word
//!   boundaries, with no body requirement, newly masked **38 distinct real
//!   `Bash` command tokens** on the live ledger (221,416 sampled) — every
//!   one an HTML data attribute from one real project (`data-sk-seg`,
//!   `data-sk-drop="gusto"`, `id="pop-sk-julio"`), all reported `openai`,
//!   none a secret — plus 3 message tokens (JWT placeholders). File paths,
//!   the population A178 actually protects, regressed **0 of 78,488**. This
//!   is the A178 failure mode recurring, caught by an adversarial corpus an
//!   independent agent authored before this fix was read. Cured by the
//!   per-detector body predicate; re-measured after the cure: **0 of 38
//!   remain**, and the only string the cure still newly masks is a JWT
//!   placeholder. Every mask the cure REMOVES relative to the old detector
//!   was checked by hand: scrubbing regexes (`'(AKIA|ASIA)[A-Z0-9]{16}'`),
//!   bare placeholders (`ANTHROPIC_API_KEY=sk-ant-...`), already-elided
//!   forms (`ghp_…`) and CSS ids. No credential lost its mask.
//!
//!   **The invariant, stated exactly.** The predicate drops a match whose
//!   body is too short **or whose charset breaks inside `body_min`** — not
//!   length alone, as A209-C first wrote it. Measured counter-example:
//!   `sk-0123456789.abcdefghijklmnopqr` carries 22 usable characters yet
//!   drops, because `.` halts the body count. The security consequence
//!   measured nil (a Monte-Carlo over 20,140 realistic base64-of-JSON
//!   heads found 0 with `+` or `/` in the first 12 body characters), but
//!   the honest sentence is the one above, and the hand inspection of the
//!   dropped masks is what stands behind the claim — not the phrasing.
//!
//!   **Redactor figure and gate figure are different numbers, and both
//!   belong in the record.** On the same 221,416-token `Bash` population:
//!   the cured *redactor* newly masks **0**, while the composed *gate*
//!   predicate (`mask` OR [`resembles_known_credential`]) newly refuses
//!   **38** — the `data-sk-*` family, which is correct behaviour for a
//!   fail-closed gate and would be wrong for a redactor. On paths and
//!   project ids (78,584 distinct) both figures are **0**. Quoting only
//!   the redactor figure made the cure look total; quoting both says what
//!   actually happens.
//! * *`sk-`-prefixed real strings that are still masked and should not be
//!   — **7** of them, measured, not two* (A209-C round 3). The earlier
//!   version of this line named two specimens and implied that was the
//!   set; D4 requires this section be measured, so it is: sweeping the
//!   live ledger's 221,416 distinct `Bash` command tokens, 7 masked tokens
//!   carry `sk-`, and every one is a CSS identifier or kebab-case name
//!   whose body clears the 16-character minimum —
//!   `id="sk-fingerprint-text"`, `id="sk-firstrun-compare"`,
//!   `id="sk-mirror-computing"`, `id="sk-resolve-log-text"`,
//!   `sk-market-untouched` in two quoting forms, and one path. They fail
//!   before and after this repair. Curing them needs a stricter body
//!   charset (e.g. requiring a digit), which risks missing a real
//!   all-alphabetic key — the under-masking direction, which this module
//!   does not trade toward. Measured, named, not fixed.
//! * *Two real GitHub token formats were absent from the detector table
//!   entirely* (A209-C round 3, found by an independent attacker).
//!   `ghu_` (user-to-server) and `ghr_` (refresh) returned `masked=0` and
//!   `resembles_known_credential=false`, so they leaked verbatim into
//!   sealed capsules through the same `open_obligations` path as the
//!   original A209 defect AND passed every fail-closed gate. The gap stood
//!   from A178 through A209-C, undisclosed in exemptions and measured
//!   misses, while the body predicate's own doc comment quoted the
//!   operator's harvested regex `gh[pousr]_[A-Za-z0-9]{20,}` — which names
//!   all five letters — three lines above the table that omitted two of
//!   them. Fixed by adding the literals; the existing predicate fits both
//!   unchanged. Pinned in `a209_masking_corpus.rs`.
//! * *A PEM private key shipped in full while the treatment said it was
//!   masked* (A209-C round 3). The token walk matched `-----BEGIN` and
//!   replaced that token only, so a real key rendered as
//!   `[masked: private_key_block] RSA PRIVATE KEY-----` with **both base64
//!   key lines byte-identical** — and the key material is unreachable by
//!   every other detector, because entropy is value-scoped (D1) and a PEM
//!   body in prose is never examined. The reported treatment
//!   (`1 masked · private_key_block`) was therefore a false statement about
//!   what had been done, which rules 1 and 2 forbid more strictly than they
//!   forbid a miss. Fixed by [`mask_pem_blocks`], which masks banner
//!   through closing banner as one span and fails toward masking when the
//!   block is unterminated. Reach was real: a capsule's `original_task`
//!   and `established_work` carry verbatim user prompt text.
//! * *An upstream TRUNCATION can push a credential below its body
//!   threshold, and this is reachable in production* (A209-C round 3,
//!   measured on a live run, not predicted). A Recovery Capsule's
//!   `original_task` line is built from the opening ask, which
//!   `candidates` truncates to 280 characters with an ellipsis BEFORE the
//!   masker ever sees it. A Google API key straddling that boundary
//!   arrived as `AIza` + 27 body characters, cleared no threshold at the
//!   then-current 30, and **shipped in a sealed capsule** while six other
//!   credentials on the same line masked correctly. Immediate fix: the
//!   `google` threshold is 20, which covers this case with margin. The
//!   CLASS is not closed by that, and saying so is the point of this
//!   entry — any renderer that truncates before masking can cut any
//!   credential below any threshold. The durable fix is ordering — mask
//!   before truncate — which lives in the candidate/render layer shared
//!   with the v1 capsule, not in this module; registered as cross-layer
//!   work rather than silently patched per-detector.
//! * *Vendor scope, enumerated rather than implied* (A209-C3). The
//!   known-format table covers GitHub (all five letters), AWS access keys,
//!   OpenAI, Anthropic, Slack, Google API keys and JWT headers. Formats it
//!   does **not** know include `glpat-` (GitLab), `sk_live_`/`whsec_`
//!   (Stripe), `hf_` (Hugging Face), `npm_`, `dckr_pat_` (Docker),
//!   `dop_v1_` (DigitalOcean), `ya29.` (Google OAuth), `xoxa-`/`xoxr-`
//!   (older Slack), and `ABIA`/`ACCA` (other AWS key classes). These are
//!   honest residuals under this module's own stated scoping — the
//!   treatment says a secret in a shape the detector does not know is
//!   still present — but D4 makes enumeration mandatory, and "the detector
//!   does not know it" is only a fair warning if the reader can see the
//!   list.
//! * *A PEM body cut below the payload bar is not detected* (A209-C6,
//!   the deliberate cost of the body gate). The mask decision requires a
//!   run of at least 40 base64 characters immediately after the banner,
//!   because a shorter bar is met by ordinary identifiers and hex digests
//!   — measured, twice: a 16-character bar fired on 98 live fields and
//!   destroyed 1,150,705 characters, and a 40-character bar allowed
//!   anywhere in the next 12 lines still fired on 5 shell scripts. RFC
//!   7468 wraps payload at 64, so a real first line clears the bar
//!   comfortably; a key truncated to under 40 body characters does not,
//!   and is missed. That trade is stated rather than assumed: the
//!   alternative destroyed a million characters of the operator's own
//!   records under a confident `private_key_block` treatment.
//! * *Anything the corpus has not yet shown us.* This list grows by
//!   measurement, and a review that lands a detector change without checking
//!   whether the corpus already holds a counter-specimen has skipped a step.
//!
//! # Callers that use this module as a validity GATE, not a redactor
//!
//! These test `mask(...).2` and **reject** the value rather than masking
//! it, so a false positive refuses otherwise-valid input and a false
//! negative admits a credential-shaped one. They must therefore also
//! consult [`resembles_known_credential`], whose threshold is the strict
//! one (a bare prefix disqualifies).
//!
//! The list is **[`MASK_CALL_SITES`], and a test enumerates the
//! workspace and fails if the two disagree** — because this comment
//! previously said "two callers" when there were four, and the third live
//! one (`mind_grounded::safe_model_string`) was left loosened for a whole
//! review round as a result. A hand-written set-completeness claim is
//! worth less than one a machine re-derives.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

pub const DETECTOR_VERSION: &str = "v2";

/// What a caller does with this module's output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskUse {
    /// Rejects the value when the masker fires. A false negative admits a
    /// credential-shaped value, so these must ALSO consult
    /// [`resembles_known_credential`].
    Gate,
    /// Keeps the masked string. A false negative leaks; a false positive
    /// destroys context.
    Redactor,
    /// Reads the masker but cannot reach a firing string — enumerated
    /// rather than judged absent, with the reason stated.
    Inert,
}

/// **Every** non-test call site of [`mask`] outside this module, and what
/// each does with it.
///
/// `a209_masking_corpus.rs` re-derives this set by walking **every member
/// crate declared in the workspace manifest** and fails when the two
/// disagree, so a new call site of either kind must be classified here or
/// break the suite. A negative-control test injects an unclassified site
/// and requires the derivation to FAIL — without it, the walk would be a
/// heuristic asserting its own completeness, which is the exact failure it
/// exists to repair. The list enumerates ALL uses, not
/// only gates: the previous version enumerated gates by hand, called them
/// "two callers", missed two, and left one loosened for a whole review
/// round. Its replacement then keyed on the `.2` spelling — which
/// `validate_shadow_label` does not use — because a heuristic that quietly
/// under-counts is the same failure in a new coat. This list is keyed on
/// the call itself.
pub const MASK_CALL_SITES: &[(&str, &str, MaskUse)] = &[
    // C44 E2: recursive public Verify JSON and human diagnostics retain the
    // masked display copy; source bytes and verdict selection stay separate.
    ("verify_cmd.rs", "display", MaskUse::Redactor),
    ("integration_cmd.rs", "execute", MaskUse::Redactor),
    ("integration_actions.rs", "handle", MaskUse::Redactor),
    // Captured repair-worker diagnostics are masked before entering the
    // receipt/UI payload. The replacement text is retained; these sites do
    // not infer a secret finding or decide whether a remedy is acceptable.
    ("repair.rs", "action", MaskUse::Redactor),
    ("native_repair.rs", "action", MaskUse::Redactor),
    ("agent_repair.rs", "action", MaskUse::Redactor),
    ("annotate.rs", "safe_language_model_name", MaskUse::Gate),
    ("candidates.rs", "derive", MaskUse::Redactor),
    ("candidates.rs", "extract_commit_message", MaskUse::Redactor),
    ("capsule.rs", "compile_with", MaskUse::Redactor),
    ("capsule_v2.rs", "compile", MaskUse::Redactor),
    ("capsule_v2.rs", "label_from_column", MaskUse::Redactor),
    ("derive.rs", "label_from", MaskUse::Redactor),
    // BUILD-SPEC §6.6 `secrets_absent`. A Gate, and an INVERTED one: the two
    // gates above refuse a value, where a false positive costs nothing; this
    // one refutes a claim, where a false positive wrongly accuses. So it takes
    // the conjunction of the masker firing and
    // `resembles_known_credential` — strictly more confident than either — and
    // lets a context-only detection leave the claim `unverifiable`, which is
    // its ceiling anyway.
    ("claims_cmd.rs", "diff_evidence_inputs", MaskUse::Gate),
    // Interface requests 11 and 16: the serving edge of `claims --json` and
    // `explain --json`, which `docs/UI-SERVER-CONTRACT.md` §2.1 publishes byte
    // for byte as `/api/claims` and `/api/explain`. Claim and promise text was
    // masked only where a PRODUCER captured it, and `fold` does not re-mask the
    // final assistant message, so a record written by a producer that did not
    // mask reached Track A's web UI raw.
    //
    // `Redactor`, and load-bearing rather than idempotent on that path: for a
    // capture the product itself wrote the mask is a fixpoint, but these two
    // sites exist for the records where it is not.
    //
    // **Each is a function of its own, and that is the point.** Both commands
    // also hold text that must NOT be masked — `explain`'s `run` builds
    // `Source.text` from the stop event and feeds it to `extract`, where a mask
    // would change which claims exist and where their spans fall. An entry
    // reading `("explain_cmd.rs", "run", …)` would record that the function
    // masks while hiding that one of its two text paths deliberately does not.
    ("claims_cmd.rs", "served_text", MaskUse::Redactor),
    ("explain_cmd.rs", "served_text", MaskUse::Redactor),
    // T-2-1: the serving edge of the Change record, `/api/changes` and
    // `/api/changes/<id>`. Two functions rather than one, for the same reason
    // the two above are: `read_change` also carries text that must NOT be
    // masked — ids, digests and timestamps, where a mask would break a deep
    // link — and one entry named after the reader would hide that.
    //
    // `Redactor` on both: the masked string is what is served, and no decision
    // is taken from whether the masker fired. The mask record travels with the
    // text and is served as the `masking` treatment, so a consumer can say what
    // ran rather than assume it.
    ("change.rs", "mask_text", MaskUse::Redactor),
    ("change.rs", "mask_list", MaskUse::Redactor),
    // BUILD-SPEC §10's timeline. Keeps the masked string and decides nothing
    // from it, so it is a Redactor rather than a Gate. Everything it sees came
    // out of the ledger already redacted, which makes this pass a fixpoint; it
    // runs so the mask-then-cut invariant is true within the one function the
    // truncation census can see.
    ("explain_cmd.rs", "truncate", MaskUse::Redactor),
    ("derive.rs", "truncate_words", MaskUse::Redactor),
    ("language.rs", "validate_shadow_label", MaskUse::Gate),
    (
        "mind_grounded.rs",
        "canonical_export_timestamp",
        MaskUse::Inert,
    ),
    ("mind_grounded.rs", "push_fact", MaskUse::Redactor),
    ("mind_grounded.rs", "safe_model_string", MaskUse::Gate),
    ("mind_grounded.rs", "safe_text", MaskUse::Redactor),
    ("operations.rs", "derive", MaskUse::Redactor),
    // BUILD-SPEC §4.6. Wraps this module rather than reimplementing it, and
    // keeps the masked string, so a false negative leaks and a false positive
    // destroys context — a Redactor, not a Gate. It never consults the mask to
    // decide anything, so `resembles_known_credential` does not apply.
    ("redaction.rs", "redact", MaskUse::Redactor),
    // The idempotent re-mask in `Redacted::bounded`, which exists so the
    // mask-then-cut invariant is true LOCALLY and not only through the type
    // boundary. Classified `Redactor` rather than `Inert`: it keeps the masked
    // string, and although it should never fire (the text arrives redacted),
    // claiming it CANNOT is a stronger statement than this list should make —
    // `Inert` is for sites that provably cannot reach a firing string.
    ("redaction.rs", "bounded", MaskUse::Redactor),
    // The fold's bounded rendering of already-redacted spool text. Same shape
    // as `bounded`: an idempotent re-mask so the mask-then-cut invariant is
    // true locally and not only through the pipeline.
    ("fold.rs", "compact", MaskUse::Redactor),
    // The sensor -> §4.1 bridge, masking the three payload fields `fold.rs`'s
    // `compact` will read (`tool_input`, `tool_response`, `error`) before the
    // spool line is written. `Redactor`, and NOT the idempotent-re-mask shape
    // the two entries above have: the sensor stores its payload verbatim, so
    // here the mask genuinely fires and its output is kept. It is also the one
    // site whose reason is a measurement — this masker and
    // `capture::redaction::Redactor` do not agree (only this one carries
    // `high_entropy`), and on the operator's corpus 318 hook-written fields
    // are NOT a fixpoint under it. On that path the mismatch is caught only by
    // a `debug_assert` in `compact`, so a release build writes the unmasked
    // text; the bridge closes the gap for the lines it writes and cannot close
    // it for the ones it does not.
    ("sensor_bridge.rs", "mask_compact_fields", MaskUse::Redactor),
    // §10's flight recorder, bounding a payload field for its detail pane.
    // Same shape again: the payload arrived redacted through the spool, and
    // the re-mask exists so the mask-then-cut invariant is true LOCALLY —
    // which is what the truncation census can see.
    ("tui_cmd.rs", "truncate", MaskUse::Redactor),
    // §4.2's Codex rollout importer, bounding an agent message or a patch's
    // stderr. **Not the same shape as the two above**: a rollout is the
    // vendor's own file, written without ThruLine involved, so this mask is
    // the first these bytes have ever seen and it is load-bearing rather than
    // idempotent. There is a test that a token in a rollout does not reach the
    // store.
    ("codex_rollout.rs", "bounded", MaskUse::Redactor),
    // The `thruline ui` JSON API's 502 body, bounding a failed projection's
    // subprocess stderr. **Load-bearing, like the rollout above and unlike the
    // three idempotent re-masks**: these bytes come off a child process's file
    // descriptor and have passed through no spool, so this is the first and
    // only mask they ever see before `docs/UI-SERVER-CONTRACT.md` §2.3
    // publishes them to Track A's web UI. Added when the truncation census
    // caught the site cutting first and not masking at all.
    ("uiserve.rs", "bounded_stderr", MaskUse::Redactor),
    // A projection failure's complete diagnostic is masked before path
    // exclusions inspect it; the masked text is kept, never used as a gate.
    ("uiserve.rs", "hidden_diagnostic", MaskUse::Redactor),
    // T-2-7's first screen: the repositories this machine has already recorded
    // work for. The label is the path the RECORDING itself names (its `cwd`),
    // and it is masked on the way to the socket. A Redactor — the masked string
    // is what the screen shows, and nothing here refuses a candidate or decides
    // anything from whether the masker fired.
    ("first_use.rs", "candidate_json", MaskUse::Redactor),
    // P6-5's run sheet (`crates/cli/src/uiserve/run_route.rs`): every string
    // it serves that the record or the machine wrote — the project folder, the
    // argv shown under Technical, the line a failing run printed, why a run did
    // not start, who gave permission — goes through this one helper on its way
    // to the socket. A Redactor: the masked string is what the sheet shows, and
    // nothing refuses a run or decides anything from whether the masker fired.
    ("run_route.rs", "masked", MaskUse::Redactor),
    // C48 Settings permission labels and the recorded actor are display text.
    // Both retain the masked string; neither decides permission authority.
    ("run_permission_settings.rs", "label", MaskUse::Redactor),
    ("run_permission_settings.rs", "get", MaskUse::Redactor),
    // P6-2's Read (`crates/cli/src/uiserve/read_route.rs`): each phase line's
    // `detail` from the `claims --phases` child's stderr is masked before it is
    // served. The pipeline writes only ids and counts there (`claims_cmd::Phase`),
    // so this is a second mask over bytes that should never fire it — kept
    // because they come off a child process's descriptor and reach the socket
    // with nothing else in between. A Redactor: the masked string is what is
    // served, and nothing decides anything from whether the masker fired. (The
    // failure `reason` goes through `bounded_stderr` above.)
    ("read_route.rs", "assemble", MaskUse::Redactor),
    // T-4-1a-FIX (`crates/cli/src/integration_cmd/review.rs`,
    // `ReviewAvailability::to_json`): the `technical` detail beside "review
    // findings aren't available" — a probe error or a runtime-lookup error,
    // which can name a path on this machine — is masked before the review
    // doors serve it. A Redactor: the masked string is what is served, and
    // nothing refuses or decides anything from whether the masker fired;
    // whether findings are available was decided before this point.
    ("review.rs", "to_json", MaskUse::Redactor),
    // HQ Next (`crates/core/src/hq*`, `crates/cli/src/uiserve/hq.rs`): every
    // served string is masked and the masked text kept. None of these
    // rejects anything on the masker's answer, so none is a Gate.
    ("hq.rs", "masked", MaskUse::Redactor),
    // C30 R-LEDGER capture holds: the names and paths a person typed for a pause
    // or an exclusion are masked on the way to the socket (`capture_holds.rs`)
    // and to the terminal (`hold_cmd.rs`) and the masked text is what is shown.
    // The record keeps what was typed. Nothing refuses a path on the masker's
    // answer, so neither is a Gate.
    ("capture_holds.rs", "mask", MaskUse::Redactor),
    ("hold_cmd.rs", "masked", MaskUse::Redactor),
    ("hq.rs", "first_line", MaskUse::Redactor),
    ("hq.rs", "run_load", MaskUse::Redactor),
    // C27: the load loop moved into `crates/core/src/hq/load.rs`; the store
    // and per-file error sentences it serves are masked there.
    ("load.rs", "run_load_with", MaskUse::Redactor),
    ("prompts.rs", "cut_title", MaskUse::Redactor),
    ("view.rs", "excerpt", MaskUse::Redactor),
    ("view.rs", "clip", MaskUse::Redactor),
    // C30 R-RUN: what a check started from HQ has printed so far is masked as one
    // text and served; nothing is rejected on the masker's answer, so not a Gate.
    ("run_route.rs", "output_json", MaskUse::Redactor),
    // Decisions v0.2: these sites retain masked evidence, source spans,
    // subprocess diagnostics, or presentation text. None uses the masker's
    // match count to accept or reject a decision. In particular, proposal
    // extraction masks a person event's text before parsing; that can change
    // the proposed wording, but it does not gate on whether masking fired.
    ("checks.rs", "check_text_absent_at", MaskUse::Redactor),
    (
        "checks.rs",
        "check_text_absent_transition",
        MaskUse::Redactor,
    ),
    ("checks_claims.rs", "edit_fact", MaskUse::Redactor),
    (
        "checks_fidelity.rs",
        "difference_summary",
        MaskUse::Redactor,
    ),
    ("checks_fidelity.rs", "file_finding", MaskUse::Redactor),
    ("checks_fidelity.rs", "run_finding", MaskUse::Redactor),
    ("checks_k_candidates.rs", "scan_one", MaskUse::Redactor),
    ("checks_k_history.rs", "record_gap", MaskUse::Redactor),
    ("checks_k_provenance.rs", "masked_text", MaskUse::Redactor),
    ("checks_k_structural.rs", "observation", MaskUse::Redactor),
    ("checks_presence.rs", "finding", MaskUse::Redactor),
    (
        "checks_preservation.rs",
        "check_assertion_floor",
        MaskUse::Redactor,
    ),
    ("checks_preservation.rs", "ci_finding", MaskUse::Redactor),
    ("checks_preservation.rs", "rule_finding", MaskUse::Redactor),
    ("checks_process.rs", "action_finding", MaskUse::Redactor),
    ("checks_process.rs", "command_finding", MaskUse::Redactor),
    ("checks_process.rs", "edit_finding", MaskUse::Redactor),
    ("checks_record.rs", "failed_run", MaskUse::Redactor),
    ("checks_record.rs", "path_finding", MaskUse::Redactor),
    ("checks_terms.rs", "finding", MaskUse::Redactor),
    (
        "checks_tests.rs",
        "check_test_present_and_runs",
        MaskUse::Redactor,
    ),
    ("checks_tests.rs", "findings_for", MaskUse::Redactor),
    ("checks_visual.rs", "check_screen", MaskUse::Redactor),
    ("checks_visual.rs", "check_visual_with", MaskUse::Redactor),
    ("checks_visual.rs", "compare_images", MaskUse::Redactor),
    ("decide_cmd.rs", "check_summary", MaskUse::Redactor),
    ("decide_cmd.rs", "bounded", MaskUse::Redactor),
    ("decide_cmd.rs", "mask_json", MaskUse::Redactor),
    ("decide_cmd.rs", "propose_event", MaskUse::Redactor),
    ("decisions.rs", "decision_view", MaskUse::Redactor),
    ("decisions.rs", "error", MaskUse::Redactor),
    ("decisions.rs", "finding_message", MaskUse::Redactor),
    ("decisions.rs", "mask_value", MaskUse::Redactor),
    ("decisions.rs", "start_check", MaskUse::Redactor),
    ("decisions.rs", "verdict_view", MaskUse::Redactor),
    (
        "record_adapter_phase2.rs",
        "project_phase2_at",
        MaskUse::Redactor,
    ),
    ("speaker.rs", "bounded_span", MaskUse::Redactor),
    ("storage.rs", "accept", MaskUse::Redactor),
    ("storage.rs", "append_revision", MaskUse::Redactor),
    // C44 E2: the same span/actor masks now run at the guarded SQL append;
    // a209 checks this one identity migration while retaining the legacy row.
    ("storage.rs", "append_revision_guarded", MaskUse::Redactor),
    ("storage.rs", "create", MaskUse::Redactor),
    ("storage.rs", "mask_json", MaskUse::Redactor),
    ("storage.rs", "masked_sources_json", MaskUse::Redactor),
    ("storage.rs", "record_verdict", MaskUse::Redactor),
    ("storage.rs", "retire", MaskUse::Redactor),
    ("storage.rs", "waive", MaskUse::Redactor),
    ("why.rs", "safe", MaskUse::Redactor),
    // HQ record search (C30 ⌘K): every string it indexes is masked once through
    // `mask_recorded` before anything is kept or cut, and an excerpt masks its
    // input again before the cut. Both only replace; neither decides anything.
    ("search.rs", "build_on", MaskUse::Redactor),
    ("search.rs", "window", MaskUse::Redactor),
    // These C1 tool identities are subprocess stdout that reaches the replay
    // basis and receipts. They now mask before taking a display-length prefix.
    (
        "checks_visual.rs",
        "visual_tool_identity",
        MaskUse::Redactor,
    ),
    ("checks_visual.rs", "cargo_version", MaskUse::Redactor),
    ("checks_visual.rs", "tool_version", MaskUse::Redactor),
];

/// Every site that TRUNCATES text which a masker will later see, with the
/// ordering each one uses.
///
/// The invariant is **mask before truncate**. A209-C3 measured why: cutting
/// first can slice a credential's body below its detector threshold, and
/// the cut string is all any downstream masker ever sees. A 143-character
/// failed command carrying a token past character 105 reached the sealed
/// capsule bytes exactly that way, and a Google key straddling the
/// 280-character opening-ask cut did the same one round earlier. Both were
/// found by probes, not by reading — and the general sentence written after
/// the first one localised the class to "the candidate layer" when
/// `operations.rs` was upstream of both capsules all along.
///
/// `a209_masking_corpus.rs` greps `chars().take(` across the workspace and
/// requires every hit that feeds rendered text to appear here.
/// Truncation sites whose output never reaches a rendered artifact, with
/// the reason each is out of band.
///
/// The census is TOTAL: every `chars().take(` in the workspace is either
/// here or in [`TRUNCATION_SITES`]. A site that cuts an id, a hash prefix,
/// or a display-only fragment cannot split a credential body in a way that
/// reaches a capsule, but it is enumerated rather than judged absent —
/// that judgement is exactly what four rounds of this stage got wrong.
pub const NON_RENDERING_TRUNCATIONS: &[(&str, &str, &str)] = &[
    (
        "check.rs",
        "kept_line",
        "C30 R-RUN live output: the tail's per-line bound (8 KiB of text), applied as \
         bytes arrive; the sheet masks the kept text and only then cuts each line to \
         1 KiB (`run_route.rs::output_json`), so the text around this cut is never served",
    ),
    (
        "storage.rs",
        "id_for_source",
        "takes 16 characters of a SHA-256 digest to form a decision identifier; no text is cut",
    ),
    (
        "hq.rs",
        "index_with",
        "a version id: the first 7 characters of a commit hash, which \
         `hq::commands::commit_hash` admits only as 7-40 hex digits off git's \
         own `[branch abc1234]` line; a hex digest prefix cannot carry a \
         credential body",
    ),
    ("hq.rs", "summary", "top-files Vec truncate, not text"),
    (
        "view.rs",
        "thread_view",
        "a version's `short`: the first 7 characters of the same hex commit \
         hash `hq.rs::index` admits",
    ),
    ("codex_rollout.rs", "import_live_file", "drops an incomplete JSONL record before parsing; never renders a partial field. Admitted complete records still pass bounded() masking. Guard: live_import_waits_for_complete_records_and_masks_before_storage"),
    ("acceptance_exec.rs", "run_controlled", "OpenOptions::truncate(false) preserves a lock file; no string is cut"),
    ("verification_pack.rs", "status", "caps history by whole session IDs and exposes history_has_more; no string is cut"),
    // The core's own two, visible only once the census stopped matching a
    // spelling (DECISIONS #122). Both take the first 12 ALPHANUMERIC characters
    // of a base image digest — a sha256 hex content address — to build a Docker
    // tag. This list's own doc already names the case: "a site that cuts an id,
    // a hash prefix, or a display-only fragment cannot split a credential body".
    //
    // Worth saying plainly: the widened pattern was written for a langpacks site
    // and immediately found two in crates/reexec that had been invisible since
    // the census was written. The spelling gap was never a tracks problem.
    (
        "docker.rs",
        "base_alias",
        "takes 12 alphanumerics of a sha256 base digest for a Docker tag; a hash prefix",
    ),
    (
        "image.rs",
        "tag_for_base",
        "takes 12 alphanumerics of a sha256 base digest for a Docker tag; a hash prefix",
    ),
    // Track G's measurement harness, sites 1 and 2, classified by G and checked
    // here against the merged source rather than against the description.
    //
    // Both cut a `Vec` by WHOLE ELEMENTS — `files.truncate(n)` bounds how many
    // files are measured, `jobs.truncate(4000)` bounds the work queue. Neither
    // cuts inside a string, so neither can split a credential body; there is no
    // rendered fragment for a mask to have been applied to.
    //
    // The census matched them because it looks for `.truncate(`, which is a
    // spelling and not a semantics. That is the same gap D30 records from the
    // other direction.
    (
        "measure.rs",
        "measure_language",
        "caps the file list by whole Vec elements; never cuts inside a string",
    ),
    (
        "measure.rs",
        "mine_renames",
        "caps the rename job queue by whole Vec elements; never cuts inside a string",
    ),
    // Track C's replay-fixture filename, classified by C (R9, D30/#103).
    //
    // C: masking completes before `truncate(180)` runs — sequential, not
    // interleaved — and the result is a filename on C's own disk that is never
    // shown to a user and never enters a receipt, a comment or a check run.
    //
    // Enumerated even though the census can never reach it:
    // `services/github-app` is `exclude`d from the workspace (`Cargo.toml:12`),
    // so the walk does not cover it. A list entry for an unwalked path is a
    // statement by its owner, not a finding by the guard, and D30 says so
    // rather than letting a green census imply the App was checked.
    (
        "transport.rs",
        "key",
        "masks the inputs, then cuts the filename to 180; a local fixture path,          never rendered",
    ),
    // Track C's base64 decoder, enumerated at the merge window (DECISIONS #98).
    //
    // `out.truncate(out.len() - pad)` on a `Vec<u8>`, removing the one or two
    // padding bytes the encoding added. It is not a text cut and cannot split a
    // credential body: the output is the exact decoded bytes, and the only
    // thing removed is padding the decoder itself counted. A rendered fragment
    // never passes through it.
    //
    // Enumerated rather than judged absent, which is the rule this census
    // exists to enforce — and it is here rather than in the incoming track's
    // lane because the list is the core's and no track can clear it. That
    // asymmetry is #98's durable half.
    //
    // **The classification is Track C's, not a reverse-engineering of C's
    // code**, and it agrees with the one reached here independently: C
    // verified the arithmetic cannot underflow (`pad > 2` rejected; `"="`,
    // `"=="`, `"==="` rejected by the incomplete-group check; `"===="` by the
    // pad check), and made the argument that matters — enumerating it into
    // `TRUNCATION_SITES` would demand a masking order for something that masks
    // nothing, leaving the census carrying a false entry, which is worse than
    // the red it showed.
    (
        "fetch.rs",
        "base64_decode",
        "drops base64 padding bytes from a decoded Vec<u8>; not a rendered cut",
    ),
    // A test's own fixture construction, in `capture/src/redaction/tests.rs`.
    // Its output reaches an assertion and nothing else.
    //
    // It is enumerated here rather than stripped because the walk's
    // `#[cfg(test)]` remover works on INLINE blocks, and a `#[cfg(test)] mod
    // tests;` pointing at a separate file under `src/` is invisible to it.
    //
    // **The blind spot is inverted, and harmless — corrected from an earlier
    // note that called it a gap.** Such a module cannot HIDE: the walk covers
    // every member `src/` tree, so an out-of-line test file is forced to be
    // hand-declared exactly like this one. The failure direction is a false
    // POSITIVE, which is the safe one. The real cost is table pollution — a
    // test function now sits in a production constant, which weakens what
    // `NON_RENDERING_TRUNCATIONS` means to a reader — not a missed site.
    (
        "tests.rs",
        "redaction_precedes_bounding_and_the_type_makes_it_the_only_order",
        "test fixture: builds the cut-first string that the type boundary makes          unreachable in production, purely to assert the ordering",
    ),
    // `crates/sync/src/tests.rs`, reached the same way: an out-of-line
    // `#[cfg(test)] mod tests;` the walk's inline-block remover cannot see.
    //
    // It truncates a **ciphertext** to assert that a cut envelope fails its
    // AEAD tag rather than yielding a prefix of the plaintext. Nothing renders
    // it and nothing masks it — the bytes are already indistinguishable from
    // random to anything without the key, and the assertion is that they stay
    // unreadable. Enumerated rather than judged absent, which is the rule this
    // census exists to enforce.
    (
        "tests.rs",
        "a_tampered_envelope_never_yields_partial_plaintext",
        "test fixture: cuts a ciphertext to assert the AEAD refuses it instead of \
         returning a plaintext prefix; the bytes never reach a renderer",
    ),
    // Identifier and digest abbreviations: the input is a hex/ULID/UUID
    // this product publishes, so no credential can be split by them.
    (
        "explain_cmd.rs",
        "short_hash",
        "abbreviates a git tree hash for the timeline",
    ),
    (
        "binding.rs",
        "short",
        "abbreviates a git tree hash inside a §6.1 binding explanation",
    ),
    ("capsule_v2.rs", "head", "abbreviates a digest for display"),
    ("continuity.rs", "short", "abbreviates a hash for display"),
    ("verifier.rs", "claim_display_id", "abbreviates a call id"),
    ("speaker.rs", "short_display_id", "abbreviates a hex digest"),
    (
        "codex.rs",
        "safe_token",
        "path-component sanitiser over a fixed alphabet",
    ),
    (
        "premise.rs",
        "leading_verb",
        "reads a six-character verb head to classify",
    ),
    (
        "premise.rs",
        "clip",
        "premise rendering; not a capsule surface",
    ),
    (
        "premise.rs",
        "tally_prefix_harness",
        "test-harness helper for prefix tallies",
    ),
    // Other rendered surfaces — enumerated, NOT claimed safe. These cut
    // transcript-derived text for the graph, speaker and CLI surfaces,
    // which are outside §34's capsule scope but inherit the same
    // mask-before-cut question. Named here so a later stage can act on
    // them rather than rediscovering them (A209-C4/B5).
    (
        "derive.rs",
        "excerpt",
        "graph claim text; non-capsule surface",
    ),
    (
        "derive.rs",
        "edit_content",
        "graph edit content; non-capsule surface",
    ),
    (
        "derive.rs",
        "result_excerpt",
        "graph tool-result text; non-capsule surface",
    ),
    (
        "derive.rs",
        "claim_state",
        "state reason string; non-capsule surface",
    ),
    (
        "speaker.rs",
        "truncate",
        "speaker digest line; non-capsule surface",
    ),
    (
        "verifier.rs",
        "truncate",
        "verdict display; non-capsule surface",
    ),
    (
        "mind_grounded.rs",
        "truncate_chars",
        "model-facing text, masked by safe_text/push_fact before this cut",
    ),
    (
        "mind_grounded.rs",
        "safe_relative",
        "relative path for model context; masked upstream",
    ),
    (
        "main.rs",
        "print_eval",
        "CLI display of already-processed text",
    ),
    ("main.rs", "speaker_budgeted", "CLI display budget"),
    // `.truncate(` on a COLLECTION, or a file-open flag — not a text cut.
    // Each was read individually (A209-C5/B7-9 widened the sweep to this
    // idiom); none can split a credential, because none touches a string.
    (
        "annotate.rs",
        "backward_lexical_candidates",
        "Vec truncate, not text",
    ),
    (
        "annotate.rs",
        "forward_candidates",
        "Vec truncate, not text",
    ),
    ("annotate.rs", "gate_tokens", "token Vec truncate, not text"),
    (
        "annotate.rs",
        "retrieval_tokens",
        "token Vec truncate, not text",
    ),
    (
        "capture.rs",
        "unsafe_free_effective_uid",
        "OpenOptions::truncate file flag",
    ),
    (
        "mind_grounded.rs",
        "catalog_from_export",
        "row Vec truncate, not text",
    ),
    ("nli.rs", "score", "token-id Vec truncate, not text"),
    (
        "premise.rs",
        "claim_focus_terms",
        "term Vec truncate, not text",
    ),
    (
        "premise.rs",
        "ratio_with_pass_word",
        "term Vec truncate, not text",
    ),
    ("speaker.rs", "compose", "line Vec truncate, not text"),
    (
        "verifier.rs",
        "refresh_claim_lines",
        "line Vec truncate, not text",
    ),
];

/// How a [`ConditionalTruncation`]'s condition is checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TruncationGuard {
    /// The manifest at this workspace-relative path declares no dependency that
    /// can carry captured data — the ledger, the capture layer, or core.
    ///
    /// **This checks LINKAGE, not input** (DECISIONS #119), and the distinction
    /// is Track G's. I proposed it as a guard on input provenance and G refuted
    /// it: `measure.rs` takes `--corpus <path>` and reads through `std::fs`, so
    /// pointing it at a spool needs no dependency, no code change and no crate
    /// — one flag. G verified the closure really is `{thruline-verify}`, so the
    /// check is TRUE; it is true about something adjacent to the claim.
    ///
    /// A guard that checks something next to the condition reads as coverage
    /// and is worse than none, so an entry relying on it uses
    /// [`TruncationGuard::UnguardedWithLinkageCheck`] instead and is counted
    /// unguarded.
    /// The condition is held by a named test, which the census checks EXISTS in
    /// the tree — it does not re-run it; the workspace gate already does.
    ///
    /// **An entry whose test is not here yet reports UNGUARDED, by name, with
    /// that reason** — and flips to guarded the moment the test arrives, with no
    /// hand edit. That is the point: a guard living on another track's branch is
    /// a promise until it lands, and the census says which it is rather than
    /// leaving a reader to assume.
    TestExists {
        file: &'static str,
        func: &'static str,
    },
    /// No mechanical guard exists; the condition is an argument a person made.
    ///
    /// The census **prints the count and names of these every run** (DECISIONS
    /// #114, constraint 3). An unguarded entry is a promise, and a promise that
    /// is never read is how a list rots — Rule 12 applied to the list itself.
    Unguarded,
}

/// A cut that reaches a person, is **not** masked first, and is safe because of
/// what its input is.
///
/// # Why a third list exists (a209 taxonomy amendment, DECISIONS #114)
///
/// [`TRUNCATION_SITES`] means *credential* mask-then-cut, enforced mechanically:
/// the census refuses an entry with no `redact::mask` before the cut.
/// [`NON_RENDERING_TRUNCATIONS`] means the cut never reaches a person. **Two
/// tracks hit the same gap on the same day, independently**, with sites that are
/// neither — rendering, unmasked, and safe because the input cannot carry a
/// credential.
///
/// Track G named the principle and it is the reason this list stores a
/// *condition* rather than a site: *"tie the declaration to the input
/// condition … if the binary ever gains a ledger/spool/session input, the cut
/// is exactly the shape the census exists to catch."* A site is safe today; a
/// condition stays checkable.
///
/// **The census's claim gets NARROWER, not wider** — from "every cut is
/// masked-first or non-rendering" to "or conditionally safe, condition stated,
/// guarded or flagged". Nothing already declared gets looser: the other two
/// lists are untouched.
#[derive(Debug, Clone, Copy)]
pub struct ConditionalTruncation {
    pub file: &'static str,
    pub func: &'static str,
    /// What the cut removes, in the site's own terms.
    pub cuts: &'static str,
    /// **Why the input cannot carry a credential.** The load-bearing field.
    pub condition: &'static str,
    /// A manifest whose `[dependencies]` must name no crate that can carry
    /// captured data. **Always checked when present, and never a guard on the
    /// condition** (DECISIONS #119).
    ///
    /// Track G refuted my use of it as one: `measure.rs` takes `--corpus
    /// <path>` and reads through `std::fs`, so pointing it at a spool needs no
    /// dependency at all. The check is TRUE and it is true about linkage, which
    /// is adjacent to the claim — so it runs on its own axis and the entry's
    /// guard is judged separately.
    pub linkage: Option<&'static str>,
    pub guard: TruncationGuard,
}

/// The conditionally-safe cuts, by condition.
///
/// Empty until an owner states a condition. `comment.rs::inert` is deliberately
/// **absent**: Track C called its markup escaping "masking", and neither the
/// core nor the coordinator knows whether its input is receipt-derived and
/// masked at ingest or free text from a pull request. It enters when C states
/// the condition, `Unguarded` if that is the truth.
pub const CONDITIONAL_TRUNCATIONS: &[ConditionalTruncation] = &[
    ConditionalTruncation {
        file: "measure.rs",
        func: "one_line",
        cuts: "collapses a source line to `cap` characters and appends an ellipsis, \
           for the measurement report a developer reads",
        // The load-bearing field, and it is G's own condition rather than a
        // reconstruction of it.
        condition: "`measure.rs` is a [[bin]] in crates/langpacks that reads only the \
                public checkouts named in MEASUREMENTS.toml. It is not reachable \
                from the product binary and never opens a ledger, a spool or a \
                session, so the text it cuts cannot carry a credential.",
        // The condition is held by G's own hardening test, which refuses a
        // `--corpus` under a captured-data root — the thing the linkage check
        // could never see. Named rather than waited for: it prints UNGUARDED
        // until G's entry lands and flips itself then, with no hand edit.
        linkage: Some("crates/langpacks/Cargo.toml"),
        guard: TruncationGuard::TestExists {
            file: "crates/langpacks/tests/measure_refuses_captured_input.rs",
            func: "a_corpus_under_a_captured_data_root_is_refused",
        },
    },
    ConditionalTruncation {
        file: "comment.rs",
        func: "inert",
        cuts: "collapses a PR-comment cell to 200 characters and appends an ellipsis",
        // Track C's condition, and the half C's FIRST answer omitted is the half
        // that matters: nothing in the App redacted any of it. The receipt arrives
        // over the network and `thruline_core`'s redaction never touched those
        // bytes in this process. So the condition is **"only fields whose shape is
        // ours are read"**, not "the bytes are clean" — which is why this is not a
        // mask-then-cut site and no fix is owed in C's lane.
        condition: "every string reaching `inert` is receipt- and verifier-derived \
                structured content: ULID claim ids, thruline-attest's own check \
                names and sentences, the claim KIND, a &'static str rule, and \
                policy-generated Reason.detail strings that interpolate no path \
                and no free text. `receipt.rs` never reads claims[].text, \
                session, or promises[].text, so the agent's prose is never \
                rendered.",
        linkage: None,
        guard: TruncationGuard::TestExists {
            file: "crates/app-core/tests/ruling.rs",
            func: "the_agents_free_text_never_reaches_the_rendered_comment",
        },
    },
    ConditionalTruncation {
        file: "measure.rs",
        func: "head_token",
        cuts: "takes the leading identifier of an unmatched site, 48 characters, \
               for the coverage diagnosis printed at measure.rs:1289",
        // Track G classified this in R11 and the census could not see it: the
        // chain flattens to `.chars().take_while(…).take(48)` and the census
        // matched the literal `.chars().take(`. Reading found it and measuring
        // did not; its neighbour `one_line` was the reverse. Same file, same
        // input, same condition — so it carries the same marks rather than a
        // second argument.
        condition: "same as `measure.rs::one_line`: a [[bin]] in crates/langpacks \
                    that reads only the public checkouts named in \
                    MEASUREMENTS.toml, unreachable from the product binary.",
        linkage: Some("crates/langpacks/Cargo.toml"),
        guard: TruncationGuard::TestExists {
            file: "crates/langpacks/tests/measure_refuses_captured_input.rs",
            func: "a_corpus_under_a_captured_data_root_is_refused",
        },
    },
];

pub const TRUNCATION_SITES: &[(&str, &str, &str)] = &[
    (
        "speaker.rs",
        "bounded_span",
        "masks the accepted Decisions span before its split char iterator takes 96",
    ),
    (
        "decide_cmd.rs",
        "bounded",
        "masks CLI Decisions summary text before its split char iterator takes the display budget",
    ),
    (
        "checks_visual.rs",
        "visual_tool_identity",
        "masks renderer version stdout, then cuts to 120 for the C1 replay basis",
    ),
    (
        "checks_visual.rs",
        "cargo_version",
        "masks cargo version stdout, then cuts to 120 for the C1 replay basis",
    ),
    (
        "checks_visual.rs",
        "tool_version",
        "masks tool version stdout, then cuts to 120 for the C1 replay basis",
    ),
    (
        "decisions.rs",
        "finding_message",
        "masks a Decisions HTTP finding location, then cuts to 160",
    ),
    (
        "hq.rs",
        "first_line",
        "HQ: masks a test command, then cuts its first line to 200 for the \
         thread summary",
    ),
    (
        "prompts.rs",
        "cut_title",
        "HQ: masks the person's title sentence, then cuts it to 90",
    ),
    (
        "view.rs",
        "excerpt",
        "HQ: masks the last 200 lines of a command's output as one text, then \
         keeps 40 and cuts each to 400",
    ),
    (
        "view.rs",
        "clip",
        "HQ session timeline: masks a tool's input or a Codex program, then \
         cuts it to 2,000",
    ),
    (
        "run_route.rs",
        "output_json",
        "C30 R-RUN live output: masks the printed lines as one text, then cuts each to \
         1 KiB; the tail keeps more of a line than is served, so a secret is whole when \
         it is masked",
    ),
    (
        "search.rs",
        "build_on",
        "HQ record search: masks every string it indexes (once, through \
         mask_recorded) before anything is kept or cut",
    ),
    (
        "search.rs",
        "window",
        "HQ record search: masks its input again, then cuts a short excerpt around \
         the match",
    ),
    ("integration_cmd.rs", "execute", "masks non-secret-scanner diagnostics before marker-preserving truncation; secret-scanner diagnostics are withheld"),
    (
        "explain_cmd.rs",
        "truncate",
        "masks the ledger-sourced line, then cuts it for the §10 timeline",
    ),
    (
        "operations.rs",
        "derive",
        "masks the normalized command, then cuts to 120",
    ),
    (
        "derive.rs",
        "truncate_words",
        "masks, then cuts to the word budget",
    ),
    (
        "candidates.rs",
        "derive",
        "masks the opening ask, then cuts to 280",
    ),
    (
        "candidates.rs",
        "extract_commit_message",
        "masks the message, then cuts to 120",
    ),
    (
        "fold.rs",
        "compact",
        "§4.6: masks (idempotently — the spool line arrived redacted) then cuts \
         to 2,000",
    ),
    (
        "tui_cmd.rs",
        "truncate",
        "§10: masks (idempotently — the payload arrived redacted through the \
         spool) then cuts a detail-pane field to 400",
    ),
    (
        "codex_rollout.rs",
        "bounded",
        "§4.2: masks a rollout's text — the FIRST mask these bytes have seen, \
         since the file is the vendor's own — then cuts to 4,000",
    ),
    (
        "uiserve.rs",
        "bounded_stderr",
        "the `thruline ui` JSON API: masks a failed projection's subprocess \
         stderr — the FIRST mask these bytes have seen, since they never went \
         through the spool — then cuts to 2,000 for the 502 body \
         `docs/UI-SERVER-CONTRACT.md` §2.3 publishes to Track A's web UI. It \
         arrived here cutting FIRST and not masking at all",
    ),
    (
        "redaction.rs",
        "bounded",
        "§4.6: masks (idempotently — the text arrived redacted) then cuts. The \
         ordering is ALSO a type: `Bounded` is constructible only from \
         `Redacted`, so an unredacted string cannot reach this cut at all",
    ),
];

/// What ran, and what it did.
///
/// Two states, distinguishable **by type** (D3), because "no detector ran" and
/// "a detector ran and matched nothing" are different facts and the gate must
/// treat them differently. `Default` is [`Treatment::NoDetectorRan`], so a
/// capsule deserialized without a treatment field — every pre-A177 artifact,
/// and every foreign producer — fails toward consent rather than away from it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Treatment {
    /// Nothing scanned this artifact. The gate is CLOSED in this state.
    NoDetectorRan { statement: String },
    /// A detector ran. `masked == 0` means it found nothing, which is a real
    /// and different claim from having never looked.
    Ran {
        detector: String,
        masked: u64,
        patterns: Vec<String>,
        statement: String,
    },
    /// A detector ran somewhere, but this artifact cannot account for what
    /// it did — provenance was lost in transit. **The gate is CLOSED.**
    ///
    /// A209-C4: the previous design parsed `[masked: X]` markers back out
    /// of the text to reconstruct a record. That is forgeable (prose
    /// containing the literal became a "detection") and truncation-fragile
    /// (a cut inside a marker erased the evidence AND opened the consent
    /// gate). Unknown provenance is now a state, not a guess.
    Unknown { reason: String },
}

impl Default for Treatment {
    fn default() -> Self {
        Treatment::NoDetectorRan {
            statement: "no detector ran over this artifact — it may carry anything the \
                        recorded sessions carried; review before sharing or injecting"
                .to_string(),
        }
    }
}

impl Treatment {
    /// A detector ran and matched nothing. NOT the same as [`Default`].
    pub fn none() -> Self {
        Treatment::Ran {
            detector: DETECTOR_VERSION.to_string(),
            masked: 0,
            patterns: Vec::new(),
            statement: format!(
                "detector {DETECTOR_VERSION} · nothing matched a known secret shape · \
                 best-effort, not a guarantee: this artifact carries verbatim prose and \
                 command text, and a secret in a shape the detector does not know is still here"
            ),
        }
    }

    fn ran(masked: u64, patterns: BTreeSet<String>) -> Self {
        if masked == 0 {
            return Treatment::none();
        }
        let patterns: Vec<String> = patterns.into_iter().collect();
        Treatment::Ran {
            detector: DETECTOR_VERSION.to_string(),
            masked,
            statement: format!(
                "detector {DETECTOR_VERSION} · {masked} masked · patterns: {} · best-effort, \
                 not a guarantee: a secret in a shape the detector does not know is still here",
                patterns.join(", ")
            ),
            patterns,
        }
    }

    pub fn masked(&self) -> u64 {
        match self {
            Treatment::NoDetectorRan { .. } | Treatment::Unknown { .. } => 0,
            Treatment::Ran { masked, .. } => *masked,
        }
    }

    /// An unknown-provenance treatment, which always blocks injection.
    pub fn unknown(reason: impl Into<String>) -> Self {
        Treatment::Unknown {
            reason: reason.into(),
        }
    }

    pub fn statement(&self) -> &str {
        match self {
            Treatment::NoDetectorRan { statement } | Treatment::Ran { statement, .. } => statement,
            Treatment::Unknown { .. } => {
                "what this artifact carries is not known: a masking pass ran upstream but its \
                 record did not survive, so no claim is made about what was found — review \
                 before sharing or injecting"
            }
        }
    }

    /// Whether a human must acknowledge before this artifact is injected into
    /// another model's session (C4.2's gate).
    ///
    /// **The gate's default state is closed** (D3): it fires when anything was
    /// masked, when no detector ran, and when a producer claims a run but
    /// names no detector. The input most likely to be unscanned is exactly the
    /// input that must not open it.
    pub fn needs_acknowledgement(&self) -> bool {
        match self {
            // Fail closed: never scanned, and scanned-but-unaccountable,
            // both require a human before this reaches another model.
            Treatment::NoDetectorRan { .. } | Treatment::Unknown { .. } => true,
            Treatment::Ran {
                masked, detector, ..
            } => *masked > 0 || detector.trim().is_empty(),
        }
    }
}

struct Detector {
    name: &'static str,
    prefixes: &'static [&'static str],
    /// How many body characters must follow the prefix before this is a
    /// credential SHAPE rather than a word that starts the same way, and
    /// which characters count as body (A209, second repair).
    ///
    /// Without this, widening the prefix scan to word boundaries masked 38
    /// real `data-sk-*` HTML attributes out of one live project — measured,
    /// not predicted. `sk-` is three characters and `ASIA` is an English
    /// word; a prefix alone carries no secret. The minimums follow the real
    /// vendor formats, and independently match the scrubbing regexes the
    /// user's own corpus already contains
    /// (`sk-(proj-)?[A-Za-z0-9_-]{16,}`, `gh[pousr]_[A-Za-z0-9]{20,}`,
    /// `(AKIA|ASIA)[A-Z0-9]{16}`).
    ///
    /// `body_min: 0` exempts a prefix that IS the whole signal —
    /// `-----BEGIN` has no body and must keep firing on its own.
    body_min: usize,
    body_char: fn(u8) -> bool,
}

fn body_alnum_underscore(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}
fn body_upper_alnum(b: u8) -> bool {
    b.is_ascii_uppercase() || b.is_ascii_digit()
}
fn body_token(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}
fn body_dashed(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}

/// Known formats. A detector fires at any word boundary in any position,
/// and only when a plausible BODY follows the prefix. Each name states what
/// was MATCHED (a shape), never what was decided (a live credential).
///
/// **`private_key_block` is deliberately absent from this table** (A209-C3).
/// It lived here with `body_min: 0`, so the token walk fired on the bare
/// literal `-----BEGIN` wherever it appeared — masking `-----BEGIN BID-----`
/// (a construction-bid delimiter, **70 of 76,203** live opening asks),
/// `-----BEGIN CERTIFICATE-----` (public material, 29 live events), a prose
/// mention, and a grep pattern, each reported as `private_key_block`: the
/// context destruction D1 exists to prevent AND the false-treatment fault
/// that made the block defect blocking in the first place. A PEM block is a
/// BLOCK, not a token, so [`mask_pem_blocks`] is now its sole owner and
/// requires the banner to actually name key material.
/// Families whose detection is **structural**, and therefore needs no second
/// opinion.
///
/// A209-C12's own conclusion: *a terminated PEM block is self-identifying and
/// needs no payload corroboration*. [`mask_pem_blocks`] already refuses every
/// look-alike A209-C3 enumerated — a certificate, a `-----BEGIN BID-----`
/// construction delimiter, a prose mention and a grep pattern all produce **no
/// family at all**, because the banner has to name key material. The controls
/// are inside the matcher, so a caller asking "did this fire?" already has the
/// answer.
///
/// This exists for the one caller that uses masking as a **refutation gate**
/// (§6.6 `secrets_absent`). That gate takes the conjunction of the masker
/// firing and [`resembles_known_credential`], because a false positive there
/// wrongly accuses a developer — but `resembles_known_credential` is a *prefix*
/// test, and a private key has no prefix to test. The conjunction therefore
/// dropped **the most serious thing `secrets_absent` exists to catch**: a
/// committed private key was masked, reported as `private_key_block`, and not
/// accused.
///
/// Structural families satisfy the gate alone. Opaque and context-derived ones
/// (`env_value`, `authorization_header`, `structured_credential`) keep the
/// conjunction, because those are the ones a key name can be wrong about.
pub const STRUCTURAL_FAMILIES: &[&str] = &["private_key_block"];

/// PEM labels that pass the masker's key-material test but must **never
/// accuse**.
///
/// `EC PARAMETERS` carries a curve OID and no private material. A209-C12 put it
/// on the masker's allowlist because over-masking is free for a redactor; it is
/// not free for an accuser.
pub const NON_ACCUSING_PEM_LABELS: &[&str] = &["EC PARAMETERS"];

/// Does `text` contain a PEM block a **refutation gate** may act on?
///
/// # The polarity boundary this function exists to hold
///
/// **Over-masking is fail-safe for a redactor and fail-dangerous for an
/// accuser.** `mask_pem_blocks` deliberately fires on a terminated
/// private-key banner whatever follows it, and A209-C12 disclosed the residual
/// that buys: *"terminated private-key blocks with genuine labels and
/// placeholder bodies, which a redactor cannot distinguish from a key without
/// reading the payload."* Fourteen such rows were an accepted **over-mask**.
///
/// §6.6's `secrets_absent` then exempted structural families from its second
/// conjunct — correctly, because a private key has no prefix for
/// [`resembles_known_credential`] to test — and in doing so moved those
/// fourteen rows across the boundary, turning an accepted over-mask into a
/// **false accusation** of a developer.
///
/// So the accuser asks a stricter question than the masker, and asks it here
/// rather than by weakening the masker: a block accuses only when its label
/// names private material (excluding [`NON_ACCUSING_PEM_LABELS`]) **and** its
/// body is shaped like real key material — base64 alphabet, at least 32
/// characters, and a mix of cases and digits, which DER-encoded key material
/// always has and a shouty placeholder does not.
///
/// The masker keeps masking exactly as it does. Only the accusation is gated.
pub fn contains_accusable_key_block(text: &str) -> bool {
    let upper = text.to_ascii_uppercase();
    let mut from = 0usize;
    while let Some(rel) = upper[from..].find("-----BEGIN ") {
        let start = from + rel + "-----BEGIN ".len();
        from = start;
        let Some(end_of_label) = upper[start..].find("-----") else {
            continue;
        };
        let label = upper[start..start + end_of_label].trim().to_string();
        let body_start = start + end_of_label + 5;
        // Everything up to the closing banner, or to the end.
        let body_end = upper[body_start..]
            .find("-----END")
            .map(|i| body_start + i)
            .unwrap_or(upper.len());
        if body_start >= body_end {
            continue;
        }
        if !label_names_private_material(&label) {
            continue;
        }
        if body_is_key_shaped(
            &text[body_start.min(text.len())..body_end.min(text.len())],
            &label,
        ) {
            return true;
        }
    }
    false
}

/// A label that names *private* material, for accusation purposes.
fn label_names_private_material(label: &str) -> bool {
    if NON_ACCUSING_PEM_LABELS.iter().any(|l| label.contains(l)) {
        return false;
    }
    label.contains("PRIVATE KEY")
        || label.contains("ENCRYPTED KEY")
        || label.contains("PGP MESSAGE")
}

/// Is this body **actually** key material?
///
/// A209-C12's own conclusion, one level further. The first version of this
/// asked a *shape* question — base64 alphabet, ≥32 characters, mixed case and
/// digits — which is a length-and-alphabet proxy. Three constructed non-keys
/// still passed it: base64 of ordinary prose under a key banner, an
/// `OPENSSH PRIVATE KEY` banner over a non-key blob, and base64 of a git SHA.
///
/// So the body is **decoded** and the bytes are asked what they are:
/// `openssh-key-v1\0` for OpenSSH, an OpenPGP packet header for a PGP message,
/// and otherwise a DER `SEQUENCE` whose first element is one a private key
/// structure actually begins with — a one-byte version `INTEGER` (PKCS#1,
/// PKCS#8, SEC1) or an `AlgorithmIdentifier` `SEQUENCE` starting with an `OID`
/// (encrypted PKCS#8).
///
/// **Truncation still accuses**, because a truncated key is a key: the declared
/// DER length is not required to be present. Only the first few bytes are
/// decoded, so this costs nothing.
fn body_is_key_shaped(body: &str, label: &str) -> bool {
    let compact: String = body.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.len() < 32 {
        return false;
    }
    if !compact
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=')
    {
        return false;
    }
    // Eighteen bytes: enough for `openssh-key-v1\0` (fifteen) with room to
    // spare, and far more than the seven a DER header needs. Twelve was not —
    // it stopped one byte short of the OpenSSH magic, which the suite caught.
    let decoded = base64_prefix(&compact, 24);
    decoded_is_private_key_material(&decoded, label)
}

/// Decode the leading whole 4-character groups of a base64 string.
///
/// Deliberately tiny and local: `crates/core` has no base64 dependency, and
/// adding one to read twelve bytes inside a redactor would put a decoder in the
/// trusted path for no gain.
fn base64_prefix(compact: &str, chars: usize) -> Vec<u8> {
    fn value(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some(u32::from(c - b'A')),
            b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let bytes = compact.as_bytes();
    let take = chars.min(bytes.len()) / 4 * 4;
    let mut out = Vec::with_capacity(take / 4 * 3);
    for group in bytes[..take].chunks(4) {
        let mut acc = 0u32;
        for &c in group {
            match value(c) {
                Some(v) => acc = (acc << 6) | v,
                // Padding or anything else ends the decodable prefix.
                None => return out,
            }
        }
        out.push((acc >> 16) as u8);
        out.push((acc >> 8) as u8);
        out.push(acc as u8);
    }
    out
}

/// Do these decoded bytes begin a private-key structure?
fn decoded_is_private_key_material(bytes: &[u8], label: &str) -> bool {
    // OpenSSH's own container format, magic first.
    if bytes.starts_with(b"openssh-key-v1\0") {
        return true;
    }
    if label.contains("PGP MESSAGE") {
        // RFC 4880 §4.2: every packet header's high bit is set. ASCII prose
        // and hex never has it.
        return bytes.first().is_some_and(|b| b & 0x80 != 0);
    }
    // DER. `SEQUENCE`, a length, then the first element.
    if bytes.first() != Some(&0x30) {
        return false;
    }
    let Some((after_len, _)) = der_length(bytes, 1) else {
        return false;
    };
    match bytes.get(after_len) {
        // A version INTEGER: one byte, value 0..=3. Every PKCS#1, PKCS#8 and
        // SEC1 private key starts with one. Requiring the length and value —
        // rather than just the tag — is what rejects base64 of a hex digest,
        // whose bytes are ASCII and happen to land on plausible tags.
        Some(&0x02) => {
            bytes.get(after_len + 1) == Some(&0x01)
                && bytes.get(after_len + 2).is_some_and(|v| *v <= 0x03)
        }
        // An AlgorithmIdentifier: `SEQUENCE { OID … }`, as encrypted PKCS#8
        // begins.
        Some(&0x30) => der_length(bytes, after_len + 1)
            .is_some_and(|(inner, _)| bytes.get(inner) == Some(&0x06)),
        _ => false,
    }
}

/// Read a DER length at `at`, returning the offset just past it.
///
/// The declared length itself is returned but deliberately **not** checked
/// against what is present: a truncated key is still a key.
fn der_length(bytes: &[u8], at: usize) -> Option<(usize, usize)> {
    match bytes.get(at)? {
        b if *b < 0x80 => Some((at + 1, *b as usize)),
        0x81 => Some((at + 2, *bytes.get(at + 1)? as usize)),
        0x82 => Some((
            at + 3,
            (*bytes.get(at + 1)? as usize) << 8 | *bytes.get(at + 2)? as usize,
        )),
        0x83 => Some((
            at + 4,
            (*bytes.get(at + 1)? as usize) << 16
                | (*bytes.get(at + 2)? as usize) << 8
                | *bytes.get(at + 3)? as usize,
        )),
        _ => None,
    }
}

const DETECTORS: &[Detector] = &[
    Detector {
        name: "aws",
        prefixes: &["AKIA", "ASIA"],
        // A real access-key id is the prefix plus 16 uppercase base32
        // characters. `ASIA` on its own is an English word, and `_` or `.`
        // ends the body — which is what keeps `REGION_ASIA_PACIFIC` and
        // `regions/ASIA.json` out.
        body_min: 16,
        body_char: body_upper_alnum,
    },
    Detector {
        name: "github",
        // p=personal, o=oauth, u=user-to-server, s=server-to-server,
        // r=refresh. `ghu_` and `ghr_` were MISSING from A178 through
        // A209-C — undisclosed in every exemption list — while the body
        // predicate's own doc comment three lines below quoted the
        // operator's harvested regex `gh[pousr]_[A-Za-z0-9]{20,}`, which
        // names all five. The evidence was in hand and the table did not
        // match it.
        prefixes: &["ghp_", "gho_", "ghu_", "ghs_", "ghr_", "github_pat_"],
        body_min: 20,
        body_char: body_alnum_underscore,
    },
    Detector {
        name: "openai",
        prefixes: &["sk-"],
        // The measured one. `sk-` is three characters and occurs inside
        // ordinary markup: 38 real `data-sk-*` attributes from one live
        // project masked as `openai` before this bound existed.
        body_min: 16,
        body_char: body_token,
    },
    Detector {
        name: "anthropic",
        prefixes: &["sk-ant-"],
        body_min: 16,
        body_char: body_token,
    },
    Detector {
        name: "slack",
        prefixes: &["xoxb-", "xoxp-", "xapp-"],
        body_min: 10,
        body_char: body_dashed,
    },
    Detector {
        name: "google",
        prefixes: &["AIza"],
        // API keys are 39 characters (`AIza` plus 35), but the threshold
        // is 20, not 35: `AIza` is not an English fragment, so any
        // substantial alphanumeric body is already a key shape, and a
        // 30-char floor made the detector miss a key TRUNCATED by an
        // upstream renderer (measured live — see the truncation miss
        // below). The corpus's nearest false positive, `Kaizala-AIza-mock`,
        // carries a 5-character body and stays clear by a wide margin.
        body_min: 20,
        body_char: body_token,
    },
    Detector {
        name: "jwt",
        prefixes: &["eyJ"],
        // The base64 of a JWT header, up to the first `.`.
        body_min: 12,
        body_char: body_token,
    },
];

/// Flags whose following token is a value by construction (D4). A token here
/// is masked on length alone — a service ref carries no distinguishing shape,
/// which is why the measured spill went unseen by entropy.
const SECRET_FLAGS: &[&str] = &[
    "--project-ref",
    "--token",
    "--api-key",
    "--apikey",
    "--password",
    "--secret",
    "--auth",
    "--access-token",
];

const ENTROPY_MIN_LEN: usize = 32;
const FLAG_VALUE_MIN_LEN: usize = 12;

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | '/' | '=' | '.')
}

fn entropy(s: &str) -> f64 {
    let mut counts = [0usize; 256];
    for b in s.as_bytes() {
        counts[*b as usize] += 1;
    }
    let len = s.len() as f64;
    -counts
        .iter()
        .filter(|c| **c > 0)
        .map(|c| {
            let p = *c as f64 / len;
            p * p.log2()
        })
        .sum::<f64>()
}

/// The identifiers this product publishes BY DESIGN. See "Known limitations".
fn is_published_identifier(token: &str) -> bool {
    let hex = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit());
    if hex(token) {
        return true;
    }
    let groups: Vec<&str> = token.split('-').collect();
    if groups.len() == 5
        && [8, 4, 4, 4, 12] == groups.iter().map(|g| g.len()).collect::<Vec<_>>()[..]
        && groups.iter().all(|g| hex(g))
    {
        return true;
    }
    token.len() == 26
        && token.chars().all(|c| {
            c.is_ascii_digit()
                || (c.is_ascii_uppercase() && c != 'I' && c != 'L' && c != 'O' && c != 'U')
        })
}

/// A filename: something ending in a short alphabetic extension. Found by the
/// live corpus regression — a quoted PDF path in a real opening ask was masked
/// because one long filename segment cleared the entropy bar. A structured
/// name is not an opaque blob, and D1's harm does not stop being D1's harm
/// because the path happened to sit inside quotes.
fn is_filename_like(segment: &str) -> bool {
    match segment.rsplit_once('.') {
        Some((stem, ext)) => {
            !stem.is_empty()
                && (1..=5).contains(&ext.len())
                && ext.chars().all(|c| c.is_ascii_alphabetic())
        }
        None => false,
    }
}

fn high_entropy(token: &str) -> bool {
    if token.len() < ENTROPY_MIN_LEN || is_published_identifier(token) || is_filename_like(token) {
        return false;
    }
    let has_digit = token.chars().any(|c| c.is_ascii_digit());
    let has_alpha = token.chars().any(|c| c.is_ascii_alphabetic());
    has_digit && has_alpha && entropy(token) > 3.6
}

/// Where a token sits, which decides whether entropy may fire (D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Position {
    /// An assignment right-hand side, a quoted lone token, or a URL credential.
    Value,
    /// The token immediately follows a secret-bearing flag.
    FlagValue,
    /// Anything else: prose, a path, a filename, a bare word.
    Prose,
}

/// Classify a token's position from the text around it.
fn position_of(before: &str, token: &str, after: &str) -> Position {
    let prev_word = before.split_whitespace().next_back().unwrap_or("");
    if SECRET_FLAGS.contains(&prev_word) {
        return Position::FlagValue;
    }
    let trimmed = before.trim_end();
    if trimmed.ends_with('=') || trimmed.ends_with(':') {
        return Position::Value;
    }
    let quoted = matches!(before.chars().last(), Some('"') | Some('\'') | Some('`'))
        && matches!(after.chars().next(), Some('"') | Some('\'') | Some('`'));
    if quoted {
        return Position::Value;
    }
    // URL credential position: `//user:pass@host`.
    if after.starts_with('@') && token.contains('/') {
        return Position::Value;
    }
    Position::Prose
}

/// The longest known-format prefix occurring at a **word boundary** inside
/// `token`, with the detector that owns it.
///
/// A boundary is the token's start, or any position whose preceding byte is
/// not ASCII-alphanumeric — so `_`, `.`, `-`, `/`, `@`, `=`, `:` and `+` all
/// open one. This is the A209 repair (measured miss below): the previous
/// rule split the token on `/@=:` only and required `starts_with`, so `_`
/// and `.` — both token characters — hid a known prefix completely.
/// `/tmp/repo/secrets_ghp_….txt` returned **unmasked**, and a capsule's
/// unresolved-work lines are built from exactly such paths.
///
/// The boundary condition is what keeps the repair from re-committing
/// A178's harm. A bare `contains` would mask `task-list.md`, `disk-usage.txt`
/// and `monkeyJump.tsx` — `sk-` and `eyJ` sit inside ordinary words — and
/// destroying real filenames in the sections a fresh session reads first is
/// the certain harm D1 was written to stop. A prefix buried mid-run is a
/// word that happens to contain those letters; a prefix after a separator
/// is a credential shape.
///
/// Byte-wise on purpose: tokens are ASCII by construction
/// ([`is_token_char`]), and comparing bytes cannot panic on any input.
fn known_format_prefix(token: &str) -> Option<(&'static str, usize)> {
    known_prefix_at_boundary(token, true)
}

/// The shared boundary scan. `require_body` distinguishes the redactor's
/// threshold (a credential shape needs a body) from the validity gates'
/// (a bare prefix is already disqualifying) — see
/// [`resembles_known_credential`].
fn known_prefix_at_boundary(token: &str, require_body: bool) -> Option<(&'static str, usize)> {
    let bytes = token.as_bytes();
    let mut best: Option<(&'static str, usize)> = None;
    for d in DETECTORS {
        for p in d.prefixes {
            let needle = p.as_bytes();
            if needle.is_empty() || needle.len() > bytes.len() {
                continue;
            }
            for at in 0..=bytes.len() - needle.len() {
                if &bytes[at..at + needle.len()] != needle {
                    continue;
                }
                if at != 0 && bytes[at - 1].is_ascii_alphanumeric() {
                    continue; // buried mid-run: a word, not a credential
                }
                // A prefix is a shape only with a body behind it.
                let body = bytes[at + needle.len()..]
                    .iter()
                    .take_while(|b| (d.body_char)(**b))
                    .count();
                if require_body && body < d.body_min {
                    continue;
                }
                // Longest prefix wins; ties break on the detector's order in
                // DETECTORS, which is fixed — so the reported name is
                // deterministic rather than an artifact of scan order (J05).
                if best.is_none_or(|(_, len)| needle.len() > len) {
                    best = Some((d.name, needle.len()));
                }
            }
        }
    }
    best
}

fn detector_for(token: &str, position: Position) -> Option<&'static str> {
    // Known formats fire in any position, at any word boundary in the token.
    if let Some((name, _)) = known_format_prefix(token) {
        return Some(name);
    }
    match position {
        // A flag's value has no shape of its own; length is the only signal,
        // and the flag already told us it is a value.
        Position::FlagValue => (token.len() >= FLAG_VALUE_MIN_LEN
            && !is_published_identifier(token))
        .then_some("flag_value"),
        Position::Value => token
            .split(['/', '@', '=', ':'])
            .any(high_entropy)
            .then_some("high_entropy"),
        // D1: entropy never fires in prose. A filename is not a secret.
        //
        // One exception, and it is still a value position rather than prose:
        // `KEY=value` arrives as a SINGLE token because `=` is a token
        // character, so `position_of` never sees the assignment. The
        // right-hand side is an assignment RHS by construction, so it is
        // evaluated as one. A filename contains no `=`, so prose stays safe.
        Position::Prose => token.split_once('=').and_then(|(_, rhs)| {
            rhs.split(['/', '@', ':'])
                .any(high_entropy)
                .then_some("high_entropy")
        }),
    }
}

/// Mask email addresses, which straddle the token boundary (`@` is not a token
/// char) and so are invisible to the token walk. A well-defined, low-false-
/// positive shape (D4).
fn mask_emails(text: &str, fired: &mut BTreeSet<String>, masked: &mut u64) -> String {
    let local_ok = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '-');
    let domain_ok = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-');
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] != '@' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let mut start = i;
        while start > 0 && local_ok(chars[start - 1]) {
            start -= 1;
        }
        let mut end = i + 1;
        while end < chars.len() && domain_ok(chars[end]) {
            end += 1;
        }
        // Sentence punctuation is not part of the address. Keep it in the
        // output after replacing the address so a final period cannot hide it.
        while end > i + 1 && matches!(chars[end - 1], '.' | '-') {
            end -= 1;
        }
        let local: String = chars[start..i].iter().collect();
        let domain: String = chars[i + 1..end].iter().collect();
        let tld_ok = domain
            .rsplit('.')
            .next()
            .is_some_and(|t| t.len() >= 2 && t.chars().all(|c| c.is_ascii_alphabetic()));
        // `ghp_xxx@github.com` in a push URL is an email BY SHAPE, and calling
        // it one would mislabel what was matched — worse than staying quiet.
        // A local part that a known-format detector claims is left for the
        // token pass, which names it correctly.
        let claimed_by_prefix = known_format_prefix(&local).is_some();
        if !local.is_empty() && domain.contains('.') && tld_ok && !claimed_by_prefix {
            out.truncate(out.len() - local.len());
            out.push_str("[masked: email]");
            fired.insert("email".to_string());
            *masked += 1;
            i = end;
        } else {
            out.push('@');
            i += 1;
        }
    }
    out
}

/// Whether `text` contains a known credential PREFIX at a word boundary —
/// body or no body.
///
/// This is deliberately stricter than [`mask`], and the two must not be
/// confused. A redactor that fires on a bare prefix destroys `task-list.md`
/// and `data-sk-seg`, so [`mask`] requires a plausible body. But two
/// callers use this module as a **validity gate** rather than a redactor —
/// `annotate::safe_language_model_name` and `language`'s shadow labels —
/// and they refuse the value rather than masking it. For them the right
/// threshold is the opposite one: refuse anything that so much as starts
/// like a credential, because refusing a model tag or a shadow label costs
/// nothing and accepting a credential-shaped one costs everything.
///
/// A209 separated these. Before the body predicate the two thresholds were
/// accidentally the same, so tightening the redactor would have silently
/// loosened both gates.
pub fn resembles_known_credential(text: &str) -> bool {
    text.split(|c: char| !is_token_char(c))
        .any(|token| known_prefix_at_boundary(token, false).is_some())
}

/// What a masking pass actually did — the record, carried as a VALUE
/// beside the text rather than encoded into it.
///
/// This is the A209-C4 redesign in one type. The record is produced only
/// by the masker, so it cannot be forged by document content, and it
/// survives truncation because it was never in the string that gets cut.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaskRecord {
    pub masked: u64,
    pub families: BTreeSet<String>,
    /// False when this record cannot account for a masking pass its text
    /// went through — provenance was lost in transit.
    ///
    /// A209-C5/B7-6: `Treatment::Unknown` existed with **zero production
    /// construction sites**, so the one condition it was added for
    /// rendered as a confident zero instead. Unknown is now reachable, and
    /// it is reachable through the record rather than by hand.
    #[serde(default = "yes")]
    pub complete: bool,
}

fn yes() -> bool {
    true
}

impl Default for MaskRecord {
    fn default() -> Self {
        MaskRecord {
            masked: 0,
            families: BTreeSet::new(),
            complete: true,
        }
    }
}

impl MaskRecord {
    /// A record that cannot speak for its own text. Merging one in makes
    /// the result unknown, which closes the consent gate.
    pub fn unknown() -> Self {
        MaskRecord {
            masked: 0,
            families: BTreeSet::new(),
            complete: false,
        }
    }

    pub fn merge(&mut self, other: &MaskRecord) {
        self.masked += other.masked;
        self.families.extend(other.families.iter().cloned());
        self.complete &= other.complete;
    }

    pub fn is_empty(&self) -> bool {
        self.masked == 0
    }

    /// The treatment this record implies, for a producer that can account
    /// for every masking pass its content went through.
    pub fn to_treatment(&self) -> Treatment {
        if !self.complete {
            return Treatment::unknown(
                "a masking pass ran over this content but its record did not survive",
            );
        }
        Treatment::ran(self.masked, self.families.clone())
    }
}

/// Text that has been through the masker, carrying the record of what the
/// masker did — **constructible only here**.
///
/// A209-C6/B8-2, the crux. Three consecutive reviews found the same class:
/// a site calls `mask(...)` and takes `.0`, the record evaporates, and a
/// capsule renders a masked span while its treatment reports "nothing
/// matched a known secret shape" — which also opens the consent gate.
/// Round 6 ADDED a third such site (`label_from`) in the act of fixing the
/// other two. Enumeration and review have now failed at this four times.
///
/// The field is private and there is no constructor but [`Masked::mask`]
/// and [`Masked::unknown`], so `.0` cannot reach a field of this type: the
/// discard becomes a compile error rather than a review miss. Scoped
/// deliberately to the `nodes.label` → capsule-category path, which is the
/// path that keeps failing; widening it further is registered open rather
/// than half-done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Masked {
    text: String,
    record: MaskRecord,
}

impl Masked {
    /// The only masking constructor.
    pub fn mask(raw: &str) -> Self {
        let (text, record) = mask_recorded(raw);
        Masked { text, record }
    }

    /// Text whose provenance is genuinely unavailable — it went through a
    /// masking pass this value cannot account for. Renders
    /// [`Treatment::Unknown`], which closes the consent gate.
    pub fn unknown(text: impl Into<String>) -> Self {
        Masked {
            text: text.into(),
            record: MaskRecord::unknown(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn into_text(self) -> String {
        self.text
    }

    pub fn record(&self) -> &MaskRecord {
        &self.record
    }

    /// Apply a rendering transform that cannot introduce new secrets —
    /// wrapping in quotes, appending a state word. The record travels.
    pub fn map_rendered(self, f: impl FnOnce(String) -> String) -> Self {
        Masked {
            text: f(self.text),
            record: self.record,
        }
    }
}

/// [`mask`], returning the record as a value.
pub fn mask_recorded(text: &str) -> (String, MaskRecord) {
    let (out, families, masked) = mask(text);
    (
        out,
        MaskRecord {
            masked,
            families,
            complete: true,
        },
    )
}

/// Truncate to `max` characters without ever cutting through a
/// `[masked: …]` marker.
///
/// A209-C4/B2: with masking now ahead of truncation, a cut landing inside
/// a marker used to persist `[masked: aw` — which, under the old
/// text-parsed treatment, also erased the detection and OPENED the consent
/// gate. The record no longer depends on the marker, but the artifact
/// still must not ship a mutilated one: if the budget would land inside a
/// marker, the cut moves back to the marker's start.
pub fn truncate_respecting_markers(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    // No early return on `len <= max`: a string can arrive ALREADY holding
    // a partial marker (a caller cut it, or an earlier stage did), and the
    // repair must still run. A209-C5/B7-4.
    let mut cut = max.min(chars.len());
    let mut idx = 0usize;
    let opens: Vec<usize> = {
        let mut v = Vec::new();
        let pat: Vec<char> = "[masked: ".chars().collect();
        while idx + pat.len() <= chars.len() {
            if chars[idx..idx + pat.len()] == pat[..] {
                v.push(idx);
                idx += pat.len();
            } else {
                idx += 1;
            }
        }
        v
    };
    for start in opens {
        if start >= cut {
            break;
        }
        let end = chars[start..]
            .iter()
            .position(|c| *c == ']')
            .map(|e| start + e + 1);
        match end {
            Some(e) if e > cut => cut = start, // the cut would split it
            None => cut = cut.min(start),      // unterminated marker
            _ => {}
        }
    }
    if cut == chars.len() {
        return text.to_string();
    }
    chars[..cut].iter().collect()
}

/// Mask every secret-shaped span in `text`, reporting which detectors fired.
/// Mask whole PEM-style armoured blocks — banner, key material, and closing
/// banner — as one span.
///
/// A209-C round 3 measured the defect this exists to fix: the token walk
/// matched `-----BEGIN` and replaced that token alone, so a real 4-line
/// private key rendered as `[masked: private_key_block] RSA PRIVATE
/// KEY-----` followed by **both base64 key lines byte-identical** and the
/// END banner. The treatment then reported `1 masked · patterns:
/// private_key_block`, which a reader parses as "a private key was
/// redacted" while the private key is present in full. That is a lie about
/// the treatment — a rule-1 and rule-2 violation, not a detection gap —
/// and the key material itself is unreachable by every other detector
/// (entropy is value-scoped by D1, so a PEM body in prose is never
/// examined).
///
/// A block runs from `-----BEGIN` to the end of the matching `-----END`
/// line. An unterminated block masks to the end of the text: a truncated
/// key is still a key, and failing toward disclosure is the one direction
/// this module never takes.
fn mask_pem_blocks(text: &str, fired: &mut BTreeSet<String>, masked: &mut u64) -> String {
    // The fast path must be sound under the transport, not just over the
    // raw bytes. A209-C11: this guard read the ORIGINAL text for
    // `-----BEGIN` and returned before the decoder ran, so `-----%42EGIN`
    // — a banner whose `B` is percent-encoded — skipped masking entirely,
    // key and all. Every other reader in this module was moved onto the
    // decoded view in round 9; this one was missed.
    //
    // Only `%` and `\` can SYNTHESIZE characters during decoding (a
    // literal CR maps to a newline and can join nothing), so text
    // containing neither cannot grow a banner it does not already have.
    // That keeps the cheap exit for ordinary prose while making the
    // encoded case reachable.
    if !text.to_ascii_uppercase().contains("-----BEGIN")
        && !text.contains('%')
        && !text.contains('\\')
    {
        return text.to_string();
    }
    // DECODE ONCE, at the top. A209-C9: round 9 landed the decoder as a
    // helper called from one place on one of two arguments —
    // `same_line_tail` was decoded and `following` was not — so the moment
    // a banner ended a physical line and the escaped body began on the
    // next, every candidate kept its literal backslashes and the block
    // shipped. Stock `node -e 'console.log({private_key: fs.readFileSync(...)})'`
    // output was returned byte-identical. Decoding the whole text before
    // any splitting is what makes those two views incapable of diverging.
    let dec = decode_transport(text);
    let spans = key_spans(&dec);
    if spans.is_empty() {
        return text.to_string();
    }
    // Mask the corresponding span in the ORIGINAL text via the index map,
    // so the artifact keeps its own transport encoding and only the key is
    // replaced.
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    for (ds, de) in spans {
        let os = dec.map.get(ds).copied().unwrap_or(text.len());
        let oe = dec.map.get(de).copied().unwrap_or(text.len());
        if os < cursor || oe > text.len() || os > oe {
            continue;
        }
        out.push_str(&text[cursor..os]);
        out.push_str("[masked: private_key_block]");
        fired.insert("private_key_block".to_string());
        *masked += 1;
        cursor = oe;
    }
    out.push_str(&text[cursor..]);
    out
}

/// A normalized view of the text plus the map back to original byte
/// offsets. `map[i]` is where decoded byte `i` began in the original;
/// `map[len]` is the original length.
struct Decoded {
    text: String,
    map: Vec<usize>,
    /// Byte offset of each source line start, for attributing a decoded
    /// line back to the physical line it came from.
    src_line_starts: Vec<usize>,
}

impl Decoded {
    fn original_line_of(&self, orig_offset: usize) -> usize {
        match self.src_line_starts.binary_search(&orig_offset) {
            Ok(k) => k,
            Err(k) => k.saturating_sub(1),
        }
    }
}

/// Every key span in the NORMALIZED text, as byte ranges.
///
/// All splitting, banner detection and body detection happen here, on the
/// single decoded view — there is no second, undecoded reader.
fn key_spans(dec: &Decoded) -> Vec<(usize, usize)> {
    let norm = &dec.text;
    let lines: Vec<&str> = norm.split('\n').collect();
    let mut offsets: Vec<usize> = Vec::with_capacity(lines.len());
    let mut running = 0usize;
    for l in &lines {
        offsets.push(running);
        running += l.len() + 1; // + the '\n'
    }
    // Which SOURCE line each decoded line came from. The window bounds
    // how far past the banner we look in the source document — decoded
    // fragments produced from the banner's own physical line are part of
    // that line, not steps away from it. Without this, `C:\tmp\re`
    // twelve times inside the banner line spent the whole budget before
    // the real body (A209-C9), and widening the budget instead brought
    // back 15 live false positives.
    let src_line: Vec<usize> = offsets
        .iter()
        .map(|o| {
            let orig = dec.map.get(*o).copied().unwrap_or(0);
            dec.original_line_of(orig)
        })
        .collect();
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut i = 0usize;
    while i < lines.len() {
        let Some(banner) = first_key_banner(lines[i]) else {
            i += 1;
            continue;
        };
        let tail = &lines[i][banner.after..];
        let span_start = offsets[i] + banner.start;

        // STRUCTURE FIRST (A209-C12). A BEGIN banner with a matching END
        // banner is a TERMINATED block, and a terminated PEM block is
        // self-identifying: the format says where the payload starts and
        // stops, so no heuristic needs to guess. Mask it whole.
        //
        // Twelve rounds asked payload heuristics to do a job the document
        // already did, and every round they were defeated on a new axis —
        // per-line decoration, transport encoding, wrap width, and finally
        // BODY LENGTH. A PKCS#8 Ed25519 or X25519 key is one line of 64
        // characters; re-wrapped anywhere from 24 to 63 it has no run of
        // 64 and cannot repeat a width three times, so a rule built on
        // "wrapped payload repeats" cannot see it at ANY of those 40
        // widths. Measured: 91 of 318 (key type x wrap width) pairs leaked
        // at `823f02a`, and through the real CLI the consent gate opened
        // over an Ed25519 key — `ssh-keygen`'s default algorithm.
        //
        // What makes this safe is the label allowlist in `is_key_banner`,
        // which is now load-bearing: PUBLIC KEY and CERTIFICATE are
        // refused there, so a certificate chain is never reached by this
        // arm. That is what the must-not-mask controls pin.
        //
        // Same line first: a complete one-line PEM must not swallow what
        // follows it — and must not ABANDON it either.
        //
        // A209-C16: this was a bare `find_end_banner` match pushed
        // straight in as the span end, with no standalone test, no
        // after-payload ordering test, and `i += 1; continue` instead of
        // the recursive rescan the prose path performs. So a line holding
        // TWO keys — a Kubernetes secret, a docker-compose environment
        // pair, a one-line JSON dump — masked the first, ended the span at
        // its END, and abandoned the rest of the line. The second key
        // reached `capsule.md` and `capsule.json` verbatim, under
        // `masked=1`, on a surface that says known secret shapes are
        // masked.
        //
        // Two requirements now, the same ones every other bounding site
        // has: payload must lie BETWEEN the banner and the END (a mention
        // on the banner's own line bounds nothing), and whatever follows
        // the END is rescanned rather than dropped.
        let same_line_end = find_end_banner(tail, &banner.label).filter(|rel| {
            all_base64_runs(&tail[..*rel])
                .iter()
                .any(|r| r.len() >= SPAN_RUN_MIN)
        });
        if let Some(rel) = same_line_end {
            let end_abs = offsets[i] + banner.after + rel;
            spans.push((span_start, end_abs));
            let rest = &lines[i][banner.after + rel..];
            let sub = Decoded {
                text: rest.to_string(),
                map: (0..=rest.len()).collect(),
                src_line_starts: vec![0],
            };
            for mut inner in key_spans(&sub) {
                inner.0 += end_abs;
                inner.1 += end_abs;
                spans.push(inner);
            }
            i += 1;
            continue;
        }
        // A matching END banner on a later line makes this a TERMINATED
        // block, and structure alone decides that it is masked. It does
        // NOT decide where the block ends: span extension still does that.
        //
        // A209-C12: masking straight through to the first END banner
        // reopens B8-4 exactly. Given
        //
        //     -----BEGIN RSA PRIVATE KEY-----
        //     the block ends with -----END RSA PRIVATE KEY-----
        //     <payload>
        //
        // the first END is a MENTION that precedes the body, and cutting
        // there ships the payload while the record still reports a mask.
        // Span extension already refuses to terminate on an END banner
        // while payload is still arriving, so the two responsibilities are
        // kept apart: structure answers WHETHER, extension answers WHERE.
        // A209-C15 — DETECTION and SPAN BOUNDING are separate questions
        // with opposite safety requirements, and round 15 had one
        // predicate answering both.
        //
        // DETECTION: a matching END banner ANYWHERE in the window means
        // this is a key block. It does not have to be standalone. An
        // unrecognised decoration must never cost detection — that was
        // the defect: `head_is_decoration` gated `terminated`, which gates
        // `has_key_body` below, so an ANSI colour on the END line demoted
        // a clean key into the payload heuristic and 78 of 312 shapes
        // shipped a whole body at `masked=0`.
        let detected = any_end_banner(&lines, i, &banner.label);
        // SPAN BOUNDING: only a STANDALONE END line may bound the span.
        // Anything else falls through, and the fall-through over-masks.
        // Now that is the only failure mode this predicate has.
        let terminator = terminating_line(&lines, i, &banner.label);
        let terminated = detected;

        // UNTERMINATED blocks get no such guarantee: the document does not
        // identify them, so heuristics are all there is.
        let banner_src = src_line.get(i).copied().unwrap_or(0);
        let same_src = &src_line[(i + 1).min(src_line.len())..];
        if !detected && !has_key_body(tail, &lines[i + 1..], same_src, banner_src) {
            // Prose about keys. Keep scanning the rest of this line.
            // Prose about keys. Keep scanning the rest of this line for a
            // later banner; the text is already decoded, so re-scanning it
            // is a plain sub-slice walk.
            let consumed = offsets[i] + banner.after;
            let sub = Decoded {
                text: tail.to_string(),
                map: (0..=tail.len()).collect(),
                src_line_starts: vec![0],
            };
            for mut inner in key_spans(&sub) {
                inner.0 += consumed;
                inner.1 += consumed;
                spans.push(inner);
            }
            i += 1;
            continue;
        }
        // SPAN EXTENSION uses a lower bar than DETECTION, deliberately.
        // Detection must be conservative — a ≥64 run, so prose near a
        // banner is not mistaken for a key. But once this IS a key, the
        // span should be greedy: a final wrap is short by construction
        // (RFC 7468 pads the last line), and a CRC line shorter still, so
        // holding span extension to the detection bar left real tails
        // shipping (A209-C10, caught by three suites at once).
        //
        // The END banner does not terminate while payload is still
        // arriving: prose that MENTIONS the terminator before the body is
        // exactly the B8-4 case, and the last payload line — not the first
        // END-looking one — is what bounds the span.
        let mut last_payload: Option<usize> = None;
        let mut end_after_payload: Option<usize> = None;
        // Span extension is NOT bounded by the detection window. That
        // window exists to decide whether a key is present; once it is,
        // the span must reach the key's end, and a real RSA-2048 PEM is 28
        // lines against a 12-line window — which left 16 lines of a live
        // CI-log key in the capsule (A209-C10). The cap here is only a
        // sanity bound on unbounded input.
        let mut gap = 0usize;
        let mut j = i + 1;
        // A TERMINATED block is scanned to its terminator and no further,
        // and prose inside it does not stop the scan.
        //
        // A209-C13: extension used to BREAK at the first non-payload line,
        // so "extension found no payload" never meant "no payload
        // follows" — it meant "extension stopped looking". One short prose
        // line between a MENTIONED end marker and the real body was enough
        // to make the fall-back terminator the mention, and the body
        // shipped. Not breaking is what makes the payload reachable; the
        // terminator bound is what stops the scan running away.
        let scan_end = match terminator {
            Some(t) => (t + 1).min(lines.len()),
            None => lines.len(),
        };
        while j < scan_end && j - i <= SPAN_MAX_LINES {
            if first_key_banner(lines[j]).is_some() {
                break; // a new key block starts; do not swallow it
            }
            let cand = body_candidate(lines[j]);
            let run = longest_base64_run(cand);
            // The FINAL wrap is short by construction, and how short
            // depends on the body length modulo the wrap width — 4, 8 and
            // 12 are all real. A short line counts as payload only when
            // the WHOLE candidate is one base64 run and the line directly
            // above it was payload, so a stray token near a key cannot
            // extend a span but a key's own last line always ends inside
            // it (A209-C11).
            let trimmed = cand.trim();
            let is_final_wrap = run >= SPAN_TAIL_MIN
                && run == trimmed.len()
                && last_payload == Some(j.saturating_sub(1));
            // The END-banner test comes FIRST. A209-C12: a line carrying
            // a matching END banner is a terminator candidate, and only
            // its position relative to payload decides whether it closes
            // the block. Testing payload first let a line be BOTH — and
            // in `rg` output it always is, because the path prefix
            // `Packages/…/MemoryPipelineTests.swift-11-` is one long
            // base64 run (`/` is a base64 character). The END line read
            // as payload, the span ran past it through every following
            // output line, and one live row lost 17,293 characters over a
            // block whose BEGIN and END were two lines apart.
            //
            // Ordering it this way keeps B8-4 intact: an END that is only
            // MENTIONED, before any payload has been seen, still does not
            // close the span.
            if standalone_end_banner(lines[j], &banner.label).is_some() {
                if last_payload.is_some() && end_after_payload.is_none() {
                    end_after_payload = Some(j);
                    break;
                }
            } else if run >= SPAN_RUN_MIN || is_final_wrap {
                last_payload = Some(j);
                end_after_payload = None;
            } else if !terminated
                && !(is_pem_header(strip_gutter(lines[j])) || cand.trim().is_empty())
            {
                break;
            }
            // Allow a bounded run of non-payload lines inside a block
            // (headers, blanks) without ending the span. Inside a
            // terminated block the bound is the terminator itself, so
            // prose does not consume this budget.
            if !terminated && (cand.trim().is_empty() || is_pem_header(strip_gutter(lines[j]))) {
                gap += 1;
                if gap > KEY_BODY_WINDOW {
                    break;
                }
            } else if !terminated {
                gap = 0;
            }
            j += 1;
        }
        let last_line = match (end_after_payload, last_payload) {
            (Some(e), _) => Some(e),
            (None, Some(p)) => Some(p),
            // Extension found NO payload anywhere in the block.
            //
            // A209-C14: the answer depends on whether the document showed
            // us an end. A STANDALONE END banner is real structure, so the
            // block bounds there — that is what returns the placeholder-
            // body fixtures to in-block removal. No standalone END
            // anywhere in the window means we genuinely cannot see where
            // this ends, and end-of-field is the only answer that cannot
            // leak.
            //
            // A209-C13 recorded why the unrestricted form of this arm was
            // a leak: terminator selection was satisfied by a MENTION, so
            // the scan stopped above the body. Restricting the terminator
            // to a standalone banner line is what makes the arm safe, not
            // the arm itself — 7 mentioned-END shapes ship a key without
            // that restriction and none with it.
            (None, None) => terminator,
        };
        let span_end = match last_line {
            Some(k) => offsets[k] + lines[k].len(),
            // A key banner with no recognisable body or terminator at all:
            // everything after it goes, which is the one place failing
            // toward masking is right.
            None => norm.len(),
        };
        spans.push((span_start, span_end.max(span_start)));
        i = last_line.map_or(lines.len(), |k| k + 1);
    }
    spans
}

/// Whether a matching END banner appears anywhere in this block's window.
///
/// This answers DETECTION only, and deliberately accepts a banner that is
/// decorated, quoted, coloured or embedded in a sentence. Being wrong here
/// costs an over-mask; being wrong the other way costs a key (A209-C15).
fn any_end_banner(lines: &[&str], from: usize, label: &str) -> bool {
    // A209-C16: there is NO new-block veto here any more.
    //
    // `first_key_banner` firing is safe where it opens a block and unsafe
    // here, where it returned false and SUPPRESSED DETECTION. Note the
    // polarity: it is RECOGNISING the banner that leaked, not failing to.
    // A BEGIN anywhere between payload and the matching END vetoed the
    // whole block — and `cat a.pem b.pem`, where the first file has no
    // trailing newline, puts key 1's END on the same line as key 2's
    // BEGIN. Detection is now exactly what it says: a matching END
    // anywhere in the window means detect.
    let hard = (from + SPAN_MAX_LINES).min(lines.len());
    lines[from + 1..hard]
        .iter()
        .any(|line| find_end_banner(line, label).is_some())
}

/// The line carrying the END banner that terminates a block opened at
/// `from`, if there is one.
///
/// A209-C12: this is what makes a terminated block self-identifying. The
/// bound is the same sanity bound span extension uses over unbounded
/// input — deliberately NOT a narrower window, because narrowing windows
/// is what broke real keys in earlier rounds.
fn terminating_line(lines: &[&str], from: usize, label: &str) -> Option<usize> {
    let hard = (from + SPAN_MAX_LINES).min(lines.len());
    let mut last = None;
    for (offset, line) in lines[from + 1..hard].iter().enumerate() {
        if first_key_banner(line).is_some() {
            break; // a new block starts; its END is not ours
        }
        if standalone_end_banner(line, label).is_some() {
            last = Some(from + 1 + offset);
        }
    }
    last
}

/// A key banner located within one line.
struct PemBanner {
    /// Byte offset of `-----BEGIN` in the line.
    start: usize,
    /// Byte offset just past the banner's closing `-----` (or end of line).
    after: usize,
    label: String,
}

/// Strip a reader's gutter — `     1\t`, `12: `, `> ` — before shape tests.
///
/// A209-C5/B7-1: a tool printing a key file with line numbers defeated
/// every base64 and END check, so the span ended at the banner and the body
/// was re-emitted verbatim.
fn strip_gutter(line: &str) -> &str {
    let t = line.trim_start();
    if let Some(rest) = t.strip_prefix('>') {
        return rest.trim_start();
    }
    let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return t;
    }
    let rest = &t[digits.len()..];
    for sep in ['\t', ':', '|'] {
        if let Some(r) = rest.strip_prefix(sep) {
            return r.trim_start_matches([' ', '\t']);
        }
    }
    t
}

/// The FIRST key banner in this line, scanning past non-key banners.
///
/// A209-C5/B7-2 (a regression the redesign introduced): the label was taken
/// as the whole remainder of the line, so `"cert":"-----BEGIN
/// CERTIFICATE-----…","key":"-----BEGIN RSA PRIVATE KEY----- …"` vetoed the
/// real key on the certificate's name. The label is now exactly the text
/// between `-----BEGIN` and the next `-----` ON THAT LINE, and a non-key
/// banner is stepped over rather than ending the search.
fn first_key_banner(line: &str) -> Option<PemBanner> {
    let upper = line.to_ascii_uppercase();
    let mut from = 0usize;
    while let Some(rel) = upper[from..].find("-----BEGIN") {
        let start = from + rel;
        let after_marker = start + "-----BEGIN".len();
        let rest = &upper[after_marker..];
        let (label_raw, after) = match rest.find("-----") {
            Some(e) => (&rest[..e], after_marker + e + 5),
            None => (rest, line.len()),
        };
        let label = label_raw
            .trim_matches(|c: char| c == '-' || c.is_whitespace())
            .to_string();
        if !label.is_empty() && is_key_banner(&label) {
            return Some(PemBanner {
                start,
                after: after.min(line.len()),
                label,
            });
        }
        from = after_marker;
    }
    None
}

/// The byte offset just past a matching `-----END <label>-----`, if one
/// The byte offset just past a matching `-----END <label>-----`, if one
/// appears in `haystack`.
fn find_end_banner(haystack: &str, label: &str) -> Option<usize> {
    locate_end_banner(haystack, label).map(|(_, after)| after)
}

/// The matching END banner's `(start, after)` offsets within one line.
fn locate_end_banner(haystack: &str, label: &str) -> Option<(usize, usize)> {
    let upper = haystack.to_ascii_uppercase();
    let at = upper.find("-----END")?;
    // A209-C6/B8-4: a MENTION of the terminator is not the terminator. A
    // bare `-----END-----`, or prose saying "the block ends with
    // -----END…", used to close the span early and re-emit the body
    // verbatim while the record still reported masked=1. The banner must
    // carry a matching, non-empty label.
    let rest = &upper[at + "-----END".len()..];
    let (got_raw, after) = match rest.find("-----") {
        Some(e) => (&rest[..e], at + "-----END".len() + e + 5),
        None => (rest, haystack.len()),
    };
    let got = got_raw.trim_matches(|c: char| c == '-' || c.is_whitespace());
    (!got.is_empty() && got == label).then_some((at, after.min(haystack.len())))
}

/// A matching END banner that is the LINE, not a phrase inside a sentence.
///
/// A209-C14. Round 14 asked only whether a matching END existed anywhere,
/// which cannot separate the end of a key from prose about one. The
/// discriminator is what KIND of line it is: after decoration is
/// stripped, a terminator's line **is** the banner.
///
/// * `-----END PRIVATE KEY-----` → terminator.
/// * `2026-09-01T12:00:00Z -----END PRIVATE KEY-----` → terminator; a
///   timestamp is decoration.
/// * `'-----END PRIVATE KEY-----' +` → terminator; renderer quoting is
///   decoration.
/// * `…through -----END PRIVATE KEY-----, inclusive.` → **not** a
///   terminator. Words on both sides mean prose about a key.
///
/// The failure direction is deliberate and is what makes this safe: a
/// decoration this function does not recognise makes a real END look
/// non-standalone, so no terminator is found and the span falls through
/// to end-of-field. Unrecognised input over-masks; it never under-masks.
fn standalone_end_banner(line: &str, label: &str) -> Option<usize> {
    // Every matching END on the line is a candidate, not just the first:
    // two banners on one line leave the first inside the second's head,
    // and checking only the first rejected the line outright.
    let upper = line.to_ascii_uppercase();
    let mut from = 0usize;
    while let Some(rel) = upper[from..].find("-----END") {
        let at = from + rel;
        from = at + "-----END".len();
        let Some((_, after)) = locate_end_banner(&line[at..], label) else {
            continue;
        };
        let after = at + after;
        // Nothing but punctuation may follow: a trailing comma inside a
        // JSON array, a `+` concatenation, a backslash continuation, a
        // closing quote or table pipe are all still the end of the block.
        if line[after..].chars().any(|c| c.is_ascii_alphanumeric()) {
            continue;
        }
        if head_is_decoration(&line[..at]) {
            return Some(after);
        }
    }
    None
}

/// Whether everything before a banner on its line is decoration rather
/// than prose.
///
/// A209-C15 replaces a NEGATIVE test with a POSITIVE one. The old rule was
/// "decoration iff no lowercase survives stripping", which accepts every
/// shouted sentence: `WARNING: DO NOT COMMIT ANYTHING THROUGH -----END
/// PRIVATE KEY-----` read as a standalone terminator, and the span closed
/// at that mention while 1,026 of 1,026 body windows shipped under a
/// `[masked: private_key_block]` marker. That is the A209-C13 leak again,
/// differing only in letter case.
///
/// So decoration must now be RECOGNISED, shape by shape: bracketed and
/// tagged wrappers, column separators, quote and comment prefixes,
/// timestamps, log levels, gutters and path-and-line prefixes. Anything
/// unrecognised is prose, the banner is not standalone, and the span falls
/// through to end-of-field — which, since detection was split out above,
/// is now purely an over-mask.
fn head_is_decoration(head: &str) -> bool {
    let mut h = head;
    loop {
        let before = h;
        h = h.trim_start();
        h = h.trim_start_matches(['>', '|', '\'', '"', '\\', '+', '#', '*', ',', '(', '=']);
        if let Some(r) = h.strip_prefix("//") {
            h = r;
        }
        // `[stdout]`, `[pod/app-7f8d9/web]`, `[2026-09-01 12:00:00]`
        if h.starts_with('[') {
            if let Some(e) = h.find(']') {
                h = &h[e + 1..];
            }
        }
        // `<line>`, `<span class="k">`
        if h.starts_with('<') {
            if let Some(e) = h.find('>') {
                h = &h[e + 1..];
            }
        }
        // `\texttt{`
        if let Some(rest) = h.strip_prefix('\\') {
            let after = rest.trim_start_matches(|c: char| c.is_ascii_alphabetic());
            if let Some(r) = after.strip_prefix('{') {
                h = r;
            }
        }
        // One tab-delimited column, as event logs and TSV emit.
        if let Some(t) = h.find('\t') {
            let col = &h[..t];
            if col.len() <= 40 && !col.contains("  ") {
                h = &h[t + 1..];
            }
        }
        // `web_1  | ` — a single identifier column before a pipe. A
        // sentence cannot qualify: the segment must be ONE token.
        if let Some(bar) = h.find('|') {
            let col = h[..bar].trim_end();
            if !col.is_empty()
                && col.len() <= 32
                && !col.contains(char::is_whitespace)
                && col
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/'))
            {
                h = &h[bar + 1..];
            }
        }
        // A preceding banner is structure, not prose: two END banners on
        // one line leave the first in the second's head.
        if h.starts_with("-----") {
            if let Some(e) = h[5..].find("-----") {
                h = &h[5 + e + 5..];
            }
        }
        h = strip_log_level(h);
        h = strip_numeric_stamp(h);
        // A gutter or path-and-line prefix is ONE token ending in `:` or
        // `-`: `12:`, `debug1:`, `src/keys/id_rsa:41:`, `file.swift-11-`.
        // Exactly one token, so `NOTE: THE BLOCK ENDS WITH` loses only
        // `NOTE:` and the sentence that follows keeps it out of this set.
        let tok_end = h.find(char::is_whitespace).unwrap_or(h.len());
        let tok = &h[..tok_end];
        if !tok.is_empty() && (tok.ends_with(':') || tok.ends_with('-')) {
            h = &h[tok_end..];
        }
        if h == before {
            break;
        }
    }
    h.trim().is_empty()
}

/// A leading severity word, as every log formatter emits.
fn strip_log_level(h: &str) -> &str {
    const LEVELS: [&str; 16] = [
        "TRACE", "DEBUG", "INFO", "NOTICE", "WARNING", "WARN", "ERROR", "FATAL", "CRITICAL",
        "VERBOSE", "ERR", "DBG", "E", "W", "I", "D",
    ];
    let t = h.trim_start();
    for lvl in LEVELS {
        if let Some(rest) = t.strip_prefix(lvl) {
            // It is a level only if a word does not continue through it.
            if !rest.starts_with(|c: char| c.is_ascii_alphanumeric()) {
                return rest.strip_prefix(':').unwrap_or(rest);
            }
        }
    }
    h
}

/// A leading timestamp or numeric gutter: `2026-09-01T12:00:00.123Z`,
/// `12:04:05`, `Sep 01 12:00:00`, `9/1/2026 12:00:00 PM`, `   1`.
fn strip_numeric_stamp(h: &str) -> &str {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let t = h.trim_start();
    let stampish = |c: char| c.is_ascii_digit() || matches!(c, '-' | ':' | '.' | '/' | 'T' | 'Z');
    if let Some(m) = MONTHS.iter().find(|m| t.starts_with(**m)) {
        let rest = t[m.len()..].trim_start();
        if rest.starts_with(|c: char| c.is_ascii_digit()) {
            let cut = rest.trim_start_matches(|c: char| stampish(c) || c == ' ');
            return cut;
        }
    }
    if t.starts_with(|c: char| c.is_ascii_digit()) {
        let cut = t.trim_start_matches(stampish);
        // `12:00:00 PM` — an AM/PM marker belongs to the stamp.
        let cut = cut.trim_start();
        for ap in ["AM", "PM"] {
            if let Some(r) = cut.strip_prefix(ap) {
                return r;
            }
        }
        if cut.len() < t.len() {
            return cut;
        }
    }
    h
}

/// A PEM encapsulated header (`Proc-Type:`, `DEK-Info:`) — inside the block.
fn is_pem_header(line: &str) -> bool {
    let t = line.trim();
    matches!(
        t.split_once(':').map(|(k, _)| k.trim().to_ascii_uppercase()),
        // RFC 1421 encapsulated headers AND RFC 4880 §6.2 armor headers.
        // The two sets are DISJOINT, and knowing only the first is what
        // let every real gpg armor block escape (A209-C7).
        Some(k) if matches!(
            k.as_str(),
            "PROC-TYPE" | "DEK-INFO" | "CONTENT-DOMAIN"
                | "VERSION" | "COMMENT" | "MESSAGEID" | "HASH" | "CHARSET"
        )
    )
}

/// Whether a PEM banner label names private key material./// Whether a PEM banner label names private key material./// Whether a PEM banner label names private key material.
///
/// A209-C3 measured the cost of not asking: firing on the bare literal
/// `-----BEGIN` masked **70 of 76,203** live sessions' opening asks —
/// almost all `-----BEGIN BID-----`, a construction-bid delimiter with no
/// key material — destroying 104 of 240 characters of a representative
/// ask, in `original_task`, one of the two sections this module's own
/// header calls the first a fresh session reads. It also reported
/// `patterns: private_key_block` over documents containing no private key:
/// the same false-treatment fault the block fix was written to remove.
/// `-----BEGIN BID-----` is uppercase, so a naive `[A-Z]` test does not
/// separate it — the label must actually name a key.
fn is_key_banner(label: &str) -> bool {
    let label = label.trim_end_matches('-').trim();
    // PERMISSIVE again (A209-C6/B8-1). Round 6 tightened this to
    // `[A-Z ]`-only + `ends_with("PRIVATE KEY")` to kill 260 live
    // false positives, and in doing so rejected `PGP PRIVATE KEY BLOCK` —
    // the RFC 4880 armor label `gpg --export-secret-keys --armor` emits,
    // the most common way a private key appears in a transcript — plus
    // every label carrying a digit (`X509 PRIVATE KEY`, `SSH2 ENCRYPTED
    // PRIVATE KEY`). All six masked correctly before that change: ground
    // lost, not a gap.
    //
    // The false positives are not a label problem and are no longer
    // treated as one. The MASK decision now requires a base64-shaped BODY
    // ([`has_key_body`]); a real key always has one, a comment mentioning
    // "PRIVATE KEY" never does. Label prose decides nothing on its own.
    if label.is_empty() {
        return false;
    }
    if label.contains("PUBLIC KEY") || label.contains("CERTIFICATE") {
        return false;
    }
    // A real PEM label is printable ASCII (RFC 7468 §2). A209-C10: the
    // last two live over-masks were prose about this very module —
    // "`-----BEGIN … PRIVATE KEY-----` blocks" — where the label carries a
    // UTF-8 ellipsis. No banner does.
    if !label.chars().all(|c| c.is_ascii() && !c.is_ascii_control()) {
        return false;
    }
    // A209-C12: and it is an RFC 7468 label — uppercase letters, digits
    // and single spaces. Nothing else.
    //
    // This became load-bearing when block STRUCTURE started deciding: a
    // terminated block is now masked whole, so a line carrying a
    // credential-scanning REGEX — `-----BEGIN (RSA |EC |OPENSSH |DSA
    // )?PRIVATE KEY-----` next to its own `-----END …-----` — was masked
    // as if it were a key. Measured on the live ledger, that is what the
    // structural rule newly fired on. A pattern is not a label: `{`, `(`,
    // `[`, `|`, `?`, `*`, `\` and `"` never appear in one.
    //
    // Every real label passes: PRIVATE KEY, RSA/EC/DSA/OPENSSH PRIVATE
    // KEY, ENCRYPTED PRIVATE KEY, PGP PRIVATE KEY BLOCK, X509 PRIVATE KEY,
    // SSH2 ENCRYPTED PRIVATE KEY, PGP MESSAGE, EC PARAMETERS.
    if !label
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == ' ')
    {
        return false;
    }
    label.contains("PRIVATE KEY")
        || label.contains("PGP MESSAGE")
        || label.contains("ENCRYPTED KEY")
        || label.contains("EC PARAMETERS")
}

/// Every call site of a safety-critical predicate in this module, with the
/// direction it must err in if it must err at all.
///
/// A209-C16. Three consecutive rounds produced the same defect: **one
/// predicate serving two consumers that disagree about which direction is
/// safe when it is wrong.** `head_is_decoration` gated both the
/// end-of-field fall-through (over-mask when wrong) and detection (leak
/// when wrong). `find_end_banner` fed both detection (firing is safe) and
/// a bare same-line span bound (firing is unsafe). `first_key_banner`
/// opened blocks (firing is safe) and vetoed detection (firing leaks).
///
/// The class is enumerable, so it is enumerated. Each entry is
/// `(predicate, consumer, sites, direction, why)` where direction is:
///
/// * `over`   — over-recognising is safe; under-recognising leaks.
/// * `under`  — under-recognising is safe; over-recognising leaks.
/// * `either` — a compensating mechanism makes both safe, named in `why`.
/// * `single` — exactly one consumer, so no disagreement is possible.
///
/// The gate in `a209_consumer_agreement.rs` asserts that this table
/// matches the source exactly, and that **no predicate carries both an
/// `over` and an `under`**. Both round-16 defects violate the second
/// condition, and the gate is verified against them.
pub const PREDICATE_CONSUMERS: &[(&str, &str, usize, &str, &str)] = &[
    ("all_base64_runs", "has_key_body", 2, "over", "more runs means more detection"),
    ("all_base64_runs", "key_spans", 1, "either", "same-line payload guard: a wrong answer either bounds and rescans, or falls to the multi-line path"),
    ("any_end_banner", "key_spans", 1, "over", "firing detects"),
    ("body_candidate", "has_key_body", 2, "under", "over-stripping removes payload characters and shortens runs"),
    ("body_candidate", "key_spans", 1, "under", "over-stripping shortens a run and stops a span early"),
    ("decode_transport", "mask_pem_blocks", 1, "single", "one consumer"),
    ("find_end_banner", "any_end_banner", 1, "over", "firing detects"),
    ("find_end_banner", "key_spans", 1, "either", "same-line bound, guarded by a payload test and followed by a rescan of the remainder of the line"),
    ("first_key_banner", "key_spans", 2, "over", "firing opens a block; the span-extension break only ends a span that a later scan reopens"),
    ("first_key_banner", "terminating_line", 1, "either", "firing yields no terminator and end-of-field; not firing over-extends a span"),
    ("has_key_body", "key_spans", 1, "over", "firing detects"),
    ("head_is_decoration", "standalone_end_banner", 1, "single", "one consumer since A209-C15 split it"),
    ("is_key_banner", "first_key_banner", 1, "single", "one consumer"),
    ("is_pem_header", "key_spans", 2, "over", "firing keeps a span open; not firing breaks it early"),
    ("locate_end_banner", "find_end_banner", 1, "either", "shared locator; every caller guards it"),
    ("locate_end_banner", "standalone_end_banner", 1, "either", "shared locator; every caller guards it"),
    ("longest_base64_run", "has_key_body", 1, "over", "more payload means more detection"),
    ("longest_base64_run", "key_spans", 1, "over", "more payload means a longer span"),
    ("standalone_end_banner", "key_spans", 1, "under", "under-recognition falls through to end-of-field"),
    ("standalone_end_banner", "terminating_line", 1, "under", "under-recognition falls through to end-of-field"),
    ("strip_ansi", "decode_transport", 1, "single", "one consumer"),
    ("strip_gutter", "body_candidate", 1, "under", "over-stripping removes payload"),
    ("strip_gutter", "key_spans", 2, "under", "over-stripping removes payload"),
    ("strip_log_level", "head_is_decoration", 1, "single", "one consumer"),
    ("strip_numeric_stamp", "head_is_decoration", 1, "single", "one consumer"),
    ("terminating_line", "key_spans", 1, "under", "under-recognition falls through to end-of-field"),
];

/// How far past a banner a real key body may begin.
const KEY_BODY_WINDOW: usize = 12;
/// Minimum base64 run to count as PEM payload, and the run length that
/// counts as a full RFC 7468 wrap on its own.
/// The run length that counts as a full RFC 7468 wrap on its own, and so
/// as payload without needing a repeat to corroborate it.
///
/// This arm exists for the SINGLE-LINE body: a PKCS#8 Ed25519 or X25519
/// key is exactly 64 characters on one line and has no wrap to repeat.
///
/// A209-C11 retracts the residual this comment used to name — "a key
/// wrapped narrower than 64 … would be missed" — as an understatement
/// that read like a corner case. It was the primary defect: 40, 44, 48,
/// 56 and 60 were all missed, and `openssl`, an editor, a log pane or
/// `fold -w` produce them routinely. Width is now carried by
/// `WRAP_WIDTH_MIN`/`WRAP_REPEAT_MIN` below.
const KEY_BODY_RUN_FULL: usize = 64;
/// A repeating wrap must be at least this wide to count, and must repeat
/// at least this many times. A209-C11: these replace the dead
/// `qualifying >= 2`, and they are the whole of the width fix.
///
/// Tuned on measurement, not intuition — see the over-mask census in
/// A209-C11. The named residual: a key wrapped NARROWER than 16 would be
/// missed, and no tool emits one.
const WRAP_WIDTH_MIN: usize = 16;
const WRAP_REPEAT_MIN: usize = 3;
/// A single run this long beside a banner is enough for an UNTERMINATED
/// block. Terminated blocks never consult it — structure decides those.
const UNTERMINATED_RUN_MIN: usize = 40;
/// The shortest run of pure hex that is treated as a DIGEST rather than as
/// base64. A git SHA is 40 and a sha256 is 64; four hex characters are
/// four hex characters (A209-C11).
const HEX_EXCLUSION_MIN: usize = 16;
/// Span extension's bar. Lower than detection's on purpose: a confirmed
/// key's final wrap and CRC line are short, and holding them to the
/// detection bar leaves a real tail in the artifact.
const SPAN_RUN_MIN: usize = 16;
/// The shortest FINAL wrap span extension will swallow, and only
/// immediately after a payload line whose own run qualified. A body of
/// 1,624 characters wrapped at 54, 16, 62 or 24 ends in 4, 8, 12 or 16
/// characters respectively; each of those is a real key's last line.
const SPAN_TAIL_MIN: usize = 4;
/// Sanity bound on span extension over unbounded input.
const SPAN_MAX_LINES: usize = 4096;

/// Whether base64-shaped key material follows the banner.
///
/// This is the discriminator round 7 replaced label prose with, and the
/// evidence for it is the round-6 verification itself: of the 37 fields
/// still firing on the live ledger, **zero** had any base64-shaped line in
/// the 12 lines after the banner, and 34 of 37 removed regions were 3-27%
/// spaces — which a base64 body cannot be. Every one was a search command,
/// a code comment, or a test file mentioning "private key", and in 21
/// cases everything from the banner to end-of-record was deleted under a
/// confident `private_key_block` treatment.
fn has_key_body(
    same_line_tail: &str,
    following: &[&str],
    following_src: &[usize],
    banner_src: usize,
) -> bool {
    // Both arguments arrive from the SINGLE decoded view (see
    // `mask_pem_blocks`), so neither can be a raw reader any more — that
    // divergence is what shipped stock Node console.log output verbatim.
    //
    // Empty segments do not consume window slots (A209-C9): they used to,
    // so a banner-line tail carrying 12 or more recognised breaks filled
    // the whole window with empty strings and the real body below was
    // never examined. 9, 10 and 11 breaks masked; 12, 13 and 14 leaked.
    // `following` is also appended BEFORE any truncation.
    // The window bounds how far past the banner we look in FOLLOWING
    // lines. The banner line's own decoded segments do not consume that
    // budget: they are bounded by the line itself, and letting them spend
    // it is what starved the window. Empty segments count for nothing
    // either way; so do short ones, which is why `C:\tmp\re` repeated
    // twelve times — a Windows path, decoding to three junk fragments each
    // — could push a real body out of view.
    let mut candidates: Vec<String> = body_candidate(same_line_tail)
        .split('\n')
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .collect();
    // Bound the search by CHARACTERS scanned, not by line count. A line
    // budget is exhausted by junk: `C:\tmp\re` twelve times decodes to
    // three short fragments each, which spent every slot before the real
    // body was reached. A real PEM body begins within a few hundred
    // characters of its banner, and short fragments cannot consume a
    // character budget the way they consume a line budget.
    // TWO budgets, because one could not serve both cases. Segments
    // decoded out of the banner's OWN source line get their own budget:
    // otherwise junk inside that line (`C:\tmp\re` twelve times) spends
    // the cross-line budget and the real body one line below is never
    // reached. But they must still be bounded — a whole document stored as
    // one JSON-escaped line is entirely "the banner's source line", and
    // scanning it without limit re-fired on 16 live records, all Python
    // secret-pattern files with a stray base64 run far from the banner.
    let mut same_src = KEY_BODY_WINDOW;
    let mut cross_src = KEY_BODY_WINDOW;
    for (k, line) in following.iter().enumerate() {
        let cand = body_candidate(line).to_string();
        if cand.trim().is_empty() {
            continue;
        }
        let budget = if following_src.get(k).copied() == Some(banner_src) {
            &mut same_src
        } else {
            &mut cross_src
        };
        if *budget == 0 {
            if same_src == 0 && cross_src == 0 {
                break;
            }
            continue;
        }
        *budget -= 1;
        candidates.push(cand);
    }

    // Payload is WRAPPED — and the discriminator is that the wrap REPEATS,
    // not that it is 64 wide.
    //
    // A209-C11 retracts the claim three lines above this one that the
    // thresholds "survived attack and are not the lever". They were the
    // lever, and they were also dead: with `KEY_BODY_RUN_MIN` and
    // `KEY_BODY_RUN_FULL` both at 64, `qualifying >= 2` could not fire on
    // any input that `full_width` had not already caught. The effective
    // rule was "some run is at least 64", so a key wrapped at 40, 44, 48,
    // 56 or 60 was not detected at all — five of seven measured widths,
    // including through the real `rescue` path, where the consent gate
    // OPENED over a real RSA-2048 key.
    //
    // Wrap width was the second generator: RFC 7468 says 64, OpenSSH 70,
    // MIME 76, and a log pane, an editor or `fold -w` says whatever it
    // likes. Binding to any one width reopens the class at every other.
    // What no prose does — and what every wrapped payload does by
    // construction — is emit the SAME run length on line after line.
    //
    // Equal-length runs are counted across the window rather than in a
    // consecutive block, because consecutiveness is itself defeated: a
    // decorated 64-wrap hard-wrapped at 80 columns alternates 49, 15, 49,
    // 15, … so no two ADJACENT lines share a width while one width still
    // repeats thirteen times. That measurement is in `a209_wrap_widths`.
    //
    // The repeats must be DISTINCT. Equal width alone is not payload:
    // `rg` and `grep` print the same long path on every match line, so
    // seven output lines can share a 60-character run that is one repeated
    // string. Measured on the operator's own ledger, the width rule
    // without this clause fired on 13 rows of 2,046,353 — every one a
    // credential-scanning command, a detector's own regex, or a test
    // fixture — and every one had at most 2 distinct runs at its repeating
    // width. A wrapped key's lines are distinct by construction.
    //
    // EVERY run on the line is censused, not just the longest. A209-C12:
    // keeping only the longest let any constant per-line token with a
    // longer run erase the payload from the census — and `/` is a base64
    // character, so a filesystem path is one long run. `rg`-style output
    // printing `/home/ci/deploy/secrets/prod/keydir/id_rsa:<n>:` before
    // each payload line hid the very key it was printing.
    let mut widths: BTreeMap<usize, BTreeSet<&str>> = BTreeMap::new();
    for c in &candidates {
        for run in all_base64_runs(c) {
            if run.len() >= WRAP_WIDTH_MIN {
                widths.entry(run.len()).or_default().insert(run);
            }
        }
    }
    let repeats_a_width = widths.values().any(|runs| runs.len() >= WRAP_REPEAT_MIN);
    // A209-C13: a single substantial run, next to a private-key banner, in
    // a block the document never terminated.
    //
    // This arm exists because the transport can destroy BOTH other
    // signals at once. A GitHub Actions runner stamps a 29-character
    // ISO-8601 timestamp on every line and hard-wraps at 80 columns, so a
    // natural 64-character body line becomes 51 + 13: no run reaches 64,
    // and the alternation means no width repeats. The key file was never
    // rewrapped by anyone — the log did it — and the record then reported
    // `fired={}`, which leaves `needs_acknowledgement()` false.
    //
    // Measured by the verifier and re-measured here: crux leaks fall
    // 345/520 -> 144/520 and decorated 90/172 -> 35/172, against a live
    // cost of 14 -> 17 rows.
    //
    // The run must not be a REPEATED constant. `rg` prints the same long
    // path on every match line, and a path is one long base64 run (`/` is
    // in the alphabet), so a bare length test re-masks the grep output
    // A209-C11 measured and pinned. A run that recurs on three or more
    // lines is furniture; payload runs recur on none.
    let mut run_lines: BTreeMap<&str, usize> = BTreeMap::new();
    for c in &candidates {
        for r in all_base64_runs(c) {
            if r.len() >= UNTERMINATED_RUN_MIN {
                *run_lines.entry(r).or_insert(0) += 1;
            }
        }
    }
    let one_substantial_run = run_lines.values().any(|n| *n < WRAP_REPEAT_MIN);
    // A single-line body has no wrap to repeat: a PKCS#8 Ed25519 or X25519
    // key is exactly 64 characters on one line. That is what this arm is
    // for, and it is the only reason a bare length test remains.
    let full_width = candidates
        .iter()
        .any(|c| longest_base64_run(c) >= KEY_BODY_RUN_FULL);
    full_width || repeats_a_width || one_substantial_run
}

/// Remove ANSI escape sequences, keeping the index map aligned.
///
/// CSI (`ESC [` … final byte `@`-`~`) and OSC (`ESC ]` … BEL) are both
/// handled, as is a lone `ESC` plus one byte. The map is rebuilt so a
/// span cut in decoded coordinates still lands on the right original
/// bytes, and the sequence itself stays in whatever is emitted.
fn strip_ansi(text: String, map: Vec<usize>) -> (String, Vec<usize>) {
    if !text.contains('\u{1b}') {
        return (text, map);
    }
    let b = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut newmap: Vec<usize> = Vec::with_capacity(map.len());
    let sentinel = map.last().copied().unwrap_or(0);
    let mut i = 0usize;
    while i < b.len() {
        if b[i] == 0x1b {
            let mut j = i + 1;
            match b.get(j) {
                Some(b'[') => {
                    j += 1;
                    while j < b.len() && !(0x40..=0x7e).contains(&b[j]) {
                        j += 1;
                    }
                    if j < b.len() {
                        j += 1;
                    }
                }
                Some(b']') => {
                    j += 1;
                    while j < b.len() && b[j] != 0x07 {
                        j += 1;
                    }
                    if j < b.len() {
                        j += 1;
                    }
                }
                Some(_) => j += 1,
                None => {}
            }
            // Snap back to a character boundary: an ANSI scan can stop
            // mid-character when the sequence is followed by multi-byte
            // UTF-8, and slicing there panics. Found by running the live
            // corpus, not by any suite (A209-C15).
            while j < b.len() && !text.is_char_boundary(j) {
                j += 1;
            }
            i = j;
            continue;
        }
        let start = i;
        let mut end = i + 1;
        while end < b.len() && (b[end] & 0xC0) == 0x80 {
            end += 1;
        }
        out.push_str(&text[start..end]);
        for k in start..end {
            newmap.push(map.get(k).copied().unwrap_or(sentinel));
        }
        i = end;
    }
    newmap.push(sentinel);
    (out, newmap)
}

/// Decode JSON escapes (at any depth) and percent-encoding into a
/// normalized view, with an index map back to the original bytes.
///
/// Every sequence that DENOTES a line break becomes one — `\n`, `\r`,
/// `\t`, `%0A`, `%0D`, `%09` — regardless of how many backslashes precede
/// it, because a transcript round-trips tool input through JSON and a
/// value may be escaped once, twice or more.
///
/// **An unknown or malformed escape does NOT break.** A209-C9 corrects the
/// opposite rule, which this module asserted three times: breaking is the
/// **unsafe** direction here. Payload detection requires runs of a
/// repeating width, so more breaks means shorter segments means fewer
/// lines sharing a width means the block ships. Under-segmenting was what let a key through in
/// one specific bug; generalizing that into "fail toward a break" was
/// false, and it produced two leaks of its own — a window starved by 12
/// unknown escapes, and a single-line PKCS#8 body cut from 64 to 63
/// characters because the arm also ate the character that terminated the
/// escape. The backslash run is dropped and the terminating character is
/// KEPT, so a long run stays long.
///
/// Detection only: the masked span is cut from the ORIGINAL text via the
/// map, so the artifact keeps its own encoding and only the key is
/// replaced.
fn decode_transport(s: &str) -> Decoded {
    let bytes = s.as_bytes();
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let mut text = String::with_capacity(s.len());
    let mut map: Vec<usize> = Vec::with_capacity(s.len() + 1);
    let push = |text: &mut String, map: &mut Vec<usize>, ch: char, origin: usize| {
        let before = text.len();
        text.push(ch);
        for _ in before..text.len() {
            map.push(origin);
        }
    };
    let hex = |c: char| c.to_digit(16);
    let mut i = 0usize;
    while i < chars.len() {
        let (off, c) = chars[i];
        match c {
            '\\' => {
                // The terminating letter decides, not the depth.
                let mut k = i;
                while k < chars.len() && chars[k].1 == '\\' {
                    k += 1;
                }
                match chars.get(k).map(|(_, c)| *c) {
                    Some('n') | Some('r') | Some('t') => {
                        push(&mut text, &mut map, '\n', off);
                        i = k + 1;
                    }
                    Some('u') => {
                        let digits: String =
                            chars.iter().skip(k + 1).take(4).map(|(_, c)| *c).collect();
                        match u32::from_str_radix(&digits, 16)
                            .ok()
                            .and_then(char::from_u32)
                        {
                            Some(ch) if digits.len() == 4 => {
                                let ch = if ch == '\n' || ch == '\r' || ch == '\t' {
                                    '\n'
                                } else {
                                    ch
                                };
                                push(&mut text, &mut map, ch, off);
                                i = k + 5;
                            }
                            // Malformed \u: keep the run intact.
                            _ => {
                                push(&mut text, &mut map, 'u', off);
                                i = k + 1;
                            }
                        }
                    }
                    Some(other) => {
                        // Known literal escapes AND unknown ones alike:
                        // drop the backslashes, KEEP the character.
                        push(&mut text, &mut map, other, off);
                        i = k + 1;
                    }
                    // Dangling backslash run at end of input.
                    None => {
                        i = k;
                    }
                }
            }
            '%' => {
                let a = chars.get(i + 1).and_then(|(_, c)| hex(*c));
                let b = chars.get(i + 2).and_then(|(_, c)| hex(*c));
                match (a, b) {
                    (Some(hi), Some(lo)) => {
                        let byte = (hi * 16 + lo) as u8;
                        let ch = if byte == b'\n' || byte == b'\r' || byte == b'\t' {
                            '\n'
                        } else {
                            byte as char
                        };
                        push(&mut text, &mut map, ch, off);
                        i += 3;
                    }
                    _ => {
                        push(&mut text, &mut map, '%', off);
                        i += 1;
                    }
                }
            }
            // A LITERAL carriage return is a line break (CRLF); a
            // literal tab is intra-line whitespace and must not be one —
            // `-----BEGIN\tRSA PRIVATE KEY-----` is a banner, and
            // breaking on the tab split it in two (A209-C9). An ESCAPED
            // `\t` is different: there it is being used as a separator,
            // and it is handled in the backslash arm above.
            '\r' => {
                push(&mut text, &mut map, '\n', off);
                i += 1;
            }
            other => {
                push(&mut text, &mut map, other, off);
                i += 1;
            }
        }
    }
    map.push(bytes.len());
    // A209-C15: ANSI/control sequences are stripped HERE, beside
    // percent-encoding and JSON escapes, because that is what they are —
    // a transport artifact, not prose decoration. `bat`, `grep --color`
    // and any coloured CI log wrap the line they print, and round 15
    // tried to recognise that in the banner-decoration test instead. It
    // could not: an ANSI colour around an END line cost the block its
    // terminated status and, with it, DETECTION — 78 of 312 shapes shipped
    // a whole key at `masked=0`.
    let (text, map) = strip_ansi(text, map);
    let mut src_line_starts = vec![0usize];
    for (idx, b) in bytes.iter().enumerate() {
        if *b == b'\n' {
            src_line_starts.push(idx + 1);
        }
    }
    Decoded {
        text,
        map,
        src_line_starts,
    }
}

/// The payload candidate within a line: gutter removed, and anything from
/// a `-----` onwards dropped so a run abutting an END banner is recovered.
fn body_candidate(line: &str) -> &str {
    let t = strip_gutter(line).trim();
    // Renderers quote and concatenate each payload line. Node's
    // `console.log` prints `    'MIIEvg…\n' +`, YAML and TOML quote,
    // shell heredocs indent. A209-C9: the leading `'` was not stripped, so
    // every candidate in stock Node output began with a quote and failed
    // the base64 test — which is how a real RSA-2048 key came back
    // byte-identical.
    //
    // A209-C11: `+` was in BOTH trim sets, and `+` is a base64 character.
    // The final wrap of this corpus's RSA key at width 24 is the
    // 16-character `+zB+Rj4/f3UQilGJ`; trimmed to 15 it fell under span
    // extension's bar, the span stopped one line early, and the tail of a
    // real key shipped. Only NON-base64 decoration may be stripped here —
    // never `+`, `/` or `=`.
    let t = t.trim_start_matches(['-', '>', ' ', '\t', '"', '\'', '\\', ',']);
    let t = t.trim_end_matches([' ', '\t', '"', '\'', ',', ';']);
    match t.find("-----") {
        Some(at) => t[..at].trim(),
        None => t,
    }
}

/// The longest maximal base64 run in a line.
///
/// **This is the generator's cure** (A209-C10). Ten rounds enumerated
/// members of the transport set — line numbers, diff gutters, `> `
/// quoting, renderer quotes — and each round found the next member by
/// construction rather than by anticipation. The generator went unnamed
/// for all ten: **any per-line prefix defeats body detection**, because
/// the test required the WHOLE trimmed line to be base64. Prefixes are
/// unbounded, so enumerating them could never terminate.
///
/// Measured: of 21 realistic per-line decorations applied to a real
/// RSA-2048 PEM, **14 leaked the key verbatim** and the only 7 that masked
/// were the ones earlier rounds had hard-coded. Timestamps
/// (`docker logs -t`, `kubectl --timestamps`, GitHub Actions raw logs),
/// log levels, `debug1:` from `ssh -vvv`, pytest's `E       `, psql and
/// markdown table pipes, and `#`/`//`/`*` comment markers all defeated it.
///
/// Testing for a run INSIDE the line subsumes every prefix family at once,
/// found and unfound. The thresholds above it — `full_width || qualifying
/// >= 2` — are unchanged; they survived attack and are not the lever.
fn longest_base64_run(s: &str) -> usize {
    let (a, b) = longest_base64_run_span(s);
    b - a
}

/// Every maximal base64 run in a line, longest-first order not required.
///
/// Digest-shaped runs are excluded on the same length-conditional rule
/// [`longest_base64_run_span`] uses.
fn all_base64_runs(s: &str) -> Vec<&str> {
    let bytes = s.as_bytes();
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut run = 0usize;
    let mut pad = 0usize;
    let mut non_hex = false;
    let mut start = 0usize;
    let flush =
        |run: usize, pad: usize, non_hex: bool, start: usize, out: &mut Vec<(usize, usize)>| {
            let len = run + pad;
            let digest_shaped = !non_hex && len >= HEX_EXCLUSION_MIN;
            if len > 0 && !digest_shaped {
                out.push((start, start + len));
            }
        };
    for (idx, b) in bytes.iter().enumerate() {
        let is_core = b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/');
        if is_core {
            if pad > 0 {
                flush(run, pad, non_hex, start, &mut spans);
                run = 0;
                pad = 0;
                non_hex = false;
            }
            if run == 0 {
                start = idx;
            }
            if !b.is_ascii_hexdigit() {
                non_hex = true;
            }
            run += 1;
        } else if *b == b'=' && run > 0 && pad < 2 {
            pad += 1;
        } else {
            flush(run, pad, non_hex, start, &mut spans);
            run = 0;
            pad = 0;
            non_hex = false;
        }
    }
    flush(run, pad, non_hex, start, &mut spans);
    spans.into_iter().map(|(a, b)| &s[a..b]).collect()
}

fn longest_base64_run_span(s: &str) -> (usize, usize) {
    let bytes = s.as_bytes();
    let mut best = (0usize, 0usize);
    let mut run = 0usize;
    let mut pad = 0usize;
    let mut non_hex = false;
    let mut start = 0usize;
    // A run of pure hex is a DIGEST, not payload — a git SHA is 40
    // characters and a sha256 is 64, both inside the base64 alphabet, and
    // two of them near a banner mention would otherwise satisfy the
    // thresholds (A209-C10). Real PEM payload is base64 over the full
    // alphabet and effectively never all-hex across 40+ characters.
    //
    // A209-C11: the exclusion is LENGTH-CONDITIONAL. A short run of hex
    // characters is not a digest, it is a few characters that happen to be
    // hex. Excluding those unconditionally broke span extension: a key
    // hard-wrapped at 80 columns leaves 4-character continuation chunks,
    // and `5f50` is all hex, so the span treated a real key's own line as
    // prose and stopped there, shipping the remaining 15 lines. Runs below
    // the bar cannot reach any detection threshold anyway.
    let consider =
        |run: usize, pad: usize, non_hex: bool, start: usize, best: &mut (usize, usize)| {
            let len = run + pad;
            let digest_shaped = !non_hex && len >= HEX_EXCLUSION_MIN;
            if !digest_shaped && len > best.1 - best.0 {
                *best = (start, start + len);
            }
        };
    for (idx, b) in bytes.iter().enumerate() {
        let is_core = b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/');
        if is_core {
            // Padding may only trail a run; core after padding starts anew.
            if pad > 0 {
                consider(run, pad, non_hex, start, &mut best);
                run = 0;
                pad = 0;
                non_hex = false;
            }
            if run == 0 {
                start = idx;
            }
            if !b.is_ascii_hexdigit() {
                non_hex = true;
            }
            run += 1;
        } else if *b == b'=' && run > 0 && pad < 2 {
            pad += 1;
        } else {
            consider(run, pad, non_hex, start, &mut best);
            run = 0;
            pad = 0;
            non_hex = false;
        }
    }
    consider(run, pad, non_hex, start, &mut best);
    best
}

pub fn mask(text: &str) -> (String, BTreeSet<String>, u64) {
    let mut fired: BTreeSet<String> = BTreeSet::new();
    let mut masked = 0u64;
    // Armoured blocks first: they span lines, so the token walk cannot see
    // them whole.
    let text = mask_pem_blocks(text, &mut fired, &mut masked);
    let text = mask_emails(&text, &mut fired, &mut masked);

    let mut out = String::with_capacity(text.len());
    let mut consumed = 0usize;
    loop {
        let rest = &text[consumed..];
        if rest.is_empty() {
            break;
        }
        let Some(start) = rest.find(is_token_char) else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let end = tail.find(|c: char| !is_token_char(c)).unwrap_or(tail.len());
        let token = &tail[..end];
        let abs = consumed + start;
        let position = position_of(&text[..abs], token, &text[abs + token.len()..]);
        match detector_for(token, position) {
            Some(name) => {
                out.push_str(&format!("[masked: {name}]"));
                fired.insert(name.to_string());
                masked += 1;
            }
            None => out.push_str(token),
        }
        consumed = abs + token.len();
    }
    (out, fired, masked)
}

/// Mask a batch of strings in place, returning the combined treatment record.
pub fn mask_all(texts: &mut [&mut String]) -> Treatment {
    let mut fired: BTreeSet<String> = BTreeSet::new();
    let mut masked = 0u64;
    for text in texts.iter_mut() {
        let (new, f, n) = mask(text);
        if n > 0 {
            **text = new;
            fired.extend(f);
            masked += n;
        }
    }
    Treatment::ran(masked, fired)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_formats_are_masked_and_named_in_any_position() {
        for (input, expect) in [
            ("AKIAIOSFODNN7EXAMPLE", "aws"),
            ("ghp_16CharsOfNonsense0000000000000000", "github"),
            ("sk-ant-api03-abcdefghijklmnop", "anthropic"),
            ("xoxb-123-456-abcdef", "slack"),
            // A bare `-----BEGIN` is NOT a key — see mask_pem_blocks and
            // the 70 live records that pin why. A real banner is.
            // A banner needs a BODY to be a key (A209-C6/B8-3): the bare
            // form is prose about keys, and firing on it destroyed 37 real
            // records. With key material behind it, it still fires.
            (
                "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEATLCANARYaaZZ11bbYY22ccXX33ddWW44eeVV55ffUU66gg77\n-----END RSA PRIVATE KEY-----",
                "private_key_block",
            ),
        ] {
            let (out, fired, n) = mask(input);
            assert_eq!(n, 1, "{input} was not masked");
            assert!(fired.contains(expect), "{input} reported {fired:?}");
            assert!(!out.contains(input), "the original survived the mask");
        }
    }

    #[test]
    fn a_secret_hiding_inside_a_url_is_still_found() {
        for input in [
            "git push https://ghp_ZZZZaaaa1111bbbb2222cccc3333dddd@github.com/x/y",
            "AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE",
            "Authorization: sk-ant-api03-abcdefghijklmnop",
        ] {
            let (out, fired, n) = mask(input);
            assert!(n >= 1, "a secret hid inside {input:?} and was not found");
            assert!(!fired.is_empty());
            assert!(out.contains("[masked:"), "{out}");
        }
    }

    #[test]
    fn d1_entropy_never_fires_in_prose_but_still_fires_in_value_positions() {
        // The defect that made this rule: a real filename, destroyed. It is
        // high-entropy by every measure and it is exactly the context a fresh
        // session needs.
        for context in [
            "NotificationDigestV2ComponentWrapper.tsx",
            "edited NotificationDigestV2ComponentWrapper.tsx twice",
            "apps/mobile/components/NotificationDigestV2ComponentWrapper.tsx",
            "src/RevenueCatSubscriptionGatewayAdapter2.ts",
        ] {
            let (out, _, n) = mask(context);
            assert_eq!(n, 0, "prose/filename was masked: {context} -> {out}");
        }
        // But a value position still gets the entropy rule.
        let secret = "f3Kq9ZbW2xLpR7vN4tJc8mHd5sYgA1eU6oI0";
        for value in [
            format!("TOKEN={secret}"),
            format!("api_key: {secret}"),
            format!("\"{secret}\""),
        ] {
            let (out, fired, n) = mask(&value);
            assert_eq!(n, 1, "a value-position secret was missed: {value} -> {out}");
            assert!(fired.contains("high_entropy"));
        }
    }

    #[test]
    fn d4_the_measured_spills_are_detected() {
        // Both of these actually travelled in a live capsule (A174). They are
        // in the module's measured-misses list and they are tested here.
        let (out, fired, n) = mask("git -c user.email=\"dana.fox@example.com\" commit");
        assert!(n >= 1 && fired.contains("email"), "{out}");
        assert!(!out.contains("dana.fox@example.com"));
        let (out, fired, n) =
            mask("supabase functions deploy revenuecat-webhook --project-ref qmfxkvtjwzbnlcdhr");
        assert!(n >= 1 && fired.contains("flag_value"), "{out}");
        assert!(!out.contains("qmfxkvtjwzbnlcdhr"));
        // The command's shape survives — a masked ref must not eat the verb.
        assert!(out.contains("supabase functions deploy revenuecat-webhook"));
    }

    #[test]
    fn a_sentence_final_email_is_masked_without_eating_the_period() {
        let (out, fired, n) = mask("Read reader@example.org. Then check the file.");
        assert_eq!(out, "Read [masked: email]. Then check the file.");
        assert_eq!(n, 1);
        assert!(fired.contains("email"));
    }

    /// A209's measured miss: a known prefix behind `_`, `.` or `-` inside a
    /// token. Every one of these returned masked=0 before the repair.
    #[test]
    fn the_a209_measured_miss_is_masked_at_every_boundary() {
        const GH: &str = "ghp_TLCANARYaaZZ11bbYY22ccXX33ddWW44e";
        let aws = "AKIAIOSFODNN7EXAMPLE";
        for (input, expect) in [
            (format!("/tmp/repo/secrets_{GH}.txt"), "github"),
            (format!("/tmp/key_{aws}.txt"), "aws"),
            (format!("backup.{GH}.bak"), "github"),
            (format!("prefix-{GH}"), "github"),
            (format!("KEY_{aws}"), "aws"),
            // The shape the capsule's unresolved-work category actually emits.
            (
                format!("3 edits after the last commit — unresolved · /r/secrets_{GH}.txt"),
                "github",
            ),
        ] {
            let (out, fired, n) = mask(&input);
            assert_eq!(n, 1, "not masked: {input} -> {out}");
            assert!(fired.contains(expect), "{input} reported {fired:?}");
            assert!(
                !out.contains(GH) && !out.contains(aws),
                "the secret survived: {out}"
            );
        }
    }

    /// The other half of the same repair, and the one A178 paid for.
    ///
    /// **This test's first version sampled only the easy half** — every
    /// specimen put `sk-`/`eyJ` mid-alphanumeric-run, which the boundary
    /// rule handles by construction — while asserting a general property.
    /// An independent adversarial corpus then measured the half it skipped:
    /// with the prefix sitting AT a separator, the boundary rule alone
    /// masked **38 real `data-sk-*` tokens** from a live project as
    /// `openai`. The separator-adjacent specimens are now first-class here,
    /// which is what makes the test's name true.
    #[test]
    fn a209_the_boundary_rule_does_not_re_commit_a178s_over_masking() {
        for safe in [
            // Mid-run: the original half.
            "task-list.md",
            "src/tasks/task-list.md",
            "disk-usage.txt",
            "risk-analysis-2026.md",
            "monkeyJump.tsx",
            "components/keyJson.ts",
            "apps/mobile/lib/revenuecat.ts",
            "packages/core/src/scheduling/NotificationBatchScheduler2.test.ts",
            "cd /Users/someone/dev/atlas_playstore && git status --porcelain",
            "-Users-dev-Desktop-Telescope-project",
            // AT a separator: the measured half. Real strings, live corpus.
            "data-sk-seg",
            "data-sk-provider",
            "data-sk-drop=\"gusto\"",
            "data-sk-flag=\"f1\"",
            "id=\"pop-sk-julio\"",
            "sk-veil",
            "regions/ASIA.json",
            "REGION_ASIA_PACIFIC",
            "-ASIA-EAST",
            "src/sk-utils.ts",
            "Kaizala-AIza-mock",
            "data-testid=\"task-list\"",
            // Case-sensitivity is intent, not accident: matching `ASIA`
            // case-insensitively would mask every IANA Asia timezone.
            "Asia/Kolkata America/New_York",
            "TZ=Asia/Kolkata",
            "SK-2026 sprint SK_TOTAL ASK-42",
            // The two masker-as-validity-gate call sites (annotate.rs,
            // language.rs) reject any value the masker touches, so a false
            // positive here refuses valid input rather than masking it.
            "risk-score task-board disk-usage",
            "deepseek-r1:8b qwen2.5:7b-instruct nli-deberta-v3-xsmall",
        ] {
            let (out, fired, n) = mask(safe);
            assert_eq!(n, 0, "over-masked: {safe} -> {out} (fired {fired:?})");
            assert_eq!(out, safe);
        }
    }

    /// A prefix with no plausible body is a word, not a credential — the
    /// cure the adversarial corpus recommended and the operator's own
    /// scrubbing regexes corroborate. `-----BEGIN` is exempt by design: the
    /// banner IS the signal.
    #[test]
    fn a209_a_known_prefix_without_a_body_is_not_a_secret() {
        for bare in [
            "sk-", "sk-ant-", "ghp_", "AKIA", "ASIA", "eyJ", "AIza", "xoxb-",
        ] {
            let (out, _, n) = mask(bare);
            assert_eq!(n, 0, "a bare prefix masked: {bare} -> {out}");
        }
        // But the banner with no body still fires, and so does a real body.
        // A bare `-----BEGIN` no longer fires: measured over-masking, and
        // a banner that names nothing names no key. Nor does a NAMED
        // banner with no key material behind it (A209-C6/B8-3) — that is
        // a comment about keys, and 37 live records proved it.
        assert_eq!(mask("-----BEGIN").2, 0);
        assert_eq!(mask("-----BEGIN RSA PRIVATE KEY-----").2, 0);
        assert_eq!(
            mask("-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEATLCANARYaaZZ11bbYY22ccXX33ddWW44eeVV55ffUU66gg77\n-----END RSA PRIVATE KEY-----").2,
            1
        );
        assert_eq!(mask("sk-ant-api03-abcdefghijklmnop").2, 1);
        assert_eq!(mask("AKIAIOSFODNN7EXAMPLE").2, 1);
    }

    #[test]
    fn ordinary_prose_and_paths_survive_untouched() {
        for safe in [
            "Make the notification digest work",
            "cd /Users/me/dev/project && git push origin main",
            "the build succeeded in 42 seconds",
            "src/components/NotificationDigest.tsx",
            "https://example.com/docs/getting-started",
            "55 edits after the last commit — unresolved",
        ] {
            let (out, _, n) = mask(safe);
            assert_eq!(n, 0, "ordinary text was masked: {safe} -> {out}");
            assert_eq!(out, safe);
        }
    }

    #[test]
    fn the_products_own_receipts_are_never_masked() {
        for identifier in [
            "36e7c68565cc2179eb76be85cf160a869e81864ea8e70b13f8761f59a847c895",
            "4f5d8b47-ec14-45b7-9a6a-206a18f43417",
            "01KZVXY6PPPJPK837HCKC9MWJ8",
        ] {
            let (out, _, n) = mask(identifier);
            assert_eq!(n, 0, "a published identifier was masked: {out}");
        }
        let line = "compiled at chain position 36e7c68565cc2179eb76be85cf160a869e81864ea8e70b13f8761f59a847c895 over 17708 recorded events";
        assert_eq!(mask(line).2, 0);
    }

    #[test]
    fn d3_the_gate_is_closed_by_default_and_for_unscanned_input() {
        let d = Treatment::default();
        assert!(matches!(d, Treatment::NoDetectorRan { .. }));
        assert!(d.needs_acknowledgement(), "the default gate opened");
        assert!(d.statement().contains("no detector ran"));
        // A real run that matched nothing: gate open, and a different type.
        let none = Treatment::none();
        assert!(matches!(none, Treatment::Ran { .. }));
        assert!(!none.needs_acknowledgement());
        assert_ne!(d, none, "the two states must not be equal");
        // A producer claiming a run but naming no detector: gate closed.
        let nameless = Treatment::Ran {
            detector: "  ".to_string(),
            masked: 0,
            patterns: vec![],
            statement: "x".to_string(),
        };
        assert!(
            nameless.needs_acknowledgement(),
            "an unnamed detector opened the gate"
        );
        let mut s = String::from("key AKIAIOSFODNN7EXAMPLE");
        let t = mask_all(&mut [&mut s]);
        assert!(t.needs_acknowledgement());
        assert_eq!(t.masked(), 1);
    }

    #[test]
    fn no_treatment_state_ever_promises_absence() {
        for t in [Treatment::default(), Treatment::none()] {
            let s = t.statement();
            assert!(
                !s.contains("excluded by default") && !s.contains("scrubbed"),
                "a treatment statement drifted into a promise: {s}"
            );
        }
    }
}

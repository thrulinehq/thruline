//! §5v — the one voice.
//!
//! Every first-level string the product shows a person is produced here. This
//! module is the executable form of `docs/UX-DOCTRINE.md`'s translation table;
//! doctrine is the source, this is the mirror, and the tables below are
//! transcribed from it rather than invented.
//!
//! Two rules give the module its point:
//!
//! 1. **Engine lexicon is banned at first level.** [`banned_in`] is the
//!    predicate, [`BANNED`] the list, and the tests walk every string this
//!    module can emit plus every string the CLI's human output can emit. A
//!    violation names the offending string rather than failing silently.
//! 2. **No naked numbers.** A count reaches a person only through
//!    [`count_phrase`] or [`composition`], both of which carry a noun and both
//!    of which drop zero-count elements instead of rendering "0 of something".
//!
//! Engine detail is not deleted anywhere in here. It lives exactly one
//! disclosure deeper, unchanged — `thruline why` prints it verbatim.

use serde::Serialize;

use crate::attention::AttentionKind;
use crate::capture::ProofApplicability;
use crate::capture_product::CaptureReadiness;
use crate::derive::{Currency, NodeState};

/// Pipeline jargon. No plain-English use exists, so these are banned as
/// substrings anywhere in a first-level string, case-insensitively.
///
/// `unverified` and `confidence` joined at C0.2 (the continuity vocabulary
/// amendment): "Unverified" hides no-proof-found, insufficient-evidence, and
/// not-evaluated behind one accusatory bucket, and a confidence number is the
/// exact thing coverage counts exist to replace.
pub const JARGON: &[&str] = &[
    "entailment",
    "fidelity",
    "matched pair",
    "matched-pair",
    "nli",
    "deterministic · local",
    "unverified",
    "confidence",
];

/// The raw verdict enums, each of which has a composition word in [`VERDICTS`].
///
/// These are NOT banned as substrings, and the distinction is the whole point.
/// "Installation not verified" is a plain-English sentence a non-developer
/// reads correctly; banning the substring would force copy into worse English
/// in the name of a rule meant to produce better English. What the doctrine
/// actually forbids is the enum used as the *name of a state* — see
/// [`banned_in`] for the two shapes that takes.
///
/// `supported` and `contradicted` left this list at C0.2: the continuity
/// vocabulary amendment makes them the doctrine-adopted rendered words, so a
/// `Supported` chip is now the reviewed label, not a leak. The ban's purpose —
/// unreviewed engine vocabulary reaching a first level — is unchanged for the
/// eight that remain.
pub const ENUMS: &[&str] = &[
    "unsupported",
    "dropped",
    "pending",
    "recorded",
    "refused",
    "unknown",
    "verified",
    "stale",
];

/// Everything the ban covers, for the mirror and for reporting.
pub const BANNED: &[&str] = &[
    "entailment",
    "fidelity",
    "matched pair",
    "matched-pair",
    "nli",
    "deterministic · local",
    "unverified",
    "confidence",
    "unsupported",
    "dropped",
    "pending",
    "recorded",
    "refused",
    "unknown",
    "verified",
    "stale",
];

/// Where a chip, badge, or count would put a label: the whole string, or one
/// `·`/comma/colon-separated part of it, once punctuation and any leading
/// count are stripped. `Refused 0` and `Contradicted: 3` both reduce here.
fn label_slots(text: &str) -> impl Iterator<Item = &str> {
    text.split(['·', ',', ':', ';', '\n', '(', ')'].as_ref())
        // One trim from both ends: punctuation, whitespace, and the count —
        // whichever side the count sits on. `Refused 0` and `0 Refused` are
        // the same leak, and a rule that caught only one would be a rule the
        // next paper cut walks around.
        .map(|seg| seg.trim_matches(|c: char| !c.is_alphabetic()))
}

/// True when `idx` begins a sentence. Sentence-initial capitalization says
/// nothing about whether a word is a state name, so the label heuristic must
/// not read it as one — "Supported Codex commands…" is prose.
fn starts_sentence(text: &str, idx: usize) -> bool {
    let before = text[..idx].trim_end();
    before.is_empty() || before.ends_with(['.', '!', '?', ':', ';', '—', '–', '\n'])
}

/// The banned term a first-level string contains, if any.
///
/// Two failure modes, because they are the two ways engine vocabulary has
/// actually leaked in this codebase:
///
/// 1. **Jargon**, anywhere. `fidelity` in a sentence is `fidelity` on screen.
/// 2. **A verdict enum used as a label.** Either the whole string (or a
///    ` · `/comma-separated segment of it) *is* the enum — the `Contradicted`
///    chip, the `Refused 0` badge — or the enum appears capitalized mid-text,
///    which is the tell of a state name leaking into prose ("3 Contradicted
///    claims"). Lowercase use inside real English is left alone.
pub fn banned_in(text: &str) -> Option<&'static str> {
    let hay = text.to_ascii_lowercase();
    if let Some(term) = JARGON.iter().copied().find(|term| hay.contains(term)) {
        return Some(term);
    }
    // Shape one: the string, or a part of it, IS the enum.
    for slot in label_slots(text) {
        let lower = slot.to_ascii_lowercase();
        if let Some(term) = ENUMS.iter().copied().find(|e| **e == lower) {
            return Some(term);
        }
    }
    // Shape two: the enum appears capitalized mid-sentence, which is a state
    // name leaking into prose.
    let mut idx = 0usize;
    for piece in text.split_inclusive(char::is_whitespace) {
        let token = piece.trim_matches(|c: char| !c.is_alphanumeric());
        let offset = idx + piece.find(token).unwrap_or(0);
        idx += piece.len();
        if !token.chars().next().is_some_and(char::is_uppercase) {
            continue;
        }
        if starts_sentence(text, offset) {
            continue;
        }
        let lower = token.to_ascii_lowercase();
        if let Some(term) = ENUMS.iter().copied().find(|e| **e == lower) {
            return Some(term);
        }
    }
    None
}

/// The [`banned_in`] census, over a line of **rendered output** that may carry
/// filesystem paths the product did not author.
///
/// # The class this closes (Track Q, D24)
///
/// Five guards assert that engine vocabulary never reaches a user-facing
/// surface, and each searched the whole rendered string — including the
/// absolute paths that surface legitimately prints. A gate run failed when
/// `mktemp` produced `.tmpNLIrdo` and the census matched `nli` inside the
/// directory's own random name; Q then seeded `TMPDIR` with census words and
/// turned one instance into five, deterministically.
///
/// **The census is about words the product wrote.** A path is chosen by the
/// operating system, the user, or a test harness, and asserting that ThruLine's
/// voice governs it is asserting something ThruLine does not control. A guard
/// that fails at random is a guard people learn to re-run, which is how the
/// next real failure gets waved through.
///
/// # What is stripped, and what deliberately is not
///
/// Whitespace-delimited tokens that begin with `/` after leading punctuation is
/// trimmed — POSIX absolute paths, which is what this product prints. A
/// relative path (`docs/STATUS.md`) is **not** stripped: those appear in
/// authored sentences, the product chooses them, and they are exactly as
/// governed as any other word.
///
/// Every other word survives. `hooks`, `settings.json` and the rest of a line
/// stay in the search space; only the run the operating system chose leaves it.
pub fn banned_in_output(text: &str) -> Option<&'static str> {
    banned_in_owned(without_paths(text))
}

/// Every absolute-path run removed, so a census reads only what the product
/// wrote.
///
/// Exposed separately because the class is wider than `banned_in`: Track Q's
/// fifth guard is a bare `text.to_lowercase().contains("verified")`, a
/// different call with the same mistake. Two ways of stripping paths would be
/// two ways of disagreeing about what a path is.
///
/// # A run, not a token — and the first version got this wrong
///
/// It filtered whitespace-delimited tokens that *begin* with `/`. Running Q's
/// own command against that version found a sixth instance neither Q nor I had
/// listed: `a207_check_wrapper.rs` prints a scope as
/// `bash:/private/tmp/…/repo:./test.sh`, where the path is embedded in a token
/// that begins with `bash:`. So this scans for a `/` and removes to the next
/// whitespace, wherever in the token it starts.
///
/// A **relative** path is untouched — `docs/STATUS.md` has no leading `/`, the
/// product chooses it, and it is as governed as any other word. A word after an
/// interior slash goes with the run (`supported/unsupported` leaves
/// `supported`); that only ever removes characters, never adds them, so a
/// census can lose a duplicate mention and never gain a false one.
pub fn without_paths(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut prev: Option<char> = None;
    while let Some(c) = chars.next() {
        // A run is ABSOLUTE when its `/` does not continue a word. `bash:/tmp`
        // and `"/tmp` are paths; `docs/STATUS.md` is a word with a slash in it.
        let absolute = c == '/'
            && !prev.is_some_and(|p| p.is_alphanumeric() || p == '.' || p == '-' || p == '_');
        if !absolute {
            out.push(c);
            prev = Some(c);
            continue;
        }
        while chars.peek().is_some_and(|n| !n.is_whitespace()) {
            chars.next();
        }
        out.push(' ');
        prev = Some(' ');
    }
    out
}

/// The borrow-checker seam for [`banned_in_output`]: [`banned_in`] returns a
/// `&'static str` from its own tables, so the temporary it searched may be
/// dropped safely.
fn banned_in_owned(text: String) -> Option<&'static str> {
    banned_in(&text)
}

/// One state's first-level presentation: the word it wears in a chip or badge,
/// and the sentence it speaks in an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Term {
    /// The engine slug. Present so the mirror can be keyed; never displayed.
    pub slug: &'static str,
    /// The lowercase composition word — the ONLY form allowed in a chip,
    /// badge, or count. "3 supported · 1 no proof found" is built from these.
    pub word: &'static str,
    /// The first-level sentence.
    pub sentence: &'static str,
}

/// The ten-verdict vocabulary, transcribed from UX-DOCTRINE. Five words re-cut
/// by the C0.2 continuity amendment: supported · contradicted · no proof found
/// · insufficient evidence · not evaluated.
pub const VERDICTS: &[Term] = &[
    Term {
        slug: "verified",
        word: "proven",
        sentence: "Proven — the run itself was captured as it happened.",
    },
    Term {
        slug: "supported",
        word: "supported",
        sentence: "Backed by the record.",
    },
    Term {
        slug: "unsupported",
        word: "no proof found",
        sentence: "No proof of this in the record. It may still be true — we just can't show it.",
    },
    Term {
        slug: "contradicted",
        word: "contradicted",
        sentence: "The AI said this was done — the record shows otherwise.",
    },
    Term {
        slug: "unknown",
        word: "insufficient evidence",
        sentence: "There's evidence here, but it couldn't be read well enough to judge.",
    },
    Term {
        slug: "stale",
        word: "out of date",
        sentence: "This was proven — but the project has changed since.",
    },
    Term {
        slug: "dropped",
        word: "abandoned",
        sentence: "Planned, then never touched again.",
    },
    Term {
        slug: "pending",
        word: "unfinished",
        sentence: "Planned — not finished yet.",
    },
    Term {
        slug: "recorded",
        word: "noted",
        sentence: "This happened.",
    },
    Term {
        slug: "refused",
        word: "not evaluated",
        sentence: "Not evaluated — this one sits outside what we check.",
    },
];

/// Whether a node still describes the project as it stands. Separate from
/// whether it was ever true — conflating the two is how a product starts
/// telling people old news is wrong news.
pub const CURRENCIES: &[Term] = &[
    Term {
        slug: "current",
        word: "current",
        sentence: "Still current.",
    },
    Term {
        slug: "superseded",
        word: "replaced",
        sentence: "Replaced by later work.",
    },
    Term {
        slug: "out_of_scope",
        word: "out of scope",
        sentence: "No longer part of this project.",
    },
];

/// The two sentences the doctrine names for evidence that exists but resists
/// judgment. Keyed by the engine's own reason strings.
/// The ledger's own condition, split (A150, P1). Two facts the old boolean
/// conflated: the chain's arithmetic, and whether the original files are
/// still on disk to re-read. "Sources gone" is a statement about the vendor's
/// disk, not about this record — the reading survives here either way.
pub const LEDGER_STATES: &[Term] = &[
    Term {
        slug: "chain_checked",
        word: "checked",
        sentence: "Every link in the record was re-checked and adds up.",
    },
    Term {
        slug: "chain_damaged",
        word: "damaged",
        sentence: "The record does not add up — something changed it after it was written.",
    },
    Term {
        slug: "sources_swept",
        word: "sources gone",
        sentence: "Some original files are no longer on disk. The reading survives here; the originals cannot be re-read.",
    },
    Term {
        slug: "never_checked",
        word: "unchecked",
        sentence: "Not re-checked yet.",
    },
];

/// The ledger-state term for a slug, abstaining to `never_checked`.
pub fn ledger_state(slug: &str) -> &'static Term {
    LEDGER_STATES
        .iter()
        .find(|t| t.slug == slug)
        .unwrap_or(&LEDGER_STATES[3])
}

pub const ABSTENTIONS: &[Term] = &[
    Term {
        slug: "too_unspecific",
        word: "just a thought",
        sentence: "Just a passing thought — nothing concrete to track. Nothing to worry about.",
    },
    Term {
        slug: "uninterpretable_outcome",
        word: "unreadable",
        sentence: "Something ran here, but the output couldn't be read.",
    },
];

/// Whether a proof still matches the project it was taken from.
pub const PROOF_CURRENTNESS: &[Term] = &[
    Term {
        slug: "current",
        word: "current",
        sentence: "Current as of the last check.",
    },
    Term {
        slug: "stale",
        word: "out of date",
        sentence: "This was proven — but the project has changed since.",
    },
    Term {
        slug: "unknown",
        word: "unclear",
        sentence: "No later check to compare this against.",
    },
];

/// Level two of the `why` disclosure: what happened, one sentence per kind.
///
/// Transcribed from UX-DOCTRINE's `why`-shape table. The last row is not a
/// verdict — it is what `why` says about an id that no longer derives because
/// its subject changed. Its dismissal record is permanent, so the question
/// stays answerable; history is never rewritten to make an answer unavailable.
pub const DISCLOSURES: &[Term] = &[
    Term {
        slug: "contradiction",
        word: "contradicted",
        sentence: "The record holds both the claim and a run that disagrees with it.",
    },
    Term {
        slug: "staleness",
        word: "out of date",
        sentence: "A later check compared the project against the state this proof was taken from, and found it changed.",
    },
    Term {
        slug: "capture_attention",
        word: "needs a person",
        sentence: "This is about how work gets recorded on this machine, not about the work itself.",
    },
    Term {
        slug: "all_clear",
        word: "all clear",
        sentence: "Nothing in the record needs a person right now.",
    },
    Term {
        slug: "expired",
        word: "no longer current",
        sentence: "This no longer needs you — what it described has changed since.",
    },
];

/// SNAPSHOT-CHILD-TIMEOUT (D-051) — what `thruline snapshot` prints on its
/// processes line when `ps` did not finish within the snapshot's time limit.
/// The stored doc names the read in `timed_out` and records no count; this is
/// the same fact in a person's words, in place of a number that was never read.
pub const SNAPSHOT_NOT_READ_IN_TIME: &str =
    "not read — the process list did not arrive within the time limit";

/// T-2-1 — the Change layer's first-level words (`docs/loop/03-USER-LENS.md`
/// rule 2's vocabulary list, transcribed).
///
/// Every human-readable label the `/api/changes` routes serve comes from here,
/// so the served label and the stored column name are allowed to differ: the
/// column may say `snapshot_id`, the label says "the exact version we looked
/// at". The engine nouns the lens bans — candidate, applicability, disposition,
/// packet version, ledger, session id — appear in no `word` or `sentence`
/// below, and `the_change_labels_use_sams_words` in
/// `crates/core/tests/change_records.rs` is the check that says so.
///
/// **Deliberately not in [`mirror_module`].** The UI mirror is generated and
/// compared byte for byte; adding a table to it would require regenerating a
/// file under `ui/src`, and this task builds no surface. The labels join the
/// mirror in the task that first draws them.
pub const CHANGE_LABELS: &[Term] = &[
    Term {
        slug: "change",
        word: "Change",
        sentence: "One bounded piece of work you are being asked to accept.",
    },
    Term {
        slug: "request",
        word: "request",
        sentence: "What was asked for, in the words it was asked in, and who asked.",
    },
    Term {
        slug: "accepted_scope",
        word: "what counts as done",
        sentence: "The list this work will be judged against.",
    },
    Term {
        slug: "scope_acceptance",
        word: "who agreed this list",
        sentence: "Agreeing what counts as done is not approving the work itself.",
    },
    Term {
        slug: "implementation",
        word: "what changed",
        sentence: "The project and the one exact version of the work this is about.",
    },
    Term {
        slug: "association",
        word: "work linked to this Change",
        sentence: "A piece of the record that someone or something says belongs here.",
    },
    Term {
        slug: "suggested",
        word: "suggested link",
        sentence: "We think this belongs here. Nothing is carried over until you say it does.",
    },
    Term {
        slug: "confirmed",
        word: "confirmed link",
        sentence: "You said this belongs to this Change.",
    },
    Term {
        slug: "rejected_link",
        word: "not part of this Change",
        sentence: "You said this work does not belong to this Change.",
    },
    Term {
        slug: "examination_none_yet",
        word: "still needs evidence",
        sentence: "Nothing has been checked for this Change yet.",
    },
    Term {
        slug: "decision_none_yet",
        word: "no decision yet",
        sentence: "Nobody has approved this or asked for changes.",
    },
];

/// One Change label by slug, or `None` when the slug is not one of ours.
pub fn change_label(slug: &str) -> Option<&'static Term> {
    CHANGE_LABELS.iter().find(|t| t.slug == slug)
}

/// One served sentence in both registers (T-2-2).
///
/// [`Term`] carries a chip word and one sentence, which is the right shape for
/// a *state*. A link between a session and a Change is not a state; it is
/// something the tool says to a person, and the same fact has to be sayable
/// twice: once in the words Sam uses, and once in the words a reviewer reading
/// the technical layer needs. Keeping both in one row is what stops the two
/// registers from drifting into two different claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Line {
    /// The engine slug. Keyed on, never displayed.
    pub slug: &'static str,
    /// Sam's register. Everyday English, no engine noun, no accusation.
    /// `{count}`, `{files}` and `{version}` are the only placeholders,
    /// rendered by [`association_sentence`].
    pub plain: &'static str,
    /// One level down: the same fact with the record's own nouns in it.
    pub technical: &'static str,
}

/// Every string T-2-2 serves about a session ↔ Change link.
///
/// Nothing in `change_link.rs` or the CLI builds a sentence; each one is
/// looked up here. The two "not counted" families are the `B3.13` refusal
/// speaking — the tool declines to carry a result across a link, and says
/// which of the six reasons applies, because "not counted" with no reason is
/// the disabled control with no explanation the lens forbids.
pub const ASSOCIATION_LINES: &[Line] = &[
    Line {
        slug: "suggestion_timing",
        plain: "This session might belong to this Change: it was open while this Change was being worked on.",
        technical: "Proposed on a timing basis: the session's capture window overlaps the Change's activity window. Provenance is `suggested`; no result crosses it.",
    },
    Line {
        slug: "suggestion_filename",
        plain: "This session might belong to this Change: it changed {count} of the same files.",
        technical: "Proposed on a filename basis: {files} of the session's changed paths fall inside the accepted scope's allowed paths. Provenance is `suggested`; no result crosses it.",
    },
    Line {
        slug: "suggestion_tie",
        plain: "This session matches in two ways, by time and by the files it changed. We are not picking one for you.",
        technical: "The session matched both the timing and the filename basis. Both proposals are recorded; neither is ranked and no tie-break is applied.",
    },
    Line {
        slug: "suggestion_none",
        plain: "Nothing in the record looks like it belongs to this Change yet.",
        technical: "No session in this project overlapped the Change's activity window or touched a path inside its allowed scope. Zero proposals, which is an answer, not a failure.",
    },
    Line {
        slug: "session_undated",
        plain: "We cannot tell when this session ran, so we did not compare it by time.",
        technical: "The session carries no usable UTC timestamp on any event and none on the session row, so the timing basis was skipped for it. It is named rather than dropped.",
    },
    /* What the whole answer left out, said once, with the count from a typed
     * field. `T-2-8-F3`: the line above explains ONE session, and a person
     * reading a report with no suggestions in it was told "nothing looks like
     * it belongs" — an empty result where the truth was an unread one. This is
     * the sentence that says so, and it is printed whether or not anything was
     * suggested. */
    Line {
        slug: "session_undated_summary",
        plain: "{items} could not be placed in time, so {they} not considered.",
        technical: "{items} carry no timestamp this matcher can normalise to UTC, so the timing basis was skipped for {them} entirely. They are named above rather than dropped, and no proposal was made or withheld on their behalf.",
    },
    Line {
        slug: "scope_pattern_unreadable",
        plain: "One line in what this Change covers is not a file path we understand, so nothing was matched against it.",
        technical: "An allowed-scope entry is neither an exact path, a directory prefix, nor a `dir/**` subtree. It matched nothing and is listed rather than silently skipped.",
    },
    Line {
        slug: "scope_pattern_unreadable_summary",
        plain: "{lines} of what this Change covers could not be read as a file path, so nothing was matched against {them}.",
        technical: "{lines} of the allowed scope are not an exact path, a directory prefix or a `dir/**` subtree, so the filename basis matched nothing against {them}. They are named above rather than treated as matching everything or nothing.",
    },
    Line {
        slug: "link_none",
        plain: "No Change is linked to this session yet.",
        technical: "No row in the association table names this session. Old work stays listed and openable exactly as before; nothing was migrated and no criteria were invented for it.",
    },
    Line {
        slug: "confirm_needs_actor",
        plain: "We cannot confirm a link without a name. Say who is confirming it.",
        technical: "A `confirmed` or `rejected` association requires basis `user` and a non-empty actor. The database CHECK refuses the row; this is that refusal in words.",
    },
    Line {
        slug: "not_applicable_no_link",
        plain: "Not counted for this Change: the session isn't confirmed as part of it.",
        technical: "No association row exists for this (Change, session) pair. Results are not attached across an absent link.",
    },
    Line {
        slug: "not_applicable_only_suggested",
        plain: "Not counted for this Change: this link is still a suggestion. Nothing is carried over until you confirm it.",
        technical: "The standing provenance for this pair is `suggested`. `B3.12`: a suggestion from timing or filenames never becomes a confirmation without an explicit human act.",
    },
    Line {
        slug: "not_applicable_rejected",
        plain: "Not counted for this Change: you said this session does not belong here.",
        technical: "The standing provenance for this pair is `rejected`, asserted by a named actor. The suggestion it answers is still on file, unedited.",
    },
    Line {
        slug: "not_applicable_no_scope",
        plain: "Not counted for this Change: nobody has written down what counts as done, so there is nothing to check this work against.",
        technical: "The Change has no scope revision with allowed paths, so the scope check `B3.13` requires cannot be performed. Refused rather than assumed.",
    },
    Line {
        slug: "not_applicable_outside_scope",
        plain: "Not counted for this Change: the files this session changed are outside what this Change covers.",
        technical: "Every changed path in the session falls outside the accepted scope's allowed paths. A confirmed link alone does not carry a result across a scope boundary.",
    },
    Line {
        slug: "not_applicable_no_changed_paths",
        plain: "Not counted for this Change: the record does not show which files this session changed, so we cannot tell whether it is inside what this Change covers.",
        technical: "No file-editing tool call with a readable path was found in the session, so the scope check has no input. Missing evidence is reported as missing, never as a pass.",
    },
    Line {
        slug: "not_applicable_check_for_another_change",
        plain: "Not counted for this Change: this check was run for a different Change.",
        technical: "The session is one captured execution, and the `change_check_sessions` row names another Change as the one it was run for. An execution is read under the Change it was run for and no other.",
    },
    Line {
        slug: "applies_examination",
        plain: "Counted for this Change: you ran this check for it.",
        technical: "The session is one captured execution (`change_check_sessions`), its recorded Change is this one, and the standing provenance is `confirmed` with basis `user`. A run edits no files, so the changed-path test is not the test here; the examined-inputs predicate still decides, item by item, whether what it examined was this version.",
    },
    Line {
        slug: "applies_in_scope",
        plain: "Counted for this Change: you confirmed this session belongs here, and it changed {count} of the files this Change covers.",
        technical: "Standing provenance `confirmed` by a named actor, and {files} changed paths fall inside scope version {version}'s allowed paths. Only now are this session's recorded outcomes read.",
    },
];

/// One association line by slug, or `None` when the slug is not one of ours.
pub fn association_line(slug: &str) -> Option<&'static Line> {
    ASSOCIATION_LINES.iter().find(|l| l.slug == slug)
}

/// An association line with its counts filled in.
///
/// `{count}` becomes a bare number, `{files}` a counted noun ("3 files",
/// "1 file"), and `{version}` the scope version a check was made against. A
/// slug that is not in the table is a programming error and returns `None`
/// rather than a half-rendered sentence: a surface that prints `{files}` at a
/// person is worse than one that prints nothing and is caught.
///
/// Three more fills exist for the two summary sentences, and they exist so a
/// sentence can agree with its own number instead of reading "1 pieces of
/// recorded work … they were not considered": `{items}` and `{lines}` are
/// counted nouns, and `{they}`/`{them}` are the pronouns that go with them.
/// The agreement is part of the copy, not something a surface patches up.
pub fn association_sentence(slug: &str, count: u64, version: u64) -> Option<(String, String)> {
    let line = association_line(slug)?;
    let fill = |s: &str| {
        s.replace("{count}", &count.to_string())
            .replace("{files}", &count_phrase(count, "file", "files"))
            .replace(
                "{items}",
                &count_phrase(count, "piece of recorded work", "pieces of recorded work"),
            )
            .replace("{lines}", &count_phrase(count, "line", "lines"))
            .replace("{they}", if count == 1 { "it was" } else { "they were" })
            .replace("{them}", if count == 1 { "it" } else { "them" })
            .replace("{version}", &version.to_string())
    };
    Some((fill(line.plain), fill(line.technical)))
}

/// Every string T-2-3 serves about a proposed criterion, a suggested check
/// profile, or the act of accepting one (`B5.1`–`B5.10`).
///
/// The same two-register [`Line`] the link layer uses, for the same reason: the
/// Plain half is what Sam reads on the first screen of the first five minutes,
/// and the Technical half is the same fact with the record's own nouns in it.
/// Four words the user lens requires appear in the Plain half — "what counts as
/// done", "proposed", "accepted", "still needs evidence" — and three it forbids
/// never do: no "criterion id", no "ask", no "predicate".
///
/// The "kept as a note" family is the important one. A proposer that silently
/// dropped the sentences it could not cut into three parts would be a proposer
/// whose output nobody could audit; each note says which part was missing, in
/// Sam's words, beside the person's own sentence.
pub const PROPOSAL_LINES: &[Line] = &[
    Line {
        slug: "proposal_reasoning",
        plain: "This is your own wording from the start of this work, cut down to the part someone could check.",
        technical: "Cut from one request's text at a sentence boundary and matched against the subject · condition · observable-outcome shape. The source request id and the character span travel with it; no word was added, removed or rephrased.",
    },
    Line {
        slug: "proposal_none",
        plain: "We could not turn anything in the record into a line you could check. What we found is kept below, in your own words.",
        technical: "No sentence in the Change's purpose, the original prompt, or any confirmed session's opening request carried all three parts of the shape. An empty set is an answer, not a failure: every sentence is listed as a note naming the part it was missing.",
    },
    Line {
        slug: "note_no_outcome",
        plain: "Kept as a note, not something to check: it does not say what you would see if it were done.",
        technical: "No word in the sentence matched the closed outcome table (`change_propose::OUTCOME_MARKERS`), so there is nothing a check could observe. Kept verbatim; nothing was invented to fill the gap.",
    },
    Line {
        slug: "note_no_condition",
        plain: "Kept as a note, not something to check: it does not say when it should hold.",
        technical: "An outcome was found, but no condition word in front of it with a subject before that, so the sentence as written holds always. Kept verbatim.",
    },
    Line {
        slug: "note_no_subject",
        plain: "Kept as a note, not something to check: it does not say what it is about.",
        technical: "The span before the condition carried no word outside the stopword list, so the sentence has no subject to check anything against. Kept verbatim.",
    },
    Line {
        slug: "note_too_long",
        plain: "Kept as a note, not something to check: it is a paragraph, and a paragraph is not one thing to check.",
        technical: "The sentence is longer than `change_propose::MAX_CRITERION_CHARS`. Splitting it would mean choosing which half was meant, which is the invention this proposer refuses. Kept verbatim.",
    },
    Line {
        slug: "note_fragment",
        plain: "Kept as a note, not something to check: this is a piece of a sentence, not a whole one, so it cannot say what would count as done.",
        technical: "The span does not read as a whole sentence — it begins mid-line in lower case, or it ends without a terminal stop — so it is never offered as a criterion whatever shape it has. Kept verbatim (`T-2-8-F1`).",
    },
    Line {
        slug: "note_too_short",
        plain: "Kept as a note, not something to check: it is too short to check on its own.",
        technical: "The sentence is shorter than `change_propose::MIN_SENTENCE_CHARS` characters, so it cannot carry a subject, a condition and an outcome. It is never proposed and never dropped: kept verbatim, in its place (`QUAL-F11-LAUNCH-R5`).",
    },
    Line {
        slug: "note_repeats_an_earlier_line",
        plain: "Kept as a note, not something to check: it repeats a line that comes before it in the same request, so that line is the one listed.",
        technical: "After case and punctuation are set aside, the sentence is identical to an earlier sentence of the same request, which is proposed or kept already. Proposing it again would count one thing twice; dropping it would hide what was written. Kept verbatim, pointing at its own span (`QUAL-F11-LAUNCH-R5`).",
    },
    Line {
        slug: "note_repeats_a_line_of_another_request",
        plain: "Kept as a note, not something to check: it repeats a line of another request for this work, so that line is the one listed.",
        technical: "After case and punctuation are set aside, the sentence is identical to a sentence already read from another request this Change is read out of (its purpose, the original prompt, or a confirmed session's opening request), which is proposed or kept already. Proposing it again would count one thing twice; dropping it would hide what this request said. Kept verbatim, with this request's own id and span (`QUAL-F11-LAUNCH-R6`).",
    },
    Line {
        slug: "note_masked",
        plain: "Kept as a note, not something to check: part of this looked like a secret, so it was replaced before we showed it, and these are no longer your exact words.",
        technical: "The masked text carries a `[masked: …]` marker. Masking runs before the cut, and a mask can consume a sentence terminator with the secret; proposing the result would be proposing a sentence nobody wrote. Kept as the masked text, verbatim.",
    },
    Line {
        slug: "profile_is_not_evidence",
        plain: "A list of checks is not evidence. Nothing here has run yet, so everything here still needs evidence.",
        technical: "`B5.10`: a suggested profile is a plan for what to examine. Suggesting it creates no check row, no run and no outcome, and nothing in this list may be read as a result.",
    },
    Line {
        slug: "profile_no_paths",
        plain: "The record does not show which files this work touched, so we cannot say what kind of work it is or what to check.",
        technical: "No confirmed session had a readable changed path and the standing scope lists none, so the change-type inference had no input. Missing input is stated rather than guessed at.",
    },
    Line {
        slug: "check_mandatory",
        plain: "Needed before this can be approved.",
        technical: "Mandatory in this profile. If it cannot run on this machine it stays on the list and stays needed — `B5.10` forbids dropping it.",
    },
    Line {
        slug: "check_advisory",
        plain: "Worth running. Not needed before approval.",
        technical: "Advisory in this profile: its absence never blocks a decision, and its presence is not a substitute for a mandatory check.",
    },
    Line {
        slug: "check_ready",
        plain: "Ready to run here.",
        technical: "The program is on `PATH` and the project holds a reviewed settings file for it. Being able to run is still not having run.",
    },
    Line {
        slug: "check_not_available_here",
        plain: "Not available here: {name}.",
        technical: "`{name}` is not on `PATH` on this machine. The check stays on the list, because a dropped check looks exactly like a passed one to anybody reading the list later.",
    },
    Line {
        slug: "check_needs_a_recipe",
        plain: "Installed here, but there is no reviewed settings file for it yet ({name}), so it cannot run.",
        technical: "The program is on `PATH`, and `{name}` does not exist. Every adapter needs explicit reviewed sources or targets before it may run; an adapter with no settings file would be examining something nobody chose.",
    },
    Line {
        slug: "type_documentation",
        plain: "This work is mostly writing, not behaviour.",
        technical: "One or more changed paths is a document by extension or directory. `B5.14`: opening a document proves nothing about behaviour.",
    },
    Line {
        slug: "type_interface_behaviour",
        plain: "This work changes what someone using the product sees or does.",
        technical: "Changed paths include interface sources or a route. The examinations worth running are the ones that drive the interface, not the ones that read it.",
    },
    Line {
        slug: "type_backend_behaviour",
        plain: "This work changes what the program does behind the screen.",
        technical: "No changed path matched a narrower kind, so the work is treated as program behaviour and the profile asks for the project's own tests.",
    },
    Line {
        slug: "type_authorization_sensitive",
        plain: "This work touches who is allowed to do what, so it is worth being careful.",
        technical: "A changed path names authorization, a credential or a policy. The profile adds a mandatory static examination; it does not claim the work is unsafe.",
    },
    Line {
        slug: "type_data_schema",
        plain: "This work changes how stored things are shaped, so what happens to things already stored is part of what counts as done.",
        technical: "A changed path names a migration, a schema or a `.sql` file. A schema change with no compatibility examination is the one class this product will not read as finished.",
    },
    /* What this Change covers (T-3-2b). The folders are read off the files the
     * confirmed work already changed; nothing is invented, and the empty case
     * says what would fill it rather than leaving a blank list on screen. */
    Line {
        slug: "scope_from_the_work",
        plain: "These are the folders the work already confirmed for this Change touched. Accepting them with what counts as done is what lets later checks be counted for it.",
        technical: "Each entry is the directory of a path one confirmed session changed, spelled as a `dir/**` subtree the scope matcher understands, with entries covered by a shorter one removed. Derived at read from stored rows; nothing on disk is inspected.",
    },
    Line {
        slug: "scope_none_found",
        plain: "The record does not show which files the work for this Change changed, so there are no folders to cover yet. Confirm the work that belongs to this Change, and they will be proposed here.",
        technical: "No file-editing tool call with a readable path was found in any confirmed session, so the allowed scope is proposed as empty rather than as the whole project. An empty answer is an answer, never a wildcard.",
    },
    Line {
        slug: "scope_over_the_bound",
        plain: "{count} more folders were left off this list, which is kept to {version} so it stays something you can read before you accept it.",
        technical: "The derived entry list exceeded `change_propose::MAX_SCOPE_PATHS`; the remainder is counted here rather than dropped silently. Accepting more is a re-scope with the folders named.",
    },
    Line {
        slug: "criterion_as_proposed",
        plain: "In the words we proposed.",
        technical: "The accepted text matches a criterion this store's proposer derives, word for word once whitespace and case are normalised.",
    },
    Line {
        slug: "criterion_your_words",
        plain: "Not in the words we proposed — this is how you wrote it.",
        technical: "The accepted text matches no proposal this store derives now, so it is attributed to the actor on the acceptance record. If the proposer's own rules change, a line accepted earlier can move into this class; that direction is the safe one, because the tool never claims authorship of words it did not write.",
    },
    Line {
        slug: "criterion_not_observable_as_written",
        plain: "As you wrote it, we cannot tell what would satisfy this. It is kept exactly as you wrote it.",
        technical: "The accepted line does not cut into the three parts. It is stored unchanged and shown with this line beside it: your list is yours, and a tool that quietly rewrote it would be worse than one that says it cannot read it.",
    },
    Line {
        slug: "accept_is_not_approval",
        plain: "Agreeing what counts as done is not approving the work. Nobody has approved anything.",
        technical: "`B5.7`: an acceptance is a row in `change_scope_acceptances` naming an actor, a time, a version and a rationale. No decision or approval record exists yet, and this act does not touch one.",
    },
    Line {
        slug: "accept_written",
        plain: "Saved as what counts as done, version {version} — {items}.",
        technical: "Appended scope revision {version} with {count} criteria and an acceptance record carrying actor, time, version and rationale (`B5.6`). Every earlier version is unchanged and still readable.",
    },
    Line {
        slug: "accept_needs_actor",
        plain: "We cannot write down what counts as done without a name. Say who is accepting it.",
        technical: "`change_scope_acceptances.accepted_by` is `NOT NULL` with a non-empty CHECK. This is that refusal in words, said before the store is opened.",
    },
    Line {
        slug: "accept_needs_a_criterion",
        plain: "There is nothing to accept. An empty list is not a list of what counts as done.",
        technical: "The selection resolved to zero criteria. An empty `criteria_json` would be a scope version that every later examination passes vacuously, which is worse than having none.",
    },
    Line {
        slug: "accept_unknown_criterion",
        plain: "There is no line numbered {count} to accept.",
        technical: "The number is outside the set this reading proposed. Refused rather than ignored: silently skipping it would save a shorter list than the one that was chosen.",
    },
    Line {
        slug: "accept_edit_not_selected",
        plain: "You rewrote line {count} but did not choose it, so nothing would have been saved. Choose it, or drop the rewrite.",
        technical: "An edit was supplied for a number outside the selection. Refused rather than dropped, because a dropped edit is a person's words silently discarded.",
    },
    Line {
        slug: "accept_nothing_new",
        plain: "This is already what counts as done, version {version}. There is nothing new to accept.",
        technical: "The resulting five scope lists hash to the same `content_sha256` as the standing version. A version that changes nothing would still consume one of the bounded slots and would make the history harder to read.",
    },
    Line {
        slug: "accept_would_overwrite_your_words",
        plain: "This would put our wording back over {lines} you wrote yourself. Nothing was saved. Say plainly that you are replacing them, and it will be.",
        technical: "{count} lines of accepted version {version} match no proposal this store derives, so they are the accepting actor's own text, and the list about to be written does not contain them. Refused: `replacing` makes it an explicit act (`B5.5`).",
    },
    Line {
        slug: "accept_would_weaken",
        plain: "This would drop {lines} you already accepted, or replace one with something that says less. Nothing was saved. Say plainly that you are replacing them, and it will be.",
        technical: "Comparing the list about to be written against accepted version {version} with the same two tests a re-proposal gets — a dropped negation, and a word set that is a proper subset — found {count} accepted lines dropped or weakened. Refused; `replacing` makes it an explicit act instead of a silent one (`B5.5`).",
    },
    Line {
        slug: "accept_version_limit",
        plain: "This Change already holds {lists} of what counts as done, which is the most we keep ({version}). Open a new Change for a new direction.",
        technical: "`change_scope_revisions` is append-only, so the series only grows. The bound is stated on every reading before it is reached, and the refusal names what to do instead.",
    },
    Line {
        slug: "accepted_scope_none",
        plain: "Nobody has written down what counts as done for this Change yet.",
        technical: "No `change_scope_revisions` row exists for this Change. Shown as missing, never as agreed, and never filled in for you.",
    },
    Line {
        slug: "scope_versions_bounded",
        plain: "{lists} of what counts as done so far. We keep at most {version}.",
        technical: "`COUNT(*)` over this Change's `change_scope_revisions` rows, against `change_propose::MAX_SCOPE_VERSIONS`. Both numbers are shown so the bound is visible before it refuses anything.",
    },
    Line {
        slug: "reproposal_removed",
        plain: "You accepted this, and nothing in the record proposes it now. It stays accepted until you say otherwise.",
        technical: "No proposal in this reading pairs with this accepted line by subject. Reading again cannot remove an accepted line; only a new acceptance can replace the list.",
    },
    Line {
        slug: "reproposal_weakened",
        plain: "This new wording says less than the line you already accepted. We are not replacing the stronger one.",
        technical: "The paired proposal either drops a negation the accepted line carries, or its word set is a proper subset of the accepted line's. Both are named as weakenings and neither is applied (`B5.5`).",
    },
    Line {
        slug: "reproposal_restated",
        plain: "The same thing in different words as a line you already accepted. Nothing changes.",
        technical: "The paired proposal differs in wording but fails both weakening tests. It is listed so the difference is visible, and it changes nothing.",
    },
    Line {
        slug: "reproposal_changes_nothing",
        plain: "Reading the record again changes nothing you accepted. Only you can change that, and only by accepting a new list.",
        technical: "`propose` performs no write. The standing version, its acceptance record and its digest are untouched by any number of readings.",
    },
];

/// One proposal line by slug, or `None` when the slug is not one of ours.
pub fn proposal_line(slug: &str) -> Option<&'static Line> {
    PROPOSAL_LINES.iter().find(|l| l.slug == slug)
}

/// A proposal line with its numbers and its one name filled in.
///
/// Fills that carry a noun where a noun is owed: `{count}` a bare number,
/// `{items}` counted things to check, `{lists}` counted lists, `{lines}` counted
/// lines, `{version}` a version number, and `{name}` the one tool or settings
/// file the sentence names. A slug outside the table returns `None` rather than a
/// half-rendered sentence.
pub fn proposal_sentence(
    slug: &str,
    count: u64,
    version: u64,
    name: &str,
) -> Option<(String, String)> {
    let line = proposal_line(slug)?;
    let fill = |s: &str| {
        s.replace("{count}", &count.to_string())
            .replace(
                "{items}",
                &count_phrase(count, "thing to check", "things to check"),
            )
            .replace("{lists}", &count_phrase(count, "list", "lists"))
            .replace("{lines}", &count_phrase(count, "line", "lines"))
            .replace("{files}", &count_phrase(count, "file", "files"))
            .replace("{version}", &version.to_string())
            .replace("{name}", name)
    };
    Some((fill(line.plain), fill(line.technical)))
}

/// Every string `thruline change new` can put in front of a person (T-2-4, fix
/// round 1).
///
/// Opening a Change is the one act in this layer that has no proposer in front
/// of it: a person says "this is the piece of work I want to be able to hand
/// over", and until that row exists there is nothing for `suggest`, `propose`,
/// `accept` or the Change screen to be about. The verifier of T-2-4 could only
/// produce one with a scratch program calling the library directly, which is a
/// capability no person has; these are the sentences the product says instead.
///
/// The same two-register [`Line`], and the same rule: the Plain half is what a
/// person reads, and it names what to do next rather than only what went wrong.
pub const CHANGE_OPEN_LINES: &[Line] = &[
    Line {
        slug: "change_opened",
        plain: "This Change is open. Nothing counts as done for it yet — ask what it could be judged against, then say which of those lines you accept.",
        technical: "One row appended to `changes` with the request's title, purpose, requester and source. No scope version, no acceptance, no link and no implementation binding was written; `change propose` reads next.",
    },
    Line {
        slug: "change_open_needs_title",
        plain: "Nothing was written down: a Change needs a title — the one line you would say out loud to describe the work.",
        technical: "`change.request.title` was empty or whitespace. `insert_change` refuses it at the boundary rather than storing a row that every later surface would have to render as a blank.",
    },
    Line {
        slug: "change_open_needs_requester",
        plain: "Nothing was written down: a Change needs the name of whoever is asking for it. Say who with --requester.",
        technical: "`change.request.requester` was empty. The request half of `B3.4` carries who asked; a row without it cannot be attributed later and would be attributed by guesswork.",
    },
    Line {
        slug: "change_open_unknown_project",
        plain: "There is no work recorded under that name, so there is nothing to open a Change about. Bring the work in first, then open the Change against it.",
        technical: "No `projects` row has that id. The names this store already holds are listed above; `ingest` adds one.",
    },
    Line {
        slug: "change_open_project_needed",
        plain: "This store holds work under more than one name, so say which one this Change is about.",
        technical: "More than one `projects` row exists and `--for` was not given. Choosing one here would bind the Change to a project by accident, which no later surface could tell from a decision.",
    },
    Line {
        slug: "change_open_no_projects",
        plain: "This store holds no work yet, so there is nothing to open a Change about. Bring some work in first.",
        technical: "`projects` is empty. `ingest <path>` writes the first row; a Change with no project id would be a Change no project screen could reach.",
    },
];

/// One `change new` line by slug, or `None` when the slug is not one of ours.
pub fn change_open_line(slug: &str) -> Option<&'static Line> {
    CHANGE_OPEN_LINES.iter().find(|l| l.slug == slug)
}

/// Every string T-3-1 serves about what bears on a thing that counts as done
/// (`B4.6`, `B7.5`–`B7.7`, `B10.2`, `Q3`).
///
/// The four coverage sentences are the point of the table, and the order they
/// are written in is the order of how much they claim: something was looked at,
/// nothing ran, the check cannot run here, something ran and settled nothing.
/// None of the four is a pass and none is a failure — a criterion with nothing
/// behind it reads as empty, in Sam's words, exactly as `03-USER-LENS.md` rule 5
/// requires.
///
/// The Plain half never says "criterion", "mapping", "basis" or "run id"; the
/// Technical half is allowed all four, because that is what the register is
/// for. `crates/core/tests/change_evidence_copy.rs` sweeps both halves.
pub const COVERAGE_LINES: &[Line] = &[
    Line {
        slug: "coverage_evidence",
        plain: "Something in the record was looked at for this. Read what it was before you accept it.",
        technical: "At least one standing link for this criterion points at evidence whose observed result is something other than \"checked nothing\". The list below is the link rows, each with its basis.",
    },
    Line {
        slug: "coverage_no_test_ran",
        plain: "No test ran for this.",
        technical: "No standing link row exists for this criterion at the accepted scope version. Absence of a link, not a failed check: nothing was evaluated.",
    },
    Line {
        slug: "coverage_tool_absent",
        plain: "The check this needs isn't available here: {tool}.",
        technical: "The accepted check profile names a check of this criterion's class whose program is not on this machine's PATH ({tool}), and no link row exists for the criterion. The check stays listed and stays required.",
    },
    Line {
        slug: "coverage_ran_checked_nothing",
        plain: "Something ran for this, and it checked nothing. It is shown below, and it settles nothing.",
        technical: "Every standing link for this criterion points at evidence whose observed result carries the checked-nothing flag: interrupted, no exit status, output not captured whole, or a test-class command whose recorded fields hold no count of what it ran.",
    },
    Line {
        slug: "coverage_other_version",
        plain: "Something ran for this, and not on the code this Change is set up against. It is shown below, and it settles nothing here.",
        technical: "Every standing link for this criterion whose evidence bears at all points at an examination whose examined-inputs record does not satisfy `examined_inputs::observation_applies` against this Change's intended record. The per-item sentence names which mismatch.",
    },
    Line {
        slug: "basis_declared",
        plain: "Looked at because the checks you accepted cover this kind of thing.",
        technical: "Basis `declared`: the accepted check profile names a check whose category maps to this criterion's class, and this run carries that class.",
    },
    Line {
        slug: "basis_named",
        plain: "Looked at because what ran names what this line is about.",
        technical: "Basis `named`: the evidence's own typed name field contains, as a whole token, one of the criterion's subject tokens.",
    },
    Line {
        slug: "basis_manual",
        plain: "You linked this yourself.",
        technical: "Basis `manual`: appended by a named actor at a recorded time. No rule would have produced this row.",
    },
    Line {
        slug: "basis_not_bearing",
        plain: "You said this does not bear on this line. It is kept here so the decision is visible.",
        technical: "Basis `not-bearing`: a tombstone appended by a named actor. The row it answers is still stored and still readable; nothing was deleted.",
    },
    Line {
        slug: "item_checked_nothing",
        plain: "Ran, checked nothing.",
        technical: "The observed result carries the checked-nothing flag, so this item satisfies nothing it is attached to. It is listed because a run that settled nothing is still something that happened.",
    },
    Line {
        slug: "item_out_of_reach",
        plain: "This link points at work that is no longer part of this Change, so there is nothing to show for it.",
        technical: "The link row stands, and the evidence it names is no longer readable under this Change: the session is unconfirmed, rejected, or its paths fell outside the accepted scope. The link is never deleted to make the screen tidier.",
    },
    Line {
        slug: "link_needs_actor",
        plain: "We cannot add or remove a link without a name. Say who is doing it.",
        technical: "A `manual` or `not-bearing` row requires a non-empty actor. The database CHECK refuses the row; this is that refusal in words.",
    },
    Line {
        slug: "link_not_in_reach",
        plain: "That piece of work is not confirmed as part of this Change, so it cannot be linked to anything here. Confirm it first.",
        technical: "The named evidence is not in this Change's candidate pool: its session failed `change_link::applicability`. The gate is the same one every other read goes through.",
    },
    Line {
        slug: "link_no_criteria",
        plain: "Nothing counts as done for this Change yet, so there is nothing to link evidence to. Write down what should count first.",
        technical: "No accepted scope version exists, so there is no criterion index to key a link on.",
    },
    Line {
        slug: "link_unknown_criterion",
        plain: "That is not one of the things this Change counts as done.",
        technical: "The criterion index is outside the accepted scope version's list. Refused rather than clamped.",
    },
    Line {
        slug: "coverage_link_rows",
        plain: "{count} in the record behind this.",
        technical: "{count} in `change_criterion_evidence` for this Change, across every scope version, including tombstones.",
    },
];

/// T-3-2 — what exactly was examined, and whether a result is about the code
/// this Change is set up against.
///
/// Two registers, as always. **Plain never says "snapshot id", "tree hash",
/// "pid", "worktree" or "predicate"** — `crates/core/tests/examined_inputs_copy.rs`
/// reads these strings and fails on any of them. The technical half says all
/// of it, because the person who needs the pid needs the pid.
///
/// **Plain never says "version" either (T-3-2b r1).** On a Change screen that
/// word already names one thing: the version of the work bound to the Change
/// (`change.version.*`, written by `change_implementations`). These sentences
/// are about a second, different record — what an examination actually looked
/// at — and the first draft called both of them "version", so one screen could
/// read "No version of the work is bound to this Change yet" above an item
/// reading "Checked on this version." Two records, one word, no way to tell
/// them apart. The examined side now says "the code this Change is set up
/// against", and `examined_inputs_copy.rs` fails on any plain line that says
/// "version" without naming a tool.
pub const INPUTS_LINES: &[Line] = &[
    Line {
        slug: "inputs_applies",
        plain: "Checked on the code this Change is set up against.",
        technical: "The examined-inputs record's base revision, source identity (wsid1, which covers uncommitted changes) and worktree path equal the candidate's, and where a running application was examined its port owner and database match the ones this Change names.",
    },
    Line {
        slug: "inputs_nothing_bound",
        plain: "This Change is not set up to check against your code yet, so nothing here can say whether a run was about it.",
        technical: "No `examination_inputs` row with subject kind `intended` exists for this Change, so no examination can be compared against it and every item's applicability is null rather than false. Changes opened on this build are bound when they are opened; this one predates that.",
    },
    Line {
        slug: "inputs_missing",
        plain: "Nothing says what was looked at here, so this isn't counted.",
        technical: "No `examination_inputs` row exists for this examination. An examination with no examined-inputs record cannot be shown to be about any version, so it is attached to no criterion's conclusion.",
    },
    Line {
        slug: "inputs_unknown_version",
        plain: "We can't tell which code this was checked on, so it isn't counted.",
        technical: "The base revision or the source identity is `unknown` on one side. Unknown never compares equal, including to another unknown.",
    },
    Line {
        slug: "inputs_other_revision",
        plain: "This was checked on different code than this Change is set up against.",
        technical: "The examined base revision differs from the candidate's.",
    },
    Line {
        slug: "inputs_other_edits",
        plain: "This was checked on the same commit, with different unsaved changes.",
        technical: "The base revisions, worktree paths are equal, at least one side reports uncommitted changes, and the source identities (wsid1, covering staged, dirty, deleted, renamed and untracked files) are not equal.",
    },
    Line {
        slug: "inputs_other_folder_state",
        plain: "This was checked on the same commit, in the same folder, and the folder was not set up the same way, so it isn't counted.",
        technical: "The base revisions and worktree paths are equal, neither side reports uncommitted changes, and the source identities (wsid1) still differ — the branch name, a submodule's checked-out head, the sparse-checkout file, an LFS attribute or a re-created git dir are all inside that digest. Reported separately so the edits sentence is never said about two clean trees (T-3-2b).",
    },
    Line {
        slug: "inputs_unknown_worktree",
        plain: "We can't tell which copy of your project folder this was checked in, so it isn't counted.",
        technical: "The worktree path is `unknown` on one side.",
    },
    Line {
        slug: "inputs_other_worktree",
        plain: "This is a second checkout of the same commit. ThruLine records one checkout per Change — pick the one you're working in.",
        technical: "The base revisions are equal and the examined worktree path differs from the candidate's. Asked before the source identity, which contains the root and git common dir and would otherwise report a clean sibling checkout as an edits difference (T-3-2b).",
    },
    Line {
        slug: "inputs_no_app_named",
        plain: "This came from a running copy of the app, and this Change doesn't say which running copy it is about.",
        technical: "The examination carries a `running_app` target and the intended record carries `none`. Nothing is inferred from the absence.",
    },
    Line {
        slug: "inputs_unknown_app",
        plain: "We can't tell which running copy of the app this came from, so it isn't counted.",
        technical: "The listening port's owning executable or the database path is `unknown` on one side; `lsof` did not answer.",
    },
    Line {
        slug: "inputs_other_app",
        plain: "This came from a different running copy of the app.",
        technical: "The examined port, or the executable of the process owning the listening socket, differs from the one this Change names.",
    },
    Line {
        slug: "inputs_other_database",
        plain: "This came from a copy of the app using a different database.",
        technical: "The port owner matches and the database file the process has open differs from the one this Change names.",
    },
    Line {
        slug: "inputs_unknown_is_not_a_value",
        plain: "We can't write down \"unknown\" as if it were an answer. Leave it out instead.",
        technical: "A caller passed the literal string `unknown` as a field value. The module refuses it before the store is touched, and the CHECK constraints refuse it again.",
    },
    Line {
        slug: "prepare_repository",
        plain: "The project folder: {value}",
        technical: "The examined worktree path, from the workspace identity's canonical root.",
    },
    Line {
        slug: "prepare_base",
        plain: "Starting from: {value}",
        technical: "The base revision (git HEAD) the examined-inputs record carries.",
    },
    Line {
        slug: "prepare_uncommitted_included",
        plain: "Your unsaved changes are included: {count}.",
        technical: "The source identity covers {count} staged, dirty, deleted, renamed or untracked entries, and the snapshot's dirty-diff digest is recorded beside it.",
    },
    Line {
        slug: "prepare_uncommitted_none",
        plain: "There are no unsaved changes: this is exactly what is committed.",
        technical: "The workspace observation found no staged, dirty, deleted, renamed or untracked entry.",
    },
    Line {
        slug: "prepare_uncommitted_unknown",
        plain: "We could not read whether you have unsaved changes.",
        technical: "The workspace observation was incomplete, so no source identity was minted. Recorded as unknown; never assumed clean.",
    },
    Line {
        slug: "prepare_running_app",
        plain: "The app running on port {port}, started by {app}.",
        technical: "The process owning the listening socket on port {port}: executable {app}, read with `lsof -nP -sTCP:LISTEN` and `ps -o comm=`.",
    },
    Line {
        slug: "prepare_app_unknown",
        plain: "Nothing is running on port {port} that we could identify.",
        technical: "`lsof` returned no owning pid for port {port}, so the owner's executable and database are recorded as unknown.",
    },
    Line {
        slug: "prepare_tool_present",
        plain: "{tool} will run. It is version {value}.",
        technical: "{tool} answered `--version` with {value} at the moment the record was written.",
    },
    Line {
        slug: "prepare_tool_version_unknown",
        plain: "{tool} will run. It does not say which version it is.",
        technical: "{tool} is on PATH and its `--version` produced nothing readable. Recorded as unknown, never guessed.",
    },
    Line {
        slug: "prepare_tool_absent",
        plain: "{tool} is not on this machine, so it will not run.",
        technical: "{tool} could not be executed here. The check stays listed and stays required; nothing is marked done because the tool is missing.",
    },
    Line {
        slug: "prepare_nothing_recorded",
        plain: "Nothing is written down yet about what will be looked at for this Change. Run `thruline change prepare` and it will be.",
        technical: "No `examination_inputs` row with subject kind `intended` exists for this change id.",
    },
    Line {
        slug: "prepare_not_written_down",
        plain: "not written down",
        technical: "The field is SQL NULL: the probe did not answer, and nothing was substituted for it.",
    },
    Line {
        slug: "inputs_record_count",
        plain: "{count} in the record behind this.",
        technical: "{count} in `examination_inputs` for this examination, newest standing.",
    },
];

/// T-S-2 (sprint 2026-09-22, narrowing T-3-3): whether a piece of linked
/// evidence still applies once the tree it was checked on has moved. Two
/// headlines and one reason sentence per class — Sam's own words, `B7.5`/`Q5`.
///
/// `{path}` and `{version}` are the only two placeholders these carry,
/// rendered by [`applicability_sentence`]. Neither "stale" nor "unknown" nor
/// "recorded" appears here; [`BANNED`] would fail this file if one did.
pub const APPLICABILITY_LINES: &[Line] = &[
    Line {
        slug: "applicability_still_applies",
        plain: "Still applies — {path} unchanged since {version}.",
        technical: "No path the dependency rule considers relevant to this criterion changed between the compared snapshots; {path} is named as the one the rule checked.",
    },
    // T-F-1-F1 — the same conclusion, when the record cannot name BOTH the
    // paths the rule compared and the version it compared against. The slug
    // the engine chooses is still `applicability_still_applies`; a surface
    // that finds a value missing falls back to this one rather than rendering
    // "Still applies — this path unchanged since ." with two holes in it.
    Line {
        slug: "applicability_still_applies_unnamed",
        plain: "Still applies — nothing it depends on has changed since this version.",
        technical: "No path the dependency rule considers relevant to this criterion changed between the compared snapshots; the record does not name the paths it compared or the version it compared against, so neither is stated here.",
    },
    Line {
        slug: "applicability_needs_a_recheck",
        plain: "Proven on the previous version — needs a re-check.",
        technical: "The dependency rule found at least one relevant path changed since this evidence was produced. See the reason one level down.",
    },
    Line {
        slug: "applicability_reason_source_changed",
        plain: "The affected code changed afterward.",
        technical: "A source path under the accepted scope, matched by the conservative branch of the dependency rule (no declared paths on this check profile), changed between the compared snapshots: {path}.",
    },
    Line {
        slug: "applicability_reason_tests_changed",
        plain: "The test itself changed afterward.",
        technical: "A path under a `tests/` directory, or carrying a recognised test-file suffix, changed between the compared snapshots: {path}.",
    },
    Line {
        slug: "applicability_reason_dependencies_changed",
        plain: "A dependency changed afterward.",
        technical: "A lockfile (`Cargo.lock`, `package-lock.json`, `pnpm-lock.yaml`, `yarn.lock` or `Gemfile.lock`) changed between the compared snapshots: {path}.",
    },
    Line {
        slug: "applicability_reason_configuration_changed",
        plain: "The configuration changed afterward.",
        technical: "A build or tooling configuration file changed between the compared snapshots: {path}.",
    },
    Line {
        slug: "applicability_reason_criteria_changed",
        plain: "What counts as done here changed, so this needs a look again.",
        technical: "The accepted scope moved to a new version and this criterion's text changed at the same index; the evidence from the earlier version cannot inherit the new criterion's approval (`Q6`).",
    },
];

/// T9 — the Technical face of a freshness line on the Change screen: what the
/// ONE applicability rule compared, as `(slug, template)`. `{path}` is the
/// record's `checked_paths[0]`, `{version}` its `old_snapshot` and `{now}` its
/// `new_snapshot`, both in their short forms. No engine path emits these: the
/// browser composes them from those typed fields (`hq-copy.js`
/// `change.coverage.carried`), and `applicability_technical_copy.rs` holds the
/// two copies word for word so a re-wording on one side alone fails.
pub const APPLICABILITY_TECHNICAL_LINES: &[(&str, &str)] = &[
    (
        "applicability_carried_forward",
        "carried forward: {path} unchanged {version}→{now}",
    ),
    (
        "applicability_carried_forward_unnamed",
        "carried forward: nothing the rule checked changed between the compared snapshots; the record does not name the path or both versions, so neither is stated here.",
    ),
    (
        "applicability_rule",
        "one rule decides this: the dependency rule",
    ),
];

/// One applicability line by slug, or `None` when the slug is not one of
/// ours.
pub fn applicability_line(slug: &str) -> Option<&'static Line> {
    APPLICABILITY_LINES.iter().find(|l| l.slug == slug)
}

/// T-3-4 (`B7.1`, `B7.6`, `B12.6`, `Q12`) — what became of the EXECUTION, as
/// against what the check observed. Two fields, two sentences, never one badge
/// (design record §3.8).
///
/// **The Plain half never says "pid", "process group" or "SIGTERM"**, which is
/// the card's own prohibition and is swept mechanically by
/// `crates/core/tests/run_execution_copy.rs`. The Technical half says all
/// three, because that is what the register is for. Neither half ever says a
/// stopped run passed: the words "pass", "passed" and "Proven" do not appear
/// in this table at all, and the same sweep asserts it.
pub const EXECUTION_LINES: &[Line] = &[
    Line {
        slug: "execution_not_run",
        plain: "This check has not been run yet.",
        technical: "No execution record exists for this run: `change_run_executions` holds no row for it, which is how `not run` is encoded.",
    },
    Line {
        slug: "execution_running",
        plain: "This check is running now. Nothing is settled until it finishes.",
        technical: "The latest `change_run_executions` row is `running`. `settles` is false: an execution still in flight can satisfy no criterion, whatever its output so far says.",
    },
    Line {
        slug: "execution_completed",
        plain: "This check ran all the way to the end.",
        technical: "The latest `change_run_executions` row is `completed`: the child's exit status was waited for and recorded. This is the only execution state for which `settles` is true.",
    },
    Line {
        slug: "execution_cancelled",
        plain: "This check was stopped before it finished. Run it again.",
        technical: "The latest `change_run_executions` row is `cancelled`: a named actor asked for it to stop at a recorded time. Whatever was observed up to that point is kept and satisfies nothing.",
    },
    Line {
        slug: "execution_interrupted",
        plain: "This check was interrupted before it finished. Run it again.",
        technical: "The latest `change_run_executions` row is `interrupted`: the process is gone and no completion was ever recorded. Partial output is kept as it stands and satisfies nothing.",
    },
    Line {
        slug: "execution_blocked",
        plain: "This check could not start, because something it needs isn't here.",
        technical: "The latest `change_run_executions` row is `blocked`: a declared prerequisite was absent, so nothing was spawned. Not a failure of the check — the check did not run.",
    },
    Line {
        slug: "execution_reason_hq_restarted",
        plain: "This check was interrupted when ThruLine restarted. Run it again.",
        technical: "A `running` row was found on start-up whose process no longer exists, so it was recorded as `interrupted` with reason `hq_restarted`. The row it supersedes is untouched.",
    },
    Line {
        slug: "execution_reason_hq_shut_down",
        plain: "This check was stopped when ThruLine shut down. Run it again.",
        technical: "ThruLine received SIGTERM or SIGINT and stopped this run's process group before exiting, within the shutdown bound, recording `cancelled` with reason `hq_shut_down`.",
    },
    Line {
        slug: "execution_reason_cancelled_by_person",
        plain: "You stopped this check.",
        technical: "A cancel request named an actor and a time; the run's process group was signalled and the record was written as `cancelled` with reason `cancelled_by_person`.",
    },
    Line {
        slug: "execution_reason_pid_reused",
        plain: "ThruLine could not tell what became of this check, so it counts as interrupted.",
        technical: "The `running` row's pid is alive and does not belong to this run (the process start time does not match). The run is recorded `interrupted` with reason `pid_reused` and is never adopted.",
    },
    Line {
        slug: "execution_reason_inputs_changed",
        plain: "The files this check was looking at changed while it ran, so its result doesn't describe what's here now.",
        technical: "`B6.8`: the examined-inputs record frozen at the spawn no longer matches the candidate's at completion. The result is recorded and its applicability is `no_longer_applies` from birth; it is never attached as a fresh pass.",
    },
    Line {
        slug: "execution_reason_prerequisite_absent",
        plain: "Something this check needs isn't on this machine.",
        technical: "A prerequisite of this run was not on this machine. Recorded `blocked`: the program could not be resolved, so no child was spawned. Recorded `completed` (D-051): the build's own output carried cargo's offline-fetch failure and no sign of compiling, so its non-zero exit is about this machine, not the project's code — it checked nothing and is never `failed`.",
    },
    Line {
        slug: "execution_reason_process_signalled",
        plain: "This check was stopped from outside ThruLine before it finished. Run it again.",
        technical: "The child was terminated by a signal (the wrapper saw `status.signal()`), and no cancellation record explains it: a crash, an external kill, or the system reclaiming it. Recorded `interrupted` with reason `process_signalled`, never `completed`.",
    },
    Line {
        slug: "execution_cleanup_verified",
        plain: "Nothing from this check was left running.",
        technical: "After the run ended, its process group was looked at and held no surviving process. Verified at a recorded time, never assumed.",
    },
    Line {
        slug: "execution_cleanup_survivors",
        plain: "Something this check started is still going. ThruLine left it alone rather than guess.",
        technical: "The run's process group still held at least one process when it was looked at; the count is on the record. Nothing further was signalled.",
    },
    Line {
        slug: "execution_cleanup_unknown",
        plain: "ThruLine can't say whether anything from this check was left running.",
        technical: "No cleanup observation was made for this run, so `cleanup_verified` is NULL. Absence of an observation, not an observation of absence.",
    },
    Line {
        slug: "execution_run_it_again",
        plain: "Run it again",
        technical: "Re-run: a new examination with a new examined-inputs record. Nothing about this run is edited or replaced (Rule 13).",
    },
];

/// T-3-5 (`B7.6`–`B7.12`, `Q1`, lens rule 5) — the readiness sentences.
///
/// **The strongest thing the tool says, and it lists what it rests on.** Three
/// prohibitions hold this table, all swept by
/// `crates/core/tests/change_readiness_copy.rs`:
///
///  * the word `green`, and any colour word, appears nowhere;
///  * no Plain half says `pass`, `passed`, `all clear`, `complete` or `100%`,
///    and no half carries a percent sign — a proportion of a proof is not a
///    thing a person can act on;
///  * the two waiver lines never read like a result: each of them says,
///    in the Plain register, that nothing was checked.
pub const READINESS_LINES: &[Line] = &[
    Line {
        slug: "readiness_ready",
        plain: "Ready for approval for version {version}. It rests on:",
        technical: "`change_readiness::compute` returned ready for {version}: an accepted scope with at least one criterion, an examined-inputs record for the candidate, and one admissible item per criterion obligation. The list below is `rests_on`.",
    },
    Line {
        slug: "readiness_nothing_accepted",
        plain: "Nothing counts as done yet, so nothing is ready. Say what counts as done for this Change first.",
        technical: "`B7.7`: no accepted scope version, or an accepted scope with no criteria. An empty obligation set is never satisfied vacuously; readiness is refused before any other condition is read.",
    },
    Line {
        slug: "readiness_needs_evidence_one",
        plain: "1 thing still needs evidence:",
        technical: "One obligation has outcome `needs_evidence`. Readiness is false; the obligation and its class are listed below.",
    },
    Line {
        slug: "readiness_needs_evidence",
        plain: "{n} things still need evidence:",
        technical: "{n} obligations have outcome `needs_evidence`. Readiness is false; each obligation and its class is listed below.",
    },
    Line {
        slug: "readiness_nothing_looked",
        plain: "No test ran for this.",
        technical: "Coverage state `none`: no link row bears on this criterion. Absence of evidence, recorded as absence.",
    },
    Line {
        slug: "readiness_unlinked_run",
        plain: "A test ran for this Change but isn't linked to this line — link it.",
        technical: "RUN-LINK-TRUTH: coverage state `none` for this criterion, and the Change holds a completed, bearing check run observed `proven` that no criterion links (`change_readiness::unlinked_completed_run`). It counts for no line until a named person links it (write address #4, basis `manual`); no automatic link is made.",
    },
    Line {
        slug: "readiness_settled_nothing",
        plain: "Something ran, and it settled nothing about this.",
        technical: "Coverage state `ran_nothing`, or every bearing item observed `loose`: linked, and nothing in it satisfies the line.",
    },
    Line {
        slug: "readiness_not_proven",
        plain: "The record holds the opposite of this.",
        technical: "A bearing item observed `not_proven` (non-zero exit with both streams captured whole).",
    },
    Line {
        slug: "readiness_tool_absent",
        plain: "The check this asks for can't run on this machine.",
        technical: "Coverage state `absent_tool`: the accepted profile asks for a class whose program was not resolvable here. Not a failure of the check — the check cannot start.",
    },
    Line {
        slug: "readiness_other_version",
        plain: "What was checked was a different version of the code.",
        technical: "`applies` is not `true` for every bearing item: the examined-inputs record of each does not match the Change's intended record. A result about another version is a result about something else.",
    },
    Line {
        slug: "readiness_stale",
        plain: "This was checked, and the files it looked at have changed since.",
        technical: "An applicability row says `no_longer_applies` for a bearing item. Freshness outranks the observed result.",
    },
    Line {
        slug: "readiness_never_assessed",
        plain: "Whether this still applies was never assessed. Run `thruline change prepare` and it will be.",
        technical: "R-2026-09-28-25: a bearing item observed `proven` on this version, with a completed execution, has no `change_criterion_freshness` row, and no item is admissible. Unassessed applicability never counts toward ready, the same way `couldnt` does not (R-2026-09-27-14). `change prepare` re-binds the intended record; `evidence_applicability::on_intended_bound` then assesses every linked row.",
    },
    Line {
        slug: "readiness_could_not_assess",
        plain: "Whether this still applies could not be assessed: what the files looked like when it was checked was not written down. Run the check again and it will be.",
        technical: "NEVER-ASSESSED-NOT-READY-R2: never assessed, and no bearing proven item on this version can be assessed by `change prepare` (`evidence_applicability::assessable_by_next_bind` is false for each): the evidence carries no stored snapshot of its own, and the Change's standing `intended` binding has none, or a binding since the evidence was recorded had none (D-051). No freshness row is written for it; a new check run records its own tree and is assessed from that.",
    },
    Line {
        slug: "readiness_stopped",
        plain: "The check that covers this was stopped before it finished.",
        technical: "A bearing item's latest `change_run_executions` row is `running`, `cancelled`, `interrupted` or `blocked`. `settles` is false for all four (T-3-4).",
    },
    Line {
        slug: "readiness_unknown",
        plain: "Something ran, and ThruLine can't tell what it covered.",
        technical: "Observed `couldnt`: a test command exited 0 and this store holds no count of what it ran, so a full suite, zero tests and every test skipped are indistinguishable (`Q3`).",
    },
    Line {
        slug: "readiness_inputs_missing",
        plain: "ThruLine hasn't written down what will be looked at for this Change.",
        technical: "`B7.12` candidate identity: no `examined_inputs` record with subject kind `intended` for this Change. Non-waivable.",
    },
    Line {
        slug: "readiness_review_missing",
        plain: "What counts as done for this Change says it needs a review, and no review is written down.",
        technical: "The accepted check profile carries a review policy entry and no review record exists. Recorded, never assumed: with no such entry there is no review obligation at all.",
    },
    Line {
        slug: "readiness_waived",
        plain: "Waived by {who}, under {authority}: {why}. Nobody checked this.",
        technical: "An appended waiver record of kind `waived` with actor, authority and rationale. It stops the obligation blocking and is never counted as evidence: it is absent from `rests_on` by construction.",
    },
    Line {
        slug: "readiness_not_applicable",
        plain: "Doesn't apply here — {why} (said by {who}, under {authority}). Nobody checked this.",
        technical: "An appended waiver record of kind `not_applicable` — the record says it does not apply — with actor, authority and rationale. Same treatment as a waiver: it clears the obligation and is never evidence.",
    },
    Line {
        slug: "readiness_not_waivable",
        plain: "This one can't be set aside by anybody.",
        technical: "`B7.12`: candidate identity, decision authority and packet consistency are refused by `ObligationWaiver::validate`, so no stored record can set them aside.",
    },
    Line {
        slug: "readiness_ready_with_exceptions",
        plain: "Ready for approval with exceptions for version {version}. It rests on:",
        technical: "`change_readiness::compute` returned ready for {version} with `with_exceptions` set: every obligation clears, and at least one of them clears on an appended waiver record rather than on an admissible item. `rests_on` holds only the satisfied ones; the set-aside lines are listed separately and are in neither.",
    },
    Line {
        slug: "readiness_set_aside_heading",
        plain: "Set aside, with nobody checking:",
        technical: "`Readiness::waived` and `Readiness::not_applicable`. Rendered under, never inside, `rests_on`.",
    },
    Line {
        slug: "readiness_no_exception_path",
        plain: "Nothing can be set aside on this Change: what counts as done for it does not say who is allowed to do that. Say so in what counts as done first, naming who decides.",
        technical: "`B7.21`: the accepted check profile carries no `policy:exceptions-allowed-by:<authority>` entry, so `change_readiness::exception_authority` is `None` and both write routes refuse. The authority is never taken from the request.",
    },
    Line {
        slug: "readiness_blocked_reason",
        plain: "This version can't be approved yet: some of what counts as done for it still needs evidence.",
        technical: "`change_readiness::compute` returned `ready: false`. The Approve act is off for that reason and for no other.",
    },
    Line {
        slug: "readiness_blocked_enabler",
        plain: "Run the checks that cover the lines listed above. Each one runs from this screen, or through ThruLine the way you normally would.",
        technical: "Each `needs_evidence` obligation needs one admissible item: bearing, observed `proven`, `applies` true, not stale, and an execution state that does not contradict completion.",
    },
];

/// One readiness line by slug, or `None` when the slug is not one of ours.
pub fn readiness_line(slug: &str) -> Option<&'static Line> {
    READINESS_LINES.iter().find(|l| l.slug == slug)
}

/// One execution line by slug, or `None` when the slug is not one of ours.
pub fn execution_line(slug: &str) -> Option<&'static Line> {
    EXECUTION_LINES.iter().find(|l| l.slug == slug)
}

/// An applicability line in both registers, with `{path}`/`{version}` filled
/// when given. A caller with neither (the reason lines that carry no path,
/// such as `criteria_changed`) gets the line with the placeholder text
/// removed rather than left dangling.
pub fn applicability_sentence(
    slug: &str,
    path: Option<&str>,
    version: Option<&str>,
) -> Option<(String, String)> {
    let line = applicability_line(slug)?;
    let fill = |template: &str| {
        let mut out = template.replace("{path}", path.unwrap_or("this path"));
        out = out.replace("{version}", version.unwrap_or("this version"));
        out
    };
    Some((fill(line.plain), fill(line.technical)))
}

/// One examined-inputs line by slug, or `None` when the slug is not one of
/// ours.
pub fn inputs_line(slug: &str) -> Option<&'static Line> {
    INPUTS_LINES.iter().find(|l| l.slug == slug)
}

/// An examined-inputs line in both registers, with its closed set of
/// placeholders filled. `{port}`, `{app}`, `{tool}`, `{value}` and `{count}`
/// are the only five, so no sentence here can be assembled out of free text.
pub fn inputs_sentence(
    slug: &str,
    fills: &crate::examined_inputs::Fills,
) -> Option<(String, String)> {
    let line = inputs_line(slug)?;
    // The noun a count carries belongs to the sentence, not to the caller: a
    // caller that could choose it could make one line say "3 files" and
    // another "3 records" about the same number.
    let noun = match slug {
        "inputs_record_count" => ("record", "records"),
        _ => ("file", "files"),
    };
    let fill = |s: &str| {
        let mut out = s.to_string();
        if let Some(port) = fills.port {
            out = out.replace("{port}", &port.to_string());
        }
        if let Some(app) = &fills.app {
            out = out.replace("{app}", app);
        }
        if let Some(tool) = &fills.tool {
            out = out.replace("{tool}", tool);
        }
        if let Some(value) = &fills.value {
            out = out.replace("{value}", value);
        }
        if let Some(count) = fills.count {
            out = out.replace("{count}", &count_phrase(count, noun.0, noun.1));
        }
        out
    };
    Some((fill(line.plain), fill(line.technical)))
}

/// One coverage line by slug, or `None` when the slug is not one of ours.
pub fn coverage_line(slug: &str) -> Option<&'static Line> {
    COVERAGE_LINES.iter().find(|l| l.slug == slug)
}

/// A coverage line in both registers, with its two placeholders filled.
///
/// `{tool}` is the program the profile names and this machine does not have;
/// `{count}` is a row count from a typed field. Nothing else is substitutable,
/// so no sentence here can be assembled out of free text.
pub fn coverage_sentence(slug: &str, tool: &str, count: u64) -> Option<(String, String)> {
    let line = coverage_line(slug)?;
    let fill = |s: &str| {
        s.replace("{tool}", tool)
            .replace("{count}", &count_phrase(count, "link", "links"))
    };
    Some((fill(line.plain), fill(line.technical)))
}

/// A `change new` line in both registers. There are no placeholders in this
/// table on purpose: every number these sentences could carry is an id or a
/// name, and an id belongs beside a sentence rather than inside one.
pub fn change_open_sentence(slug: &str) -> Option<(&'static str, &'static str)> {
    change_open_line(slug).map(|l| (l.plain, l.technical))
}

/// Level two of the same disclosure, asked of a *claim* rather than an
/// attention item — L2 Claim & Receipt's "what happened" line.
///
/// Keyed by state, because that is what a claim has. Transcribed from
/// UX-DOCTRINE's "What happened, for a claim" table. The `contradicted` and
/// `stale` sentences repeat `DISCLOSURES` word for word on purpose: one fact
/// reached by a different door is still one fact, and two wordings for it is
/// how a voice drifts. `unsupported` and `dropped` are present so the table is
/// total; both classes are in shadow and render nowhere.
pub const CLAIM_STORIES: &[Term] = &[
    Term {
        slug: "verified",
        word: "proven",
        sentence: "The run that proves this was captured as it happened, and the record holds it.",
    },
    Term {
        slug: "supported",
        word: "supported",
        sentence: "The record holds work in the same turn that matches what was said.",
    },
    Term {
        slug: "contradicted",
        word: "contradicted",
        sentence: "The record holds both the claim and a run that disagrees with it.",
    },
    Term {
        slug: "stale",
        word: "out of date",
        sentence: "A later check compared the project against the state this proof was taken from, and found it changed.",
    },
    Term {
        slug: "pending",
        word: "unfinished",
        sentence: "It was proposed in this run, and no work in the same turn follows it.",
    },
    Term {
        slug: "recorded",
        word: "noted",
        sentence: "Work happened here. Nothing has checked whether it did what was said.",
    },
    Term {
        slug: "unknown",
        word: "insufficient evidence",
        sentence: "Something was recorded here, but the outcome could not be read well enough to judge.",
    },
    Term {
        slug: "refused",
        word: "not evaluated",
        sentence: "This one sits outside what we check, so no judgment was made.",
    },
    Term {
        slug: "unsupported",
        word: "no proof found",
        sentence: "The record holds the statement, and nothing that would show it was done.",
    },
    Term {
        slug: "dropped",
        word: "abandoned",
        sentence: "It was planned, and nothing later in the record touches it again.",
    },
];

/// The level-two line for a claim in `state`. An unrecognised state falls back
/// to this table's own `unknown` row rather than printing a slug — a slug on a
/// screen is how engine vocabulary leaks, and the generic fallback in `term`
/// would answer at the wrong level.
pub fn claim_story(state: &str) -> &'static str {
    CLAIM_STORIES
        .iter()
        .find(|t| t.slug == state)
        .unwrap_or_else(|| term(CLAIM_STORIES, "unknown"))
        .sentence
}

/// A capture-readiness state as a person reads it: a title, then one line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Readiness {
    pub slug: &'static str,
    pub title: &'static str,
    pub detail: &'static str,
}

/// The twelve A78 states. Adopted verbatim from M5.R2's shipped copy, which
/// already satisfied this doctrine — routing it unchanged is the point of a
/// single authority, not rewriting good strings to prove ownership.
pub const READINESSES: &[Readiness] = &[
    Readiness {
        slug: "not_available",
        title: "Assured Capture unavailable",
        detail: "This source has no admitted local assurance contract.",
    },
    Readiness {
        slug: "bounded_only",
        title: "Bounded observations only",
        detail: "This source can record explicit observations, but cannot prove the command boundary.",
    },
    Readiness {
        slug: "disabled",
        title: "Assured Capture disabled",
        detail: "No new commands are captured; historical proof remains.",
    },
    Readiness {
        slug: "available_not_installed",
        title: "Assured Capture available",
        detail: "Review the exact local hook changes before installation.",
    },
    Readiness {
        slug: "degraded_source_version",
        title: "Source version not qualified",
        detail: "Earlier proof is preserved; new commands will not be assured.",
    },
    Readiness {
        slug: "degraded_configuration_drift",
        title: "Capture configuration changed",
        detail: "New assurance is paused until a fresh plan is reviewed.",
    },
    Readiness {
        slug: "degraded_key_or_identity",
        title: "Collector identity unavailable",
        detail: "Existing signatures remain recorded; new assurance is paused.",
    },
    Readiness {
        slug: "installed_unverified",
        title: "Installation not verified",
        detail: "Run the isolated source-native self-test before authorizing this project.",
    },
    Readiness {
        slug: "degraded_source_pairing",
        title: "Source pairing incomplete",
        detail: "A complete source result link was not accepted; new assurance is paused.",
    },
    Readiness {
        slug: "unsupported_profile",
        title: "Command profile not assured",
        detail: "This command is outside the measured local foreground contract.",
    },
    Readiness {
        slug: "capture_error",
        title: "Capture needs attention",
        detail: "The latest capture did not complete; historical proof remains.",
    },
    Readiness {
        slug: "assured_ready",
        title: "Assured Capture ready",
        detail: "Supported Codex commands can create execution-boundary proof for this authorized project.",
    },
];

fn term(table: &'static [Term], slug: &str) -> &'static Term {
    // Every caller below passes a slug from an exhaustive match over a real
    // enum, so a miss is a table that lost a row — a bug, not bad input.
    table.iter().find(|t| t.slug == slug).unwrap_or(&Term {
        slug: "",
        word: "unclear",
        sentence: "There's evidence here, but it couldn't be read well enough to judge.",
    })
}

/// Look a verdict up by slug. Returns `None` for a slug outside the
/// vocabulary rather than guessing at one.
pub fn verdict(slug: &str) -> Option<&'static Term> {
    VERDICTS.iter().find(|t| t.slug == slug)
}

/// Exhaustive over [`NodeState`]: an eighth state cannot silently inherit a
/// neighbour's sentence.
pub fn node_state(state: NodeState) -> &'static Term {
    term(VERDICTS, state.as_str())
}

/// Exhaustive over [`Currency`].
pub fn currency(value: Currency) -> &'static Term {
    term(CURRENCIES, value.as_str())
}

/// Exhaustive over [`ProofApplicability`] — whether a *proof* still matches the
/// project. Its own table rather than a reuse of the verdict words: proof
/// currentness and node currency are separate dimensions in the engine
/// (`capture.rs`), and collapsing them in the copy would re-merge in the user's
/// head what the engine went to trouble to keep apart.
pub fn proof_currentness(value: ProofApplicability) -> &'static Term {
    term(PROOF_CURRENTNESS, value.as_str())
}

/// Exhaustive over all twelve [`CaptureReadiness`] states.
pub fn readiness(state: CaptureReadiness) -> &'static Readiness {
    let slug = state.as_str();
    READINESSES
        .iter()
        .find(|r| r.slug == slug)
        .unwrap_or(&READINESSES[0])
}

/// The chip label for a state: the composition word, capitalized. Chips never
/// carry an engine enum, so this is the only label source in the product.
pub fn chip_label(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// A count with its noun. UX-DOCTRINE §4: a number alone is a bug.
pub fn count_phrase(n: u64, singular: &str, plural: &str) -> String {
    if n == 1 {
        format!("1 {singular}")
    } else {
        format!("{n} {plural}")
    }
}

/// A node badge: "3 supported · 1 no proof found".
///
/// Zero-count elements are dropped rather than rendered, which is §4's
/// conditional-chip rule expressed once instead of at every call site — the
/// paper cut where `Refused 0` survived came from having it at call sites.
pub fn composition(parts: &[(u64, &str)]) -> String {
    parts
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, word)| format!("{n} {word}"))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// The one sentence above the items.
///
/// Calm is a sentence, never an absence: a blank headline makes a person
/// wonder whether the product ran.
/// The headline for a project whose projection has not been built.
///
/// Kept separate from [`headline`] because that one ends "Everything else
/// checks out", which is false here: nothing else was checked. A93/A94 —
/// absence of a projection is not absence of a problem, and the sentence above
/// the items must not imply otherwise.
pub fn headline_not_read(project: &str) -> String {
    format!("{project} — not read yet, so nothing has been checked.")
}

/// The headline for a project the recorder was not fully watching.
///
/// Kept separate from [`headline`] for the same reason as [`headline_not_read`]:
/// "everything checks out" is a claim about the record, and it is only true
/// when the record is complete. When a coverage gap is known — a shim that
/// nothing invokes, test-runner invocations a tool recorded but the recorder
/// never captured — the calm sentence is refused here rather than corrected in
/// a view. A93/A94, applied to coverage: absence of a finding is not absence of
/// a problem when part of the machine was never read.
///
/// Measured before this existed: 90 of 92 exported projects read
/// "everything checks out." while `doctor` reported the shim was not on PATH,
/// so nothing invoked it.
/// The calm ITEM, when coverage is incomplete.
///
/// The headline and the item beneath it are two sentences about the same
/// record; if only the headline is gated, the screen contradicts itself —
/// "not everything was watched" above "Everything checks out. Nothing to do."
/// Measured: that is exactly what the first cut of the gate produced.
pub fn all_clear_uncovered_sentence() -> &'static str {
    "No flags were raised, but part of the machine was not being recorded."
}

/// The remedy, named literally. An item that states a gap without saying what
/// closes it is a worry, not an action.
pub fn all_clear_uncovered_action() -> &'static str {
    "Run `thruline doctor` to see what was missed."
}

pub fn headline_uncovered(project: &str, needing_attention: u64) -> String {
    if needing_attention == 0 {
        format!("{project} — nothing raised a flag, but not everything was watched.")
    } else {
        format!(
            "{project} — {}, and not everything was watched.",
            count_phrase(
                needing_attention,
                "thing worth a look",
                "things worth a look"
            )
        )
    }
}

pub fn headline(project: &str, needing_attention: u64) -> String {
    if needing_attention == 0 {
        format!("{project} — everything checks out.")
    } else {
        format!(
            "{project} — {}. Everything else checks out.",
            count_phrase(
                needing_attention,
                "thing worth a look",
                "things worth a look"
            )
        )
    }
}

/// The one sentence above every project — L0's headline (M6.R2).
///
/// [`headline`] answers for one project; this answers "is anything wrong
/// *anywhere*". It lives here rather than in the UI for the same reason every
/// other first-level string does: a sentence composed in a view is a sentence
/// no lexicon gate has ever read. The no-records case is a sentence too —
/// ATTRACT MODE is a state, not a blank screen.
pub fn index_headline(projects: u64, needing_attention: u64) -> String {
    if projects == 0 {
        return "No records read yet.".to_string();
    }
    let across = count_phrase(projects, "project", "projects");
    if needing_attention == 0 {
        format!("Everything checks out across {across}.")
    } else {
        format!(
            "{} across {across}. Everything else checks out.",
            count_phrase(
                needing_attention,
                "thing worth a look",
                "things worth a look"
            )
        )
    }
}

/// The second sentence of the no-records screen — ATTRACT MODE's own copy.
///
/// Calm is designed, and so is empty: the cabinet demonstrates itself rather
/// than showing a person a blank panel. It lives in the authority for the same
/// reason the headline does — a sentence written in a view is a sentence no
/// lexicon gate has read.
pub fn attract_body() -> &'static str {
    "Point ThruLine at the records your coding agents already leave behind, and it will tell you what was actually done."
}

/// L3 Trace's caption strip — the sentence that says what the screen is.
///
/// DESIGN-HANDOFF F6: nothing on the old graph said what the graph was, and
/// "even Trace, once demoted, must always carry a sentence". The subject is
/// the claim's own first-level sentence and is quoted, not paraphrased.
pub fn trace_caption(subject: &str, day: &str) -> String {
    if day.is_empty() {
        format!("You are looking at the recorded events around \u{201c}{subject}\u{201d}.")
    } else {
        format!("You are looking at the recorded events around \u{201c}{subject}\u{201d}, {day}.")
    }
}

/// What the maze does not show, said out loud.
///
/// The window is the whole point of the demotion, so the screen states its own
/// boundary rather than letting a person assume they are seeing everything.
pub fn trace_foot(at_widest: bool) -> &'static str {
    if at_widest {
        "This is as far as the maze opens. The rest of the run stays unread until you ask for it."
    } else {
        "Everything else in this run is out of frame. Widen to open one more corridor."
    }
}

/// The one-line reminder above the corridors.
pub fn trace_scope() -> &'static str {
    "The maze only ever opens around what you asked about."
}

/// The first-level sentence for an attention item.
///
/// `claimed`/`observed` fill the doctrine's "with numbers when numeric"
/// clause; a numeric contradiction that has both says what they were, because
/// "the record shows otherwise" is weaker than the two numbers side by side.
pub fn attention_sentence(
    kind: AttentionKind,
    claimed: Option<&str>,
    observed: Option<&str>,
    readiness_state: Option<CaptureReadiness>,
) -> String {
    match kind {
        AttentionKind::Contradiction => match (claimed, observed) {
            (Some(c), Some(o)) => format!(
                "{} It said {c}; the run shows {o}.",
                term(VERDICTS, "contradicted").sentence
            ),
            _ => term(VERDICTS, "contradicted").sentence.to_string(),
        },
        AttentionKind::Staleness => term(VERDICTS, "stale").sentence.to_string(),
        AttentionKind::CaptureAttention => readiness_state
            .map(|s| readiness(s).title.to_string())
            .unwrap_or_else(|| readiness(CaptureReadiness::CaptureError).title.to_string()),
        AttentionKind::AllClear => "Everything checks out.".to_string(),
        AttentionKind::NotRead => "This project has not been read yet.".to_string(),
    }
}

/// The one suggested action for an attention item.
///
/// Every one of these is an instruction to the human. ThruLine records; it
/// never re-runs. A suggested action that could execute would make the receipt
/// proof of our own action rather than of the project's history — see §5u and
/// the `suggestions_cannot_execute` gate.
pub fn attention_action(kind: AttentionKind, readiness_state: Option<CaptureReadiness>) -> String {
    match kind {
        AttentionKind::Contradiction => "Look at the receipt.".to_string(),
        AttentionKind::Staleness => "Run it again, then record what happens.".to_string(),
        AttentionKind::CaptureAttention => readiness_state
            .map(|s| readiness(s).detail.to_string())
            .unwrap_or_else(|| readiness(CaptureReadiness::CaptureError).detail.to_string()),
        AttentionKind::AllClear => "Nothing to do.".to_string(),
        AttentionKind::NotRead => "Read it, then look again.".to_string(),
    }
}

/// Level two: what happened, in the same voice as level one.
///
/// The receipt clause carries its noun and disappears at zero rather than
/// announcing "0 receipts" — §4 expressed once, here, instead of at the two
/// call sites that would each have to remember it.
pub fn what_happened(
    kind: AttentionKind,
    readiness_state: Option<CaptureReadiness>,
    receipt_count: u64,
) -> String {
    let slug = match kind {
        AttentionKind::Contradiction => "contradiction",
        AttentionKind::Staleness => "staleness",
        AttentionKind::CaptureAttention => "capture_attention",
        AttentionKind::AllClear => "all_clear",
        AttentionKind::NotRead => "not_read",
    };
    let mut out = term(DISCLOSURES, slug).sentence.to_string();
    if kind == AttentionKind::CaptureAttention {
        if let Some(state) = readiness_state {
            out.push(' ');
            out.push_str(readiness(state).detail);
        }
    }
    if receipt_count > 0 {
        out.push_str(&format!(
            " It traces to {} in the original transcript.",
            count_phrase(receipt_count, "receipt", "receipts")
        ));
    }
    out
}

/// What `why` says about an id that no longer derives.
pub fn no_longer_derives() -> &'static str {
    term(DISCLOSURES, "expired").sentence
}

/// The line that keeps a calm headline honest when a dismissal is hiding
/// something. Never rendered at zero.
pub fn hidden_note(dismissed: u64) -> Option<String> {
    (dismissed > 0).then(|| {
        format!(
            "{} hidden — `thruline now --all` shows them.",
            count_phrase(dismissed, "dismissed item", "dismissed items")
        )
    })
}

/// The whole table as the UI's generated module.
///
/// A `.js` module rather than a `.json` file so both Vite and `node --test`
/// load it without import attributes — the mirror should not need a runtime
/// caveat to be readable. The UI's copy is generated from this and compared
/// byte for byte, so "one voice" is a mechanical fact rather than a convention
/// two codebases are trusted to keep.
pub fn mirror_module() -> String {
    #[derive(Serialize)]
    struct Table {
        verdicts: &'static [Term],
        currencies: &'static [Term],
        ledger_states: &'static [Term],
        proof_currentness: &'static [Term],
        abstentions: &'static [Term],
        disclosures: &'static [Term],
        claim_stories: &'static [Term],
        readinesses: &'static [Readiness],
        jargon: &'static [&'static str],
        enums: &'static [&'static str],
    }
    let mut json = serde_json::to_string_pretty(&Table {
        verdicts: VERDICTS,
        currencies: CURRENCIES,
        ledger_states: LEDGER_STATES,
        proof_currentness: PROOF_CURRENTNESS,
        abstentions: ABSTENTIONS,
        disclosures: DISCLOSURES,
        claim_stories: CLAIM_STORIES,
        readinesses: READINESSES,
        jargon: JARGON,
        enums: ENUMS,
    })
    .unwrap_or_default();
    json.push('\n');
    format!(
        "/* GENERATED from crates/core/src/voice.rs — do not hand-edit.\n\
         \x20  Regenerate: THRULINE_BLESS_VOICE=1 cargo test -p thruline-core --test voice_first_level\n\
         \x20  Doctrine is the source (docs/UX-DOCTRINE.md); the Rust table is the authority. */\n\
         export default {json}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_first_level_string_is_free_of_engine_lexicon() {
        // §5v's teeth, applied to the authority itself. The CLI-wide walk
        // lives in tests/voice_first_level.rs.
        let mut checked = 0usize;
        for t in VERDICTS
            .iter()
            .chain(CURRENCIES)
            .chain(LEDGER_STATES)
            .chain(PROOF_CURRENTNESS)
            .chain(ABSTENTIONS)
            .chain(DISCLOSURES)
            .chain(CLAIM_STORIES)
        {
            for s in [t.word, t.sentence] {
                assert!(
                    banned_in(s).is_none(),
                    "banned term {:?} in first-level string {s:?}",
                    banned_in(s)
                );
                checked += 1;
            }
        }
        for r in READINESSES {
            for s in [r.title, r.detail] {
                assert!(
                    banned_in(s).is_none(),
                    "banned term {:?} in readiness string {s:?}",
                    banned_in(s)
                );
                checked += 1;
            }
        }
        assert!(checked >= 50, "the walk covered suspiciously little");
    }

    #[test]
    fn the_doctrine_example_is_generated_not_hardcoded() {
        // UX-DOCTRINE's own badge example must fall out of the table — the
        // C0.2 amendment re-cut the example along with the words.
        let supported = node_state(NodeState::Supported).word;
        let no_proof = node_state(NodeState::Unsupported).word;
        assert_eq!(
            composition(&[(3, supported), (1, no_proof)]),
            "3 supported · 1 no proof found"
        );
    }

    #[test]
    fn zero_counts_never_render_and_numbers_never_arrive_naked() {
        assert_eq!(
            composition(&[(3, "supported"), (0, "not evaluated")]),
            "3 supported"
        );
        assert_eq!(composition(&[(0, "supported"), (0, "no proof found")]), "");
        assert_eq!(count_phrase(1, "thing", "things"), "1 thing");
        assert_eq!(count_phrase(2, "thing", "things"), "2 things");
        assert!(headline("thruline", 0).ends_with("everything checks out."));
        assert!(headline("thruline", 2).contains("2 things worth a look"));
    }

    #[test]
    fn every_engine_state_has_a_translation() {
        // Exhaustive by construction: adding a variant breaks the match, and
        // the lookup asserts the table actually carries the row.
        for state in [
            NodeState::Recorded,
            NodeState::Supported,
            NodeState::Pending,
            NodeState::Unknown,
            NodeState::Unsupported,
            NodeState::Dropped,
            NodeState::Contradicted,
        ] {
            assert_eq!(node_state(state).slug, state.as_str());
        }
        for c in [
            Currency::Current,
            Currency::Superseded,
            Currency::OutOfScope,
        ] {
            assert_eq!(currency(c).slug, c.as_str());
        }
        for r in [
            CaptureReadiness::NotAvailable,
            CaptureReadiness::BoundedOnly,
            CaptureReadiness::Disabled,
            CaptureReadiness::AvailableNotInstalled,
            CaptureReadiness::DegradedSourceVersion,
            CaptureReadiness::DegradedConfigurationDrift,
            CaptureReadiness::DegradedKeyOrIdentity,
            CaptureReadiness::InstalledUnverified,
            CaptureReadiness::DegradedSourcePairing,
            CaptureReadiness::UnsupportedProfile,
            CaptureReadiness::CaptureError,
            CaptureReadiness::AssuredReady,
        ] {
            assert_eq!(readiness(r).slug, r.as_str());
        }
    }

    #[test]
    fn the_ban_actually_catches_things() {
        // A test that only ever passes proves nothing; these are the exact
        // regressions §5v exists to stop.
        // Label leaks — the shapes that actually shipped.
        assert_eq!(banned_in("Refused 0"), Some("refused"));
        assert_eq!(banned_in("unsupported"), Some("unsupported"));
        assert_eq!(banned_in("Unsupported · 6"), Some("unsupported"));
        // C0.2, evolved in the open: "Supported" and "Contradicted" are now
        // the doctrine-adopted rendered words, so the label shape that used to
        // be a leak is the reviewed vocabulary.
        assert_eq!(banned_in("Contradicted"), None);
        assert_eq!(banned_in("3 Contradicted claims"), None);
        assert_eq!(banned_in("Supported"), None);
        // C0.2's dead vocabulary and confidence theater, banned as jargon.
        assert_eq!(banned_in("chain UNVERIFIED"), Some("unverified"));
        assert_eq!(banned_in("Unverified"), Some("unverified"));
        assert_eq!(banned_in("94% confidence"), Some("confidence"));
        // Jargon, anywhere.
        assert_eq!(banned_in("high fidelity parse"), Some("fidelity"));
        assert_eq!(
            banned_in("deterministic · local"),
            Some("deterministic · local")
        );
        assert_eq!(banned_in("NLI said so"), Some("nli"));
        // Real English that happens to use an enum word is NOT a violation.
        // Banning these would push copy into worse English in the name of a
        // rule that exists to produce better English.
        assert_eq!(banned_in("Installation not verified"), None);
        assert_eq!(
            banned_in("Existing signatures remain recorded; new assurance is paused."),
            None
        );
        assert_eq!(banned_in("Backed by the record."), None);
        // Sentence-initial capitals say nothing about label-ness.
        assert_eq!(
            banned_in("Supported Codex commands can create execution-boundary proof."),
            None
        );
        // C0.2: a counted chip in the adopted vocabulary is not a leak either.
        assert_eq!(banned_in("Contradicted: 3"), None);
        // Kept in step with ui/src/lib/voice.test.js: both sides must agree on
        // this exact case, or "one voice" is two.
        assert_eq!(banned_in("Supported by evidence"), None);
    }
}

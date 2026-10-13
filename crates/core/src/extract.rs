//! Claim extraction v2 — marker-first, precision-first (§5 M3.3).
//!
//! ## Why this is a rewrite and not a patch
//!
//! M2's extractor matched sentence-initial first-person past ("I've added…").
//! Measured against the A19 yardstick it scored **0.00 completed-work recall**:
//! the dominant completed-work form in real transcripts is a **completion
//! marker tied to a test or build artifact** — "104/104 green", "Release ✓ —
//! 0 errors, 0 warnings", "Deployed (Version `4f50f678`)" — not the first-person
//! construction. So markers lead here and first-person is the *second* rule.
//!
//! ## Precision is worth more than recall
//!
//! A missed claim costs a node its verdict. A false claim costs the user a ⚠
//! against work that was really done, which is the failure mode that discredits
//! the whole product. So every rule below is gated:
//!
//! * **Artifact-tying.** A completion marker is only a claim when the sentence
//!   also names something a tool could have touched — a backticked token, a
//!   path, a command family, a test/build noun, or a machine-shaped outcome.
//!   "Audit complete." is not extractable; "All 54 proxy tests pass." is.
//! * **Verifiability.** Only assertions about actions that leave tool evidence.
//!   "I've considered the tradeoffs" is never a claim, however past-tense.
//! * **Negation and hedging kill extraction**, per sentence, before anything
//!   else runs. "The Write didn't land the bytes I intended" is not a claim
//!   that the write landed.
//! * **Intent pre-empts.** A sentence that opens by announcing work ("Now
//!   rebuild + install on the sim and confirm green.") is a proposal even
//!   though it contains "green". Reading it as completed work would invent a
//!   claim the assistant never made.
//!
//! Code fences, headings and table rows are never scanned at all.

/// Extractor tag stored on every claim row, part of the idempotency key.
pub const EXTRACTOR_NAME: &str = "marker-v2";

/// §5b's closed set of claim kinds — the scope principle, as a type.
///
/// M3.7's adjudication found 0/12 warnings true, and the residue was **not**
/// a scoring problem: it was the product accusing about categories it has no
/// means to verify. Real transcripts are dominated by *findings* — diagnoses,
/// measurements, root causes, admissions — and the extractor was reading them
/// as claims of work because they use verification verbs.
///
/// Recognising a category we cannot judge, and staying silent about it, is the
/// product working. Only [`ClaimKind::Work`] and [`ClaimKind::Marker`] may ever
/// produce an accusation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimKind {
    /// A specific world-changing action was performed. "I've added retry logic
    /// to `client.rs`." Accusable.
    Work,
    /// A machine-checkable outcome occurred. "104/104 green." Accusable.
    Marker,
    /// Something is or was the case — a discovery, diagnosis, measurement or
    /// answer. The assistant learned something; it did not change the world.
    /// Extracted and stored for A29's future findings tier, never accused.
    Finding,
    /// A small immediate intention naming no durable target, fulfilled in the
    /// same turn. "Let me read it." Never a pending node, never a ghost.
    MicroIntent,
    /// Intent to act on a nameable target. May become `dropped`, but only when
    /// there was something to search for.
    Proposal,
}

impl ClaimKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ClaimKind::Work => "work",
            ClaimKind::Marker => "marker",
            ClaimKind::Finding => "finding",
            ClaimKind::MicroIntent => "micro_intent",
            ClaimKind::Proposal => "proposal",
        }
    }

    /// Every value the §7 CHECK constraint permits.
    pub const ALL: [ClaimKind; 5] = [
        ClaimKind::Work,
        ClaimKind::Marker,
        ClaimKind::Finding,
        ClaimKind::MicroIntent,
        ClaimKind::Proposal,
    ];

    /// **The scope principle in one predicate.** Only a material-work claim can
    /// ever be accused; everything else is out of scope for a verdict, however
    /// confidently the scorer feels about it.
    pub fn is_material_work(self) -> bool {
        matches!(self, ClaimKind::Work | ClaimKind::Marker)
    }

    /// Forward-looking kinds. Only [`ClaimKind::Proposal`] can be dropped;
    /// a micro-intent names nothing to look for, so its silence means nothing.
    pub fn is_forward(self) -> bool {
        matches!(self, ClaimKind::Proposal | ClaimKind::MicroIntent)
    }
}

/// Which rule fired. Carried into the receipt so a verdict can explain itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    /// "33/33 pass", "91 PASS / 0 FAIL".
    Ratio,
    /// "Build succeeded", "clippy clean", "0 errors, 0 warnings", "exit 0".
    Outcome,
    /// "Done", "Deployed", "merged", "shipped" — artifact-tied.
    Completion,
    /// "I've added…", "I set…".
    FirstPersonPast,
    /// "the tests were updated".
    Passive,
    /// "I'll…", "Let me…", "I'm going to…".
    Intent,
    /// "Adding `.ts`.", "Fixing:", "Merging P7".
    Gerund,
    /// "Hook the welcome message…", "Update the JS DOM refs".
    Imperative,
    /// "Now the panel class, notch layout, and controller:".
    Next,
    /// "`sessionId` matched the stem", "Files confirmed append-only" — an
    /// epistemic verb: the assistant learned this, it did not do it.
    Epistemic,
    /// "§8's session-identity rule **was wrong**", "`DCLogic` **is** a shim" —
    /// a copular assertion about the state of the world.
    Copular,
    /// "**A8 answer:** same location", "**Root cause found** — …".
    Answer,
    /// "Let me read it." — names nothing a later action could be matched to.
    Micro,
}

impl Marker {
    pub fn as_str(self) -> &'static str {
        match self {
            Marker::Ratio => "ratio",
            Marker::Outcome => "outcome",
            Marker::Completion => "completion",
            Marker::FirstPersonPast => "first_person_past",
            Marker::Passive => "passive",
            Marker::Intent => "intent",
            Marker::Gerund => "gerund",
            Marker::Imperative => "imperative",
            Marker::Next => "next",
            Marker::Epistemic => "epistemic",
            Marker::Copular => "copular",
            Marker::Answer => "answer",
            Marker::Micro => "micro",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    pub text: String,
    /// Byte offsets into the source text, for receipt highlighting.
    pub span_start: usize,
    pub span_end: usize,
    pub kind: ClaimKind,
    pub marker: Marker,
    /// Lowercased tokens a cheap lexical gate can match evidence against.
    pub artifacts: Vec<String>,
}

/// A structurally valid prose sentence offered to M4's language tier.
///
/// Segmentation remains deterministic and excludes code, headings, tables,
/// and other non-assertive containers before a model is called. `heuristic`
/// is the closed-world v2 classification, when one exists; the language tier
/// may confirm it or demote it, but cannot manufacture a claim from `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentenceCandidate {
    pub text: String,
    pub context: String,
    pub span_start: usize,
    pub span_end: usize,
    pub heuristic: Option<Claim>,
}

/// §10's `ClaimExtractor`, behind a trait so an LLM extractor slots in at M4.
pub trait ClaimExtractor {
    fn name(&self) -> &'static str;
    fn extract(&self, text: &str) -> Vec<Claim>;
}

pub struct MarkerExtractor;

impl ClaimExtractor for MarkerExtractor {
    fn name(&self) -> &'static str {
        EXTRACTOR_NAME
    }

    fn extract(&self, text: &str) -> Vec<Claim> {
        sentence_candidates(text)
            .into_iter()
            .filter_map(|candidate| candidate.heuristic)
            .collect()
    }
}

/// Deterministic sentence frontier shared by the heuristic and language tiers.
pub fn sentence_candidates(text: &str) -> Vec<SentenceCandidate> {
    let mut out = Vec::new();
    for (line_start, line) in prose_lines(text) {
        let content_off = strip_bullet(line);
        let content = &line[content_off..];
        for (s, e) in sentences(content) {
            let raw = &content[s..e];
            let sent = raw.trim();
            if sent.chars().count() < 3 {
                continue;
            }
            let lead = raw.len() - raw.trim_start().len();
            let plan = plan_form(line, sent);
            let resolved = if plan {
                Some((sent, 0))
            } else {
                match composite_resolution(sent) {
                    CompositeResolution::Whole => Some((sent, 0)),
                    CompositeResolution::Final(off) => {
                        let tail = &sent[off..];
                        let trim = tail.len() - tail.trim_start().len();
                        Some((tail.trim(), off + trim))
                    }
                    CompositeResolution::Abstain => None,
                }
            };
            let (subject, subject_off) = resolved.unwrap_or((sent, 0));
            let start = line_start + content_off + s + lead + subject_off;
            let classified = resolved.and_then(|_| {
                if plan {
                    Some((forward_kind(subject), Marker::Intent))
                } else {
                    classify(subject)
                }
            });
            let heuristic = classified.map(|(kind, marker)| Claim {
                text: subject.to_string(),
                span_start: start,
                span_end: start + subject.len(),
                kind,
                marker,
                artifacts: artifacts(subject),
            });
            out.push(SentenceCandidate {
                text: subject.to_string(),
                context: sent.to_string(),
                span_start: start,
                span_end: start + subject.len(),
                heuristic,
            });
        }
    }
    out
}

/* ── segmentation ────────────────────────────────────────────────────── */

/// Lines that are prose. Fenced code, headings, table rows, block quotes and
/// horizontal rules never produce claims — a heading like
/// "### 1. Models — add `.roleplay`" reads exactly like an imperative
/// proposal, and treating it as one is how an extractor manufactures work
/// that was never proposed.
fn prose_lines(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut fenced = false;
    let mut off = 0usize;
    for line in text.split_inclusive('\n') {
        let start = off;
        off += line.len();
        let t = line.trim();
        if t.starts_with("```") || t.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced || t.is_empty() {
            continue;
        }
        if t.starts_with('#') || t.starts_with('|') || t.starts_with('>') {
            continue;
        }
        // Horizontal rules and separator lines.
        if t.chars()
            .all(|c| c == '-' || c == '=' || c == '*' || c == '_' || c == ' ')
        {
            continue;
        }
        out.push((start, line));
    }
    out
}

/// Byte offset where a bullet's content begins. Bullets carry real claims
/// ("- **P7-fix: blockers closed, 14/14 mutations** → merge."), so the marker
/// is skipped but the content is kept.
fn strip_bullet(line: &str) -> usize {
    let b = line.as_bytes();
    let mut i = 0usize;
    while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
        i += 1;
    }
    if i + 1 < b.len() && matches!(b[i], b'-' | b'*' | b'+') && b[i + 1] == b' ' {
        i += 2;
    } else {
        let mut j = i;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > i && j + 1 < b.len() && matches!(b[j], b'.' | b')') && b[j + 1] == b' ' {
            i = j + 2;
        }
    }
    while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
        i += 1;
    }
    i
}

/// M3.11.1 plan forms that must never become completed-work claims.
///
/// An unchecked checklist is future by syntax, regardless of outcome words.
/// `Next:` / `Then:` and explicit `Step N:` prefixes are equally structural.
/// Numbered imperatives are plans unless they contain an unmistakable
/// past-result predicate (`succeeded`, `passed`, `failed`, `completed`).
fn plan_form(line: &str, sent: &str) -> bool {
    let raw = line.trim_start();
    let bullet = raw
        .strip_prefix("- ")
        .or_else(|| raw.strip_prefix("* "))
        .or_else(|| raw.strip_prefix("+ "))
        .unwrap_or(raw)
        .trim_start();
    if bullet.starts_with("[ ]") || bullet.starts_with("[ ]") {
        return true;
    }

    let low = normalise(sent);
    let t = low.trim_start();
    if t.starts_with("next:") || t.starts_with("then:") {
        return true;
    }
    if let Some(rest) = t.strip_prefix("step ") {
        let n = rest.chars().take_while(|c| c.is_ascii_digit()).count();
        if n > 0 && rest[n..].trim_start().starts_with(':') {
            return true;
        }
    }

    let mut chars = raw.char_indices();
    let mut end_digits = 0usize;
    for (i, c) in chars.by_ref() {
        if c.is_ascii_digit() {
            end_digits = i + c.len_utf8();
        } else {
            break;
        }
    }
    if end_digits == 0 {
        return false;
    }
    let Some(after) = raw.get(end_digits..) else {
        return false;
    };
    if !(after.starts_with(". ") || after.starts_with(") ")) {
        return false;
    }
    let first = first_word(t);
    const PLAN_VERBS: [&str; 14] = [
        "run", "build", "test", "verify", "add", "update", "archive", "compile", "deploy",
        "commit", "release", "fix", "install", "revert",
    ];
    const PAST_RESULTS: [&str; 8] = [
        "succeeded",
        "passed",
        "failed",
        "completed",
        "finished",
        "landed",
        "merged",
        "deployed",
    ];
    PLAN_VERBS.contains(&first.as_str()) && !PAST_RESULTS.iter().any(|w| t.contains(w))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompositeResolution {
    Whole,
    Final(usize),
    Abstain,
}

/// Resolve a failure-then-fix narrative to its final asserted state.
///
/// The whole sentence cannot be handed to the numeric contract: the initial
/// `failed` token makes a later all-pass clause look like a claim *about*
/// failure. We retain only the final explicit result clause. When recovery is
/// narrated but no final state is asserted, abstention is the only honest
/// extraction.
fn composite_resolution(sent: &str) -> CompositeResolution {
    // ASCII folding preserves byte offsets so `Final(off)` still points into
    // the original sentence (receipt spans are an invariant).
    let low = sent.to_ascii_lowercase();
    let failure = [" failed", " failure", " red", " broke", " error"]
        .iter()
        .filter_map(|w| low.find(w))
        .min();
    let Some(failure) = failure else {
        return CompositeResolution::Whole;
    };
    let recovery = [
        ";",
        " then ",
        " after ",
        " but ",
        " finally ",
        " rerun ",
        " re-run ",
    ]
    .iter()
    .filter_map(|sep| low[failure..].find(sep).map(|i| failure + i + sep.len()))
    .min();
    let Some(off) = recovery else {
        return CompositeResolution::Whole;
    };
    let final_low = low[off..].trim();
    let explicit = ratio_marker(final_low).is_some()
        || [
            " pass",
            "passed",
            "passing",
            " green",
            "succeeded",
            "success",
            "exit 0",
            "0 fail",
            "no failures",
            "build failed",
            "still fails",
        ]
        .iter()
        .any(|w| final_low.contains(w));
    if explicit {
        CompositeResolution::Final(off)
    } else {
        CompositeResolution::Abstain
    }
}

/// Sentence spans within one line.
///
/// Splits on `.`/`!`/`?` followed by whitespace or end. Deliberately does not
/// split on `:` — "Verified in shipped source: the counts increment inside the
/// per-page head loop" is one claim, and cutting it at the colon throws away
/// the half that names the artifact.
fn sentences(s: &str) -> Vec<(usize, usize)> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut start = 0usize;
    for i in 0..b.len() {
        if !matches!(b[i], b'.' | b'!' | b'?') {
            continue;
        }
        // A48: a stop may be followed by CLOSING markdown before the space.
        // "**MarketKit 400, MarketCore 43, gates green.** Recording the row …"
        // has `.` then `*`, `*`, ` ` — so the old whitespace-only test saw no
        // boundary, the whole message stayed one sentence, and because it ended
        // in a colon the A40 lead-in rule swallowed a marker claim carrying
        // counts. The yardstick caught it as a marker miss on row 59; the cost
        // is a false negative on the one class that can convict.
        let mut j = i + 1;
        while j < b.len() && matches!(b[j], b'*' | b'_' | b'`' | b')' | b']' | b'"' | b'\'') {
            j += 1;
        }
        let next_ws = j >= b.len() || (b[j] as char).is_whitespace();
        if !next_ws {
            // "test-gate.sh", "22.2 MB", ":856" — an inner dot, not a stop.
            continue;
        }
        if b[i] == b'.' && i >= 2 {
            // A single-letter initial ("§5.1 A. Smith") is not a stop.
            let prev = b[i - 1] as char;
            let prev2 = b[i - 2] as char;
            if prev.is_ascii_uppercase() && !prev2.is_alphanumeric() {
                continue;
            }
        }
        out.push((start, j));
        start = j;
    }
    if start < b.len() {
        out.push((start, b.len()));
    }
    out
}

/* ── vocabularies (explicit, so the rules stay auditable) ────────────── */

/// Verbs whose completion leaves no tool evidence. Never extractable, however
/// past-tense the sentence is. §5's rule 3, verbatim in spirit.
const NON_VERIFIABLE: [&str; 22] = [
    "considered",
    "thought",
    "believed",
    "understood",
    "realised",
    "realized",
    "noticed",
    "suspected",
    "assumed",
    "felt",
    "hoped",
    "wanted",
    "liked",
    "preferred",
    "agreed",
    "decided",
    "knew",
    "meant",
    "guessed",
    "wondered",
    "expected",
    "intended",
];

/// Hedges and negations. Any occurrence kills the sentence outright.
const HEDGES: [&str; 34] = [
    "never",
    "zero ",
    "no longer",
    "nothing ",
    "haven't",
    "have not",
    "hasn't",
    "has not",
    "didn't",
    "did not",
    "doesn't",
    "does not",
    "wasn't",
    "was not",
    "weren't",
    "were not",
    "isn't",
    "is not",
    "aren't",
    "are not",
    "won't",
    "will not",
    "can't",
    "cannot",
    "couldn't",
    "not yet",
    "we should",
    "you should",
    "should be",
    "would need",
    "might ",
    "maybe ",
    "probably ",
    "appears to",
];

const COMMAND_FAMILIES: [&str; 33] = [
    "cargo",
    "npm",
    "pnpm",
    "yarn",
    "pytest",
    "python",
    "node",
    "deno",
    "make",
    "git",
    "docker",
    "swift",
    "xcodebuild",
    "xcrun",
    "xcodegen",
    "tsc",
    "vitest",
    "jest",
    "eslint",
    "clippy",
    "rustfmt",
    "wrangler",
    "gradle",
    "dotnet",
    "cmake",
    "terraform",
    "kubectl",
    "sqlite3",
    "curl",
    "grep",
    "rg",
    "bash",
    "sh",
];

const ARTIFACT_NOUNS: [&str; 60] = [
    // Component nouns: a proposal naming one of these has a target a later
    // action can be matched against, which is all `forward_kind` needs.
    "class",
    "layout",
    "controller",
    "view",
    "panel",
    "component",
    "handler",
    "service",
    "client",
    "server",
    "route",
    "router",
    "config",
    "settings",
    "screen",
    "shim",
    "check",
    "checks",
    "job",
    "pipeline",
    "test",
    "tests",
    "suite",
    "suites",
    "spec",
    "specs",
    "build",
    "builds",
    "compile",
    "compilation",
    "lint",
    "deploy",
    "deployment",
    "gate",
    "gates",
    "harness",
    "snapshot",
    "snapshots",
    "benchmark",
    "bench",
    "ci",
    "commit",
    "branch",
    "tag",
    "file",
    "files",
    "module",
    "package",
    "binary",
    "bundle",
    "fixture",
    "fixtures",
    "migration",
    "schema",
    "endpoint",
    "endpoints",
    "script",
    "worktree",
    "worktrees",
    "release",
];

/// Verbs that name a machine-observable action, so they are their own artifact
/// referent. "Deployed." needs no further tie; "Authed." gets none.
const SELF_TYING: [&str; 16] = [
    "deployed",
    "merged",
    "shipped",
    "committed",
    "compiled",
    "built",
    "pushed",
    "tagged",
    "installed",
    "released",
    "published",
    "reverted",
    "rebased",
    "migrated",
    "pruned",
    "linted",
];

/// Past participles that assert something finished.
///
/// Failure words belong here as much as success words. "The mmap assertion
/// failed" is a completed-work claim about a machine-checkable outcome, and it
/// is precisely the misstated-result class the product exists to catch — a
/// claim of failure can be as wrong as a claim of success.
const DONE_WORDS: [&str; 30] = [
    "failed",
    "failing",
    "errored",
    "broke",
    "done",
    "complete",
    "completed",
    "closed",
    "landed",
    "fixed",
    "stripped",
    "pruned",
    "merged",
    "deployed",
    "shipped",
    "green",
    "certified",
    "verified",
    "succeeded",
    "passing",
    "passed",
    "pass",
    "clean",
    "resolved",
    "removed",
    "added",
    "updated",
    "created",
    "wired",
    "implemented",
];

/// Machine-shaped outcome phrases.
const OUTCOME_WORDS: [&str; 14] = [
    "exit 0",
    "0 errors",
    "0 warnings",
    "no errors",
    "build succeeded",
    "build failed",
    "compiles",
    "test result",
    "all green",
    "pass",
    "passed",
    "passing",
    "failed",
    "failing",
];

/// Past-tense verbs that assert an action was performed or an outcome
/// observed. Explicit, because "-ed" alone also matches adjectives.
const ACTION_PAST: [&str; 34] = [
    "reported",
    "surfaced",
    "returned",
    "produced",
    "showed",
    "printed",
    "emitted",
    "confirmed",
    "verified",
    "added",
    "removed",
    "deleted",
    "updated",
    "created",
    "fixed",
    "landed",
    "passed",
    "failed",
    "completed",
    "finished",
    "built",
    "compiled",
    "deployed",
    "merged",
    "shipped",
    "committed",
    "pushed",
    "installed",
    "replaced",
    "renamed",
    "wrote",
    "ran",
    "caught",
    "found",
];

const IRREGULAR_PAST: [&str; 30] = [
    "set", "wrote", "made", "built", "ran", "put", "sent", "took", "got", "found", "left", "kept",
    "held", "told", "gave", "came", "went", "saw", "did", "said", "cut", "split", "spent",
    "brought", "caught", "taught", "bought", "chose", "drew", "won",
];

/// Sentence-initial gerunds that announce work. Explicit rather than an
/// "-ing" suffix test, which would swallow "nothing", "missing", "remaining".
const GERUNDS: [&str; 47] = [
    "delegating",
    "inserting",
    "running",
    "building",
    "adding",
    "removing",
    "updating",
    "creating",
    "fixing",
    "deploying",
    "committing",
    "merging",
    "reverting",
    "replacing",
    "extending",
    "wiring",
    "writing",
    "reading",
    "checking",
    "verifying",
    "testing",
    "migrating",
    "refactoring",
    "renaming",
    "deleting",
    "splitting",
    "journaling",
    "authoring",
    "provisioning",
    "executing",
    "launching",
    "correcting",
    "pruning",
    "pushing",
    "recording",
    "scaffolding",
    "installing",
    "hooking",
    "threading",
    "polishing",
    "proving",
    "moving",
    "rewriting",
    "adjusting",
    "investigating",
    "scanning",
    "copying",
];

/// Sentence-initial imperatives. Deliberately excludes noun-ambiguous verbs
/// (`build`, `test`, `run`, `check`, `fix`, `deploy`, `commit`, `release`),
/// because "Build succeeded." and "Release ✓ — 0 errors" open with those and
/// are completed work, not proposals.
const IMPERATIVES: [&str; 26] = [
    "hook", "wire", "update", "add", "remove", "extend", "create", "replace", "drop", "split",
    "revert", "append", "polish", "thread", "hoist", "migrate", "rename", "refactor", "scaffold",
    "install", "plumb", "delete", "rewrite", "adjust", "tighten", "harden",
];

const INTENT_PREFIXES: [&str; 12] = [
    "i'll ",
    "i will ",
    "let me ",
    "let's ",
    "i'm going to ",
    "i am going to ",
    "going to ",
    "i need to ",
    "i must ",
    "next, ",
    "planning to ",
    "i plan to ",
];

/* ── classification ──────────────────────────────────────────────────── */

/// Lowercased, emphasis stripped, backticks preserved (they mark artifacts).
fn normalise(s: &str) -> String {
    s.replace(['*', '_', '#'], "").to_lowercase()
}

fn first_word(s: &str) -> String {
    s.trim_start_matches(|c: char| !c.is_alphanumeric())
        .split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .next()
        .unwrap_or("")
        .to_string()
}

fn has_backticked(s: &str) -> bool {
    let n = s.matches('`').count();
    n >= 2
}

/// A token a tool could plausibly have touched: a path, a dotted filename, a
/// snake_case or CamelCase identifier.
fn looks_like_identifier(tok: &str) -> bool {
    let t = tok.trim_matches(|c: char| {
        !c.is_alphanumeric() && c != '_' && c != '/' && c != '.' && c != '-'
    });
    if t.len() < 3 || !t.chars().next().is_some_and(|c| c.is_alphabetic()) {
        return false;
    }
    if t.contains('/') || t.contains('_') {
        return true;
    }
    // Hyphenated compounds name real things a later action can be matched
    // against — "cast-assignment screen", "session-identity rule",
    // "ended-panel HTML". Requiring both halves to be words keeps ordinary
    // dashes out.
    if let Some((a, b)) = t.split_once('-') {
        if a.len() >= 3
            && b.len() >= 3
            && a.chars().all(|c| c.is_alphanumeric())
            && b.chars().all(|c| c.is_alphanumeric())
        {
            return true;
        }
    }
    // dotted filename: at least one dot with alphanumerics on both sides
    if let Some(i) = t.find('.') {
        if i > 0 && t[i + 1..].chars().next().is_some_and(|c| c.is_alphabetic()) {
            return true;
        }
    }
    // CamelCase: a lowercase followed later by an uppercase
    let mut seen_lower = false;
    for c in t.chars() {
        if c.is_lowercase() {
            seen_lower = true;
        } else if c.is_uppercase() && seen_lower {
            return true;
        }
    }
    false
}

/// `d+/d+` anywhere in the sentence.
fn ratio(s: &str) -> Option<(u32, u32)> {
    let b: Vec<char> = s.chars().collect();
    for (i, c) in b.iter().enumerate() {
        if *c != '/' {
            continue;
        }
        let mut a = i;
        while a > 0 && b[a - 1].is_ascii_digit() {
            a -= 1;
        }
        let mut z = i + 1;
        while z < b.len() && b[z].is_ascii_digit() {
            z += 1;
        }
        if a < i && z > i + 1 {
            let l: String = b[a..i].iter().collect();
            let r: String = b[i + 1..z].iter().collect();
            if let (Ok(l), Ok(r)) = (l.parse(), r.parse()) {
                return Some((l, r));
            }
        }
    }
    None
}

/// Lowercased artifact tokens for the M3.4 lexical gate.
pub fn artifacts(sent: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |t: String| {
        if !t.is_empty() && !out.contains(&t) {
            out.push(t);
        }
    };
    // Backticked spans first — the most reliable signal.
    let mut in_tick = false;
    let mut cur = String::new();
    for c in sent.chars() {
        if c == '`' {
            if in_tick && !cur.trim().is_empty() {
                push(cur.trim().to_lowercase());
            }
            cur.clear();
            in_tick = !in_tick;
        } else if in_tick {
            cur.push(c);
        }
    }
    for tok in sent.split_whitespace() {
        let clean = tok.trim_matches(|c: char| {
            !c.is_alphanumeric() && c != '_' && c != '/' && c != '.' && c != '-'
        });
        let low = clean.to_lowercase();
        if COMMAND_FAMILIES.contains(&low.as_str())
            || ARTIFACT_NOUNS.contains(&low.as_str())
            || looks_like_identifier(clean)
        {
            push(low);
        }
    }
    out
}

/// Does the sentence name something a tool could have touched?
fn artifact_tied(sent: &str, low: &str) -> bool {
    if has_backticked(sent) || ratio(sent).is_some() || machine_shaped(low) {
        return true;
    }
    if OUTCOME_WORDS
        .iter()
        .any(|w| w.contains(' ') && low.contains(w))
    {
        return true;
    }
    for tok in low.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '/' || c == '.')) {
        if tok.is_empty() {
            continue;
        }
        if COMMAND_FAMILIES.contains(&tok) || ARTIFACT_NOUNS.contains(&tok) {
            return true;
        }
    }
    sent.split_whitespace().any(looks_like_identifier)
}

fn hedged(low: &str) -> bool {
    HEDGES.iter().any(|h| low.contains(h))
}

fn non_verifiable(low: &str) -> bool {
    NON_VERIFIABLE.iter().any(|v| {
        low.split(|c: char| !c.is_alphanumeric() && c != '\'')
            .any(|w| w == *v)
    })
}

fn opens_with_intent(low: &str) -> Option<Marker> {
    if INTENT_PREFIXES.iter().any(|p| low.starts_with(p)) {
        return Some(Marker::Intent);
    }
    let w = first_word(low);
    if GERUNDS.contains(&w.as_str()) {
        return Some(Marker::Gerund);
    }
    if IMPERATIVES.contains(&w.as_str()) {
        return Some(Marker::Imperative);
    }
    // "Now <anything>" announces the next unit of work. Both "Now the panel
    // class…" and "Now: iOS critical-path audit." occur in the corpus.
    if low.starts_with("now ") || low.starts_with("now:") {
        return Some(Marker::Next);
    }
    None
}

fn running_status(low: &str) -> bool {
    let t = low.trim_start_matches(|c: char| !c.is_alphanumeric());
    (t.starts_with("running (id ")
        || t.starts_with("still running (id ")
        || t.starts_with("running — id ")
        || t.starts_with("running - id "))
        && t.contains(')')
}

fn conditional_recipe_reference(low: &str) -> bool {
    low.contains(" if ")
        && (low.contains("for example")
            || low.contains("e.g.")
            || low.contains("pattern ")
            || low.contains("placeholder"))
}

fn clause_opens_with_intent(low: &str) -> bool {
    low.split(['.', ';', '—', '\n'])
        .map(|clause| clause.trim_start_matches(|c: char| !c.is_alphanumeric()))
        .any(|clause| INTENT_PREFIXES.iter().any(|p| clause.starts_with(p)))
}

/// §5q's proposal-polarity whitelist.
///
/// This is public because derivation re-checks legacy ledger rows before it
/// may emit `dropped`. Tightening extraction protects new rows; this guard
/// makes the same rule structural for rows extracted by older code.
pub fn proposal_is_affirmatively_forward(text: &str, marker: &str) -> bool {
    let low = normalise(text);
    if running_status(&low) || conditional_recipe_reference(&low) {
        return false;
    }
    match marker {
        "intent" => clause_opens_with_intent(&low),
        "gerund" => GERUNDS.contains(&first_word(&low).as_str()),
        "imperative" => IMPERATIVES.contains(&first_word(&low).as_str()),
        "next" => low.starts_with("now ") || low.starts_with("now:"),
        _ => false,
    }
}

fn is_past_word(w: &str) -> bool {
    (w.len() > 3 && w.ends_with("ed")) || IRREGULAR_PAST.contains(&w)
}

/// Verbs of *learning*, not of *doing*. A sentence built on one of these
/// reports what the assistant discovered — the world is unchanged by it.
///
/// This list is the single largest lever on M3.7's false-warning rate: real
/// transcripts are dominated by diagnosis and measurement, and every one of
/// those sentences was previously read as a claim of work.
const EPISTEMIC: [&str; 30] = [
    "confirmed",
    "confirms",
    "verified",
    "verifies",
    "checked",
    "found",
    "finds",
    "discovered",
    "observed",
    "measured",
    "reads",
    "reported",
    "reports",
    "showed",
    "shows",
    "proved",
    "proven",
    "proves",
    "matched",
    "matches",
    "indicates",
    "suggests",
    "means",
    "implies",
    "turns",
    "turned",
    "reveals",
    "revealed",
    "surfaced",
    "explains",
];

/// Phrases that announce a conclusion rather than an action.
const ANSWER_PHRASES: [&str; 16] = [
    "root cause",
    "root-cause",
    "clean:",
    "verified:",
    "status:",
    "the answer is",
    "answer:",
    "diagnosis:",
    "conclusion:",
    "finding:",
    "turns out",
    "turned out",
    "it looks like",
    "the reason is",
    "which explains",
    "the culprit",
];

/// Copular state assertions: "X is/was/are/were Y". Present tense especially —
/// a sentence in the present tense is describing how things *are*, not
/// reporting something that was done.
fn copular_state_low(low: &str) -> bool {
    // Contractions the word-splitter cannot see: "that's my own bug",
    // "it's a shim over React.Component".
    [
        "that's ", "it's ", "there's ", "here's ", "this is ", "that is ",
    ]
    .iter()
    .any(|c| low.contains(c))
}

fn copular_state(words: &[&str]) -> bool {
    for (i, w) in words.iter().enumerate() {
        if !matches!(
            *w,
            "is" | "are" | "was" | "were" | "isn" | "stays" | "remains"
        ) {
            continue;
        }
        // "was edited" / "were updated" is passive voice about an action, which
        // is work, not a state description. Only treat the copula as stative
        // when what follows is not a past participle.
        match words.get(i + 1) {
            // "was edited" — passive voice about an action, not a state.
            Some(next) if is_past_word(next) => continue,
            // "is done", "is green" — a completion marker wearing a copula.
            Some(next) if DONE_WORDS.contains(next) => continue,
            Some(_) => return true,
            None => continue,
        }
    }
    false
}

/// Does this sentence report something learned rather than something done?
fn finding_signal(low: &str, words: &[&str]) -> Option<Marker> {
    if ANSWER_PHRASES.iter().any(|p| low.contains(p)) {
        return Some(Marker::Answer);
    }
    if words.iter().any(|w| EPISTEMIC.contains(w)) {
        return Some(Marker::Epistemic);
    }
    if copular_state(words) || copular_state_low(low) {
        return Some(Marker::Copular);
    }
    None
}

/// Everything up to the first clause break. A finding signal *here* describes
/// what the whole sentence is doing; one buried in a later clause may be
/// subordinate to an action the assistant is reporting.
fn lead_clause(low: &str) -> &str {
    let cut = low
        .find(['—', ';', ':'])
        .or_else(|| low.find(" - "))
        .unwrap_or(low.len());
    &low[..cut]
}

/// "the tests were updated", "the scene was bundled" — an action reported with
/// no actor named.
fn agentless_passive(words: &[&str]) -> bool {
    words.iter().enumerate().any(|(i, w)| {
        matches!(*w, "was" | "were" | "been") && words.get(i + 1).is_some_and(|n| is_past_word(n))
    })
}

/// An explicitly agentive first-person claim: "I've added…", "I set…".
///
/// This is the one signal strong enough to override a finding reading, because
/// the assistant is naming itself as the actor. Everything weaker — a bare
/// past participle, a completion word — is exactly what M3.7 caught misfiring
/// on ordinary prose.
fn first_person_agentive(low: &str, words: &[&str]) -> bool {
    if low.contains("i've ") || low.contains("i have ") {
        return true;
    }
    words
        .iter()
        .enumerate()
        .any(|(i, w)| *w == "i" && words.get(i + 1).is_some_and(|n| is_past_word(n)))
}

/// The whole ruleset, in §5b's order.
///
/// The ordering is the scope principle made operational. Markers are checked
/// first because they are the most specific and least ambiguous form. Findings
/// are checked **before** work, because the sentences that cost M3.7 its
/// precision — "Files confirmed append-only empirically", "§8's rule was wrong"
/// — satisfy the work rules too, and under this product's doctrine the safe
/// direction to be wrong in is silence.
fn classify(sent: &str) -> Option<(ClaimKind, Marker)> {
    let low = normalise(sent);
    if low.trim().is_empty() {
        return None;
    }

    // A68: an interrogative is a request or hypothesis, never an assertion
    // that work completed and never a durable proposal. `Build not saved?`
    // reached `supported` through an unrelated build-number grep in Round 7.
    // The language tier stores questions as classification annotations; the
    // heuristic claim ledger stays silent.
    if sent.trim_end().ends_with('?') {
        return None;
    }

    // Round 6: progress notifications such as "Running (ID `…`)" were read
    // as gerund proposals, and a conditional code recipe was read as an
    // imperative commitment. Neither line points forward with affirmative
    // proposal polarity. Closed-world extraction declines both shapes.
    if running_status(&low) {
        return Some((ClaimKind::Finding, Marker::Copular));
    }
    if conditional_recipe_reference(&low) {
        return None;
    }

    // §5o: an INTENDED failure is a marker outcome — not a hedge, not a
    // finding, and not a confession. "The `guard_probe` fails to compile,
    // exactly as intended." asserts a machine-checkable outcome whose desired
    // value is failure; the contract compares with inverted polarity.
    // Artifact-tied, like every marker rule, so prose about failure in the
    // abstract stays unextracted. Runs BEFORE the hedge/non-verifiable kills:
    // "intended" is itself a mental-state verb, and the kill would eat the
    // very phrase that marks the polarity.
    {
        const FAILURE_INTENT: [&str; 8] = [
            "as intended",
            "as expected",
            "exactly as",
            "correctly fails",
            "fails closed",
            "fail-closed",
            "by design",
            "deliberately",
        ];
        if artifact_tied(sent, &low)
            && low.contains("fail")
            && FAILURE_INTENT.iter().any(|w| low.contains(w))
        {
            return Some((ClaimKind::Marker, Marker::Completion));
        }
    }

    // Negation, hedging and unverifiable verbs kill extraction outright.
    if hedged(&low) || non_verifiable(&low) {
        return None;
    }

    let words: Vec<&str> = low
        .split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .filter(|w| !w.is_empty())
        .collect();
    let tied = artifact_tied(sent, &low);

    // A40 — a sentence ending in a colon INTRODUCES what follows.
    //
    // A section label in a summary ("**Web** (`joinClient.ts`):"), or a lead-in
    // to a code block. It is not a commitment to future work: the thing it
    // introduces is in the same message, so nothing can have been abandoned.
    // Two of M3.9.4's six false ghosts were exactly this, and one of them was
    // sitting in this file's own test list asserted as a correct proposal.
    //
    // Checked here, with the other structural kills, because the intent
    // pre-empt below would otherwise claim these first — "Now the panel class,
    // notch layout, and controller:" opens with an intent phrase AND names
    // artifacts, which is a searchable proposal by every later rule.
    //
    // A ratio still wins: "Suites — 98/98 passing:" is a result that happens
    // to end in a colon, and the crown-jewel catch is never given away.
    if low.trim_end().ends_with(':') && ratio_marker(&low).is_none() {
        return Some((ClaimKind::MicroIntent, Marker::Next));
    }

    // Intent pre-empts: a sentence announcing work is forward-looking even when
    // it names an outcome it merely hopes to reach ("…and confirm green").
    if let Some(m) = opens_with_intent(&low) {
        return Some((forward_kind(sent), m));
    }

    // 1. Ratios first — "33/33 pass", "91 PASS / 0 FAIL". The most specific and
    //    least ambiguous form there is, and the crown-jewel catch, so nothing
    //    is allowed to pre-empt it.
    if let Some(m) = ratio_marker(&low) {
        return Some((ClaimKind::Marker, m));
    }

    // 2. Findings — learned, not done. Before the softer marker forms and
    //    before work, deliberately: the sentences that cost M3.7 its precision
    //    ("Files confirmed append-only empirically", "§8's rule was wrong",
    //    "blockers closed, verified by the critics re-probing") satisfy those
    //    rules too, and under this product's doctrine the safe direction to be
    //    wrong in is silence.
    let finding = finding_signal(&low, &words);
    if let Some(m) = finding {
        // A finding signal in the LEAD clause governs the whole sentence.
        // Only an explicitly agentive first-person past construction later on
        // ("… — I placed the guard before …") can outrank a signal that
        // appeared after the lead, and nothing outranks one in the lead.
        let lead = lead_clause(&low);
        let lead_words: Vec<&str> = lead
            .split(|c: char| !(c.is_alphanumeric() || c == '\''))
            .filter(|w| !w.is_empty())
            .collect();
        let in_lead = finding_signal(lead, &lead_words).is_some();
        if in_lead || !(first_person_agentive(&low, &words) && tied) {
            return Some((ClaimKind::Finding, m));
        }
    }

    // 3. The softer marker forms — outcome phrases and completion verbs.
    if let Some(m) = soft_marker(&low, &words, tied) {
        return Some((ClaimKind::Marker, m));
    }

    // 4. Work — a specific action the assistant performed.
    if let Some(m) = work_claim(&low, &words, tied) {
        return Some((ClaimKind::Work, m));
    }

    // Agentless passive: kept as a finding so A29 has the raw material, and
    // unaccusable because nothing in the grammar says who acted.
    if tied && agentless_passive(&words) {
        return Some((ClaimKind::Finding, Marker::Passive));
    }

    // 6. Trailing intent, checked only after the assertive rules so a sentence
    //    reporting a result AND announcing the next step keeps its result.
    //    A40 moved the colon rule ABOVE this one: "Now the panel class, notch
    //    layout, and controller:" matches an intent prefix and names
    //    artifacts, so it used to become a searchable proposal and then a
    //    ghost — while being nothing but the label on the paragraph beneath.
    if clause_opens_with_intent(&low) {
        return Some((forward_kind(sent), Marker::Intent));
    }

    None
}

/// Forward-looking claims split on one question: **is there anything to look
/// for?**
///
/// A proposal naming a durable target can be searched for, so its silence
/// afterwards is meaningful and it may become a 👻. "Let me read it." names
/// nothing; no search is possible, so its silence means nothing at all. M3.7's
/// adjudication found *every single ghost* on a real project was of the second
/// kind — conversational glue, fulfilled by the very next tool call, reported
/// to the user as abandoned work.
fn forward_kind(sent: &str) -> ClaimKind {
    // A40: narration of the step being taken RIGHT NOW, however specific.
    //
    // "Let me examine its structure and locate the seeded data" names real
    // artifacts, so the artifact test below called it a proposal and its
    // silence afterwards became a 👻. But the tool call that satisfies it is
    // usually the very next event — four of M3.9.4's six false ghosts opened
    // exactly like this. The opener is the signal, not the specificity: a
    // speaker who says "let me" is describing the next thing they do, not
    // making a commitment they could later abandon.
    let opener = sent
        .trim_start_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase();
    const NARRATION: [&str; 6] = [
        "let me ",
        "let's ",
        "let us ",
        "now let me ",
        "first let me ",
        "next let me ",
    ];
    if NARRATION.iter().any(|n| opener.starts_with(n)) {
        return ClaimKind::MicroIntent;
    }

    if !artifacts(sent).is_empty() {
        return ClaimKind::Proposal;
    }
    // An action noun is a target too: "Now rebuild to get the root error"
    // names a rebuild, which a later action can be matched against even though
    // no file is mentioned.
    const ACTION_NOUNS: [&str; 22] = [
        "build",
        "rebuild",
        "deploy",
        "test",
        "commit",
        "merge",
        "install",
        "migrate",
        "refactor",
        "scaffold",
        "audit",
        "benchmark",
        "lint",
        "publish",
        "release",
        "snapshot",
        "harness",
        "gate",
        "compile",
        "revert",
        "rollback",
        "profile",
    ];
    let low = sent.to_lowercase();
    let has_action = low
        .split(|c: char| !c.is_alphanumeric())
        .any(|w| ACTION_NOUNS.contains(&w));
    if has_action {
        return ClaimKind::Proposal;
    }
    // Genuinely contentless: short, no target, no action. "Let me read it."
    // Being generous with `proposal` here is safe — `claim_state` refuses to
    // drop any proposal without artifact tokens, so the ghost guard does not
    // depend on this call being tight.
    if sent.split_whitespace().count() <= 6 {
        ClaimKind::MicroIntent
    } else {
        ClaimKind::Proposal
    }
}

/// "33/33 pass", "104/104 green" — an explicit pass ratio.
fn ratio_marker(low: &str) -> Option<Marker> {
    let (l, r) = ratio(low)?;
    let pass_word =
        OUTCOME_WORDS.iter().any(|w| low.contains(w)) || low.contains("green") || low.contains('✓');
    (pass_word || l == r).then_some(Marker::Ratio)
}

/// The softer machine-checkable outcomes: "Build succeeded", "0 errors",
/// "Deployed.", "P4 merged".
///
/// Completion verbs live here rather than under work because that is what they
/// actually assert: an outcome occurred. "Deployed." does not say what changed,
/// only that a deploy happened — which is exactly a marker.
fn soft_marker(low: &str, words: &[&str], tied: bool) -> Option<Marker> {
    if tied
        && OUTCOME_WORDS
            .iter()
            .any(|w| w.contains(' ') && low.contains(w))
    {
        return Some(Marker::Outcome);
    }
    // A verb that names a machine-observable action is its own referent when it
    // leads the sentence — "Deployed.", "Merged P4." — because a subject-elided
    // predicate can only be about the action. Anywhere else it needs an
    // artifact tie, since "a **compiled** React app" is an adjective (M3.7).
    let leads = words.first().is_some_and(|w| SELF_TYING.contains(w));
    if leads || (tied && words.iter().any(|w| SELF_TYING.contains(w))) {
        return Some(Marker::Completion);
    }
    if tied && done_word_in_predicate(low) {
        return Some(Marker::Completion);
    }
    None
}

/// Is a completion word used as a PREDICATE ("tree clean, gates green") rather
/// than as an ADJECTIVE modifying a noun ("a clean clone", "the seeded data")?
///
/// The third sighting of the same bug. M3.7 caught it on `-ed` participles and
/// on SELF_TYING verbs; M3.8's relabelled yardstick caught the last instance
/// hiding in DONE_WORDS, where "passes on a **clean** clone" was read as a
/// build result. A completion word only asserts an outcome when nothing follows
/// it for it to describe.
fn done_word_in_predicate(low: &str) -> bool {
    const FOLLOWERS: [&str; 14] = [
        "and", "or", "but", "with", "in", "on", "at", "to", "for", "so", "then", "after", "before",
        "while",
    ];
    let mut word = String::new();
    let mut chars = low.chars().peekable();
    let mut pending: Option<String> = None;
    loop {
        let c = chars.next();
        let is_word_char = c.is_some_and(|c| c.is_alphanumeric() || c == '\'');
        if is_word_char {
            word.push(c.unwrap_or_default());
            continue;
        }
        if let Some(prev) = pending.take() {
            // `prev` was a completion word; `word` is what follows it.
            if word.is_empty() || FOLLOWERS.contains(&word.as_str()) {
                return true;
            }
            let _ = prev;
        }
        if DONE_WORDS.contains(&word.as_str()) {
            // Terminated by punctuation other than a space -> predicate.
            match c {
                None => return true,
                Some(ch) if ch != ' ' => return true,
                _ => pending = Some(std::mem::take(&mut word)),
            }
        }
        word.clear();
        if c.is_none() {
            break;
        }
    }
    false
}

/// A specific action the assistant performed. Every branch requires an
/// artifact tie — the sentence must name something a tool could have touched.
fn work_claim(low: &str, words: &[&str], tied: bool) -> Option<Marker> {
    if !tied {
        return None;
    }
    // First-person past/perfect, matched anywhere: real transcripts bury it
    // behind a lead-in ("Test issue: I set `max: 5` in …").
    if first_person_agentive(low, words) {
        return Some(Marker::FirstPersonPast);
    }
    // Subject-elided past, restricted to explicit action verbs. Not any "-ed"
    // word: "Resumed sessions copy the parent's prefix" opens with a participle
    // modifying a noun, and M3.7 caught the looser version accusing that.
    if words.first().is_some_and(|w| ACTION_PAST.contains(w)) {
        return Some(Marker::Passive);
    }
    // Active past with a noun subject, from the same explicit action list.
    if words.iter().skip(1).any(|w| ACTION_PAST.contains(w)) {
        return Some(Marker::Passive);
    }
    // Agentless passive voice is deliberately NOT work.
    //
    // "The tests were updated" and "The scene was bundled (`film_set_storm`),
    // not from proxy" are grammatically identical, and only the second is an
    // observation about a system rather than a report of the assistant's own
    // edit. Nothing in the sentence distinguishes them. §5b's tie-break
    // applies: reading it as work licenses an accusation, reading it as a
    // finding only costs silence — so it falls through to `finding` below.
    None
}

/// Numbers carrying a unit — "43 min, 22.2 MB, 4,850 events", "exit 0".
///
/// Machine-shaped output is itself an artifact referent: a sentence reporting
/// measured quantities is reporting what a tool printed, whether or not it
/// also names the tool.
fn machine_shaped(low: &str) -> bool {
    const UNITS: [&str; 22] = [
        "ms", "s", "sec", "secs", "min", "mins", "hr", "hrs", "mb", "gb", "kb", "events", "event",
        "lines", "line", "rows", "bytes", "chars", "tokens", "warnings", "errors", "failures",
    ];
    let toks: Vec<&str> = low
        .split(|c: char| !(c.is_alphanumeric() || c == '.' || c == ','))
        .filter(|t| !t.is_empty())
        .collect();
    for (i, t) in toks.iter().enumerate() {
        let numeric = t
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == ',')
            && t.chars().any(|c| c.is_ascii_digit());
        if numeric && toks.get(i + 1).is_some_and(|n| UNITS.contains(n)) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<(ClaimKind, Marker, String)> {
        MarkerExtractor
            .extract(text)
            .into_iter()
            .map(|c| (c.kind, c.marker, c.text))
            .collect()
    }

    fn completed(text: &str) -> bool {
        kinds(text).iter().any(|(k, _, _)| k.is_material_work())
    }
    fn proposal(text: &str) -> bool {
        kinds(text)
            .iter()
            .any(|(k, _, _)| *k == ClaimKind::Proposal)
    }

    #[test]
    fn marker_forms_are_the_primary_rule() {
        // The A19 finding: these are what real completed-work claims look like.
        for t in [
            "104/104 green — the flip has no live effect.",
            "**19/19 room logic tests pass.**",
            "All 54 proxy tests pass.",
            "Release ✓ — 0 errors, 0 warnings.",
            "Browser harness: 91 PASS / 0 FAIL.",
            "107/107 still ✓",
        ] {
            assert!(completed(t), "marker form not extracted: {t:?}");
        }
    }

    #[test]
    fn mutation_failure_result_is_a_marker_not_a_finding() {
        let text = "MutationProbe intentionally has 5 tests pass and 2 tests fail.";
        let ks = kinds(text);
        assert!(
            ks.iter()
                .any(|(kind, _, claim)| *kind == ClaimKind::Marker && claim == text),
            "recognized asserts-failure result did not enter the verdict path: {ks:?}"
        );
        assert!(
            !ks.iter().any(|(kind, _, _)| *kind == ClaimKind::Finding),
            "mutation result was misclassified as a non-verdict finding: {ks:?}"
        );
    }

    #[test]
    fn a_bare_marker_without_an_artifact_is_not_a_claim() {
        // "In doubt → don't extract". Nothing here names a test, build, file
        // or command, so there is nothing a verdict could be checked against.
        for t in ["Audit complete.", "Authed.", "Plan confirmed.", "Good."] {
            assert!(!completed(t), "unanchored marker extracted: {t:?}");
        }
    }

    #[test]
    fn a_past_participle_adjective_is_not_a_claim() {
        // Caught by M3.7 hand-adjudication, not by the A19 yardstick and not
        // by the synthetic corpus: an "-ed" word modifying a noun describes
        // the world, it does not assert that work was done. Every one of these
        // produced a false ⚠ before the action-verb list was added.
        for t in [
            "The seeded data lives in `fixtures/demo.js`.",
            "It only feeds `projects.root_path`; `id` stays the munged form.",
            "Nothing session-shaped exists under `~/Library/Caches`.",
            "The nested config has no home for the derived fields in `schema.sql`.",
        ] {
            assert!(
                !completed(t),
                "past-participle adjective read as completed work: {t:?} -> {:?}",
                kinds(t)
            );
        }
        // …while the real active-past forms still extract — but as FINDINGS,
        // not as work. §5b: "reported" and "surfaced" are verbs of learning.
        // The assistant is telling us what a check said, not that it changed
        // anything, so there is nothing here to accuse it of failing to do.
        for t in [
            "The smoke check reported three services warm.",
            "The artifact check surfaced a new defect in `app.rs`.",
        ] {
            let ks = kinds(t);
            assert!(
                ks.iter().any(|(k, _, _)| *k == ClaimKind::Finding),
                "should be kept as a finding: {t:?} -> {ks:?}"
            );
            assert!(
                !completed(t),
                "a finding must never be material work: {t:?}"
            );
        }
    }

    #[test]
    fn findings_are_extracted_but_never_accusable() {
        // The category that cost M3.7 its precision, now named and excluded.
        for t in [
            "Files confirmed append-only empirically (prefix hash stable while the file grew).",
            "§8's session-identity rule was wrong, and only the full corpus showed it.",
            "`sessionId` matched the stem in both projects I checked.",
            "Root cause found — `webSocketClose` in lobby calls `removePlayer`.",
            "The mmap assertion failed — `mmap_size` reads back 134217728, not 0.",
        ] {
            let ks = kinds(t);
            assert!(
                ks.iter().any(|(k, _, _)| *k == ClaimKind::Finding),
                "not recognised as a finding: {t:?} -> {ks:?}"
            );
            assert!(
                !completed(t),
                "a finding reached the accusable kinds: {t:?} -> {ks:?}"
            );
        }
    }

    #[test]
    fn micro_intents_name_nothing_and_are_never_proposals() {
        // Every ghost M3.7 found on a real project was one of these: named no
        // target, so nothing could be searched for, so its silence was read as
        // abandonment.
        for t in ["Let me read it.", "Let me examine that bundle.", "Fixing:"] {
            let ks = kinds(t);
            assert!(
                ks.iter().any(|(k, _, _)| *k == ClaimKind::MicroIntent),
                "not a micro-intent: {t:?} -> {ks:?}"
            );
            assert!(
                !ks.iter().any(|(k, _, _)| *k == ClaimKind::Proposal),
                "a targetless intent must not be a proposal: {t:?} -> {ks:?}"
            );
        }
        // …while an intent with a real target stays a proposal, because a
        // later action can be matched against it.
        assert!(proposal("I'll add rate limiting to `src/api/limiter.rs`."));
    }

    #[test]
    fn negation_and_hedging_kill_extraction() {
        for t in [
            "I haven't added the retry logic yet.",
            "The Write didn't land the bytes I intended.",
            "I was going to update `auth.rs` but didn't.",
            "We should add a regression test.",
            "The build has not succeeded.",
            "That probably fixed `parser.rs`.",
        ] {
            assert!(
                kinds(t).is_empty(),
                "hedged/negated text extracted: {t:?} -> {:?}",
                kinds(t)
            );
        }
    }

    #[test]
    fn unverifiable_assertions_are_never_claims() {
        // §5 rule 3, by name.
        for t in [
            "I've considered the tradeoffs.",
            "I understood the schema well enough.",
            "I decided the `parser.rs` split was right.",
        ] {
            assert!(!completed(t), "unverifiable claim extracted: {t:?}");
        }
    }

    #[test]
    fn intent_preempts_an_outcome_word_it_merely_hopes_for() {
        // Reading this as completed work invents a claim never made.
        let t = "Now rebuild + install on the sim and confirm green.";
        assert!(proposal(t), "{:?}", kinds(t));
        assert!(
            !completed(t),
            "hoped-for outcome read as done: {:?}",
            kinds(t)
        );
    }

    #[test]
    fn headings_and_code_fences_are_never_scanned() {
        // A heading reads exactly like an imperative proposal.
        let t = "### 1. Models — add `.roleplay` + `RoleplayTier`";
        assert!(kinds(t).is_empty(), "heading extracted: {:?}", kinds(t));

        let fenced = "Here is the gate:\n```swift\nAdded the guard to Session.swift\n```\n";
        assert!(
            !completed(fenced),
            "code fence extracted: {:?}",
            kinds(fenced)
        );
    }

    #[test]
    fn first_person_and_passive_still_work_as_the_second_rule() {
        assert!(completed("I've added the retry logic to `webhook.rs`."));
        assert!(completed("I set `max: 5` in the vote-phase check."));
        assert!(completed("The tests were updated to cover the new branch."));
    }

    #[test]
    fn proposals_cover_the_forms_the_corpus_actually_uses() {
        // Real commitments: a later action either fulfils them or does not, so
        // their silence afterwards carries information.
        for t in [
            "I'll assemble the final post when it returns.",
            "Adding `.ts`.",
            "Hook the welcome message in `PartyHostView.swift`.",
        ] {
            assert!(proposal(t), "proposal form missed: {t:?}");
        }
    }

    #[test]
    fn a40_narration_and_lead_ins_are_never_searchable_proposals() {
        // Every one of these was asserted as a `proposal` before M3.10.3, and
        // M3.9.4's adjudication found all six of its sampled false ghosts in
        // exactly these two shapes. The first two rows were IN this test,
        // encoded as correct — the corpus was certifying the defect.
        for t in [
            // narration of the step being taken right now
            "Let me check if `xcodegen` is available.",
            "Let me examine its structure and locate the seeded data.",
            "Now let me explore the App structure and HUD-related files.",
            // a lead-in label: what it introduces is in the same message
            "Now the panel class, notch layout, and controller:",
            "Now the iOS host role card — finding and applying the same wrap:",
            "**Web** (`atlas-rooms/src/joinClient.ts`):",
        ] {
            assert!(
                !proposal(t),
                "A40: narration/lead-in still read as an abandonable proposal: {t:?}"
            );
        }
    }

    #[test]
    fn round6_status_and_recipe_shapes_are_never_proposals() {
        // Pattern-only regressions: invented names, measured grammars.
        for t in [
            "Running (ID `job7m2`).",
            "Still running (ID `task9q4`) on `feat/cache-v2`.",
            "Add `state.rows ??= [];` in `normaliseState` — use a per-item loop \
             if entries carry their own arrays (pattern `state.items ??= [];`).",
            "[GatePanel.tsx](apps/web/GatePanel.tsx) — `+redirect?:string` used \
             by `startHref(redirect, mode)`; the switch toggles in place and \
             keeps the existing links byte-identical.",
        ] {
            assert!(
                !proposal(t),
                "Round-6 status/recipe pattern still read as a proposal: {t:?} -> {:?}",
                kinds(t)
            );
        }

        let legacy = "Running (ID `legacy42`).";
        assert!(
            !proposal_is_affirmatively_forward(legacy, "gerund"),
            "legacy proposal rows need the same closed-world guard"
        );
        assert!(proposal_is_affirmatively_forward(
            "I'll update `src/cache.rs` after the probe.",
            "intent"
        ));
    }

    #[test]
    fn m311_composites_assert_only_the_final_explicit_state() {
        let claims = MarkerExtractor
            .extract("The WidgetSuite failed at first; after the fix, all 8 tests pass.");
        let material: Vec<&Claim> = claims
            .iter()
            .filter(|c| c.kind.is_material_work())
            .collect();
        assert_eq!(material.len(), 1, "{claims:?}");
        assert_eq!(material[0].text, "after the fix, all 8 tests pass.");
        assert!(!material[0].text.contains("failed"));

        let claims =
            MarkerExtractor.extract("The build failed at first; after the patch it was rerun.");
        assert!(
            claims.iter().all(|c| !c.kind.is_material_work()),
            "a composite with no explicit final state must abstain: {claims:?}"
        );
    }

    #[test]
    fn m311_checklists_and_plan_lines_are_never_completed_work() {
        for t in [
            "- [ ] All 24 tests pass",
            "Next: build Release cleanly.",
            "Then: run `cargo test` and confirm green.",
            "Step 3: compile the Release bundle cleanly.",
            "3. Build clean in Release.",
        ] {
            let ks = kinds(t);
            assert!(
                ks.iter().all(|(k, _, _)| !k.is_material_work()),
                "future plan read as completed work: {t:?} -> {ks:?}"
            );
            assert!(
                ks.iter().any(|(k, _, _)| k.is_forward()),
                "future plan was not retained as forward work: {t:?} -> {ks:?}"
            );
        }
        assert!(
            completed("1. Build succeeded."),
            "a numbered report with an explicit past result is not a plan"
        );
    }

    #[test]
    fn spans_point_at_the_claim_in_the_source() {
        let text = "Some preamble here.\n- **104/104 green** on the suite.\n";
        let claims = MarkerExtractor.extract(text);
        let c = claims
            .iter()
            .find(|c| c.kind.is_material_work())
            .expect("a material-work claim");
        assert_eq!(&text[c.span_start..c.span_end], c.text);
        assert!(c.text.contains("104/104"), "{:?}", c.text);
    }

    #[test]
    fn artifacts_are_collected_for_the_lexical_gate() {
        let c =
            &MarkerExtractor.extract("I've updated `src/auth/jwt.rs` and cargo test passes.")[0];
        assert!(
            c.artifacts.iter().any(|a| a == "src/auth/jwt.rs"),
            "{:?}",
            c.artifacts
        );
        assert!(
            c.artifacts.iter().any(|a| a == "cargo"),
            "{:?}",
            c.artifacts
        );
    }

    #[test]
    fn extraction_is_deterministic() {
        let t = "Deployed (Version `4f50f678`). Now verify the endpoints.";
        assert_eq!(MarkerExtractor.extract(t), MarkerExtractor.extract(t));
    }

    #[test]
    fn a68_interrogatives_never_enter_the_claim_or_proposal_ledger() {
        for text in [
            "Build not saved?",
            "Did `cargo test` pass 48/48?",
            "Should I update `src/cache.rs` now?",
        ] {
            assert!(
                MarkerExtractor.extract(text).is_empty(),
                "interrogative extracted from {text:?}"
            );
        }
        assert!(
            !MarkerExtractor
                .extract("The release build succeeded.")
                .is_empty(),
            "the interrogative guard must not eat assertions"
        );
    }
}

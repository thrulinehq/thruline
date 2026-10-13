//! Rendering tool evidence into premises the NLI model can actually reason on.
//!
//! ## Why this file exists at all
//!
//! The obvious premise — paste the tool call and its output — **does not
//! work**, and fails in the worst possible direction. Measured on
//! `nli-deberta-v3-xsmall` (M3.2 probe):
//!
//! | premise for "cargo test → 214 passed, 0 failed" | P(entail) for "All 214 tests pass." |
//! |---|---|
//! | `Ran command \`cargo test\`. Result: test result: ok. 214 passed; 0 failed.` | **0.01** (contradiction 0.97) |
//! | `The command cargo test produced 214 passing tests.` | **0.89** |
//! | `cargo test completed. All 214 tests passed.` | **0.995** |
//!
//! MNLI-trained models are out of distribution on shell output and read it
//! adversarially, so a *truthful* claim scored as a contradiction — a false ⚠,
//! the exact failure this milestone exists to prevent.
//!
//! ## The zero-count trap
//!
//! Across seven logically equivalent phrasings of the same fact, entailment
//! ranged **0.138 – 0.995**. The low ones all enumerate the zero: "and 0
//! failures", "0 failing tests", "0 tests failed". Fail-tokens sitting beside
//! "All … pass" read as contradiction. So when nothing failed we say so
//! positively; when something did fail we state both counts, which is exactly
//! the case where low entailment is the *correct* answer.
//!
//! False claims were unaffected by any of this: max entailment 0.002 across
//! all seven phrasings. The model is reliable at rejecting and brittle at
//! confirming, which is why the renderer's job is to give a true claim its
//! fair chance.
//!
//! Every rule here is deterministic and snapshot-tested.

use crate::derive::EvidenceRow;

/// How much the renderer actually understood about a piece of evidence.
///
/// This tag is the difference between "we read the evidence and it does not
/// support the claim" and "we could not read the evidence at all". M3.5's
/// verdict logic turns on it: a low score against **parsed** evidence is a
/// real `unverified`; a low score against only **fallback** evidence is
/// `unclassified` with the receipt "Evidence present but could not be
/// faithfully interpreted." The product only accuses when it understood.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fidelity {
    /// The renderer recognised the shape — test counts, a build/lint outcome,
    /// or a file-writing tool with a target path — and rendered grounded prose.
    Parsed,
    /// Unrecognised output, carried through the reported-speech framing
    /// ("Its output ended with: …"). Interpretation is not claimed.
    Fallback,
}

impl Fidelity {
    pub fn as_str(self) -> &'static str {
        match self {
            Fidelity::Parsed => "parsed",
            Fidelity::Fallback => "fallback",
        }
    }
    pub fn is_parsed(self) -> bool {
        self == Fidelity::Parsed
    }
}

/// A rendered premise and how much of it the renderer actually understood.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    pub text: String,
    pub fidelity: Fidelity,
}

/// What a command's output says happened, when we can tell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Test-runner counts.
    ///
    /// Both fields are optional because real runners report partial
    /// information: swift-testing's failing summary names the issue count and
    /// not the pass count, and an XCTest suite line names neither. M3.10 made
    /// these `Option` rather than defaulting to `0`, because a fabricated
    /// `passed: 0` is a wrong count, and §5e ranks a wrong count below an
    /// abstention — it is the raw material of a confident false accusation.
    Tests {
        passed: Option<u32>,
        failed: Option<u32>,
    },
    /// A build/compile/lint that clearly succeeded or clearly did not.
    Succeeded,
    Failed,
    /// Nothing recognisable — the tail is carried verbatim instead.
    Unknown,
}

/// Largest count immediately preceding any occurrence of `kw`.
///
/// Every occurrence is scanned, not just the first. Runner output routinely
/// contains the keyword twice — `test result: FAILED. 31 passed; 2 failed` has
/// a bare "failed" before the counted one — and stopping at the first match
/// found no digits, returned `None`, and rendered a FAILING run as
/// "completed successfully". That is the product validating a false claim,
/// which is worse than any miss.
fn num_before(hay: &str, kw: &str) -> Option<u32> {
    let mut best: Option<u32> = None;
    let mut from = 0usize;
    while let Some(rel) = hay[from..].find(kw) {
        let idx = from + rel;
        let digits: String = hay[..idx]
            .chars()
            .rev()
            .skip_while(|c| c.is_whitespace())
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if !digits.is_empty() {
            if let Ok(n) = digits.chars().rev().collect::<String>().parse::<u32>() {
                best = Some(best.map_or(n, |b: u32| b.max(n)));
            }
        }
        from = idx + kw.len();
    }
    best
}

/* ── §5g evidence provenance ──────────────────────────────────────────── */

/// Where a span of premise text came from.
///
/// §5g's claim is that **provenance is a property of where text lives, not of
/// what it says**, so this is derived from the event structure — `tool_input`
/// versus the correlated `tool_result` versus assistant message text — and
/// never from inspecting the characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanKind {
    /// The correlated `tool_result`: what actually ran and what it printed.
    /// The only span an outcome may be read from.
    ExecutedOutput,
    /// `tool_input` — the command, its flags, its heredocs. A `printf 'FINAL:
    /// 31 pass'` here is a *plan to print*, not a result.
    CommandSource,
    /// Assistant prose: plans, mentions, quotations, summaries.
    ProseContext,
}

impl SpanKind {
    /// Whether an outcome may be read from this span at all.
    pub fn is_scannable(self) -> bool {
        matches!(self, SpanKind::ExecutedOutput)
    }
    pub fn as_str(self) -> &'static str {
        match self {
            SpanKind::ExecutedOutput => "executed_output",
            SpanKind::CommandSource => "command_source",
            SpanKind::ProseContext => "prose_context",
        }
    }
    pub fn parse(s: &str) -> Option<SpanKind> {
        match s {
            "executed_output" => Some(SpanKind::ExecutedOutput),
            "command_source" => Some(SpanKind::CommandSource),
            "prose_context" => Some(SpanKind::ProseContext),
            _ => None,
        }
    }
}

/// **The only entry point a verdict path may use.**
///
/// Non-scannable spans return `Unknown` before a single character is examined.
/// That ordering is the point: A46's three holdout-2 failures were a plan, a
/// mention, and shell source, and no amount of string cleverness reliably
/// separates *"should show 32 pass"* from *"32 pass"* — the sentences are
/// lexically near-identical. Provenance separates them for free, and it is the
/// only signal that does.
pub fn classify_span(kind: SpanKind, text: &str) -> Outcome {
    if !kind.is_scannable() {
        return Outcome::Unknown;
    }
    classify_outcome(text)
}

/// The provenance-typed spans of one evidence row, derived from its structure.
///
/// Returned in a fixed order so callers are deterministic. A caller wanting an
/// outcome filters on `is_scannable`; a caller building a receipt or an NLI
/// premise keeps all three, because command and prose are legitimate *context*
/// — they are simply not evidence of a result.
pub fn provenance_spans(row: &EvidenceRow) -> Vec<(SpanKind, &str)> {
    let mut out: Vec<(SpanKind, &str)> = Vec::new();
    if let Some(t) = row.result_tail.as_deref() {
        out.push((SpanKind::ExecutedOutput, t));
    }
    if let Some(t) = row.tool_input.as_deref() {
        out.push((SpanKind::CommandSource, t));
    }
    if let Some(t) = row.new_content.as_deref() {
        // What an edit introduced is authored text, not a run's report.
        out.push((SpanKind::CommandSource, t));
    }
    out
}

/// The outcome of an evidence row, reading only its executed output.
///
/// This is what `annotate` freezes into `evidence_links`. Written as one
/// function so there is exactly one place in the codebase where an outcome is
/// derived from a row, and it is structurally incapable of reading the command.
pub fn row_outcome(row: &EvidenceRow) -> Outcome {
    provenance_spans(row)
        .into_iter()
        .filter(|(k, _)| k.is_scannable())
        .map(|(k, t)| classify_span(k, t))
        .find(|o| !matches!(o, Outcome::Unknown))
        .unwrap_or(Outcome::Unknown)
}

/// Turn a stored payload into something with real lines in it.
///
/// The ledger keeps `tool_result` as the raw JSON Claude Code wrote, so a
/// newline arrives as the two characters `\` `n`, not as U+000A. Every rule
/// below is line-scoped and anchored — `# pass 98` is only meaningful at the
/// start of a line — so without this step the entire payload is one enormous
/// line and none of them can fire.
///
/// This is most of A36's 1.4%: node TAP was never unreadable, it was never
/// *split*. Measured on ATLAS, undoing the escaping moved interpreted
/// matched-evidence coverage from 53.8% to the figure recorded in §5e.
fn normalise_tail(tail: &str) -> String {
    // The happy path: the payload is a whole JSON object and `content` is the
    // human-visible text. Parsing it beats unescaping by hand because it also
    // removes the `{"content":"` prefix — without which the first line of
    // output is never at the start of a line, and every anchored rule below
    // misses it. Measured: three of ATLAS's node-TAP payloads read `# tests 54`
    // as their opening bytes and matched nothing at all.
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(tail) {
        if let Some(c) = v.get("content").and_then(|c| c.as_str()) {
            return c.to_string();
        }
        if let Some(c) = v
            .get("toolUseResult")
            .and_then(|t| t.get("stdout"))
            .and_then(|c| c.as_str())
        {
            return c.to_string();
        }
    }

    // The payload is a 2,000-character PREFIX, so it is usually truncated
    // mid-string and cannot be parsed. Unescape by hand and drop the wrapper.
    let body = tail
        .strip_prefix("{\"content\":\"")
        .or_else(|| tail.strip_prefix("{\"content\": \""))
        .unwrap_or(tail);
    if !body.contains("\\n") && !body.contains("\\t") && std::ptr::eq(body, tail) {
        return tail.to_string();
    }
    let mut out = String::with_capacity(body.len());
    let mut it = body.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('u') => {
                // \uXXXX — keep the character if it decodes, drop it if not.
                let hex: String = it.by_ref().take(4).collect();
                match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    Some(ch) => out.push(ch),
                    None => out.push_str(&format!("\\u{hex}")),
                }
            }
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// A test-count reading taken from one line of output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Counts {
    passed: Option<u32>,
    failed: Option<u32>,
    /// Total tests, when the format states one separately (node TAP).
    total: Option<u32>,
}

impl Counts {
    fn is_empty(self) -> bool {
        self.passed.is_none() && self.failed.is_none() && self.total.is_none()
    }
    /// `# tests 54` + `# fail 0` pins the pass count. Only ever applied when
    /// the subtraction is well-defined.
    fn resolve(mut self) -> Self {
        if self.passed.is_none() {
            if let (Some(t), Some(f)) = (self.total, self.failed) {
                if t >= f {
                    self.passed = Some(t - f);
                }
            }
        }
        self
    }
    /// Two readings conflict when both name the same field differently. §5e:
    /// a payload that says two things is a payload we do not understand.
    fn conflicts_with(self, o: Self) -> bool {
        (self.passed.is_some() && o.passed.is_some() && self.passed != o.passed)
            || (self.failed.is_some() && o.failed.is_some() && self.failed != o.failed)
            || (self.total.is_some() && o.total.is_some() && self.total != o.total)
    }
    fn merge(self, o: Self) -> Self {
        Counts {
            passed: self.passed.or(o.passed),
            failed: self.failed.or(o.failed),
            total: self.total.or(o.total),
        }
    }
}

/// The digits immediately AFTER `kw` — "# pass 98", "exit code 3".
fn num_after(hay: &str, kw: &str) -> Option<u32> {
    let idx = hay.find(kw)? + kw.len();
    let digits: String = hay[idx..]
        .chars()
        .skip_while(|c| c.is_whitespace())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// Is this line file *content* rather than program *output*?
///
/// The Read tool renders numbered lines (`161\tpublic var isPass…`, `120:bad()
/// { printf 'FAIL: …' }`), and grep renders `file:12:match`. Source code that
/// merely mentions passing and failing is the single richest source of false
/// readings in the corpus — four of the ten sealed traps are exactly this. One
/// cheap structural test removes the whole class.
fn is_source_listing(line: &str) -> bool {
    let t = line.trim_start();
    let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return false;
    }
    matches!(t[digits.len()..].chars().next(), Some(':') | Some('\t'))
}

/// `n/m` immediately followed by a pass word — "6/6 pass", "7/7 PASS".
///
/// The adjacency requirement is the whole rule. A37 found `n/m` in the wild as
/// a date (`3/4/2026, 11:02:07 PM`) and a progress counter (`[2/7] wait:`);
/// reading either as a test result would manufacture an accusation out of a
/// timestamp. A ratio is a legitimate *claim* grammar and an illegitimate
/// *evidence* grammar unless a pass word is sitting right next to it.
fn ratio_with_pass_word(l: &str) -> Option<Counts> {
    let b: Vec<char> = l.chars().collect();
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
        if a == i || z == i + 1 {
            continue;
        }
        // Whatever follows, minus decoration, must start with a pass word.
        let rest: String = b[z..]
            .iter()
            .collect::<String>()
            .trim_start_matches(|c: char| c.is_whitespace() || "*`_)]}:".contains(c))
            .chars()
            .take(8)
            .collect();
        if !rest.to_ascii_lowercase().starts_with("pass") {
            continue;
        }
        let lhs: u32 = b[a..i].iter().collect::<String>().parse().ok()?;
        let rhs: u32 = b[i + 1..z].iter().collect::<String>().parse().ok()?;
        if lhs > rhs {
            return None; // 9/6 pass is not a thing; abstain rather than guess.
        }
        return Some(Counts {
            passed: Some(lhs),
            failed: Some(rhs - lhs),
            total: None,
        });
    }
    None
}

/// §5g second line: a future/conditional marker means this sentence is about a
/// run that has not happened.
///
/// Holdout-2 trap: *"Next … should show 32 pass"*. The counts are real
/// characters and the claim they support is entirely imaginary.
fn is_future_modality(l: &str) -> bool {
    const FUTURE: [&str; 8] = [
        "should ",
        "will ",
        "expect",
        "going to ",
        "next:",
        "next ",
        "once ",
        "plan:",
    ];
    FUTURE.iter().any(|k| l.contains(k))
}

/// §5g second line: a mention verb means the line is *reporting that a report
/// existed*, not reporting a result.
///
/// Holdout-2 trap: *"an earlier report mentioned the counts"*. Use versus
/// mention — the distinction the classifier had no concept of.
fn is_mention(l: &str) -> bool {
    const MENTION: [&str; 7] = [
        "mentioned",
        "reported in",
        "as reported",
        "said earlier",
        "previous log",
        "prior tail",
        "according to",
    ];
    MENTION.iter().any(|k| l.contains(k))
}

/// §5g: shell source quoted *inside* a result — the holdout-2 trap-3 shape.
///
/// `cat harness.sh` legitimately produces a `tool_result` whose text contains
/// `printf 'FINAL: %d pass, %d fail\n'`. Provenance cannot rescue this one:
/// the span really is executed output. What gives it away structurally is that
/// the counts sit inside a *format string or a shell assignment*, not in a
/// summary line — a real runner prints values, never `%d`.
fn is_shell_source(line: &str) -> bool {
    let l = line.trim_start();
    // A format specifier means the numbers are placeholders, not values.
    if (l.contains("printf") || l.contains("echo ")) && (l.contains("%d") || l.contains("%s")) {
        return true;
    }
    // `good() { printf 'PASS: %s\n' "$1"; }` — a function that PRINTS a result
    // marker is not a result marker.
    if l.contains("printf ")
        && (l.contains("'PASS:")
            || l.contains("'FAIL:")
            || l.contains("\"PASS:")
            || l.contains("\"FAIL:"))
    {
        return true;
    }
    // Shell arithmetic on a counter, e.g. `fails=$((fails + 1))`.
    if l.contains("$((") {
        return true;
    }
    false
}

/// Read one line for test counts. Returns `None` when the line says nothing
/// about counts — which is the common case and must stay cheap.
fn counts_in_line(line: &str) -> Option<Counts> {
    if is_source_listing(line) || is_shell_source(line) {
        return None;
    }
    let l = line.to_ascii_lowercase();
    // §5g modality guard. Applied per line so one speculative sentence in a
    // long log cannot poison the real summary beside it, and one real summary
    // cannot be rescued by a speculative sentence either.
    if is_future_modality(&l) || is_mention(&l) {
        return None;
    }

    // cargo — "test result: ok. 23 passed; 0 failed; …"
    if l.contains("test result:") {
        return Some(Counts {
            passed: num_before(&l, " passed"),
            failed: num_before(&l, " failed"),
            total: None,
        });
    }

    // node TAP summary — "# pass 98", "# fail 0". One field per line.
    if l.starts_with("# pass") {
        return num_after(&l, "# pass").map(|n| Counts {
            passed: Some(n),
            failed: None,
            total: None,
        });
    }
    if l.starts_with("# fail") {
        return num_after(&l, "# fail").map(|n| Counts {
            passed: None,
            failed: Some(n),
            total: None,
        });
    }
    // "# tests N" is the total. On its own it says nothing about health, but
    // paired with "# fail 0" elsewhere in the payload it pins the pass count by
    // subtraction — which is arithmetic, not a guess. Carried as a total and
    // resolved after the sweep.
    if l.starts_with("# tests") {
        return num_after(&l, "# tests").map(|n| Counts {
            passed: None,
            failed: None,
            total: Some(n),
        });
    }

    // swift-testing — "Test run with 42 tests in 6 suites passed after 3.1s."
    // and the failing form "... failed after 3.1 seconds with 2 issues."
    if l.contains("test run with") {
        let total = num_before(&l, " tests");
        if l.contains(" passed") {
            return Some(Counts {
                passed: total,
                failed: Some(0),
                total: None,
            });
        }
        if l.contains(" failed") {
            let issues = num_before(&l, " issue");
            // The pass count is NOT derivable: "issues" and "failing tests" are
            // different units. Report what was said and nothing more.
            return Some(Counts {
                passed: None,
                failed: issues,
                total: None,
            });
        }
    }

    // Suite- and file-level tallies count a DIFFERENT UNIT from tests, and
    // jest/vitest print both:
    //
    //     Test Suites: 4 passed, 4 total
    //     Tests:       31 passed, 31 total
    //
    // Reading the first line as a test count and the second as a conflicting
    // one made the whole payload abstain. They do not disagree; they are
    // answers to different questions, and only the second is a test count.
    let head = l.trim_start();
    let suite_scoped = head.starts_with("test suites")
        || head.starts_with("test files")
        || head.starts_with("suites")
        || head.starts_with("files")
        || head.starts_with("# suites");
    if suite_scoped {
        return None;
    }

    // "=== final: 30 pass, 0 fail ===" and its delimiter/casing mutations.
    if let (Some(p), Some(f)) = (num_before(&l, " pass"), num_before(&l, " fail")) {
        return Some(Counts {
            passed: Some(p),
            failed: Some(f),
            total: None,
        });
    }

    // mocha/jest/pytest participles, and bare "37 tests passing".
    let passed = num_before(&l, " passed").or_else(|| num_before(&l, " passing"));
    let failed = num_before(&l, " failed")
        .or_else(|| num_before(&l, " failing"))
        .or_else(|| num_before(&l, " failures"));
    let c = Counts {
        passed,
        failed,
        total: None,
    };
    if !c.is_empty() {
        return Some(c);
    }

    ratio_with_pass_word(line)
}

/// Count `PASS:` / `FAIL:` and `✓` / `✗` lines — harnesses that report one
/// line per assertion and never print a summary. Anchored at line start so
/// shell source that *prints* `FAIL:` is not mistaken for a run that failed.
fn tally_prefix_harness(tail: &str) -> Option<Counts> {
    let (mut p, mut f) = (0u32, 0u32);
    for line in tail.lines() {
        // §5g: a shell function that PRINTS "PASS:" is not an assertion that
        // passed. Counting those lines is how `cat harness.sh` became a
        // 2-passing test run in holdout-2.
        if is_source_listing(line) || is_shell_source(line) {
            continue;
        }
        let low = line.to_ascii_lowercase();
        if is_future_modality(&low) || is_mention(&low) {
            continue;
        }
        let t = line.trim_start();
        let head: String = t.chars().take(6).collect::<String>().to_ascii_uppercase();
        // §5p (r5-tu3): per-check ledgers also come WITHOUT the colon —
        // "PASS membership-1". Uppercase-only in the raw line, followed by a
        // space, so prose ("pass the token along") cannot tally.
        let word_pass = t.starts_with("PASS ");
        let word_fail = t.starts_with("FAIL ");
        if head.starts_with("PASS:") || word_pass || t.starts_with('✓') || t.starts_with('✔') {
            p += 1;
        } else if head.starts_with("FAIL:") || word_fail || t.starts_with('✗') || t.starts_with('✘')
        {
            f += 1;
        }
    }
    if p + f == 0 {
        None
    } else {
        Some(Counts {
            passed: Some(p),
            failed: Some(f),
            total: None,
        })
    }
}

/// Classify a command result tail. Deterministic, order-sensitive: failure
/// signals are checked before success signals so a run that printed both is
/// never reported as clean.
///
/// **§5d makes this function the only thing allowed to convict.** M3.9.4
/// measured it interpreting 1.4% of real test output while the renderer
/// reported 100% `parsed` fidelity, so NLI decided by default and convicted a
/// premise that proved its claim (A36). Everything below is therefore written
/// to the rule that a *wrong* count is worse than *no* count: every grammar is
/// line-scoped, every match is anchored, and disagreement abstains.
pub fn classify_outcome(tail: &str) -> Outcome {
    let tail = &normalise_tail(tail);
    // Summary grammars first. More than one same-family count pair means more
    // than one run, even when the pairs agree. A41 is the production incident:
    // three suites were merged into a count no runner printed. One explicit
    // FINAL summary may resolve earlier context; otherwise ambiguity abstains.
    let mut acc: Option<Counts> = None;
    let mut final_acc: Option<Counts> = None;
    let mut final_pending = false;
    let mut duplicate_field = false;
    for line in tail.lines() {
        let low = line.trim().to_ascii_lowercase();
        let final_line = {
            let undecorated = low.trim_start_matches(|c: char| {
                c.is_whitespace() || matches!(c, '=' | '-' | '*' | ':' | '[' | ']')
            });
            undecorated.starts_with("final")
        };
        let Some(c) = counts_in_line(line) else {
            final_pending = final_line;
            continue;
        };
        if c.is_empty() {
            continue;
        }
        if final_line || final_pending {
            final_pending = false;
            if final_acc.is_some() {
                return Outcome::Unknown;
            }
            final_acc = Some(c);
            continue;
        }
        if let Some(prev) = acc {
            duplicate_field |= (prev.passed.is_some() && c.passed.is_some())
                || (prev.failed.is_some() && c.failed.is_some())
                || (prev.total.is_some() && c.total.is_some());
        }
        acc = Some(match acc {
            None => c,
            Some(prev) if prev.conflicts_with(c) => return Outcome::Unknown,
            Some(prev) => prev.merge(c),
        });
    }

    if duplicate_field && final_acc.is_none() {
        return Outcome::Unknown;
    }

    // An explicit final summary governs. Only when no summary spoke do we fall
    // back to counting assertion lines.
    let counts = final_acc
        .or(acc)
        .map(Counts::resolve)
        .or_else(|| tally_prefix_harness(tail));

    if let Some(c) = counts {
        // A bare total ("# tests 54" with the pass/fail lines truncated away)
        // says a suite ran, not how it went. That is not an interpreted
        // outcome and must not be reported as one.
        if c.passed.is_some() || c.failed.is_some() {
            return Outcome::Tests {
                passed: c.passed,
                failed: c.failed,
            };
        }
    }

    let l = tail.to_ascii_lowercase();

    // An XCTest suite line states an outcome and no counts. Say exactly that.
    if l.contains("test suite") {
        if l.contains(" failed") {
            return Outcome::Failed;
        }
        if l.contains(" passed") {
            return Outcome::Succeeded;
        }
    }

    // Exit codes, anchored — "Each scenario asserts the exit code AND …" is a
    // sentence about exit codes, not one.
    for line in tail.lines() {
        if is_source_listing(line) {
            continue;
        }
        let t = line.trim_start().to_ascii_lowercase();
        if t.starts_with("exit code") || t.starts_with("exit status") {
            if let Some(n) = num_after(&t, "code").or_else(|| num_after(&t, "status")) {
                return if n == 0 {
                    Outcome::Succeeded
                } else {
                    Outcome::Failed
                };
            }
        }
    }

    // Non-test outcomes.
    const BAD: [&str; 8] = [
        "error:",
        "fatal:",
        "failed",
        "panicked",
        "aborted",
        "no such file",
        "cannot find",
        "build failed",
    ];
    const GOOD: [&str; 7] = [
        "test result: ok",
        "build succeeded",
        "finished `",
        "compiling",
        "success",
        "clean",
        "✨",
    ];
    // Prose is not a result. A heading or a sentence that merely uses the word
    // "failed" must not be read as a run that failed, so the BAD/GOOD sweep
    // skips source listings too.
    let scan: String = tail
        .lines()
        .filter(|x| !is_source_listing(x) && !is_shell_source(x))
        .filter(|x| {
            let low = x.to_ascii_lowercase();
            !is_future_modality(&low) && !is_mention(&low)
        })
        .collect::<Vec<_>>()
        .join("\n")
        .to_ascii_lowercase();
    if BAD.iter().any(|b| scan.contains(b)) {
        return Outcome::Failed;
    }
    if GOOD.iter().any(|g| scan.contains(g)) {
        return Outcome::Succeeded;
    }
    Outcome::Unknown
}

/// One evidence row → one natural-language premise. Text only; callers that
/// need the fidelity tag use [`render_tagged`].
///
/// Never a raw log paste: see the module docs.
pub fn render(ev: &EvidenceRow) -> String {
    render_tagged(ev).text
}

/// Whether this row belongs to the first-class subagent/task evidence family.
///
/// The `Read` arm is not a generic file read: Claude Code stores completed
/// workflow output under `tasks/<id>.output`, and the notification frequently
/// tells the parent to read that file when the inline result is truncated.
pub fn is_agent_record(ev: &EvidenceRow) -> bool {
    matches!(
        ev.tool_name.as_str(),
        "Agent" | "Task" | "TaskCreate" | "TaskUpdate" | "TaskNotification" | "Workflow"
    ) || (ev.tool_name == "Read"
        && ev.path.as_deref().is_some_and(|p| {
            let low = p.to_ascii_lowercase();
            low.contains("/tasks/") && low.ends_with(".output")
        }))
}

fn is_agent_completion_record(ev: &EvidenceRow) -> bool {
    matches!(ev.tool_name.as_str(), "Agent" | "Task" | "TaskNotification")
        || (ev.tool_name == "Read"
            && ev.path.as_deref().is_some_and(|p| {
                let low = p.to_ascii_lowercase();
                low.contains("/tasks/") && low.ends_with(".output")
            }))
}

fn claim_focus_terms(claim: &str) -> Vec<String> {
    const STOP: [&str; 30] = [
        "about",
        "after",
        "already",
        "also",
        "been",
        "built",
        "claim",
        "complete",
        "completed",
        "done",
        "every",
        "from",
        "green",
        "have",
        "into",
        "most",
        "only",
        "pass",
        "passed",
        "proof",
        "record",
        "result",
        "that",
        "their",
        "this",
        "through",
        "with",
        "work",
        "works",
        "would",
    ];
    let mut out = crate::extract::artifacts(claim);
    for word in claim
        .to_ascii_lowercase()
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        .filter(|w| w.len() >= 4 && !STOP.contains(w))
    {
        if !out.iter().any(|t| t == word) {
            out.push(word.to_string());
        }
    }
    out.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    out.dedup();
    out.truncate(12);
    out
}

/// Pull the best grounded neighborhoods out of a multi-part result.
///
/// Agent records are often 25k-token JSON objects and compound command
/// receipts put the decisive assertion hundreds of characters before their
/// final summary. Passing either raw blob to NLI recreates the M3.2 failure.
/// This selects at most two verbatim windows by claim-token coverage; it never
/// creates a fact or interprets a count.
fn claim_focused_excerpt(detail: &str, claim: &str) -> Option<String> {
    let terms = claim_focus_terms(claim);
    if terms.is_empty() || detail.trim().is_empty() {
        return None;
    }
    let low = detail.to_ascii_lowercase();
    let mut windows: Vec<(usize, usize, usize)> = Vec::new(); // score, start, end
    for term in &terms {
        let mut from = 0usize;
        let mut seen = 0usize;
        while let Some(rel) = low[from..].find(term) {
            let hit = from + rel;
            let mut start = hit.saturating_sub(420);
            while start < detail.len() && !detail.is_char_boundary(start) {
                start += 1;
            }
            let mut end = (hit + term.len() + 420).min(detail.len());
            while end > start && !detail.is_char_boundary(end) {
                end -= 1;
            }
            let window_low = &low[start..end];
            if is_future_modality(window_low) || is_mention(window_low) {
                from = hit + term.len();
                seen += 1;
                if seen == 8 {
                    break;
                }
                continue;
            }
            let score = terms.iter().filter(|t| window_low.contains(*t)).count();
            windows.push((score, start, end));
            from = hit + term.len();
            seen += 1;
            if seen == 8 {
                break;
            }
        }
    }
    windows.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));

    let mut chosen: Vec<(usize, usize)> = Vec::new();
    for (_, start, end) in windows {
        if chosen.iter().any(|(a, b)| start < *b && end > *a) {
            continue;
        }
        chosen.push((start, end));
        if chosen.len() == 2 {
            break;
        }
    }
    if chosen.is_empty() {
        return None;
    }
    chosen.sort();
    Some(
        chosen
            .into_iter()
            .map(|(a, b)| clip(&detail[a..b], 520))
            .collect::<Vec<_>>()
            .join(" … "),
    )
}

/// Claim-aware rendering for agent/task records and compound receipts.
///
/// The ordinary renderer remains byte-stable. Annotation calls this wrapper
/// with a transient, bounded detail span so the decisive part of a captured
/// record is not lost merely because it was neither the first nor last 300
/// characters.
pub fn render_tagged_for_claim(ev: &EvidenceRow, claim: &str, detail: Option<&str>) -> Rendered {
    let base = render_tagged(ev);
    let focus = detail.and_then(|d| claim_focused_excerpt(d, claim));
    if is_agent_completion_record(ev) && focus.is_none() {
        return Rendered {
            text: "An agent/task completion record was captured, but no completed result about \
                   this claim could be extracted."
                .to_string(),
            fidelity: Fidelity::Fallback,
        };
    }
    let Some(focus) = focus else {
        return base;
    };
    if is_agent_completion_record(ev) {
        let status = if ev.is_error {
            "completed with an error"
        } else {
            "completed"
        };
        return Rendered {
            text: format!("An agent/task record {status} and reported: {focus}"),
            fidelity: Fidelity::Parsed,
        };
    }
    Rendered {
        text: format!("{} The captured result also reports: {focus}", base.text),
        fidelity: Fidelity::Parsed,
    }
}

/// One evidence row → one premise, tagged with how much was understood.
///
/// The wording is byte-identical to what M3.2 calibrated; only the tag is new.
/// `premise_shapes` snapshots that, because per CLAUDE.md a change to this
/// function is a re-measurement, never a refactor.
pub fn render_tagged(ev: &EvidenceRow) -> Rendered {
    let tool = ev.tool_name.as_str();

    // File-writing tools. A named target is what makes this parsed: we know
    // which file changed, even if we cannot judge the content.
    if ev.kind == "edit" {
        let path = ev.path.as_deref().unwrap_or("a file");
        let verb = match tool {
            "Write" => "was created",
            _ => "was edited",
        };
        let mut s = format!("The file {path} {verb}.");
        if let Some(c) = &ev.new_content {
            if !c.trim().is_empty() {
                s.push_str(&format!(" The new content includes: {}", clip(c, 200)));
            }
        }
        if ev.is_error {
            s.push_str(" The edit reported an error.");
        }
        let fidelity = if ev.path.is_some() {
            Fidelity::Parsed
        } else {
            Fidelity::Fallback
        };
        return Rendered { text: s, fidelity };
    }

    // Shell commands: the case the probe was built around.
    if tool == "Bash" {
        let cmd = ev.command.as_deref().unwrap_or("a shell command");
        let cmd = clip(cmd, 120);
        let tail = ev.result_tail.as_deref().unwrap_or("");
        let (text, fidelity) = match classify_outcome(tail) {
            // Zero failures stated positively — enumerating the zero is what
            // tanked entailment in the probe.
            Outcome::Tests {
                passed: Some(passed),
                failed: Some(0),
            } => (
                format!("The command {cmd} completed successfully. All {passed} tests passed."),
                Fidelity::Parsed,
            ),
            // A failing run must LEAD with the failure.
            //
            // Re-measured in M3.6, and the reason this wording changed: the
            // old "…was run. 31 tests passed and 2 tests failed." scored the
            // misstated claim "33/33 tests pass." at **0.633** — a confident
            // ✅ on the single most valuable catch the product makes, because
            // 31 + 2 = 33 reads as compatible. Leading with "and it failed"
            // drops that to **0.051** while leaving truthful readings intact
            // ("2 tests are failing." 0.985, "31 tests passed." 0.992).
            //
            // Both counts are kept: dropping the pass count also tanked
            // truthful partial claims (0.992 → 0.000), which would have traded
            // one false ✅ for a new class of false ⚠.
            Outcome::Tests {
                passed: Some(passed),
                failed: Some(failed),
            } => (
                format!(
                    "The command {cmd} was run and it failed. \
                     {passed} tests passed and {failed} tests failed."
                ),
                Fidelity::Parsed,
            ),
            // Partial readings: say only what the run actually reported. A
            // fabricated zero here would read as a clean run (§5e).
            Outcome::Tests {
                passed: None,
                failed: Some(failed),
            } => (
                format!(
                    "The command {cmd} was run and it failed. \
                     {failed} tests failed."
                ),
                Fidelity::Parsed,
            ),
            Outcome::Tests {
                passed: Some(passed),
                failed: None,
            } => (
                format!("The command {cmd} was run. {passed} tests passed."),
                Fidelity::Parsed,
            ),
            Outcome::Tests {
                passed: None,
                failed: None,
            } => (
                format!("The command {cmd} was run; the result could not be read."),
                Fidelity::Fallback,
            ),
            Outcome::Succeeded => (
                format!("The command {cmd} completed successfully."),
                Fidelity::Parsed,
            ),
            Outcome::Failed => (
                format!("The command {cmd} was run and it failed."),
                Fidelity::Parsed,
            ),
            // Unrecognised output, but the harness still told us whether the
            // call errored — and that is ground truth, not inference.
            //
            // M3.8b: this one branch was 60% of all unparsed evidence. A
            // command whose output we cannot classify is still a command we
            // know ran and know did not fail, and saying exactly that is
            // grounded prose. Saying less was not caution, it was throwing
            // away a fact the transcript had already given us.
            Outcome::Unknown if ev.is_error => (
                format!("The command {cmd} was run and it failed."),
                Fidelity::Parsed,
            ),
            Outcome::Unknown if tail.is_empty() => (
                format!("The command {cmd} completed successfully."),
                Fidelity::Parsed,
            ),
            Outcome::Unknown => (
                format!(
                    "The command {cmd} completed successfully. Its output ended with: {}",
                    clip(tail, 300)
                ),
                Fidelity::Parsed,
            ),
        };
        return Rendered { text, fidelity };
    }

    // Structured tools carry their meaning in their INPUT, not their output.
    // `TaskUpdate` returns almost nothing; what it did is entirely in
    // `{taskId, status}`. Together these were 740 rows — 20% of all unparsed
    // evidence — rendered as an uninformative blob because the renderer only
    // ever looked at results.
    if let Some(text) = structured_input(tool, ev) {
        return Rendered {
            text,
            fidelity: Fidelity::Parsed,
        };
    }

    // Everything else: read-only tools, MCP calls, and so on. A named target
    // makes it specific; failing that, the harness still told us whether the
    // call errored, which is a fact and not an inference.
    let target = ev
        .path
        .as_deref()
        .map(|p| format!(" {p}"))
        .unwrap_or_default();
    let verb = if ev.is_error {
        "was run and it failed"
    } else {
        "was run and completed without error"
    };
    let mut s = format!("The tool {tool} {verb}{target}.");
    if let Some(t) = &ev.result_tail {
        if !t.trim().is_empty() {
            s.push_str(&format!(" It returned: {}", clip(t, 200)));
        }
    }
    Rendered {
        text: s,
        fidelity: Fidelity::Parsed,
    }
}

/// Tools whose meaning lives in their input JSON.
///
/// Deliberately a short, explicit list. A generic "dump the input" would put
/// raw JSON back into the premise, which is exactly what M3.2 measured scoring
/// a truthful claim as contradiction 0.97.
fn structured_input(tool: &str, ev: &EvidenceRow) -> Option<String> {
    let raw = ev.tool_input.as_deref()?;
    let field = |k: &str| crate::derive::json_string_field(raw, k);
    match tool {
        "TaskCreate" => {
            let subject = field("subject")?;
            Some(format!("A task was created: {}.", clip(&subject, 160)))
        }
        "TaskUpdate" => {
            let id = field("taskId").unwrap_or_else(|| "a task".to_string());
            match field("status") {
                Some(st) => Some(format!("Task {id} was set to {st}.")),
                None => Some(format!("Task {id} was updated.")),
            }
        }
        "Workflow" => {
            let d = field("description").or_else(|| field("name"))?;
            Some(format!("A workflow was launched: {}.", clip(&d, 160)))
        }
        "TodoWrite" => Some("The task list was updated.".to_string()),
        _ => None,
    }
}

/// Whole-character clip with an ellipsis; never splits a char boundary.
fn clip(s: &str, n: usize) -> String {
    let one = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() <= n {
        one
    } else {
        format!(
            "{}…",
            one.chars().take(n.saturating_sub(1)).collect::<String>()
        )
    }
}

/// Premises for a node's evidence, in row order, deduplicated.
///
/// Scored pairwise and max-pooled by the caller — never concatenated.
pub fn render_all(rows: &[EvidenceRow]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(rows.len());
    for r in rows {
        let p = render(r);
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(kind: &'static str, tool: &str, cmd: Option<&str>, tail: Option<&str>) -> EvidenceRow {
        EvidenceRow {
            kind,
            path: None,
            summary: String::new(),
            tool_name: tool.to_string(),
            command: cmd.map(str::to_owned),
            result_tail: tail.map(str::to_owned),
            new_content: None,
            result_excerpt: None,
            tool_use_id: None,
            is_error: false,
            tool_input: None,
            raw_file: None,
            raw_line: None,
            session_id: None,
            seq: None,
            cwd: None,
            git_branch: None,
            command_identity: None,
            raw_sha256: None,
        }
    }

    #[test]
    fn agent_records_and_compound_receipts_focus_on_claim_bearing_parts() {
        let mut task = row("command", "TaskNotification", None, Some("completed"));
        task.tool_name = "TaskNotification".to_string();
        let noise = "unrelated inventory field ".repeat(400);
        let detail = format!(
            "<task-notification><status>completed</status><result>{noise}\
             {{\"capId\":\"A4\",\"title\":\"Strict structured Care Intelligence schema\",\
             \"verdict\":\"built\",\"caveat\":\"AI enrichment remains gated\"}}\
             </result></task-notification>"
        );
        let rendered = render_tagged_for_claim(
            &task,
            "A4 — Strict structured Care Intelligence schema is built, with a caveat.",
            Some(&detail),
        );
        assert_eq!(rendered.fidelity, Fidelity::Parsed);
        assert!(rendered.text.contains("Strict structured"));
        assert!(rendered.text.contains("\"verdict\":\"built\""));
        assert!(
            rendered.text.len() < 1_300,
            "the raw task record leaked into the premise"
        );

        let bash = row(
            "command",
            "Bash",
            Some("node --test GroundingSuite"),
            Some("# tests 21\n# pass 21\n# fail 0"),
        );
        let detail = "ok 11 - WRAP: split OCR value is grounded to a merged chunk that contains \
                      the full value\nok 12 - bounding stays intact\n# tests 21\n# pass 21\n# fail 0";
        let rendered = render_tagged_for_claim(
            &bash,
            "The grounding-contract assertion says the merged chunk contains the full value.",
            Some(detail),
        );
        assert!(rendered.text.contains("All 21 tests passed"));
        assert!(rendered.text.contains("merged chunk"));
        assert!(rendered.text.contains("full value"));
    }

    #[test]
    fn future_agent_prose_is_not_promoted_to_completed_proof() {
        let task = row("command", "TaskNotification", None, Some("completed"));
        let rendered = render_tagged_for_claim(
            &task,
            "ParserGate handles 32 cases.",
            Some(
                "<status>completed</status><result>Next: ParserGate should handle 32 cases \
                 after the implementation lands.</result>",
            ),
        );
        assert_eq!(rendered.fidelity, Fidelity::Fallback);
        assert!(!rendered.text.contains("should handle 32"));
    }

    #[test]
    fn a_repeated_keyword_does_not_hide_the_failure_count() {
        // Regression, caught by reading the calibration output: cargo prints a
        // bare "FAILED" before the counted "2 failed", and matching only the
        // first occurrence rendered the run as a success.
        let tail = "test result: FAILED. 31 passed; 2 failed; 0 ignored";
        assert_eq!(
            classify_outcome(tail),
            Outcome::Tests {
                passed: Some(31),
                failed: Some(2)
            }
        );
        let p = render(&row("command", "Bash", Some("cargo test"), Some(tail)));
        assert_eq!(
            p,
            "The command cargo test was run and it failed. 31 tests passed and 2 tests failed."
        );
        assert!(
            !p.contains("successfully"),
            "a failing run must never render as success: {p}"
        );
    }

    #[test]
    fn counts_are_parsed_from_common_runners() {
        assert_eq!(
            classify_outcome("test result: ok. 214 passed; 0 failed; 0 ignored"),
            Outcome::Tests {
                passed: Some(214),
                failed: Some(0)
            }
        );
        assert_eq!(
            classify_outcome("Tests: 31 passed, 2 failed, 33 total"),
            Outcome::Tests {
                passed: Some(31),
                failed: Some(2)
            }
        );
        assert_eq!(
            classify_outcome("  12 passing (340ms)\n  1 failing"),
            Outcome::Tests {
                passed: Some(12),
                failed: Some(1)
            }
        );
    }

    #[test]
    fn a_run_that_printed_both_is_never_reported_clean() {
        // The property that matters is not which variant we land on, but that a
        // run mentioning failure is never rendered as success. ("1 test failed"
        // puts a word between the count and the keyword, so no count is parsed —
        // that is fine, as long as the outcome is not `Succeeded`.)
        for tail in [
            "Compiling ok\nerror: 1 test failed",
            "Finished `release` profile\nerror: could not compile",
            "success\nfatal: bad revision",
        ] {
            let o = classify_outcome(tail);
            assert_ne!(
                o,
                Outcome::Succeeded,
                "reported clean despite failure: {tail:?}"
            );
            let p = render(&row("command", "Bash", Some("x"), Some(tail)));
            assert!(
                !p.contains("completed successfully"),
                "premise claims success despite failure: {p}"
            );
        }
        // And the clean case still reads clean.
        assert_eq!(
            classify_outcome("test result: ok. 3 passed; 0 failed"),
            Outcome::Tests {
                passed: Some(3),
                failed: Some(0)
            }
        );
    }

    #[test]
    fn a41_multiple_count_pairs_abstain_unless_one_final_summary_resolves_them() {
        assert_eq!(
            classify_outcome(
                "UnitDeck: 61 pass, 0 fail\nModelDeck: 8 pass, 0 fail\n\
                 RevealDeck: 14 pass, 2 fail"
            ),
            Outcome::Unknown
        );
        assert_eq!(
            classify_outcome(
                "test result: FAILED. 41 passed; 1 failed\n=== FINAL SUMMARY ===\n\
                 test result: ok. 42 passed; 0 failed"
            ),
            Outcome::Tests {
                passed: Some(42),
                failed: Some(0)
            }
        );
        assert_eq!(
            classify_outcome("FINAL: 20 pass, 0 fail\nFINAL SUMMARY: 19 pass, 1 fail"),
            Outcome::Unknown
        );
        assert_eq!(
            classify_outcome("=== run one: 12 pass, 0 fail ===\n=== run two: 12 pass, 0 fail ==="),
            Outcome::Unknown,
            "identical pairs are still two runs"
        );
    }

    #[test]
    fn zero_failures_are_stated_positively_never_enumerated() {
        let p = render(&row(
            "command",
            "Bash",
            Some("cargo test"),
            Some("test result: ok. 214 passed; 0 failed"),
        ));
        assert_eq!(
            p,
            "The command cargo test completed successfully. All 214 tests passed."
        );
        // The measured trap: naming the zero tanks entailment on true claims.
        assert!(!p.contains('0'), "premise must not enumerate the zero: {p}");
    }

    #[test]
    fn real_failures_are_stated_plainly_with_both_counts() {
        let p = render(&row(
            "command",
            "Bash",
            Some("npm test"),
            Some("Tests: 31 passed, 2 failed, 33 total"),
        ));
        assert_eq!(
            p,
            "The command npm test was run and it failed. 31 tests passed and 2 tests failed."
        );
    }

    #[test]
    fn unrecognised_output_leads_with_the_fact_we_do_have() {
        // The exit status comes from the harness, so it is ground truth; the
        // blob follows as detail rather than standing in for meaning. M3.8b
        // measured why this ordering matters: under the old "was run. Its
        // output ended with: …" framing a FALSE claim scored 0.125 against a
        // TRUE paraphrase's 0.018 — the ranking inverted, so a low score meant
        // nothing. Leading with the fact drops the false claim to 0.001.
        let p = render(&row("command", "Bash", Some("ls"), Some("a.txt b.txt")));
        assert!(
            p.starts_with("The command ls completed successfully. Its output ended with:"),
            "{p}"
        );
        let mut errored = row("command", "Bash", Some("ls"), Some("a.txt"));
        errored.is_error = true;
        assert_eq!(
            render(&errored),
            "The command ls was run and it failed.",
            "an errored call must never read as success"
        );
    }

    #[test]
    fn an_edit_with_no_recoverable_path_is_still_fallback() {
        // `fallback` is not dead machinery just because the real corpus stopped
        // triggering it: evidence we genuinely cannot place must still abstain.
        let r = row("edit", "Edit", None, None);
        assert_eq!(render_tagged(&r).fidelity, Fidelity::Fallback);
    }

    #[test]
    fn edits_name_the_file_and_bound_the_excerpt() {
        let mut r = row("edit", "Edit", None, None);
        r.path = Some("src/auth.rs".to_string());
        r.new_content = Some("x".repeat(500));
        let p = render(&r);
        assert!(p.starts_with("The file src/auth.rs was edited."), "{p}");
        assert!(
            p.chars().count() < 300,
            "excerpt must be bounded: {}",
            p.len()
        );
    }

    #[test]
    fn rendering_is_deterministic_and_deduplicated() {
        let a = row(
            "command",
            "Bash",
            Some("cargo test"),
            Some("214 passed; 0 failed"),
        );
        let b = a.clone();
        assert_eq!(render(&a), render(&b));
        assert_eq!(render_all(&[a, b]).len(), 1, "identical rows collapse");
    }

    #[test]
    fn fidelity_marks_exactly_what_the_renderer_understood() {
        use Fidelity::*;
        let cases: [(EvidenceRow, Fidelity); 7] = [
            (
                row(
                    "command",
                    "Bash",
                    Some("cargo test"),
                    Some("214 passed; 0 failed"),
                ),
                Parsed,
            ),
            (
                row(
                    "command",
                    "Bash",
                    Some("cargo build"),
                    Some("Finished `release` profile"),
                ),
                Parsed,
            ),
            // M3.8b: unrecognised OUTPUT is not unrecognised EVIDENCE. The
            // harness told us the call did not error, and "this command ran
            // and did not fail" is a fact, not an inference — so the shape is
            // parsed even though the blob inside it is not understood.
            (
                row("command", "Bash", Some("ls"), Some("a.txt b.txt")),
                Parsed,
            ),
            (row("command", "Bash", Some("true"), None), Parsed),
            (row("command", "Read", None, Some("some blob")), Parsed),
            // An errored call is equally well understood — in the other
            // direction.
            (
                {
                    let mut r = row("command", "Bash", Some("./deploy.sh"), None);
                    r.is_error = true;
                    r
                },
                Parsed,
            ),
            (
                {
                    let mut r = row("edit", "Edit", None, None);
                    r.path = Some("src/auth.rs".to_string());
                    r
                },
                Parsed,
            ),
        ];
        for (r, want) in cases {
            let got = render_tagged(&r);
            assert_eq!(got.fidelity, want, "{} -> {got:?}", r.tool_name);
        }
    }

    #[test]
    fn tagging_did_not_change_a_single_rendered_byte() {
        // CLAUDE.md: a change to this function is a re-measurement, not a
        // refactor. Adding the tag must leave the calibrated wording alone.
        let mut edit = row("edit", "Write", None, None);
        edit.path = Some("docs/plan.md".to_string());
        for r in [
            row(
                "command",
                "Bash",
                Some("cargo test"),
                Some("test result: ok. 55 passed; 0 failed"),
            ),
            row("command", "Bash", Some("./run.sh"), Some("error: broke")),
            row("command", "Bash", Some("ls"), Some("a.txt")),
            edit,
        ] {
            assert_eq!(render(&r), render_tagged(&r).text);
        }
    }

    #[test]
    fn premise_shapes_are_snapshotted() {
        let mut edit = row("edit", "Write", None, None);
        edit.path = Some("docs/plan.md".to_string());
        edit.new_content = Some("# Plan\nStep one".to_string());
        let rows = vec![
            row(
                "command",
                "Bash",
                Some("cargo build --release"),
                Some("Finished `release` profile"),
            ),
            row(
                "command",
                "Bash",
                Some("cargo test"),
                Some("test result: ok. 55 passed; 0 failed"),
            ),
            row(
                "command",
                "Bash",
                Some("./run.sh"),
                Some("error: something broke"),
            ),
            row("command", "Read", None, Some("line one\nline two")),
            edit,
        ];
        insta::assert_debug_snapshot!("premise_shapes", render_all(&rows));
    }
}

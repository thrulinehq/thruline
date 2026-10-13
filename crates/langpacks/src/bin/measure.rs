//! The corpus measurement harness. **Every constant in `src/measured.rs` comes
//! out of this binary, and `MEASUREMENTS.toml` records what it printed.**
//!
//! # Why a harness at all, rather than four numbers someone agreed on
//!
//! `crates/verify` measured its shape classifier over 15,835 real assertions in
//! 833 files and found 13 ambiguous (0.082%) — and recorded, in the same
//! sentence, that **Go was unmeasured because there was no Go corpus on that
//! machine**. That is the honest shape of a measured constant: a number, a
//! corpus, and a named hole. This crate owns four languages `verify` never
//! measured, so it owes four numbers of the same kind, and the way it pays is
//! by running this.
//!
//! Nothing here is a budget. `docs/perf-basis.toml` is this project's precedent
//! and its rule is the one followed in `MEASUREMENTS.toml`: a figure without a
//! recorded basis is re-measure-only, never quotable.
//!
//! # What it measures
//!
//! 1. **Assertions per language**, counted by the parser — `tamper::parse_tests`
//!    plus the pack's `assertion_call_prefixes` — never by grep. The floor is
//!    5,000 per language; the measured numbers are far above it and the point
//!    of printing them is that the reader can see by how much.
//! 2. **Shape-classifier ambiguity**: the fraction of admitted assertions whose
//!    [`Strength`] would be decided by a **message string** —
//!    `shape::message_string_decides_shape`. Budget **< 0.1%**. The residue is
//!    printed in full up to a cap, because a rate with no specimens behind it
//!    is a claim.
//!
//!    The crate's published predicate, `shape::is_ambiguous`, is reported
//!    beside it and is **not** the figure the budget is enforced on. It cannot
//!    be: every pack strips literals as the first line of its own `classify`,
//!    so `is_ambiguous` compares `strip(x)` against `strip(strip(x))` and finds
//!    0 of 72,696 Java assertions, 0 of 67,923 C#, 0 of 58,839 Ruby and 37 of
//!    47,700 Go — the Go 37 being pre-strip truncation damage, not messages.
//!    Both numbers are printed and both are recorded, under separate keys, so
//!    neither can be quoted as the other. `src/shape.rs` carries the full
//!    statement and the reason the published predicate was not repaired here.
//! 3. **`RENAME_SIMILARITY`** — three columns, described at
//!    [`measure_rename_similarity`].
//! 4. **Table coverage**: which `fn_kinds`, `assertion_kinds` and
//!    `assertion_call_prefixes` the corpus actually exercised, and what fraction
//!    of the assertion-kind nodes the walk *visited* the prefixes admitted. A
//!    pack whose table covers 3% of its corpus's assertion sites is a pack that
//!    will report `assertion_removed` on an honest edit, and this is the number
//!    that says so.
//!
//! # The walk in this file is a MIRROR of `tamper::collect_assertions`, and
//! # that is a hazard, so it is gated
//!
//! `tamper::collect_assertions`, `tamper::is_assertion` and the `body_tokens`
//! construction in `tamper::as_test_fn` are all **private**, and a binary is a
//! separate crate, so the measurement cannot call them. It therefore restates
//! them ([`walk_assertions`], [`matched_prefix`], [`make_test_fn`]). A restated
//! rule that drifts from the rule it restates measures a classifier nobody
//! ships.
//!
//! So every file is checked both ways: the mirror's per-test `(name,
//! assertions, body_tokens)` must equal `tamper::parse_tests`'s, exactly, for
//! every file in the corpus. A mismatch is counted, the first few are printed
//! by path, and the harness **exits non-zero**. The mirror is not trusted; it
//! is held.
//!
//! # Usage
//!
//! ```text
//! cargo run -p thruline-langpacks --bin measure -- --corpus <dir> [--json <path>] [--emit-fixtures]
//! ```
//!
//! `--corpus` defaults to the path stamped in `MEASUREMENTS.toml`. The harness
//! refuses to invent a corpus: if the directory is missing, it says so and
//! exits 2 rather than printing zeros, because zeros parse as a passing budget.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use thruline_langpacks::pack::{packs, LangPack, LangSpec, PackLanguage, Strength};
use thruline_langpacks::shape;
use thruline_langpacks::tamper::{self, TestFn};

/// The corpus this crate was measured against, as stamped in `MEASUREMENTS.toml`.
const DEFAULT_CORPUS: &str = "/private/tmp/claude-501/-Users-dev-thruline/00000000-0000-4000-8000-0000000000a1/scratchpad/corpus";

/// How many ambiguous specimens to print and to commit. The residue is small by
/// construction (the budget is 0.1%); the cap exists so a regression that blows
/// the budget prints a diagnosis instead of a wall.
const RESIDUE_CAP: usize = 400;

/// Committed-sample size per language for `tests/corpus_ambiguity.rs`.
///
/// Stride-sampled, never cherry-picked: at a ~0.1% rate a few hundred
/// assertions would usually contain zero ambiguous cases, and a gate that
/// passes because its sample is too small to contain the thing it checks for is
/// the vacuous green this crate exists to catch. 2,000 is the size at which one
/// ambiguous case is 0.05% — inside the budget rather than three times over it.
const SAMPLE_N: usize = 2000;

/// Thresholds the rename tables are printed at.
const T_GRID: &[f64] = &[0.3, 0.4, 0.45, 0.5, 0.55, 0.6, 0.7, 0.8];

fn main() {
    let mut corpus = PathBuf::from(DEFAULT_CORPUS);
    let mut json_out: Option<PathBuf> = None;
    let mut emit_fixtures = false;
    let mut file_limit: Option<usize> = None;
    let mut only: Option<String> = None;
    let mut mine_commits: usize = 800;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--corpus" => {
                i += 1;
                corpus = PathBuf::from(&args[i]);
                if let Err(why) = refuse_captured_input(&corpus) {
                    eprintln!("measure: refusing --corpus {}\n  {why}", corpus.display());
                    // **Exit 3, not 2.** This binary already exits 2 for an
                    // incomplete corpus ("corpus/java is missing"), and a caller
                    // that cannot tell "your corpus is short a language" from
                    // "you pointed me at captured data" will treat the second as
                    // the first and fix it by pointing somewhere else that is
                    // also wrong. The house convention is a distinct code for a
                    // distinct refusal — `thruline` exits 3 for a pending schema
                    // upgrade for the same reason.
                    //
                    // Found by this guard's own control test, which asserted the
                    // ordinary path was "not refused" by checking for exit 2 and
                    // caught the collision instead.
                    std::process::exit(3);
                }
            }
            "--json" => {
                i += 1;
                json_out = Some(PathBuf::from(&args[i]));
            }
            "--emit-fixtures" => emit_fixtures = true,
            "--limit-files" => {
                i += 1;
                file_limit = Some(args[i].parse().expect("--limit-files wants a number"));
            }
            "--only" => {
                i += 1;
                only = Some(args[i].clone());
            }
            "--mine-commits" => {
                i += 1;
                mine_commits = args[i].parse().expect("--mine-commits wants a number");
            }
            other => {
                eprintln!("unknown flag {other}");
                std::process::exit(2);
            }
        }
        i += 1;
    }

    if !corpus.is_dir() {
        eprintln!(
            "corpus not found at {}\n\
             This harness will not print zeros for a corpus it cannot read: a zero \
             assertion count reads as a passing ambiguity budget, and that is the \
             exact failure MEASUREMENTS.toml exists to make impossible.\n\
             Pass --corpus <dir>, or re-clone the repositories MEASUREMENTS.toml names.",
            corpus.display()
        );
        std::process::exit(2);
    }

    let mut reports = Vec::new();
    let mut any_parity_failure = false;
    for pack in packs() {
        let lang = pack.language();
        if let Some(want) = &only {
            if lang.as_str() != want {
                continue;
            }
        }
        let report = measure_language(*pack, &corpus, file_limit, mine_commits);
        any_parity_failure |= report.parity_mismatches > 0;
        print_language(&report);
        if emit_fixtures {
            emit_sample_fixtures(&report);
        }
        reports.push(report);
    }

    let rename = derive_rename_similarity(&reports);
    print_rename_summary(&reports, &rename);

    let json = render_json(&reports, &rename, &corpus);
    if let Some(path) = &json_out {
        std::fs::write(path, &json).expect("write --json");
        eprintln!("json written to {}", path.display());
    }
    println!("\n===== MACHINE-READABLE BLOCK (schema thruline-langpacks/measure/1) =====");
    println!("{json}");
    println!("===== END MACHINE-READABLE BLOCK =====");

    if any_parity_failure {
        eprintln!(
            "\nFAIL: the mirrored walk in this binary disagreed with tamper::parse_tests. \
             Every number above is therefore about a classifier this crate does not ship. \
             Fix the mirror (or the library) before recording anything."
        );
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// Per-language measurement
// ---------------------------------------------------------------------------

struct Residue {
    path: String,
    line: usize,
    /// What the pack decides today, with its literals stripped.
    stripped: Strength,
    /// What it would decide reading the literals — the counterfactual.
    exposed: Strength,
    text: String,
}

struct Sample {
    path: String,
    line: usize,
    text: String,
}

struct RenameTables {
    /// Pairs of distinct tests that BOTH survive into the head tree, so neither
    /// can be a rename of the other. Jaccard scores.
    unrelated: Vec<f64>,
    /// Real renames mined from the corpus repositories' own git history.
    mined: Vec<f64>,
    mined_named: Vec<(String, String, String, f64)>,
    /// 1:1 replacement events whose names share no word — the population the
    /// mining rule deliberately does NOT call renames.
    mined_disjoint_names: Vec<f64>,
    /// Synthetic "renamed, and also edited k lines" mutants, k -> scores.
    synthetic: BTreeMap<usize, Vec<f64>>,
    commits_scanned: usize,
    file_versions_read: usize,
    repos: Vec<String>,
}

struct LangReport {
    language: PackLanguage,
    repos: Vec<RepoStamp>,
    candidate_files: usize,
    measured_files: usize,
    unreadable_files: usize,
    parse_errors: usize,
    parity_mismatches: usize,
    parity_examples: Vec<String>,
    tests: usize,
    assertions: usize,
    /// `shape::message_string_decides_shape` — the figure the budget is on.
    ambiguous: usize,
    /// `shape::is_ambiguous`, the crate's published predicate, kept beside it
    /// so the two can never be quoted as each other. Near-zero by
    /// construction; see `src/shape.rs`.
    ambiguous_published: usize,
    /// Exposed texts that re-form a Go comment (`//`, `/*`) inside the message.
    /// The counterfactual under-reads those, and the count is how far it could
    /// be wrong rather than a claim that it is not.
    expose_artifacts: usize,
    residue: Vec<Residue>,
    residue_truncated: usize,
    sample: Vec<Sample>,
    sample_stride: usize,
    /// How many of the committed sample's records are ambiguous. Pinned in
    /// `MEASUREMENTS.toml` and asserted by `tests/corpus_ambiguity.rs`: a
    /// stride sample is a fixed set, so this is a fact about the sample and not
    /// an estimate with a confidence interval.
    sample_ambiguous: usize,
    /// How the admitted assertions distribute over the four strengths. A pack
    /// that classified its whole corpus `Truthy` would have a perfect ambiguity
    /// residue and a useless `assertion_weakened`, so the rate is meaningless
    /// without this beside it.
    strengths: BTreeMap<String, usize>,
    fn_kinds: BTreeMap<String, usize>,
    assertion_kinds: BTreeMap<String, usize>,
    prefix_counts: BTreeMap<String, usize>,
    unused_prefixes: Vec<String>,
    sites_visited: usize,
    sites_matched: usize,
    unmatched_heads: BTreeMap<String, usize>,
    rename: RenameTables,
}

impl LangReport {
    fn ambiguity_pct(&self) -> f64 {
        if self.assertions == 0 {
            return 0.0;
        }
        100.0 * self.ambiguous as f64 / self.assertions as f64
    }
    fn coverage_pct(&self) -> f64 {
        if self.sites_visited == 0 {
            return 0.0;
        }
        100.0 * self.sites_matched as f64 / self.sites_visited as f64
    }
}

struct RepoStamp {
    name: String,
    commit: String,
    committed: String,
}

fn measure_language(
    pack: &'static dyn LangPack,
    corpus: &Path,
    file_limit: Option<usize>,
    mine_commits: usize,
) -> LangReport {
    let lang = pack.language();
    let root = corpus.join(lang.as_str());
    if !root.is_dir() {
        eprintln!(
            "corpus/{} is missing — refusing to report a zero for it",
            lang.as_str()
        );
        std::process::exit(2);
    }

    let repos = stamp_repos(&root);
    let mut files = Vec::new();
    collect_files(&root, pack, &mut files);
    files.sort();
    let candidate_files = files.len();
    if let Some(n) = file_limit {
        files.truncate(n);
    }

    let spec = pack.spec();
    let mut report = LangReport {
        language: lang,
        repos,
        candidate_files,
        measured_files: 0,
        unreadable_files: 0,
        parse_errors: 0,
        parity_mismatches: 0,
        parity_examples: Vec::new(),
        tests: 0,
        assertions: 0,
        ambiguous: 0,
        ambiguous_published: 0,
        expose_artifacts: 0,
        residue: Vec::new(),
        residue_truncated: 0,
        sample: Vec::new(),
        sample_stride: 1,
        sample_ambiguous: 0,
        strengths: BTreeMap::new(),
        fn_kinds: BTreeMap::new(),
        assertion_kinds: BTreeMap::new(),
        prefix_counts: BTreeMap::new(),
        unused_prefixes: Vec::new(),
        sites_visited: 0,
        sites_matched: 0,
        unmatched_heads: BTreeMap::new(),
        rename: RenameTables {
            unrelated: Vec::new(),
            mined: Vec::new(),
            mined_named: Vec::new(),
            mined_disjoint_names: Vec::new(),
            synthetic: BTreeMap::new(),
            commits_scanned: 0,
            file_versions_read: 0,
            repos: Vec::new(),
        },
    };

    // Every admitted assertion, in corpus order, so the committed sample can be
    // a stride over a deterministic sequence rather than a choice.
    let mut all: Vec<Sample> = Vec::new();

    for path in &files {
        let Ok(source) = std::fs::read_to_string(path) else {
            report.unreadable_files += 1;
            continue;
        };
        let rel = relative(path, corpus);
        let Some(found) = walk_file(&source, pack, &spec, &mut report) else {
            report.parse_errors += 1;
            continue;
        };
        if found.is_empty() {
            continue;
        }
        report.measured_files += 1;
        report.tests += found.len();

        // Parity: the mirror against the library, per file, always.
        match tamper::parse_tests(&source, pack) {
            Ok(library) => {
                let mut mine: Vec<&FoundTest> = found.iter().collect();
                mine.sort_by(|a, b| a.name.cmp(&b.name));
                let agree = mine.len() == library.len()
                    && mine.iter().zip(library.iter()).all(|(m, l)| {
                        m.name == l.name
                            && m.assertions.iter().map(|a| a.strength).collect::<Vec<_>>()
                                == l.assertions
                            && make_test_fn(&m.name, &m.text).body_tokens == l.body_tokens
                    });
                if !agree {
                    report.parity_mismatches += 1;
                    if report.parity_examples.len() < 8 {
                        report.parity_examples.push(rel.clone());
                    }
                }
            }
            Err(_) => report.parse_errors += 1,
        }

        for test in &found {
            *report.fn_kinds.entry(test.kind.to_string()).or_default() += 1;
            for a in &test.assertions {
                report.assertions += 1;
                *report
                    .assertion_kinds
                    .entry(a.kind.to_string())
                    .or_default() += 1;
                *report
                    .prefix_counts
                    .entry(a.prefix.to_string())
                    .or_default() += 1;
                if shape::is_ambiguous(pack, &a.text) {
                    report.ambiguous_published += 1;
                }
                let exposed_text = shape::expose_literals(&a.text);
                if exposed_text.contains("//") || exposed_text.contains("/*") {
                    report.expose_artifacts += 1;
                }
                *report
                    .strengths
                    .entry(strength_name(a.strength).to_string())
                    .or_default() += 1;
                let exposed = pack.classify(&exposed_text);
                if a.strength != exposed {
                    report.ambiguous += 1;
                    if report.residue.len() < RESIDUE_CAP {
                        report.residue.push(Residue {
                            path: rel.clone(),
                            line: a.line,
                            stripped: a.strength,
                            exposed,
                            text: a.text.clone(),
                        });
                    } else {
                        report.residue_truncated += 1;
                    }
                }
                all.push(Sample {
                    path: rel.clone(),
                    line: a.line,
                    text: a.text.clone(),
                });
            }
        }

        collect_rename_pairs(&found, &mut report.rename);
    }

    for p in spec.assertion_call_prefixes {
        if !report.prefix_counts.contains_key(*p) {
            report.unused_prefixes.push((*p).to_string());
        }
    }

    // The committed sample: a stride over the corpus-ordered sequence. No
    // cherry-picking, and reproducible from this line alone.
    let stride = std::cmp::max(1, all.len() / SAMPLE_N);
    report.sample_stride = stride;
    report.sample = all
        .into_iter()
        .step_by(stride)
        .take(SAMPLE_N)
        .collect::<Vec<_>>();
    report.sample_ambiguous = report
        .sample
        .iter()
        .filter(|s| shape::message_string_decides_shape(pack, &s.text))
        .count();

    mine_renames(&root, pack, &mut report.rename, mine_commits);
    report
}

// ---------------------------------------------------------------------------
// The mirrored walk
// ---------------------------------------------------------------------------

struct FoundAssertion {
    kind: &'static str,
    prefix: &'static str,
    text: String,
    line: usize,
    strength: Strength,
}

struct FoundTest {
    name: String,
    kind: &'static str,
    text: String,
    assertions: Vec<FoundAssertion>,
}

/// Mirror of `tamper::parse_tests` + `tamper::collect`, with the per-node facts
/// the measurement needs and the library does not return.
fn walk_file(
    source: &str,
    pack: &dyn LangPack,
    spec: &LangSpec,
    report: &mut LangReport,
) -> Option<Vec<FoundTest>> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&pack.ts_language()).ok()?;
    let tree = parser.parse(source, None)?;
    let mut out = Vec::new();
    let mut cursor = tree.walk();
    walk_tests(&mut cursor, source, pack, spec, &mut out, report);
    Some(out)
}

fn walk_tests(
    cursor: &mut tree_sitter::TreeCursor,
    source: &str,
    pack: &dyn LangPack,
    spec: &LangSpec,
    out: &mut Vec<FoundTest>,
    report: &mut LangReport,
) {
    let node = cursor.node();
    if spec.fn_kinds.contains(&node.kind()) {
        if let Some(name) = pack.test_name(node, source) {
            if let Ok(text) = node.utf8_text(source.as_bytes()) {
                let mut assertions = Vec::new();
                let mut c = node.walk();
                walk_assertions(&mut c, source, pack, spec, &mut assertions, report);
                out.push(FoundTest {
                    name,
                    kind: node.kind(),
                    text: text.to_string(),
                    assertions,
                });
                return;
            }
        }
    }
    if cursor.goto_first_child() {
        loop {
            walk_tests(cursor, source, pack, spec, out, report);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
}

/// Mirror of `tamper::collect_assertions`, including its **early return**: an
/// admitted node is not descended into, because `assertThat(x).isEqualTo(y)` is
/// one assertion and its receiver is a `call_expression` too.
fn walk_assertions(
    cursor: &mut tree_sitter::TreeCursor,
    source: &str,
    pack: &dyn LangPack,
    spec: &LangSpec,
    out: &mut Vec<FoundAssertion>,
    report: &mut LangReport,
) {
    let node = cursor.node();
    if spec.assertion_kinds.contains(&node.kind()) {
        if let Ok(text) = node.utf8_text(source.as_bytes()) {
            // A "plausible assertion site": a node of a kind the pack says can
            // carry an assertion, reached by the walk with no admitted ancestor
            // above it. This is the denominator of the coverage figure, and it
            // is the decision point itself rather than a grep's guess at one.
            report.sites_visited += 1;
            if let Some(prefix) = matched_prefix(text, spec) {
                report.sites_matched += 1;
                out.push(FoundAssertion {
                    kind: node.kind(),
                    prefix,
                    text: text.to_string(),
                    line: node.start_position().row + 1,
                    strength: pack.classify(text),
                });
                return;
            }
            *report.unmatched_heads.entry(head_token(text)).or_default() += 1;
        }
    }
    if cursor.goto_first_child() {
        loop {
            walk_assertions(cursor, source, pack, spec, out, report);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
}

/// Mirror of `tamper::is_assertion`, returning **which** prefix matched so the
/// table's coverage can be reported entry by entry. Ties go to the first entry
/// in the pack's own order, which is the order `is_assertion`'s `any()` scans.
fn matched_prefix(text: &str, spec: &LangSpec) -> Option<&'static str> {
    let head = text.trim_start().to_ascii_lowercase();
    let first_line = head.lines().next().unwrap_or("");
    spec.assertion_call_prefixes.iter().copied().find(|p| {
        let p = p.to_ascii_lowercase();
        if let Some(infix) = p.strip_prefix('.') {
            first_line.contains(&format!(".{infix}"))
        } else {
            head.starts_with(&p)
        }
    })
}

/// Mirror of the `body_tokens` construction inside `tamper::as_test_fn`.
fn make_test_fn(name: &str, text: &str) -> TestFn {
    let body_tokens: Vec<String> = shape::strip_language_literals(text)
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|t| t.len() > 1 && *t != name)
        .map(|t| t.to_ascii_lowercase())
        .collect();
    TestFn {
        name: name.to_string(),
        assertions: Vec::new(),
        mock_targets: Vec::new(),
        has_skip_marker: false,
        swallows: false,
        has_sleep_or_retry: false,
        body_tokens,
    }
}

/// The leading identifier of an unmatched site, for the coverage diagnosis.
fn head_token(text: &str) -> String {
    let first = text.trim_start().lines().next().unwrap_or("");
    let tok: String = first
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.' || *c == ' ')
        .take(48)
        .collect();
    let tok = tok.trim();
    if tok.is_empty() {
        "<punctuation>".to_string()
    } else {
        tok.to_string()
    }
}

// ---------------------------------------------------------------------------
// File discovery
// ---------------------------------------------------------------------------

/// Is this a test file for this language?
///
/// The rule is the **runner's or the build tool's**, not a guess:
///
/// - Go: `go test` compiles exactly `*_test.go`. Nothing else is a test file.
/// - Java: Maven/Gradle put tests under `src/test/…`; Surefire's default
///   includes are `Test*`, `*Test`, `*Tests`, `*TestCase`.
/// - C#: no compiler rule exists, so the convention is used — `*Test.cs`,
///   `*Tests.cs`, `*Spec(s).cs`, or a path segment `test`/`tests`.
/// - Ruby: `rspec` collects `*_spec.rb`; `rake test` collects `*_test.rb`.
///
/// A file selected here that the parser finds no test in is counted as a
/// candidate and not as a measured file. Both numbers are reported: the gap is
/// how wrong the convention was.
fn is_test_file(path: &Path, pack: &dyn LangPack) -> bool {
    let name = match path.file_name().and_then(|n| n.to_str()) {
        Some(n) => n,
        None => return false,
    };
    let lower_path = path.to_string_lossy().to_ascii_lowercase();
    let ext_ok = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| pack.extensions().contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false);
    if !ext_ok {
        return false;
    }
    match pack.language() {
        PackLanguage::Go => name.ends_with("_test.go"),
        PackLanguage::Java => {
            let stem = name.trim_end_matches(".java");
            lower_path.contains("/src/test/")
                || lower_path.contains("/test/")
                || lower_path.contains("/tests/")
                || stem.starts_with("Test")
                || stem.ends_with("Test")
                || stem.ends_with("Tests")
                || stem.ends_with("TestCase")
        }
        PackLanguage::CSharp => {
            let stem = name.trim_end_matches(".cs");
            lower_path.contains("/test/")
                || lower_path.contains("/tests/")
                || stem.ends_with("Test")
                || stem.ends_with("Tests")
                || stem.ends_with("Spec")
                || stem.ends_with("Specs")
        }
        PackLanguage::Ruby => name.ends_with("_spec.rb") || name.ends_with("_test.rb"),
    }
}

fn collect_files(dir: &Path, pack: &dyn LangPack, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            // `.git` is skipped for speed; `testdata` is NOT — Go's stdlib puts
            // real `_test.go` files under it and excluding them would be a
            // corpus choice made by a filename.
            if name == ".git" || name == "node_modules" || name == "target" {
                continue;
            }
            collect_files(&path, pack, out);
        } else if is_test_file(&path, pack) {
            out.push(path);
        }
    }
}

fn relative(path: &Path, corpus: &Path) -> String {
    path.strip_prefix(corpus)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

fn stamp_repos(root: &Path) -> Vec<RepoStamp> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    for dir in dirs {
        let commit = git(&dir, &["rev-parse", "HEAD"]).unwrap_or_else(|| "<not a git repo>".into());
        let committed = git(&dir, &["log", "-1", "--format=%cI"]).unwrap_or_default();
        out.push(RepoStamp {
            name: dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            commit,
            committed,
        });
    }
    out
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

// ---------------------------------------------------------------------------
// RENAME_SIMILARITY
// ---------------------------------------------------------------------------

/// Every pair of distinct tests that both survive in the same file. Neither can
/// be a rename of the other — they coexist — so a pair scoring at or above the
/// threshold is exactly `verify`'s "accidental pairing" column.
fn collect_rename_pairs(found: &[FoundTest], tables: &mut RenameTables) {
    // Bounded: a file with 300 tests would contribute 44,850 pairs and drown
    // every other file in the corpus. The first 30 tests in tree order is a
    // per-file cap, stated rather than tuned.
    let cap = std::cmp::min(found.len(), 30);
    let fns: Vec<TestFn> = found[..cap]
        .iter()
        .map(|t| make_test_fn(&t.name, &t.text))
        .collect();
    for i in 0..fns.len() {
        for j in (i + 1)..fns.len() {
            tables.unrelated.push(tamper::jaccard(&fns[i], &fns[j]));
        }
    }

    // The synthetic column: `verify`'s "rename + k edits". A rename alone is
    // invisible to `jaccard` (the body-token filter drops the function's own
    // name on both sides), so k = 0 must score 1.0 and is measured as the
    // control. An "edit" here is one body line replaced by a line taken from a
    // different test in the same file — the shape of a developer who renamed a
    // test and also changed part of it. Deterministic: line positions are a
    // stride, the donor is the next test in tree order.
    for (idx, test) in found.iter().enumerate().take(cap) {
        let donor = &found[(idx + 1) % found.len()];
        if std::ptr::eq(donor, test) {
            continue;
        }
        let lines: Vec<&str> = test.text.lines().collect();
        let donor_lines: Vec<&str> = donor
            .text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .collect();
        if lines.len() < 6 || donor_lines.is_empty() {
            continue;
        }
        for k in [0usize, 1, 2, 3, 5] {
            if lines.len() < k + 4 {
                continue;
            }
            let mut mutated: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
            let body = 1..(lines.len() - 1);
            let span = body.end - body.start;
            for e in 0..k {
                let at = body.start + (e * span) / std::cmp::max(1, k);
                mutated[at] = donor_lines[e % donor_lines.len()].to_string();
            }
            let text = mutated
                .join("\n")
                .replace(&test.name, "RenamedByTheHarness");
            let mutant = make_test_fn("RenamedByTheHarness", &text);
            let original = make_test_fn(&test.name, &test.text);
            tables
                .synthetic
                .entry(k)
                .or_default()
                .push(tamper::jaccard(&original, &mutant));
        }
    }
}

/// Real renames, mined from the corpus repositories' own history.
///
/// # The acceptance rule is about NAMES, and the measurement is about BODIES
///
/// Mining renames by body similarity and then measuring body similarity would
/// be a circle: the threshold would come out wherever the mining rule was set.
/// So a pair is accepted as a rename on evidence that never looks at a body —
/// **a 1:1 replacement event**: in one commit, in one test file, exactly one
/// test name disappeared and exactly one appeared, and the two names share at
/// least one word after camelCase/snake_case splitting (`ratesASmallParcel` ->
/// `ratesSmallParcels` shares `rates`).
///
/// The 1:1 events whose names share **no** word are kept separately
/// (`mined_disjoint_names`). They are the population the rule refuses to call
/// renames, and printing their distribution beside the accepted one is how a
/// reader can tell whether the name rule was doing work or just agreeing with
/// the body rule.
///
/// # What this population is NOT
///
/// It is renames as they occur in **merged, reviewed history**, which skews
/// toward small edits. It does not contain the case the threshold actually
/// exists for — a rename that also rewrites half the body — because such a
/// commit is indistinguishable from a delete+add by any evidence available
/// here. That case is covered by the synthetic column instead, and the two are
/// reported separately rather than pooled into one number that hides which
/// evidence it came from.
fn mine_renames(root: &Path, pack: &dyn LangPack, tables: &mut RenameTables, max_commits: usize) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let mut repos: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join(".git").exists())
        .collect();
    repos.sort();

    for repo in repos {
        let name = repo
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        // The same test-file convention `is_test_file` uses, expressed in the
        // only vocabulary `git log` has. It is coarser: git cannot express "or
        // anywhere under src/test/", so a Java test named `FooBarSpec.java`
        // under src/test is mined by neither. Named rather than hidden.
        let pathspecs: &[&str] = match pack.language() {
            PackLanguage::Go => &["*_test.go"],
            PackLanguage::Java => &["*Test.java", "*Tests.java", "*TestCase.java"],
            PackLanguage::CSharp => &["*Test.cs", "*Tests.cs", "*Spec.cs", "*Specs.cs"],
            PackLanguage::Ruby => &["*_spec.rb", "*_test.rb"],
        };
        let n = format!("-n{max_commits}");
        let mut args: Vec<&str> = vec![
            "log",
            "--no-merges",
            "--format=@@@%H",
            "--name-only",
            "--diff-filter=M",
            &n,
            "--",
        ];
        args.extend_from_slice(pathspecs);
        let log = match git(&repo, &args) {
            Some(l) => l,
            None => continue,
        };
        tables.repos.push(name);

        // (sha, path) pairs, and the blob requests for both trees.
        let mut jobs: Vec<(String, String)> = Vec::new();
        let mut sha = String::new();
        for line in log.lines() {
            if let Some(rest) = line.strip_prefix("@@@") {
                sha = rest.to_string();
                tables.commits_scanned += 1;
            } else if !line.trim().is_empty() && !sha.is_empty() {
                jobs.push((sha.clone(), line.to_string()));
            }
        }
        jobs.truncate(4000);
        if jobs.is_empty() {
            continue;
        }

        let mut requests = String::new();
        for (sha, path) in &jobs {
            requests.push_str(&format!("{sha}^:{path}\n{sha}:{path}\n"));
        }
        let blobs = cat_file_batch(&repo, &requests);
        tables.file_versions_read += blobs.iter().filter(|b| b.is_some()).count();

        for (idx, (_sha, path)) in jobs.iter().enumerate() {
            let (Some(base), Some(head)) = (
                blobs.get(idx * 2).and_then(|b| b.as_ref()),
                blobs.get(idx * 2 + 1).and_then(|b| b.as_ref()),
            ) else {
                continue;
            };
            let (Ok(base_fns), Ok(head_fns)) = (
                tamper::parse_tests(base, pack),
                tamper::parse_tests(head, pack),
            ) else {
                continue;
            };
            let head_names: BTreeSet<&str> = head_fns.iter().map(|f| f.name.as_str()).collect();
            let base_names: BTreeSet<&str> = base_fns.iter().map(|f| f.name.as_str()).collect();
            let gone: Vec<&TestFn> = base_fns
                .iter()
                .filter(|f| !head_names.contains(f.name.as_str()))
                .collect();
            let came: Vec<&TestFn> = head_fns
                .iter()
                .filter(|f| !base_names.contains(f.name.as_str()))
                .collect();
            if gone.len() != 1 || came.len() != 1 {
                continue;
            }
            let (b, h) = (gone[0], came[0]);
            if b.body_tokens.is_empty() || h.body_tokens.is_empty() {
                continue;
            }
            let score = tamper::jaccard(b, h);
            if name_words(&b.name).is_disjoint(&name_words(&h.name)) {
                tables.mined_disjoint_names.push(score);
            } else {
                tables.mined.push(score);
                if tables.mined_named.len() < 40 {
                    tables
                        .mined_named
                        .push((path.clone(), b.name.clone(), h.name.clone(), score));
                }
            }
        }
    }
}

/// camelCase / PascalCase / snake_case split, lower-cased, one-character and
/// framework-noise words dropped.
fn name_words(name: &str) -> BTreeSet<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    for c in name.chars() {
        if c == '_' || c == ' ' {
            if !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
        } else if c.is_uppercase() && !cur.is_empty() {
            words.push(std::mem::take(&mut cur));
            cur.push(c);
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
        .into_iter()
        .map(|w| w.to_ascii_lowercase())
        .filter(|w| w.len() > 1 && w != "test" && w != "should" && w != "it")
        .collect()
}

/// A temp path that removes itself, whatever happens.
///
/// `cat_file_batch` used to write a request file, run `git`, and remove the file
/// on the line after. That is the happy path only: a panic, a kill, or an early
/// return between the write and the removal leaks the file, and `let _ =
/// remove_file(..)` swallowed a failed removal silently on top of it.
///
/// A `Drop` guard removes it on every exit from the scope, including unwinding.
/// Written rather than pulled in: `tempfile` is in the workspace, but this
/// crate's `Cargo.toml` is watched by the core's linkage census — the axis that
/// records what this crate can reach — and adding a dependency to save eight
/// lines would be paid for in a census diff someone has to read.
struct TempPath(PathBuf);

impl Drop for TempPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// `git cat-file --batch`, fed from a file so a full pipe can never deadlock.
fn cat_file_batch(repo: &Path, requests: &str) -> Vec<Option<String>> {
    let tmp = TempPath(std::env::temp_dir().join(format!(
        "thruline-measure-{}-{}.req",
        std::process::id(),
        requests.len()
    )));
    let tmp = &tmp.0;
    if std::fs::write(tmp, requests).is_err() {
        return Vec::new();
    }
    let out = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "cd {} && git cat-file --batch < {}",
            shell_quote(&repo.to_string_lossy()),
            shell_quote(&tmp.to_string_lossy())
        ))
        .output();
    // Removal is the `TempPath` guard's, so it happens on a panic too.
    let Ok(out) = out else {
        return Vec::new();
    };
    parse_cat_file(&out.stdout)
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// `<oid> <type> <size>\n<size bytes>\n`, or `<request> missing\n`.
fn parse_cat_file(bytes: &[u8]) -> Vec<Option<String>> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let Some(nl) = bytes[i..].iter().position(|b| *b == b'\n') else {
            break;
        };
        let header = String::from_utf8_lossy(&bytes[i..i + nl]).to_string();
        i += nl + 1;
        let fields: Vec<&str> = header.split_whitespace().collect();
        if fields.len() < 3 || fields[1] != "blob" {
            out.push(None);
            continue;
        }
        let Ok(size) = fields[2].parse::<usize>() else {
            out.push(None);
            continue;
        };
        if i + size > bytes.len() {
            out.push(None);
            break;
        }
        let body = String::from_utf8_lossy(&bytes[i..i + size]).to_string();
        out.push(Some(body));
        i += size + 1;
    }
    out
}

struct RenameChoice {
    chosen: f64,
    per_language: BTreeMap<String, f64>,
    /// (threshold, summed cost) over the whole grid, so the choice can be
    /// re-derived under a different weighting without re-running the corpus.
    pooled: Vec<(f64, f64)>,
    basis: String,
}

/// Pick the threshold the measurement supports.
///
/// # The objective, and the doctrine it deliberately departs from
///
/// `verify::tamper::similar_bodies` argues that the **accidental-pairing**
/// column is worth nothing: "an agent that knows this rule simply writes a
/// replacement body >= t similar, so adversarially the miss rate is ~100% at any
/// threshold". Taken literally that column gets weight zero — and then the
/// objective is monotone and the optimum is t = 0, which makes every deletion a
/// rename and deletes `test_deleted` from the product. So the prose overstates
/// its own case, and the objective used here weights the two **equally**:
///
/// ```text
/// cost(t) = P(mined real rename scores < t)          // false accusation
///         + P(rename-plus-3-edits scores < t)        // false accusation
///         + P(two coexisting tests score >= t)       // accidental pairing
/// ```
///
/// summed over the four languages, each weighted equally regardless of how many
/// pairs it contributed. Equal weighting is a **choice**, not a measurement, and
/// it is stated here so a reader who weights them differently can re-derive from
/// the printed table rather than from this number.
///
/// The grid is coarse on purpose. 200 mined renames across four languages
/// cannot resolve a threshold to three decimal places, and a threshold quoted to
/// three decimal places is a number that looks measured.
fn derive_rename_similarity(reports: &[LangReport]) -> RenameChoice {
    let mut per_language = BTreeMap::new();
    let mut pooled: Vec<(f64, f64)> = Vec::new();
    for t in T_GRID {
        let mut total = 0.0;
        for r in reports {
            total += cost_at(r, *t);
        }
        pooled.push((*t, total));
    }
    for r in reports {
        let mut best = (f64::MAX, T_GRID[0]);
        for t in T_GRID {
            let c = cost_at(r, *t);
            if c < best.0 {
                best = (c, *t);
            }
        }
        per_language.insert(r.language.as_str().to_string(), best.1);
    }
    let chosen = pooled
        .iter()
        .copied()
        .fold(
            (T_GRID[0], f64::MAX),
            |acc, x| if x.1 < acc.1 { x } else { acc },
        )
        .0;
    RenameChoice {
        chosen,
        per_language,
        pooled,
        basis: "pooled argmin of (mined-rename false accusation + rename+3-edits false \
                accusation + coexisting-pair accidental pairing), the four languages \
                weighted equally"
            .to_string(),
    }
}

fn cost_at(r: &LangReport, t: f64) -> f64 {
    frac_below(&r.rename.mined, t)
        + frac_below(r.rename.synthetic.get(&3).map(|v| &v[..]).unwrap_or(&[]), t)
        + frac_at_or_above(&r.rename.unrelated, t)
}

fn frac_below(values: &[f64], t: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().filter(|v| **v < t).count() as f64 / values.len() as f64
}

fn frac_at_or_above(values: &[f64], t: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().filter(|v| **v >= t).count() as f64 / values.len() as f64
}

fn percentile(values: &[f64], p: f64) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rank = ((p / 100.0) * (v.len() as f64 - 1.0)).round() as usize;
    v[rank.min(v.len() - 1)]
}

// ---------------------------------------------------------------------------
// Reporting
// ---------------------------------------------------------------------------

fn strength_name(s: Strength) -> &'static str {
    match s {
        Strength::Truthy => "Truthy",
        Strength::Containment => "Containment",
        Strength::Approximate => "Approximate",
        Strength::Equality => "Equality",
    }
}

fn print_language(r: &LangReport) {
    println!(
        "\n================ {} ================",
        r.language.as_str()
    );
    for repo in &r.repos {
        println!(
            "  repo {:<20} {}  {}",
            repo.name, repo.commit, repo.committed
        );
    }
    println!(
        "  files: {} candidate (by the runner's own test-file rule), {} with >=1 parsed test, \
         {} unreadable, {} parse errors",
        r.candidate_files, r.measured_files, r.unreadable_files, r.parse_errors
    );
    println!("  tests parsed: {}", r.tests);
    println!(
        "  assertions admitted: {}  (floor is 5,000 — {})",
        r.assertions,
        if r.assertions >= 5000 {
            "met"
        } else {
            "NOT MET"
        }
    );
    println!(
        "  ambiguous — a message string would decide the shape: {} = {:.4}%  \
         (budget < 0.1000% — {})",
        r.ambiguous,
        r.ambiguity_pct(),
        if r.ambiguity_pct() < 0.1 {
            "met"
        } else {
            "OVER BUDGET"
        }
    );
    println!(
        "  ambiguous by the crate's PUBLISHED predicate (shape::is_ambiguous): {} = {:.4}% \
         — near-vacuous by construction, see src/shape.rs",
        r.ambiguous_published,
        if r.assertions == 0 {
            0.0
        } else {
            100.0 * r.ambiguous_published as f64 / r.assertions as f64
        }
    );
    println!(
        "  counterfactual's own residue: {} exposed messages re-form a comment (`//`, `/*`), \
         where the exposed reading is under-read rather than wrong",
        r.expose_artifacts
    );
    if r.parity_mismatches > 0 {
        println!(
            "  PARITY FAILURE: {} files where this harness's mirrored walk disagreed with \
             tamper::parse_tests. Examples: {:?}",
            r.parity_mismatches, r.parity_examples
        );
    } else {
        println!(
            "  mirror parity: {} files agree with tamper::parse_tests on (name, assertion \
             strengths, body tokens)",
            r.measured_files
        );
    }

    println!(
        "  committed sample: {} records (every {}th assertion), {} of them ambiguous = {:.4}%",
        r.sample.len(),
        r.sample_stride,
        r.sample_ambiguous,
        if r.sample.is_empty() {
            0.0
        } else {
            100.0 * r.sample_ambiguous as f64 / r.sample.len() as f64
        }
    );
    println!("  strengths admitted: {:?}", r.strengths);
    println!("  test node kinds seen:");
    for (kind, n) in &r.fn_kinds {
        println!("      {kind:<28} {n}");
    }
    println!("  assertion node kinds admitted:");
    for (kind, n) in &r.assertion_kinds {
        println!("      {kind:<28} {n}");
    }
    println!(
        "  prefix table coverage: {} of {} visited assertion-kind sites admitted = {:.2}%",
        r.sites_matched,
        r.sites_visited,
        r.coverage_pct()
    );
    let mut prefixes: Vec<(&String, &usize)> = r.prefix_counts.iter().collect();
    prefixes.sort_by(|a, b| b.1.cmp(a.1));
    for (p, n) in prefixes {
        println!("      {p:<24} {n}");
    }
    if r.unused_prefixes.is_empty() {
        println!("      (every prefix in the pack's table matched at least once)");
    } else {
        println!(
            "      NEVER MATCHED in this corpus: {:?} — an entry that matches nothing is \
             either coverage for a framework this corpus does not use, or dead",
            r.unused_prefixes
        );
    }
    let mut unmatched: Vec<(&String, &usize)> = r.unmatched_heads.iter().collect();
    unmatched.sort_by(|a, b| b.1.cmp(a.1));
    println!("  most common UNMATCHED sites (the coverage gap, by leading text):");
    for (head, n) in unmatched.into_iter().take(15) {
        println!("      {n:>7}  {head}");
    }

    let mut by_file: BTreeMap<&str, usize> = BTreeMap::new();
    for item in &r.residue {
        *by_file.entry(item.path.as_str()).or_default() += 1;
    }
    let mut by_file: Vec<(&&str, &usize)> = by_file.iter().collect();
    by_file.sort_by(|a, b| b.1.cmp(a.1));
    println!(
        "  residue concentration — the files that produce it (a rate spread evenly over \
         a corpus and a rate produced by three helper methods are different facts):"
    );
    for (path, n) in by_file.iter().take(8) {
        println!("      {n:>5}  {path}");
    }
    println!(
        "  ambiguity residue ({} shown{}):",
        r.residue.len(),
        if r.residue_truncated > 0 {
            format!(", {} more not shown", r.residue_truncated)
        } else {
            String::new()
        }
    );
    for item in r.residue.iter().take(60) {
        println!(
            "      {}:{}  stripped {} / exposed {}  |  {}",
            item.path,
            item.line,
            strength_name(item.stripped),
            strength_name(item.exposed),
            one_line(&item.text, 140)
        );
    }

    let t = &r.rename;
    println!(
        "  rename evidence: {} coexisting-test pairs, {} mined 1:1 renames (name-overlapping) \
         from {} commits over {:?}, {} mined 1:1 replacements with disjoint names",
        t.unrelated.len(),
        t.mined.len(),
        t.commits_scanned,
        t.repos,
        t.mined_disjoint_names.len()
    );
    println!("      t     mined-rename<t   synth+3<t   coexisting>=t");
    for th in T_GRID {
        println!(
            "      {:<5} {:>12.2}%  {:>10.2}%  {:>14.2}%",
            th,
            100.0 * frac_below(&t.mined, *th),
            100.0 * frac_below(t.synthetic.get(&3).map(|v| &v[..]).unwrap_or(&[]), *th),
            100.0 * frac_at_or_above(&t.unrelated, *th)
        );
    }
    for k in [0usize, 1, 2, 3, 5] {
        if let Some(v) = t.synthetic.get(&k) {
            println!(
                "      synthetic rename+{k} edits: n={}, p05={:.3} p50={:.3}",
                v.len(),
                percentile(v, 5.0),
                percentile(v, 50.0)
            );
        }
    }
    if !t.mined.is_empty() {
        println!(
            "      mined renames: n={} p05={:.3} p50={:.3} min={:.3}",
            t.mined.len(),
            percentile(&t.mined, 5.0),
            percentile(&t.mined, 50.0),
            percentile(&t.mined, 0.0)
        );
        for (path, from, to, score) in t.mined_named.iter().take(12) {
            println!("        {score:.3}  {from} -> {to}   ({path})");
        }
    }
    println!(
        "      coexisting pairs: n={} p50={:.3} p95={:.3} p99={:.3} max={:.3}",
        t.unrelated.len(),
        percentile(&t.unrelated, 50.0),
        percentile(&t.unrelated, 95.0),
        percentile(&t.unrelated, 99.0),
        percentile(&t.unrelated, 100.0)
    );
}

fn print_rename_summary(reports: &[LangReport], choice: &RenameChoice) {
    println!("\n================ RENAME_SIMILARITY ================");
    for r in reports {
        println!(
            "  {:<8} optimum {:?}",
            r.language.as_str(),
            choice.per_language.get(r.language.as_str())
        );
    }
    println!("  pooled cost over all four languages (lower is better):");
    for (t, cost) in &choice.pooled {
        println!("      t={t:<5} cost={cost:.4}");
    }
    println!("  chosen: {}", choice.chosen);
    println!("  basis: {}", choice.basis);
}

/// Refuse a corpus path that could hold captured data.
///
/// # Why this exists, and what it replaces
///
/// This binary prints text it has cut short — `head_token` cuts an assertion's
/// leading identifier to 48 characters and `one_line` cuts residue text to 140.
/// The census entry for those sites claims their input cannot carry a
/// credential, and that claim rested on *what this binary reads*.
///
/// The guard originally proposed for it checked the crate's **dependency
/// graph**: `thruline-langpacks` links no ledger, capture or spool crate, which
/// is true directly and transitively. It is also adjacent to the claim rather
/// than equal to it — this binary takes a `--corpus` path and reads through
/// `std::fs`, so it could be pointed at a spool with no dependency change, no
/// code change and no new crate. Forty-eight characters is short enough to cut
/// a credential below its detectable length, which is exactly what the census
/// exists to catch.
///
/// So the condition is checked where it actually lives: at the input.
///
/// # What it refuses, and why these three
///
/// - `$THRULINE_HOME`, and `~/.thruline` when that is unset — the product home,
///   which `ledger::schema` resolves `exec.db` under.
/// - `$THRULINE_HOOK_SPOOL`, which `capture::harness` lets name a spool file
///   directly, so it need not be under the home at all.
/// - Any path with a `spool` component, which catches a spool directory reached
///   by some other route.
///
/// Compared after `canonicalize` where the path exists, so `..` and symlinks
/// cannot walk around it. A path that does not exist yet is compared
/// lexically — refusing on the literal text is the cautious direction, and a
/// non-existent corpus fails immediately afterwards anyway.
fn refuse_captured_input(corpus: &Path) -> Result<(), String> {
    let real = corpus
        .canonicalize()
        .unwrap_or_else(|_| corpus.to_path_buf());

    let mut forbidden: Vec<(PathBuf, &str)> = Vec::new();
    match std::env::var_os("THRULINE_HOME") {
        Some(h) => forbidden.push((PathBuf::from(h), "$THRULINE_HOME")),
        None => {
            if let Some(h) = std::env::var_os("HOME") {
                forbidden.push((PathBuf::from(h).join(".thruline"), "~/.thruline"));
            }
        }
    }
    if let Some(s) = std::env::var_os("THRULINE_HOOK_SPOOL") {
        forbidden.push((PathBuf::from(s), "$THRULINE_HOOK_SPOOL"));
    }

    for (root, name) in &forbidden {
        let root_real = root.canonicalize().unwrap_or_else(|_| root.clone());
        if real == root_real || real.starts_with(&root_real) {
            return Err(format!(
                "it is inside {name} ({}), which holds captured session data. \
                 This binary prints truncated text and does not mask; its census \
                 entry claims the input is public source. Point it at a corpus \
                 checkout instead.",
                root_real.display()
            ));
        }
    }

    if real.components().any(|c| c.as_os_str() == "spool") {
        return Err(
            "it has a `spool` path component, which is where captured hook \
             payloads land. See the note on this function."
                .to_string(),
        );
    }
    Ok(())
}

fn one_line(s: &str, cap: usize) -> String {
    let flat: String = s
        .chars()
        .map(|c| {
            if c == '\n' || c == '\r' || c == '\t' {
                ' '
            } else {
                c
            }
        })
        .collect();
    let flat = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > cap {
        flat.chars().take(cap).collect::<String>() + " …"
    } else {
        flat
    }
}

// ---------------------------------------------------------------------------
// Committed fixtures
// ---------------------------------------------------------------------------

/// One assertion per line, `path:line<TAB>escaped text`.
///
/// Escaped rather than delimited: an assertion is routinely multi-line (a Java
/// fluent chain, a Go `if` statement), and a record format that cannot hold a
/// newline would have to drop exactly the assertions whose shape is hardest to
/// read. `\n`, `\t` and `\\` are escaped; nothing else is touched, so the text
/// is the corpus's bytes and not a normalisation of them.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

fn emit_sample_fixtures(r: &LangReport) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/corpus");
    std::fs::create_dir_all(&dir).expect("create fixture dir");
    let lang = r.language.as_str();

    let mut head = String::new();
    head.push_str(&format!(
        "# {lang} — a STRIDE SAMPLE of real assertions from the corpus in MEASUREMENTS.toml.\n\
         #\n\
         # This is a SAMPLE. The full measurement is `cargo run -p thruline-langpacks \\\n\
         #   --bin measure`, over {} assertions in {} files; this file holds every {}th of\n\
         # them in corpus order, {} records, so that `cargo test` can enforce the ambiguity\n\
         # budget on a machine with no corpus checked out. A sample cannot prove the\n\
         # corpus figure and this file does not claim to.\n\
         #\n\
         # Format: <path>:<line><TAB><assertion text, with \\\\ \\n \\r \\t escaped>.\n\
         # Lines beginning with '#' are comments.\n",
        r.assertions,
        r.measured_files,
        r.sample_stride,
        r.sample.len()
    ));
    for repo in &r.repos {
        head.push_str(&format!("# corpus {} @ {}\n", repo.name, repo.commit));
    }
    let mut body = String::new();
    for s in &r.sample {
        body.push_str(&format!("{}:{}\t{}\n", s.path, s.line, escape(&s.text)));
    }
    let path = dir.join(format!("{lang}-assertions.txt"));
    std::fs::write(&path, format!("{head}{body}")).expect("write sample");

    let mut rhead = String::new();
    rhead.push_str(&format!(
        "# {lang} — THE AMBIGUITY RESIDUE: every assertion in the corpus whose Strength\n\
         # changes when string literals are stripped. {} of {} admitted assertions\n\
         # ({:.4}%). This is the whole residue, not a sample{}.\n\
         #\n\
         # `tests/corpus_ambiguity.rs` asserts every record here is STILL ambiguous. That\n\
         # is deliberate: a change to a pack's `classify` that silently fixes one of these\n\
         # also silently invalidates the rate in MEASUREMENTS.toml, and the way this crate\n\
         # refuses to let a figure outlive its basis is to fail the test that pins it.\n\
         #\n\
         # Format: <path>:<line><TAB><stripped>-><exposed><TAB><assertion text, escaped>.\n",
        r.ambiguous,
        r.assertions,
        r.ambiguity_pct(),
        if r.residue_truncated > 0 {
            format!(
                " — EXCEPT that {} were over the {RESIDUE_CAP} cap and are missing",
                r.residue_truncated
            )
        } else {
            String::new()
        }
    ));
    let mut rbody = String::new();
    for item in &r.residue {
        rbody.push_str(&format!(
            "{}:{}\t{}->{}\t{}\n",
            item.path,
            item.line,
            strength_name(item.stripped),
            strength_name(item.exposed),
            escape(&item.text)
        ));
    }
    std::fs::write(
        dir.join(format!("{lang}-ambiguous.txt")),
        format!("{rhead}{rbody}"),
    )
    .expect("write residue");
    eprintln!("fixtures written for {lang}");
}

// ---------------------------------------------------------------------------
// JSON
// ---------------------------------------------------------------------------

fn render_json(reports: &[LangReport], choice: &RenameChoice, corpus: &Path) -> String {
    let mut langs = Vec::new();
    for r in reports {
        let repos: Vec<serde_json::Value> = r
            .repos
            .iter()
            .map(|repo| {
                serde_json::json!({
                    "name": repo.name,
                    "commit": repo.commit,
                    "committed": repo.committed,
                })
            })
            .collect();
        let residue: Vec<serde_json::Value> = r
            .residue
            .iter()
            .map(|i| {
                serde_json::json!({
                    "path": i.path,
                    "line": i.line,
                    "stripped": strength_name(i.stripped),
                    "exposed": strength_name(i.exposed),
                    "text": i.text,
                })
            })
            .collect();
        let t = &r.rename;
        let grid: Vec<serde_json::Value> = T_GRID
            .iter()
            .map(|th| {
                serde_json::json!({
                    "t": th,
                    "mined_rename_below": frac_below(&t.mined, *th),
                    "synthetic_plus3_below": frac_below(
                        t.synthetic.get(&3).map(|v| &v[..]).unwrap_or(&[]), *th),
                    "coexisting_at_or_above": frac_at_or_above(&t.unrelated, *th),
                })
            })
            .collect();
        langs.push(serde_json::json!({
            "language": r.language.as_str(),
            "repos": repos,
            "candidate_files": r.candidate_files,
            "measured_files": r.measured_files,
            "unreadable_files": r.unreadable_files,
            "parse_errors": r.parse_errors,
            "parity_mismatches": r.parity_mismatches,
            "tests": r.tests,
            "assertions": r.assertions,
            "ambiguous": r.ambiguous,
            "ambiguity_pct": r.ambiguity_pct(),
            "ambiguous_published_predicate": r.ambiguous_published,
            "expose_artifacts": r.expose_artifacts,
            "sites_visited": r.sites_visited,
            "sites_matched": r.sites_matched,
            "prefix_coverage_pct": r.coverage_pct(),
            "fn_kinds": r.fn_kinds,
            "assertion_kinds": r.assertion_kinds,
            "prefix_counts": r.prefix_counts,
            "unused_prefixes": r.unused_prefixes,
            "sample_records": r.sample.len(),
            "sample_stride": r.sample_stride,
            "sample_ambiguous": r.sample_ambiguous,
            "strengths": r.strengths,
            "residue": residue,
            "rename": {
                "coexisting_pairs": t.unrelated.len(),
                "mined_renames": t.mined.len(),
                "mined_disjoint_names": t.mined_disjoint_names.len(),
                "commits_scanned": t.commits_scanned,
                "file_versions_read": t.file_versions_read,
                "repos": t.repos,
                "grid": grid,
                "coexisting_p95": percentile(&t.unrelated, 95.0),
                "coexisting_p99": percentile(&t.unrelated, 99.0),
                "mined_p05": percentile(&t.mined, 5.0),
                "mined_min": percentile(&t.mined, 0.0),
            },
        }));
    }
    let v = serde_json::json!({
        "schema": "thruline-langpacks/measure/1",
        "corpus_root": corpus.to_string_lossy(),
        "assertion_floor": 5000,
        "ambiguity_budget_pct": 0.1,
        "rename_similarity": {
            "chosen": choice.chosen,
            "per_language": choice.per_language,
            "pooled_cost": choice.pooled.iter()
                .map(|(t, c)| serde_json::json!({"t": t, "cost": c}))
                .collect::<Vec<_>>(),
            "basis": choice.basis,
        },
        "languages": langs,
    });
    let mut s = serde_json::to_string_pretty(&v).expect("serialise");
    s.push('\n');
    let _ = std::io::stdout().flush();
    s
}

//! Two source-walking censuses over this crate, both with negative controls.
//!
//! # Why these are tests and not review notes
//!
//! `Provenance` carries evidence now, so a corpus cannot be *relabelled* by
//! editing a word. That closes the edit and not the construction: nothing stops
//! a future caller from building an `ExternalSource` for a corpus that is not
//! external, in a file with no loader in it, and it would look ordinary in a
//! diff. The rule that makes it visible is a boundary — **provenance and labels
//! are constructed where the artifact is read, and nowhere else** — and a rule
//! that lives only in a comment is a rule until someone is in a hurry.
//!
//! The second census is the one the operator asked for by name: the ingest loop
//! must publish everything, and "publishes everything" has to be a property of
//! the code rather than a property of the last time someone read it.

use std::path::{Path, PathBuf};

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Every `.rs` under `src/`, paired with the part of it that is **not** a test
/// module. Fixtures legitimately construct anything; shipped code does not.
fn shipped_sources() -> Vec<(String, String)> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).expect("read_dir").flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    walk(&src_dir(), &mut files);
    files.sort();
    assert!(files.len() >= 5, "the walk found {} files", files.len());
    files
        .into_iter()
        .map(|p| {
            let name = p
                .strip_prefix(src_dir())
                .expect("prefix")
                .to_string_lossy()
                .replace('\\', "/");
            let text = std::fs::read_to_string(&p).expect("read");
            // `tests.rs` files are fixtures wholesale; elsewhere, cut at the
            // test module.
            let shipped = if name.ends_with("tests.rs") {
                String::new()
            } else {
                match text.find("#[cfg(test)]") {
                    Some(i) => text[..i].to_string(),
                    None => text,
                }
            };
            (name, shipped)
        })
        .collect()
}

/// Where a class may be constructed: the module that defines it, and the
/// loaders that read an artifact and can therefore *know* the answer.
///
/// `drive.rs` was added on 2026-09-05, **after this census refused it**, which
/// is the same route `class_two.rs` took onto `LABEL_SITES` below and is the
/// only route onto either list. The artefacts it reads before constructing the
/// class, all of them before the `Provenance` value exists: the committed split
/// re-derived from its own bytes (`TerminalBenchSplit::load`), the corpus
/// directory compared id-for-id against it (`check_against_corpus`), and the
/// append-only holdout ledger, whose `spend` is what returns the run manifest.
/// A class-2 claim asserts "generated under a protocol that was committed
/// first", and those three reads are what make it knowable rather than stated.
///
/// **It is on this list rather than written to slip past the classifier.**
/// `constructions()` below ignores any line whose needle is preceded by `let `,
/// so `let p = Provenance::GeneratedUnderProtocol(..)` would have passed this
/// census silently. `drive.rs` builds it in a named function instead, and pays
/// for the exemption here where a reader can see it.
const PROVENANCE_SITES: [&str; 3] = ["protocol.rs", "tau2.rs", "drive.rs"];
/// Where a label may be constructed: only where an oracle field is being read.
///
/// `class_two.rs` is on this list because it is a loader — it reads
/// `run-tests.sh`'s exit status for Terminal-Bench and a task's impossibility
/// for ImpossibleBench. It was added here **because this census refused it
/// first**, which is the point: a new label constructor cannot appear anywhere
/// without someone naming the oracle it reads.
const LABEL_SITES: [&str; 3] = ["lib.rs", "tau2.rs", "class_two.rs"];

fn hits(text: &str, needle: &str) -> usize {
    text.matches(needle).count()
}

/// Occurrences of `needle` that **build** a value, not ones that take it apart.
///
/// `if let Provenance::ExternalLabelled(src) = ...` and a `=>` arm are reads,
/// and reads are legitimate anywhere — the boundary this census draws is around
/// construction. Distinguishing them by the surrounding syntax is coarse, and
/// it is coarse in the safe direction: a construction the classifier mistakes
/// for a pattern would have to be written on a line that already binds a `let`
/// or opens a match arm.
fn constructions(text: &str, needle: &str) -> usize {
    text.lines()
        .filter_map(|line| line.find(needle).map(|i| (line, i)))
        .filter(|(line, i)| !line[..*i].contains("let ") && !line.contains("=>"))
        .count()
}

#[test]
fn provenance_is_constructed_only_where_an_artifact_is_read() {
    let mut offenders: Vec<String> = Vec::new();
    let mut allowed = 0usize;
    for (name, text) in shipped_sources() {
        for needle in [
            "Provenance::ExternalLabelled(",
            "Provenance::GeneratedUnderProtocol(",
            "ExternalSource::new(",
        ] {
            let n = constructions(&text, needle);
            if n == 0 {
                continue;
            }
            if PROVENANCE_SITES.iter().any(|s| name.ends_with(s)) {
                allowed += n;
            } else {
                offenders.push(format!("{name}: {n}× {needle}"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "provenance constructed away from an artifact read:\n  {}\n\
         A class carries the evidence its claim implies; building one where no \
         artifact was read is asserting the evidence rather than having it.",
        offenders.join("\n  ")
    );
    // Positive control: the loaders really do construct, so this census is
    // looking at something rather than passing on an empty set.
    assert!(
        allowed >= 2,
        "the census matched {allowed} legitimate sites"
    );
}

#[test]
fn labels_are_constructed_only_where_an_oracle_is_read() {
    let mut offenders: Vec<String> = Vec::new();
    let mut allowed = 0usize;
    for (name, text) in shipped_sources() {
        for needle in ["Ground::from_oracle(", "OracleReading::new("] {
            let n = hits(&text, needle);
            if n == 0 {
                continue;
            }
            if LABEL_SITES.iter().any(|s| name.ends_with(s)) {
                allowed += n;
            } else {
                offenders.push(format!("{name}: {n}× {needle}"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "a label built away from an oracle read:\n  {}",
        offenders.join("\n  ")
    );
    assert!(
        allowed >= 2,
        "the census matched {allowed} legitimate sites"
    );
}

/// **Negative control for both censuses.** The same walk, run over a source
/// text with the offence injected, must find it — otherwise the two tests above
/// pass because the needles never match anything.
#[test]
fn the_census_catches_a_construction_it_should_refuse() {
    let injected = "fn make() -> Provenance {\n    \
                    Provenance::ExternalLabelled(ExternalSource::new(\"p\", \"d\", \"v\", \"l\").unwrap())\n}";
    assert_eq!(constructions(injected, "Provenance::ExternalLabelled("), 1);
    assert_eq!(constructions(injected, "ExternalSource::new("), 1);
    assert!(!PROVENANCE_SITES.iter().any(|s| "report.rs".ends_with(s)));
    // And the classifier does not simply exclude everything: a read is not
    // counted, a construction on the next line is.
    let read = "if let Provenance::ExternalLabelled(src) = &self.provenance {";
    assert_eq!(constructions(read, "Provenance::ExternalLabelled("), 0);
    let arm = "            Provenance::GeneratedUnderProtocol(c) => Some(c),";
    assert_eq!(constructions(arm, "Provenance::GeneratedUnderProtocol("), 0);
    let labelled = "let g = Ground::from_oracle(\"x\", \"1\", Label::HonestPass);";
    assert_eq!(hits(labelled, "Ground::from_oracle("), 1);
    assert!(!LABEL_SITES.iter().any(|s| "normalize.rs".ends_with(s)));
}

// ---------------------------------------------------------------------------
// The ingest loop publishes everything.
// ---------------------------------------------------------------------------

/// The body of `tau2::load`, which is the only place a published trajectory can
/// fail to become a row.
fn ingest_body() -> String {
    let text = std::fs::read_to_string(src_dir().join("tau2.rs")).expect("tau2.rs");
    let start = text
        .find("pub fn load(")
        .expect("`load` is still named `load`");
    let rest = &text[start + 12..];
    let end = rest
        .find("\npub fn ")
        .or_else(|| rest.find("\nfn "))
        .expect("a function follows `load`");
    rest[..end].to_string()
}

/// Constructs that turn a row into no row without saying so. Each is legitimate
/// somewhere; none is legitimate in an ingest loop, where the only two honest
/// outcomes are "a trajectory" and "the whole load fails".
const SILENT_DROPS: [&str; 5] = [
    "continue",
    ".filter_map(",
    ".flatten()",
    "if let Ok(",
    ".ok()",
];

#[test]
fn the_ingest_loop_has_no_path_that_drops_a_row() {
    let body = ingest_body();
    let found: Vec<&str> = SILENT_DROPS
        .iter()
        .copied()
        .filter(|n| body.contains(n))
        .collect();
    assert!(
        found.is_empty(),
        "`tau2::load` contains {found:?}. A benchmark loader that skips a \
         malformed row reports on the rows it liked, and the corpus it \
         describes is not the corpus on disk. Fail the load instead."
    );
    // Positive control: the body really is the body.
    assert!(
        body.contains("trajectories.push("),
        "the extracted body is not the ingest loop"
    );
    // And the arithmetic that makes it a measurement rather than a promise.
    assert!(
        body.contains("trajectories.len() != sims.len()"),
        "the loader no longer checks that every simulation became a trajectory"
    );
}

/// Negative control: the same check over a body with the defect reintroduced.
#[test]
fn the_ingest_census_catches_a_skip() {
    let defective = ingest_body().replace(
        "let trial = sim.get(\"trial\")",
        "let Some(_) = sim.get(\"reward_info\") else { continue };\n        let trial = sim.get(\"trial\")",
    );
    assert!(
        SILENT_DROPS.iter().any(|n| defective.contains(n)),
        "the census would not notice a `continue` in the ingest loop"
    );
}

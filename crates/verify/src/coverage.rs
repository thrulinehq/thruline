//! BUILD-SPEC §6.3's last clause — coverage of changed lines.
//!
//! > *Coverage of changed lines (when a coverage artifact exists:
//! > `coverage.xml`, `lcov.info`, `.coverage`, `tarpaulin`): compute % of
//! > changed executable lines covered; below
//! > `policy.min_changed_line_coverage` → `warn` (or `fail` per policy).*
//!
//! # The absent artifact is the common case, and it is not zero
//!
//! Most sessions produce no coverage file. That must read as **"not measured"**
//! and never as 0% — a missing artifact reported as zero coverage would refute
//! every honest claim in every repository that does not collect coverage, which
//! is most of them.
//!
//! # Only lines the coverage tool knows about count
//!
//! A changed line the report does not mention is a blank line, a comment, a
//! brace or an import — not an uncovered statement. Counting those as uncovered
//! would make the percentage a function of formatting.

use std::collections::{BTreeMap, BTreeSet};

use crate::verdict::{Finding, Severity};

/// Per-file line hit counts, as a coverage report gives them.
pub type LineHits = BTreeMap<String, BTreeMap<u32, u64>>;

/// What the coverage clause concluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Coverage {
    /// No artifact, or none naming any changed file.
    NotMeasured { reason: String },
    Measured {
        changed_executable: usize,
        covered: usize,
        /// Basis points, integer — no floats in anything that may be hashed
        /// (BUILD-PLAN C.4/N0).
        pct_bp: u32,
    },
}

impl Coverage {
    /// §6.3's finding. `None` when there is nothing to say.
    pub fn finding(&self, min_pct: u32) -> Option<Finding> {
        match self {
            Coverage::NotMeasured { reason } => Some(Finding::new(
                Severity::Info,
                "coverage_not_measured",
                format!("changed-line coverage was not measured: {reason}"),
            )),
            Coverage::Measured {
                changed_executable,
                covered,
                pct_bp,
            } => {
                let pct = pct_bp / 100;
                (pct < min_pct).then(|| {
                    Finding::new(
                        Severity::Warn,
                        "low_changed_line_coverage",
                        format!(
                            "{covered} of {changed_executable} changed executable lines are \
                             covered ({}.{:02}%), below the {min_pct}% floor",
                            pct_bp / 100,
                            pct_bp % 100
                        ),
                    )
                })
            }
        }
    }
}

/// §6.3's default floor, used until §9's policy engine exists.
pub const DEFAULT_MIN_PCT: u32 = 70;

/// Compute changed-line coverage.
///
/// `changed_lines` is `path -> line numbers added or modified`, from the diff.
pub fn changed_line_coverage(
    changed_lines: &BTreeMap<String, BTreeSet<u32>>,
    hits: &LineHits,
) -> Coverage {
    if hits.is_empty() {
        return Coverage::NotMeasured {
            reason: "no coverage artifact was found (looked for lcov.info, coverage.xml, \
                     cobertura.xml)"
                .to_string(),
        };
    }
    let mut executable = 0usize;
    let mut covered = 0usize;
    for (path, lines) in changed_lines {
        let Some(file_hits) = lookup(hits, path) else {
            continue;
        };
        for line in lines {
            // Only lines the tool knows are executable. See the module docs.
            let Some(count) = file_hits.get(line) else {
                continue;
            };
            executable += 1;
            if *count > 0 {
                covered += 1;
            }
        }
    }
    if executable == 0 {
        return Coverage::NotMeasured {
            reason: "a coverage artifact exists but names none of the changed lines — \
                     the report may predate the change"
                .to_string(),
        };
    }
    Coverage::Measured {
        changed_executable: executable,
        covered,
        pct_bp: u32::try_from(covered * 10_000 / executable).unwrap_or(0),
    }
}

/// Coverage reports and diffs disagree about path prefixes constantly, so a
/// suffix match is tried after an exact one.
fn lookup<'a>(hits: &'a LineHits, path: &str) -> Option<&'a BTreeMap<u32, u64>> {
    if let Some(h) = hits.get(path) {
        return Some(h);
    }
    hits.iter()
        .find(|(k, _)| k.ends_with(path) || path.ends_with(k.as_str()))
        .map(|(_, v)| v)
}

/// Parse an `lcov.info` report.
pub fn parse_lcov(text: &str) -> LineHits {
    let mut out: LineHits = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        if let Some(path) = line.strip_prefix("SF:") {
            current = Some(path.trim().to_string());
        } else if let Some(da) = line.strip_prefix("DA:") {
            let Some(file) = current.as_ref() else {
                continue;
            };
            let mut parts = da.trim().split(',');
            let (Some(n), Some(c)) = (parts.next(), parts.next()) else {
                continue;
            };
            let (Ok(n), Ok(c)) = (n.parse::<u32>(), c.parse::<u64>()) else {
                continue;
            };
            out.entry(file.clone()).or_default().insert(n, c);
        } else if line.trim() == "end_of_record" {
            current = None;
        }
    }
    out
}

/// Parse a Cobertura / `coverage.xml` report.
///
/// Attribute scraping rather than a full XML parse: the shape is
/// `<class filename="…"> <lines> <line number="N" hits="M"/>`, and pulling an
/// XML dependency in for two attributes is not worth its supply chain.
pub fn parse_cobertura(text: &str) -> LineHits {
    let mut out: LineHits = BTreeMap::new();
    let mut current: Option<String> = None;
    for chunk in text.split('<') {
        if let Some(rest) = chunk.strip_prefix("class ") {
            current = attr(rest, "filename");
        } else if chunk.starts_with("line ") {
            let Some(file) = current.as_ref() else {
                continue;
            };
            let (Some(n), Some(h)) = (attr(chunk, "number"), attr(chunk, "hits")) else {
                continue;
            };
            let (Ok(n), Ok(h)) = (n.parse::<u32>(), h.parse::<u64>()) else {
                continue;
            };
            out.entry(file.clone()).or_default().insert(n, h);
        }
    }
    out
}

fn attr(chunk: &str, name: &str) -> Option<String> {
    let key = format!("{name}=\"");
    let at = chunk.find(&key)? + key.len();
    Some(chunk[at..].split('"').next()?.to_string())
}

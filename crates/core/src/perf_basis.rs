//! P2b — the stamped-basis registry, and currency derived at read.
//!
//! Three sessions in a row (A186, A189, A190) were sent chasing a cost that had
//! been mis-attributed, and each one quoted a number whose measuring conditions
//! were no longer written down anywhere. The E2 ruling that came out of it is
//! the contract this module implements:
//!
//! > Every performance pin carries a **stamped basis** — corpus fingerprint,
//! > code state, build profile, derived-state counts. Its currency is **derived
//! > at read** by stamp comparison. A pin whose basis has diverged renders
//! > **"stale basis — re-measure"** and cannot be quoted as current.
//!
//! Two design decisions carry the weight, and both are deliberate:
//!
//! 1. **Every comparison axis is OBSERVABLE.** Nothing here is a string a human
//!    declares about the conditions ("never rebuilt", "release build"). A
//!    declared axis is a promise, and this whole item exists because promises
//!    about measuring conditions did not survive three days. `rebuild state`
//!    from the E2 wording is therefore represented by what can actually be
//!    counted — `project_heads`, `node_projection_heads`, and `nodes` — because
//!    those are what a rebuild *changes*. See [`Observed::from_ledger`].
//!
//! 2. **The code axis is a digest over the engine source, not the commit sha.**
//!    E2's own words are "when the code beneath it changes". A commit that edits
//!    a markdown file does not change the code beneath a measurement, and a
//!    stamp that expired on every commit would be discarded within a day. The
//!    digest is computed at build time by `crates/cli/build.rs` over
//!    `crates/core/src`, `crates/cli/src`, the manifests and `Cargo.lock`, so
//!    the binary that renders a figure is made of code it can identify. The
//!    algorithm lives here, in [`code_digest`], and `crates/cli/tests` pins the
//!    embedded value against a fresh walk of the tree — the embedded digest is
//!    verified, not asserted.
//!
//! What this module deliberately does NOT do is decide that a figure is fine
//! because it is *nearly* current. There is no tolerance, no "close enough on
//! the counts". A190's finding was that a figure whose basis cannot be recovered
//! can only be re-measured, and the same is true of one whose basis has moved.

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::ledger::Ledger;

/// The registry as committed. Embedded so a binary with no source tree beside
/// it still knows which figures exist and what they were measured against.
pub const REGISTRY_TOML: &str = include_str!("../../../docs/perf-basis.toml");

/// What a diverged figure renders instead of its numbers.
///
/// The numbers are not printed alongside it. A stale figure that still shows
/// "39 ms" next to a warning is a stale figure that will be quoted anyway.
pub const STALE: &str = "stale basis — re-measure";

/// The axes that decide currency, in render order.
///
/// Anything a basis records that is not on this list is descriptive context —
/// it appears in the stamp so a future session can reconstruct the conditions,
/// and it never causes or prevents divergence. `ledger_bytes` is the clearest
/// example: a `VACUUM` moves it by hundreds of megabytes without one row of the
/// corpus changing, so it cannot discriminate and must not be allowed to try.
pub const AXES: &[&str] = &[
    "projects",
    "events",
    "sessions",
    "nodes",
    "project_heads",
    "node_projections",
    "code_digest",
    "profile",
];

/// The exact query behind each corpus axis, so "projects" can never again mean
/// two different numbers in two different documents.
///
/// This is not pedantry. On the ledger this file was written against,
/// `COUNT(*) FROM projects` is **91** and `COUNT(DISTINCT project_id) FROM
/// events` is **90** — one project row carries no events. A186 reported "91
/// projects" and A191 stamped "90 projects" for the same corpus on the same
/// day, and nothing in either document said which count was meant.
pub const AXIS_QUERIES: &[(&str, &str)] = &[
    ("projects", "SELECT COUNT(DISTINCT project_id) FROM events"),
    ("events", "SELECT COUNT(*) FROM events"),
    ("sessions", "SELECT COUNT(*) FROM sessions"),
    ("nodes", "SELECT COUNT(*) FROM nodes"),
    ("project_heads", "SELECT COUNT(*) FROM project_heads"),
    (
        "node_projections",
        "SELECT COUNT(*) FROM node_projection_heads",
    ),
    ("code_digest", "build-time digest of the engine source"),
    ("profile", "cfg!(debug_assertions) at compile time"),
];

/// A set of measuring conditions, named once and referenced by every figure
/// measured under it.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Basis {
    /// Stable id, referenced by [`Figure::basis`].
    pub id: String,
    /// ISO date the measurement ran.
    pub recorded: String,
    /// Whether the conditions were recorded completely enough to be compared
    /// against. `false` means the basis can never match anything, and every
    /// figure under it is permanently re-measure-only. That is the truthful
    /// state of every figure taken before this registry existed, and writing it
    /// down is the point — see A190: *a figure whose basis can't be recovered
    /// can't be compared against, only re-measured.*
    #[serde(default)]
    pub recoverable: bool,
    /// Free prose: what is known about the conditions, especially when the
    /// basis is unrecoverable and the fields below are therefore absent.
    #[serde(default)]
    pub note: Option<String>,

    // --- comparison axes. Required when `recoverable`; validated, not assumed.
    #[serde(default)]
    pub projects: Option<u64>,
    #[serde(default)]
    pub events: Option<u64>,
    #[serde(default)]
    pub sessions: Option<u64>,
    #[serde(default)]
    pub nodes: Option<u64>,
    #[serde(default)]
    pub project_heads: Option<u64>,
    #[serde(default)]
    pub node_projections: Option<u64>,
    #[serde(default)]
    pub code_digest: Option<String>,
    #[serde(default)]
    pub profile: Option<String>,

    // --- descriptive context. Never compared.
    #[serde(default)]
    pub ledger_bytes: Option<u64>,
    #[serde(default)]
    pub projects_table: Option<u64>,
    #[serde(default)]
    pub commit: Option<String>,
    #[serde(default)]
    pub machine: Option<String>,
    /// The measuring instrument itself — script path and its sha256. Not a
    /// comparison axis, because the code digest already covers the thing being
    /// measured and the harness is not it; recorded because a figure taken with
    /// a different harness is a figure taken a different way, and A190's
    /// micro-benchmark failure was exactly that.
    #[serde(default)]
    pub harness: Option<String>,
}

impl Basis {
    fn axis_value(&self, axis: &str) -> Option<String> {
        match axis {
            "projects" => self.projects.map(|v| v.to_string()),
            "events" => self.events.map(|v| v.to_string()),
            "sessions" => self.sessions.map(|v| v.to_string()),
            "nodes" => self.nodes.map(|v| v.to_string()),
            "project_heads" => self.project_heads.map(|v| v.to_string()),
            "node_projections" => self.node_projections.map(|v| v.to_string()),
            "code_digest" => self.code_digest.clone(),
            "profile" => self.profile.clone(),
            _ => None,
        }
    }
}

/// One measured number, and the basis it was measured under.
///
/// A figure never carries a verdict. "PASS" is derived from `budget_ms` at
/// render time and only for a figure whose basis is current, because a pass
/// recorded against conditions that no longer hold is the exact artefact this
/// registry exists to prevent.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Figure {
    pub id: String,
    pub label: String,
    pub basis: String,
    #[serde(default)]
    pub p50_ms: Option<u64>,
    #[serde(default)]
    pub p95_ms: Option<u64>,
    /// Absent means the row is attribution, not a budgeted claim.
    #[serde(default)]
    pub budget_ms: Option<u64>,
    /// For figures that are not a millisecond pair — "> 600 s, one run".
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Registry {
    pub version: u32,
    #[serde(default, rename = "basis")]
    pub bases: Vec<Basis>,
    #[serde(default, rename = "figure")]
    pub figures: Vec<Figure>,
}

/// The conditions right now, every field measured rather than declared.
#[derive(Debug, Clone, Serialize)]
pub struct Observed {
    pub projects: u64,
    pub events: u64,
    pub sessions: u64,
    pub nodes: u64,
    pub project_heads: u64,
    pub node_projections: u64,
    pub code_digest: String,
    pub profile: String,
    /// Descriptive, never compared.
    pub ledger_bytes: Option<u64>,
    pub projects_table: u64,
}

impl Observed {
    /// Read the corpus axes off a ledger.
    ///
    /// `code_digest` comes from the caller because only the binary knows what
    /// it was built from; `crates/cli` passes `env!("THRULINE_CODE_DIGEST")`.
    /// `profile` is compile-time, so it is derived here and cannot be spoofed
    /// by a caller who wants a release-basis figure to look current.
    pub fn from_ledger(ledger: &Ledger, code_digest: &str) -> Result<Self> {
        let conn = ledger.conn();
        let mut counts = Vec::new();
        for (axis, query) in AXIS_QUERIES {
            if *axis == "code_digest" || *axis == "profile" {
                continue;
            }
            let n: i64 = conn.query_row(query, [], |r| r.get(0))?;
            counts.push((*axis, n.max(0) as u64));
        }
        // An axis with no query would silently read as zero, and a zero that
        // happened to match a recorded zero would report a diverged basis as
        // current. Refuse instead.
        let get = |axis: &str| -> Result<u64> {
            counts
                .iter()
                .find(|(a, _)| *a == axis)
                .map(|(_, n)| *n)
                .ok_or_else(|| {
                    CoreError::Config(format!(
                        "perf_basis: axis {axis} is compared but has no query in AXIS_QUERIES"
                    ))
                })
        };
        let projects_table: i64 =
            conn.query_row("SELECT COUNT(*) FROM projects", [], |r| r.get(0))?;
        Ok(Observed {
            projects: get("projects")?,
            events: get("events")?,
            sessions: get("sessions")?,
            nodes: get("nodes")?,
            project_heads: get("project_heads")?,
            node_projections: get("node_projections")?,
            code_digest: code_digest.to_string(),
            profile: build_profile().to_string(),
            ledger_bytes: None,
            projects_table: projects_table.max(0) as u64,
        })
    }

    fn axis_value(&self, axis: &str) -> String {
        match axis {
            "projects" => self.projects.to_string(),
            "events" => self.events.to_string(),
            "sessions" => self.sessions.to_string(),
            "nodes" => self.nodes.to_string(),
            "project_heads" => self.project_heads.to_string(),
            "node_projections" => self.node_projections.to_string(),
            "code_digest" => self.code_digest.clone(),
            "profile" => self.profile.clone(),
            _ => String::new(),
        }
    }
}

/// Which build produced the reading binary. Compile-time, so a debug binary
/// cannot report a release basis as current.
pub fn build_profile() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
}

/// One axis that does not match, with both sides, so the reader is told what
/// moved rather than merely that something did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Divergence {
    pub axis: &'static str,
    pub recorded: String,
    pub observed: String,
}

/// The derived answer. Not stored anywhere: computed on every read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum BasisCurrency {
    /// Every comparison axis matches. The figure may be quoted.
    Current,
    /// At least one axis moved. Named, so the next session knows whether the
    /// corpus grew or the engine changed.
    Diverged(Vec<Divergence>),
    /// The basis was never recorded completely enough to compare. Permanently
    /// unquotable; the only way out is a fresh measurement.
    Unrecoverable,
    /// The figure names a basis that is not in the registry. A registry error,
    /// caught by [`Registry::validate`], but rendered rather than panicked so
    /// a malformed registry cannot take a surface down.
    UnknownBasis,
}

impl BasisCurrency {
    /// The one predicate the rest of the tree should ask.
    pub fn quotable(&self) -> bool {
        matches!(self, BasisCurrency::Current)
    }

    /// A short reason, for the row that replaces the numbers.
    pub fn reason(&self) -> String {
        match self {
            BasisCurrency::Current => "current".to_string(),
            BasisCurrency::Diverged(ds) => {
                let axes: Vec<&str> = ds.iter().map(|d| d.axis).collect();
                format!("{STALE} ({})", axes.join(", "))
            }
            BasisCurrency::Unrecoverable => format!("{STALE} (basis unrecoverable)"),
            BasisCurrency::UnknownBasis => format!("{STALE} (basis not in the registry)"),
        }
    }
}

impl Registry {
    /// Parse the committed registry.
    pub fn load() -> Result<Self> {
        Self::parse(REGISTRY_TOML)
    }

    pub fn parse(text: &str) -> Result<Self> {
        let registry: Registry =
            toml::from_str(text).map_err(|e| CoreError::Config(format!("perf-basis.toml: {e}")))?;
        registry.validate()?;
        Ok(registry)
    }

    /// Fail loud on a registry that cannot mean what it says.
    ///
    /// The checks are the contract in executable form: ids unique, every figure
    /// resolves, a recoverable basis records every comparison axis, and — the
    /// one that matters most — two figures for the same measurement must sit on
    /// two *different* named bases. That is the "never assert two figures
    /// without named bases" clause; `verify` at 21.9 s and `verify` at over
    /// 600 s are both true, and the registry may hold both, but only as two
    /// stamped rows and never as one contradiction.
    pub fn validate(&self) -> Result<()> {
        let mut seen = std::collections::BTreeSet::new();
        for basis in &self.bases {
            if !seen.insert(basis.id.as_str()) {
                return Err(CoreError::Config(format!(
                    "perf-basis.toml: duplicate basis id {}",
                    basis.id
                )));
            }
            if basis.recoverable {
                for axis in AXES {
                    if basis.axis_value(axis).is_none() {
                        return Err(CoreError::Config(format!(
                            "perf-basis.toml: basis {} claims to be recoverable but records no {axis}",
                            basis.id
                        )));
                    }
                }
            }
        }
        let mut per_measurement: std::collections::BTreeMap<&str, Vec<&str>> = Default::default();
        for figure in &self.figures {
            if self.basis(&figure.basis).is_none() {
                return Err(CoreError::Config(format!(
                    "perf-basis.toml: figure {} names basis {}, which does not exist",
                    figure.id, figure.basis
                )));
            }
            per_measurement
                .entry(figure.id.as_str())
                .or_default()
                .push(figure.basis.as_str());
        }
        for (id, bases) in per_measurement {
            let distinct: std::collections::BTreeSet<&str> = bases.iter().copied().collect();
            if distinct.len() != bases.len() {
                return Err(CoreError::Config(format!(
                    "perf-basis.toml: measurement {id} has two figures on the same basis — \
                     one of them is an unstamped assertion"
                )));
            }
        }
        Ok(())
    }

    pub fn basis(&self, id: &str) -> Option<&Basis> {
        self.bases.iter().find(|b| b.id == id)
    }

    /// Derive currency for one figure, now.
    pub fn currency(&self, figure: &Figure, observed: &Observed) -> BasisCurrency {
        let Some(basis) = self.basis(&figure.basis) else {
            return BasisCurrency::UnknownBasis;
        };
        if !basis.recoverable {
            return BasisCurrency::Unrecoverable;
        }
        let mut diverged = Vec::new();
        for axis in AXES {
            let recorded = basis.axis_value(axis).unwrap_or_default();
            let observed_value = observed.axis_value(axis);
            if recorded != observed_value {
                diverged.push(Divergence {
                    axis,
                    recorded,
                    observed: observed_value,
                });
            }
        }
        if diverged.is_empty() {
            BasisCurrency::Current
        } else {
            BasisCurrency::Diverged(diverged)
        }
    }
}

/// Milliseconds, in the unit a reader can hold. Ten seconds is the crossover:
/// below it the millisecond is the meaningful digit, above it nobody reads
/// "600000 ms" as ten minutes.
fn ms(v: u64) -> String {
    if v >= 10_000 {
        format!("{:.1} s", v as f64 / 1000.0)
    } else {
        format!("{v} ms")
    }
}

/// Render one figure as a markdown table row.
///
/// A quotable figure prints its numbers and a derived verdict. A stale one
/// prints [`STALE`] across the number columns and nothing else — the numbers do
/// not appear anywhere on the row, because a number printed beside a warning is
/// a number that gets quoted.
pub fn render_row(figure: &Figure, currency: &BasisCurrency) -> String {
    if !currency.quotable() {
        return format!(
            "| {} | {} | {} |",
            figure.label,
            currency.reason(),
            figure.basis
        );
    }
    // A figure whose value is prose rather than a millisecond pair — peak RSS,
    // the O(1) head read — must still print its value. An earlier draft rendered
    // those as three em-dashes and dropped the only content the row had.
    if figure.p50_ms.is_none() && figure.p95_ms.is_none() {
        return format!(
            "| {} | {} | {} |",
            figure.label,
            figure
                .note
                .as_deref()
                .unwrap_or("no value recorded")
                .replace('\n', " ")
                .trim(),
            figure.basis
        );
    }
    let cell = |v: Option<u64>| v.map(ms).unwrap_or_else(|| "—".into());
    let budget = figure.budget_ms.map(ms).unwrap_or_else(|| "—".into());
    let verdict = match (figure.budget_ms, figure.p95_ms) {
        (Some(b), Some(p95)) if p95 <= b => "PASS",
        (Some(_), Some(_)) => "FAIL",
        _ => "attribution",
    };
    format!(
        "| {} | {} | {} | {} | {} | {} |",
        figure.label,
        cell(figure.p50_ms),
        cell(figure.p95_ms),
        budget,
        verdict,
        figure.basis
    )
}

/// The whole table, currency derived per row against `observed`.
pub fn render_table(registry: &Registry, observed: &Observed) -> String {
    let mut out = String::new();
    out.push_str("| measurement | p50 | p95 | budget | verdict | basis |\n");
    out.push_str("|---|---:|---:|---:|---|---|\n");
    for figure in &registry.figures {
        let currency = registry.currency(figure, observed);
        out.push_str(&render_row(figure, &currency));
        out.push('\n');
    }
    out
}

/// The block `docs/perf-baseline.md` carries, generated from the registry.
///
/// Deliberately currency-free. A document cannot say whether a figure is
/// current, because currency depends on the machine reading it — that is what
/// `thruline perf` is for. What the document CAN do, and what this block makes
/// it do, is refuse to state a figure without naming the basis it came from.
///
/// `crates/core/tests/a192_perf_basis.rs` asserts the committed document
/// contains this block byte for byte, so a figure added to the registry and not
/// to the document, or a figure edited in the document by hand, fails the
/// suite.
pub fn render_doc_block(registry: &Registry) -> String {
    let mut out = String::new();
    out.push_str("| measurement | as recorded | basis | comparable against today? |\n");
    out.push_str("|---|---|---|---|\n");
    for figure in &registry.figures {
        let value = match (figure.p50_ms, figure.p95_ms, figure.budget_ms) {
            (Some(p50), Some(p95), Some(b)) => {
                format!("{} / {} vs {}", ms(p50), ms(p95), ms(b))
            }
            (Some(p50), Some(p95), None) => format!("{} / {}", ms(p50), ms(p95)),
            (Some(p50), None, _) => ms(p50),
            _ => String::new(),
        };
        let recorded = match (value.is_empty(), figure.note.as_deref()) {
            (true, Some(note)) => note.to_string(),
            (true, None) => "no numeric value recorded".to_string(),
            (false, Some(note)) => format!("{value} — {note}"),
            (false, None) => value,
        };
        let comparable = match registry.basis(&figure.basis) {
            Some(b) if b.recoverable => "yes — `thruline perf` derives currency here",
            Some(_) => "**no** — basis unrecoverable, re-measure only",
            None => "**no** — basis not in the registry",
        };
        let recorded = recorded.replace('\n', " ");
        out.push_str(&format!(
            "| {} | {} | `{}` | {} |\n",
            figure.label,
            recorded.trim(),
            figure.basis,
            comparable
        ));
    }
    out
}

/// The canonical code digest.
///
/// `entries` is `(repo-relative path, file bytes)`. The caller decides which
/// files are "the engine"; `crates/cli/build.rs` is that caller in the shipped
/// binary and a test walks the same set. Sorting happens here so two callers
/// that disagree about directory order still agree about the digest.
pub fn code_digest(entries: &mut [(String, Vec<u8>)]) -> String {
    use sha2::{Digest, Sha256};

    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut outer = Sha256::new();
    outer.update(b"thruline-code-digest-v1\n");
    for (path, bytes) in entries.iter() {
        let mut inner = Sha256::new();
        inner.update(bytes);
        outer.update(path.as_bytes());
        outer.update(b"\n");
        outer.update(crate::chain::to_hex(&inner.finalize()).as_bytes());
        outer.update(b"\n");
    }
    format!("sha256:{}", &crate::chain::to_hex(&outer.finalize())[..16])
}

/// The canonical definition of "the engine source", as repo-relative paths.
///
/// Duplicated — deliberately and exactly once — in `crates/cli/build.rs`, which
/// cannot depend on this crate without building it for the host. The
/// duplication is made safe by `crates/cli/tests/a192_code_digest.rs`, which
/// walks the tree with THIS function and asserts the result equals the digest
/// the build script embedded. Drift between the two copies fails that test
/// rather than quietly producing two digests for one tree.
///
/// Errors on any missing path, because a digest over a partial tree would be a
/// confident answer about the wrong thing.
pub fn engine_sources(root: &std::path::Path) -> Result<Vec<(String, Vec<u8>)>> {
    let mut out = Vec::new();
    for fixed in [
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        "crates/core/Cargo.toml",
        "crates/cli/Cargo.toml",
        "crates/cli/build.rs",
    ] {
        let path = root.join(fixed);
        let bytes = std::fs::read(&path).map_err(|e| CoreError::io(&path, e))?;
        out.push((fixed.to_string(), bytes));
    }
    for dir in ["crates/core/src", "crates/cli/src"] {
        collect_rs(root, &root.join(dir), &mut out)?;
    }
    Ok(out)
}

fn collect_rs(
    root: &std::path::Path,
    dir: &std::path::Path,
    out: &mut Vec<(String, Vec<u8>)>,
) -> Result<()> {
    let entries = std::fs::read_dir(dir).map_err(|e| CoreError::io(dir, e))?;
    for entry in entries {
        let entry = entry.map_err(|e| CoreError::io(dir, e))?;
        let path = entry.path();
        if path.is_dir() {
            collect_rs(root, &path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(path.as_path())
                .to_string_lossy()
                .replace('\\', "/");
            let bytes = std::fs::read(&path).map_err(|e| CoreError::io(&path, e))?;
            out.push((rel, bytes));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_committed_registry_parses_and_validates() {
        Registry::load().expect("docs/perf-basis.toml must parse and validate");
    }

    #[test]
    fn digest_is_order_independent_and_content_sensitive() {
        let mut a = vec![
            ("b.rs".to_string(), b"two".to_vec()),
            ("a.rs".to_string(), b"one".to_vec()),
        ];
        let mut b = vec![
            ("a.rs".to_string(), b"one".to_vec()),
            ("b.rs".to_string(), b"two".to_vec()),
        ];
        assert_eq!(code_digest(&mut a), code_digest(&mut b));
        let mut c = vec![
            ("a.rs".to_string(), b"one".to_vec()),
            ("b.rs".to_string(), b"two!".to_vec()),
        ];
        assert_ne!(code_digest(&mut a), code_digest(&mut c));
        // A renamed file with identical content is a different digest: the path
        // is hashed, not just the bytes.
        let mut d = vec![
            ("a.rs".to_string(), b"one".to_vec()),
            ("c.rs".to_string(), b"two".to_vec()),
        ];
        assert_ne!(code_digest(&mut a), code_digest(&mut d));
    }
}

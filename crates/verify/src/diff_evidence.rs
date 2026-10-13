//! BUILD-SPEC §6.6 — the claims whose evidence is the diff.
//!
//! > *Evidence is the fs/snapshot diff: does a file matching the ecosystem's
//! > migration path pattern exist in the diff; do docs paths change; does the
//! > manifest/lockfile change consistently; does the redaction/secret scanner
//! > find anything in the diff; does the named file/symbol actually appear.*
//!
//! # `secrets_absent` has a ceiling, and it is not a bug
//!
//! **[CONFLICT-8.]** `redact.rs`'s stated doctrine (A177) is that it *records
//! the treatment received, never asserts absence of secrets*. So a scanner
//! finding something makes `secrets_absent` **`unsupported`**, and a scanner
//! finding nothing makes it **`unverifiable`** — never `supported`. "I looked
//! and did not see one" is not "there is none", and §6's non-collapse rule is
//! exactly the rule that difference lives under.

use crate::verdict::{Finding, Severity, Verdict, VerdictRecord};

/// Path shapes each ecosystem uses for migrations.
const MIGRATION_PATTERNS: &[&str] = &[
    "migrations/",
    "migration/",
    "db/migrate/",
    "alembic/versions/",
    "prisma/migrations/",
    "flyway/",
    "liquibase/",
    "/migrations.",
    "schema.rb",
];

const DOC_PATTERNS: &[&str] = &[
    "readme",
    "docs/",
    "doc/",
    ".md",
    ".rst",
    ".adoc",
    "changelog",
    "mkdocs.yml",
    "book.toml",
];

/// Manifests and their lockfiles, paired so "consistently" can be checked.
const MANIFESTS: &[(&str, &[&str])] = &[
    ("cargo.toml", &["cargo.lock"]),
    (
        "package.json",
        &["package-lock.json", "yarn.lock", "pnpm-lock.yaml"],
    ),
    ("pyproject.toml", &["poetry.lock", "uv.lock", "pdm.lock"]),
    ("requirements.txt", &[]),
    ("go.mod", &["go.sum"]),
    ("gemfile", &["gemfile.lock"]),
    ("composer.json", &["composer.lock"]),
    ("pom.xml", &[]),
    ("build.gradle", &["gradle.lockfile"]),
];

fn matches_any(path: &str, patterns: &[&str]) -> bool {
    let lower = path.to_ascii_lowercase();
    patterns.iter().any(|p| lower.contains(p))
}

/// `migration_added`.
pub fn migration_added(claim_id: &str, changed: &[String]) -> VerdictRecord {
    let hits: Vec<&String> = changed
        .iter()
        .filter(|p| matches_any(p, MIGRATION_PATTERNS))
        .collect();
    if hits.is_empty() {
        VerdictRecord::new(claim_id, "migration_added@1", Verdict::Unsupported).reason(format!(
            "no path in the session's diff looks like a migration ({} path(s) changed)",
            changed.len()
        ))
    } else {
        VerdictRecord::new(claim_id, "migration_added@1", Verdict::Supported)
            .reason(format!("the diff adds or changes {}", hits[0]))
    }
}

/// `docs_updated`.
pub fn docs_updated(claim_id: &str, changed: &[String]) -> VerdictRecord {
    let hits: Vec<&String> = changed
        .iter()
        .filter(|p| matches_any(p, DOC_PATTERNS))
        .collect();
    if hits.is_empty() {
        VerdictRecord::new(claim_id, "docs_updated@1", Verdict::Unsupported).reason(format!(
            "no documentation path changed in this session ({} path(s) changed)",
            changed.len()
        ))
    } else {
        VerdictRecord::new(claim_id, "docs_updated@1", Verdict::Supported)
            .reason(format!("the diff changes {}", hits[0]))
    }
}

/// `dependency_added` — and *"consistently"* is the part that does the work.
pub fn dependency_added(claim_id: &str, changed: &[String]) -> VerdictRecord {
    let lower: Vec<String> = changed.iter().map(|p| p.to_ascii_lowercase()).collect();
    let mut record = VerdictRecord::new(claim_id, "dependency_added@1", Verdict::Unsupported);
    for (manifest, locks) in MANIFESTS {
        if !lower.iter().any(|p| p.ends_with(manifest)) {
            continue;
        }
        record = VerdictRecord::new(claim_id, "dependency_added@1", Verdict::Supported)
            .reason(format!("the diff changes {manifest}"));
        // A manifest changed with its lockfile untouched is the shape that
        // breaks a colleague's build, so it is named rather than passed over.
        if !locks.is_empty() && !locks.iter().any(|l| lower.iter().any(|p| p.ends_with(l))) {
            record = record.with_finding(Finding::new(
                Severity::Warn,
                "lockfile_not_updated",
                format!(
                    "{manifest} changed and none of its lockfiles ({}) did",
                    locks.join(", ")
                ),
            ));
        }
        return record;
    }
    record.reason(format!(
        "no dependency manifest changed in this session ({} path(s) changed)",
        changed.len()
    ))
}

/// `file_changed` / `did_x` — did the named thing actually move?
pub fn file_changed(claim_id: &str, names: &[String], changed: &[String]) -> VerdictRecord {
    if names.is_empty() {
        return VerdictRecord::new(claim_id, "file_changed@1", Verdict::Unverifiable).reason(
            "the claim names no file or symbol, so there is nothing to look for in the diff",
        );
    }
    let missing: Vec<&String> = names
        .iter()
        .filter(|n| {
            !changed
                .iter()
                .any(|p| p.to_ascii_lowercase().contains(&n.to_ascii_lowercase()))
        })
        .collect();
    if missing.is_empty() {
        VerdictRecord::new(claim_id, "file_changed@1", Verdict::Supported).reason(format!(
            "every named path appears in the diff: {}",
            names.join(", ")
        ))
    } else {
        VerdictRecord::new(claim_id, "file_changed@1", Verdict::Unsupported).reason(format!(
            "named but absent from the diff: {}",
            missing
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }
}

/// `secrets_absent` — **ceiling: never `supported`.** See the module docs.
pub fn secrets_absent(claim_id: &str, findings: &[(String, String)]) -> VerdictRecord {
    if findings.is_empty() {
        return VerdictRecord::new(claim_id, "secrets_absent@1", Verdict::Unverifiable).reason(
            "the scanner found nothing in the diff — which is not the same as there \
                 being nothing. ThruLine records the treatment text received and never \
                 asserts the absence of a secret, so this claim can reach `unsupported` \
                 and never `supported` (CONFLICT-8)",
        );
    }
    let mut record = VerdictRecord::new(claim_id, "secrets_absent@1", Verdict::Unsupported).reason(
        format!("{} credential-shaped value(s) in the diff", findings.len()),
    );
    for (path, family) in findings {
        record = record.with_finding(
            Finding::new(
                Severity::Fail,
                "secret_in_diff",
                format!("a {family}-shaped value appears in {path}"),
            )
            .at(path),
        );
    }
    record
}

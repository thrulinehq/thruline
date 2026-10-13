//! Human-reviewed, exact byte scopes pinned to one regular Git source blob.
//!
//! Purpose and support references are accepted data; this adapter verifies
//! identity and bounds. It does not infer a waiver from the source's wording.
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::text_exclusions::{NotLookedAt, TestScope, TestSpan};
use crate::redact::Masked;

const MAX_BYTES: usize = 2 * 1024 * 1024;
const MAX_SCOPES: usize = 256;

#[path = "test_declaration_v1.rs"]
mod test_declaration_v1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanPurpose {
    TestSupport,
    RemovalComment,
    AbsenceGuard,
    TestDeclarationV1,
}

/// Identity of one separately accepted and AST-resolved declaration. This
/// version never infers a test name from a computed JavaScript title.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeclarationReferenceV1 {
    pub path: String,
    pub source_blob_sha256: String,
    pub start_line: u64,
    pub end_line: u64,
    pub start_column: usize,
    pub end_column: usize,
}
impl DeclarationReferenceV1 {
    fn of(scope: &SpanScope) -> Self {
        Self {
            path: scope.path.clone(),
            source_blob_sha256: scope.source_blob_sha256.clone(),
            start_line: scope.start_line,
            end_line: scope.end_line,
            start_column: scope.start_column,
            end_column: scope.end_column,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpanScope {
    pub path: String,
    pub source_blob_sha256: String,
    pub start_line: u64,
    pub end_line: u64,
    pub start_column: usize,
    pub end_column: usize,
    pub purpose: SpanPurpose,
    pub supports: Vec<TestScope>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub declaration_supports: Vec<DeclarationReferenceV1>,
}

pub(crate) struct ResolvedSpan {
    pub path: String,
    pub receipt: NotLookedAt,
    pub excluded_lines: Vec<u64>,
}
impl ResolvedSpan {
    pub(crate) fn contains(&self, path: &str, line: u64, start: usize, end: usize) -> bool {
        self.path == path
            && (line, start)
                >= (
                    self.receipt.start_line.unwrap_or(0),
                    self.receipt.start_column.unwrap_or(0),
                )
            && (line, end)
                <= (
                    self.receipt.end_line.unwrap_or(0),
                    self.receipt.end_column.unwrap_or(0),
                )
    }
    pub(crate) fn pathname(&self) -> bool {
        self.receipt.start_line == Some(0)
    }
}

#[derive(Default)]
pub(crate) struct SpanResolution {
    pub spans: Vec<ResolvedSpan>,
    pub missing: Vec<String>,
}

pub(crate) fn validate_scopes(
    scopes: &[SpanScope],
    tests: &[TestScope],
) -> Result<(), &'static str> {
    if scopes.len() > MAX_SCOPES {
        return Err("span exclusion has too many exact selectors");
    }
    for (index, scope) in scopes.iter().enumerate() {
        if scope.path.is_empty()
            || scope.path.len() > 1024
            || scope.path.starts_with(['/', '~'])
            || scope.path.contains(['\\', ':', '*', '?', '[', ']'])
            || scope.path.chars().any(char::is_control)
            || scope
                .path
                .split('/')
                .any(|p| p.is_empty() || matches!(p, "." | ".."))
        {
            return Err("span exclusion needs one safe exact Git path");
        }
        if scope.source_blob_sha256.len() != 64
            || !scope
                .source_blob_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("span exclusion needs a full lowercase source blob SHA-256");
        }
        if scope.start_line == 0 || scope.end_line == 0 {
            if scope.start_line != 0
                || scope.end_line != 0
                || scope.start_column != 0
                || scope.end_column != scope.path.len()
            {
                return Err("pathname exclusion must select the complete path at line zero");
            }
        } else if (scope.start_line, scope.start_column) >= (scope.end_line, scope.end_column)
            || scope.end_line > MAX_BYTES as u64
            || scope.start_column > MAX_BYTES
            || scope.end_column > MAX_BYTES
        {
            return Err("content span exclusion needs nonempty bounded coordinates");
        }
        match scope.purpose {
            SpanPurpose::TestSupport => {
                if (scope.supports.is_empty() && scope.declaration_supports.is_empty())
                    || scope.declaration_supports.len() > MAX_SCOPES
                    || super::text_exclusions::validate_scopes(&scope.supports).is_err()
                {
                    return Err("support span needs bounded exact accepted test references");
                }
                let mut unique = BTreeSet::new();
                for support in &scope.supports {
                    if support.name_pattern.contains('*')
                        || !tests.contains(support)
                        || !unique.insert((&support.path, &support.name_pattern))
                    {
                        return Err("support span references must be unique exact accepted tests");
                    }
                }
                let mut declarations = BTreeSet::new();
                for support in &scope.declaration_supports {
                    if !declarations.insert(support)
                        || scopes
                            .iter()
                            .filter(|candidate| {
                                candidate.purpose == SpanPurpose::TestDeclarationV1
                                    && DeclarationReferenceV1::of(candidate) == *support
                            })
                            .count()
                            != 1
                    {
                        return Err(
                            "support span needs unique exact accepted declaration references",
                        );
                    }
                }
            }
            _ if !scope.supports.is_empty() || !scope.declaration_supports.is_empty() => {
                return Err("non-support span cannot carry test references")
            }
            _ => {}
        }
        if scope.purpose == SpanPurpose::TestDeclarationV1
            && (scope.start_line == 0
                || !(scope.path.ends_with(".rs")
                    || [".js", ".jsx", ".mjs", ".cjs"]
                        .iter()
                        .any(|ext| scope.path.ends_with(ext))))
        {
            return Err("versioned test declaration needs one supported content AST span");
        }
        for previous in &scopes[..index] {
            if previous.path == scope.path {
                if previous.source_blob_sha256 != scope.source_blob_sha256 {
                    return Err("span exclusions for one path need the same source blob pin");
                }
                if (previous.start_line, previous.start_column) < (scope.end_line, scope.end_column)
                    && (scope.start_line, scope.start_column)
                        < (previous.end_line, previous.end_column)
                {
                    return Err("span exclusions duplicate or overlap exact coordinates");
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn resolve_spans_at(
    repo: &Path,
    commit: &str,
    scopes: &[SpanScope],
    tests: &[TestScope],
    support_spans: &[TestSpan],
    deadline: Instant,
) -> SpanResolution {
    let mut result = SpanResolution::default();
    if scopes.is_empty() {
        return result;
    }
    if let Err(reason) = validate_scopes(scopes, tests) {
        result.missing.push(reason.into());
        return result;
    }
    // Reuse the checker's one exact-version AST parse of accepted tests.
    // Time already spent parsing tests counts against the caller's five-second
    // span resolution deadline. The test parser has its own bounded pass;
    // reuse its result instead of restarting that pass for every span.
    let mut sources = BTreeMap::<String, Result<String, &'static str>>::new();
    let mut declarations = BTreeSet::new();
    // Resolve exact declarations before their helper references, independently
    // of parameter ordering. Merely naming a selector cannot grant support.
    for scope in scopes
        .iter()
        .filter(|s| s.purpose == SpanPurpose::TestDeclarationV1)
        .chain(
            scopes
                .iter()
                .filter(|s| s.purpose != SpanPurpose::TestDeclarationV1),
        )
    {
        let resolved = (|| {
            if Instant::now() >= deadline {
                return Err("span source resolution exceeded its five second bound");
            }
            if scope.purpose == SpanPurpose::TestSupport {
                for support in &scope.supports {
                    if support_spans
                        .iter()
                        .filter(|s| {
                            s.path == support.path
                                && s.receipt.test_name.as_deref() == Some(&support.name_pattern)
                        })
                        .count()
                        != 1
                    {
                        return Err("support span must reference exactly one parsed accepted test");
                    }
                }
                if scope
                    .declaration_supports
                    .iter()
                    .any(|support| !declarations.contains(support))
                {
                    return Err(
                        "support span needs every accepted declaration to resolve on this version",
                    );
                }
            }
            let source = sources
                .entry(scope.path.clone())
                .or_insert_with(|| git_source(repo, commit, &scope.path, deadline));
            let source = source.as_ref().map_err(|reason| *reason)?;
            if hex_sha(source.as_bytes()) != scope.source_blob_sha256 {
                return Err("span source blob SHA-256 differs from the accepted pin");
            }
            if scope.purpose == SpanPurpose::TestDeclarationV1 {
                test_declaration_v1::validate(source, scope, deadline)?;
            }
            let mut excluded_lines = Vec::new();
            if scope.start_line != 0 {
                // split_terminator preserves CR bytes, matching Git grep's
                // byte columns; a final LF does not invent another line.
                let lines = source.split_terminator('\n').collect::<Vec<_>>();
                let first = lines
                    .get(scope.start_line as usize - 1)
                    .ok_or("span start line is outside its source blob")?;
                let last = lines
                    .get(scope.end_line as usize - 1)
                    .ok_or("span end line is outside its source blob")?;
                if scope.start_column > first.len()
                    || scope.end_column > last.len()
                    || !first.is_char_boundary(scope.start_column)
                    || !last.is_char_boundary(scope.end_column)
                {
                    return Err("span columns are outside source bytes or UTF-8 boundaries");
                }
                for number in scope.start_line..=scope.end_line {
                    let line = lines[number as usize - 1];
                    let lo = if number == scope.start_line {
                        scope.start_column
                    } else {
                        0
                    };
                    let hi = if number == scope.end_line {
                        scope.end_column
                    } else {
                        line.len()
                    };
                    if lo == 0 && hi == line.len() {
                        excluded_lines.push(number);
                    }
                }
            }
            if Instant::now() >= deadline {
                return Err("span source resolution exceeded its five second bound");
            }
            Ok(ResolvedSpan {
                path: scope.path.clone(),
                excluded_lines,
                receipt: NotLookedAt {
                    path: Masked::mask(&scope.path).into_text(),
                    source: if scope.purpose == SpanPurpose::TestDeclarationV1 {
                        "accepted_test_declaration_v1"
                    } else {
                        "accepted_span_scope"
                    }
                    .into(),
                    pattern: format!(
                        "{}:{}:{}-{}:{} [{}; {}]",
                        Masked::mask(&scope.path).text(),
                        scope.start_line,
                        scope.start_column,
                        scope.end_line,
                        scope.end_column,
                        match scope.purpose {
                            SpanPurpose::TestSupport => "test_support",
                            SpanPurpose::RemovalComment => "removal_comment",
                            SpanPurpose::AbsenceGuard => "absence_guard",
                            SpanPurpose::TestDeclarationV1 => "test_declaration_v1",
                        },
                        scope.source_blob_sha256
                    ),
                    test_name: None,
                    start_line: Some(scope.start_line),
                    end_line: Some(scope.end_line),
                    start_column: Some(scope.start_column),
                    end_column: Some(scope.end_column),
                },
            })
        })();
        match resolved {
            Ok(span) => {
                if scope.purpose == SpanPurpose::TestDeclarationV1 {
                    declarations.insert(DeclarationReferenceV1::of(scope));
                }
                result.spans.push(span);
            }
            Err(reason) => result.missing.push(format!(
                "{}:{}:{}-{}:{}: {reason}",
                Masked::mask(&scope.path).text(),
                scope.start_line,
                scope.start_column,
                scope.end_line,
                scope.end_column
            )),
        }
    }
    result
}

fn hex_sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn git_source(
    repo: &Path,
    commit: &str,
    path: &str,
    deadline: Instant,
) -> Result<String, &'static str> {
    let listing = git(repo, &["ls-tree", "-z", commit, "--", path], deadline)?;
    let rows = listing
        .split(|b| *b == 0)
        .filter(|r| !r.is_empty())
        .collect::<Vec<_>>();
    if rows.len() != 1 {
        return Err("span source has no exact regular Git entry");
    }
    let tab = rows[0]
        .iter()
        .position(|b| *b == b'\t')
        .ok_or("span source Git identity is invalid")?;
    if &rows[0][tab + 1..] != path.as_bytes() {
        return Err("span source Git path differs");
    }
    let identity =
        std::str::from_utf8(&rows[0][..tab]).map_err(|_| "span source Git identity is invalid")?;
    let parts = identity.split(' ').collect::<Vec<_>>();
    if parts.len() != 3
        || !matches!(parts[0], "100644" | "100755")
        || parts[1] != "blob"
        || !matches!(parts[2].len(), 40 | 64)
        || !parts[2]
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("span source is not the exact regular Git blob");
    }
    let oid = parts[2];
    let size = git(repo, &["cat-file", "-s", oid], deadline)?;
    let size = std::str::from_utf8(&size)
        .ok()
        .and_then(|s| s.trim().parse::<usize>().ok())
        .ok_or("span source size is unavailable")?;
    if size > MAX_BYTES {
        return Err("span source exceeds its two MiB bound");
    }
    let bytes = git(repo, &["cat-file", "blob", oid], deadline)?;
    if bytes.len() != size {
        return Err("span source blob length differs");
    }
    String::from_utf8(bytes).map_err(|_| "span source blob is not UTF-8")
}
fn git(repo: &Path, args: &[&str], deadline: Instant) -> Result<Vec<u8>, &'static str> {
    let mut command = Command::new("git");
    crate::git_isolation::isolate_config(&mut command)
        .args(args)
        .current_dir(repo);
    match crate::bounded_child::output_until(&mut command, deadline) {
        Ok(crate::bounded_child::Outcome::Finished(out)) if out.status.success() => Ok(out.stdout),
        _ => Err("span exact Git source is unavailable"),
    }
}

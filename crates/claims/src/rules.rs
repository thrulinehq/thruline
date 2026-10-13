//! BUILD-SPEC §5.2 — the ordered, deterministic pattern table.
//!
//! # Why the table is data and the matcher is dumb
//!
//! §5.2 requires an *ordered pattern table, editable, in YAML*. The matcher
//! here does one thing — find a phrase in a normalised sentence — and every
//! decision about what counts lives in [`DEFAULT_TABLE`]. That keeps the
//! interesting part reviewable as text rather than buried in control flow, and
//! it is what lets a repository override the table without a rebuild.
//!
//! The YAML subset is hand-parsed, matching `capture::redaction::config`
//! (Rule 11). No `serde_yaml` in the tree, and this is not the place to add a
//! dependency for six keys.
//!
//! # Negation is handled HERE, and that is a ruling, not a preference
//!
//! **[RULING R2 / BUILD-PLAN C.9.]** §6.9 originally let honest-failure markers
//! suppress a *verdict*. That is transcript prose changing an outcome, which
//! §0.1 and §0.2 forbid, and it is trivially exploitable: an agent that claims
//! success and appends "I couldn't get the tests running" would have its
//! missing evidence excused by a sentence it chose to write.
//!
//! The real case behind §6.9 is a sentence that *is* an honest failure but from
//! which a naive matcher extracts a claim — "I failed to make all tests pass"
//! → `tests_pass`. That is a **negation bug in the extractor**, and it belongs
//! here, where it changes *whether a claim exists*, not at verdict, where it
//! would change whether an existing claim's failure counts.
//!
//! Extraction may be conservative. Verdicts may not be excused.
//!
//! # The scoping rule that keeps negation from eating real claims
//!
//! A cue anywhere in the sentence would make *"I fixed the failing test and now
//! all tests pass"* negated — a real claim silently dropped. So a cue counts
//! only in the clause immediately preceding the match: the text before the
//! match is cut at the last clause boundary (`and`, `so`, `then`, `now`, `;`,
//! `,`) and only the remainder is searched.

use crate::claim::{ClaimArgs, ClaimKind, Scope};

/// One row of the table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    pub kind: ClaimKind,
    /// Lower-case phrase, matched against the normalised sentence.
    pub phrase: String,
    /// Scope this phrase implies when it carries no qualifier of its own.
    pub scope: Scope,
}

/// Words that end the clause a negation cue can reach across.
const CLAUSE_BOUNDARIES: &[&str] = &[
    " and ",
    " so ",
    " then ",
    " now ",
    " but ",
    "; ",
    ", ",
    " however ",
    " once ",
    " after ",
];

/// Cues that make a following phrase not a claim.
///
/// Deliberately includes the shapes §5.2 names — *"tests do not pass"*,
/// *"couldn't get tests running"*, *"skipped tests"* — and the ones an agent
/// actually writes.
const NEGATION_CUES: &[&str] = &[
    "not ",
    "n't ",
    "no ",
    "never ",
    "fail",
    "unable",
    "cannot",
    "can not",
    "without ",
    "except",
    "skip",
    "unfortunately",
    "still ",
    "yet to",
    "trying to",
    "attempt",
    "should ",
    "would ",
    "need to",
    "needs to",
    "hope",
    "if ",
    "unless",
    "before ",
    "make sure",
    "ensure",
    "verify",
    "check that",
];

/// The default table, in the YAML §5.2 asks for.
///
/// Ordered: the first match wins, so more specific phrases come first. Written
/// against what agents actually say — the `pytest -q` fixture that carried a
/// banner pytest does not emit is the warning here.
pub const DEFAULT_TABLE: &str = r#"
version: 1
patterns:
  - kind: tests_pass
    scope: all
    phrases:
      - "all tests pass"
      - "all tests are passing"
      - "all of the tests pass"
      - "the full test suite passes"
      - "test suite passes"
      - "the tests pass"
      - "tests are passing"
      - "tests pass"
      - "all green"
      - "suite is green"
  - kind: tests_pass
    scope: named
    phrases:
      - "the test passes"
      - "this test passes"
      - "that test passes"
  - kind: build_ok
    scope: all
    phrases:
      - "the build succeeds"
      - "build succeeds"
      - "builds cleanly"
      - "it compiles"
      - "compiles cleanly"
      - "cargo build ok"
  - kind: lint_ok
    scope: all
    phrases:
      - "lint clean"
      - "lint is clean"
      - "no clippy warnings"
      - "clippy is clean"
      - "eslint passes"
      - "ruff is clean"
  - kind: typecheck_ok
    scope: all
    phrases:
      - "type checks"
      - "typechecks"
      - "tsc passes"
      - "mypy clean"
      - "mypy is clean"
  - kind: behavior_unchanged
    scope: all
    phrases:
      - "no behavior change"
      - "no behaviour change"
      - "no functional changes"
      - "no functional change"
      - "purely a refactor"
      - "pure refactor"
      - "refactor only"
      - "behaviour is unchanged"
      - "behavior is unchanged"
  - kind: migration_added
    scope: all
    phrases:
      - "added a migration"
      - "migration added"
      - "wrote a migration"
  - kind: docs_updated
    scope: all
    phrases:
      - "updated the docs"
      - "docs updated"
      - "updated documentation"
  - kind: dependency_added
    scope: all
    phrases:
      - "added the dependency"
      - "added a dependency"
      - "dependency added"
  - kind: secrets_absent
    scope: all
    phrases:
      - "no secrets committed"
      - "no secrets in the diff"
      - "no credentials committed"
  - kind: task_complete
    scope: all
    phrases:
      - "all tasks complete"
      - "ready to merge"
      - "this is complete"
      - "task is complete"
      - "everything is done"
"#;

/// Something wrong with a pattern table.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TableError {
    #[error("{path}:{line}: {message}")]
    Syntax {
        path: String,
        line: usize,
        message: String,
    },
    #[error("{path}: unsupported table version {version:?} (this build reads 1)")]
    Version { path: String, version: String },
    #[error("{path}:{line}: {name:?} is not a §3.2 claim kind")]
    UnknownKind {
        path: String,
        line: usize,
        name: String,
    },
    /// A repository row that can only re-scope a phrase the built-in table
    /// already matches. See [`Table::load`] for why this is refused rather
    /// than merged.
    ///
    /// Boxed so the whole `Result` does not carry seven fields on the happy
    /// path — `clippy::result_large_err`, and this is the only variant with
    /// enough of them to trip it.
    #[error("{0}")]
    Shadow(Box<Shadow>),
}

/// The detail behind [`TableError::Shadow`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "{path}:{line}: this row would shadow a built-in pattern rather than add to it. \
     {kind} {phrase:?} at scope `{repo_scope}` sits over the built-in {default_phrase:?} \
     at scope `{default_scope}`: every sentence this phrase matches is a sentence the \
     built-in already matches, so its only possible effect is to change the scope — and \
     the scope is what decides whether a narrowed test run counts. A repository table \
     may only ADD patterns."
)]
pub struct Shadow {
    pub path: String,
    pub line: usize,
    pub kind: &'static str,
    pub phrase: String,
    pub repo_scope: &'static str,
    pub default_phrase: String,
    pub default_scope: &'static str,
}

impl Scope {
    fn as_str(self) -> &'static str {
        match self {
            Scope::All => "all",
            Scope::Changed => "changed",
            Scope::Named => "named",
        }
    }
}

/// Where the effective table came from.
///
/// The built-in table is not an override and must never be announced as one:
/// a disclosure that fires on every session is a disclosure nobody reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Origin {
    /// The repository file that contributed, if any.
    pub repo_path: Option<String>,
    /// Patterns the file added that the built-in table did not have.
    pub added: usize,
    /// Patterns the file restated exactly. Harmless, and counted rather than
    /// hidden, so "my rules are not being applied" is answerable.
    pub redundant: usize,
}

/// The ordered pattern table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    patterns: Vec<Pattern>,
    origin: Origin,
}

impl Default for Table {
    fn default() -> Self {
        // The embedded table is part of the build and is covered by tests, so a
        // parse failure here is a build-time bug, not a runtime condition.
        // Falling back to an EMPTY table would silently extract nothing, which
        // reads downstream as "the agent claimed nothing" — the worst possible
        // failure for this crate. So the fallback is the smallest table that
        // still finds the claim the product is chiefly about.
        Table::parse(DEFAULT_TABLE, "<built-in>").unwrap_or_else(|_| Table {
            patterns: vec![Pattern {
                kind: ClaimKind::TestsPass,
                phrase: "all tests pass".to_string(),
                scope: Scope::All,
            }],
            origin: Origin::default(),
        })
    }
}

impl Table {
    /// Where a repository may override the table.
    pub const PATH: &'static str = ".thruline/claims.yml";

    pub fn patterns(&self) -> &[Pattern] {
        &self.patterns
    }

    /// Where the effective table came from.
    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    /// True when nothing outside this binary contributed a pattern.
    pub fn is_built_in(&self) -> bool {
        self.origin.repo_path.is_none()
    }

    /// The `claims_table_hash`: a digest over the **effective** table, in
    /// order, domain-separated.
    ///
    /// Over the effective table rather than the file bytes, for the reason
    /// `thruline_policy::digest` gives: what a reviewer needs pinned is the
    /// rules that were in force, not the comments around them — and hashing
    /// bytes would happily produce a digest for a file that does not parse.
    /// It moves when the built-in table moves *and* when a repository adds to
    /// it, which is the point: a reader must be able to tell which vocabulary
    /// decided what they were shown.
    pub fn digest(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(DIGEST_DOMAIN.as_bytes());
        h.update(b"\n");
        for p in &self.patterns {
            h.update(p.kind.as_str().as_bytes());
            h.update(b"\t");
            h.update(p.scope.as_str().as_bytes());
            h.update(b"\t");
            h.update(p.phrase.as_bytes());
            h.update(b"\n");
        }
        let mut out = String::from("sha256:");
        for b in h.finalize() {
            out.push_str(&format!("{b:02x}"));
        }
        out
    }

    /// One line naming a repository table and pinning it, or `None` for the
    /// built-in table.
    ///
    /// **F4.** A repository file that chooses the words a reader is shown was
    /// invisible in the receipt: `grep -ic claims.yml <payload>` answered `0`.
    /// This string is what closes that, and it is `None` for the built-in
    /// table on purpose — a disclosure that fires on every session is a
    /// disclosure nobody reads.
    pub fn disclosure(&self) -> Option<String> {
        let path = self.origin.repo_path.as_ref()?;
        Some(format!(
            "claim table extended by {path} ({}): {} pattern(s) added to the built-in table, \
             {} already present. A repository table may only ADD; it can neither remove nor \
             re-scope a built-in pattern.",
            self.digest(),
            self.origin.added,
            self.origin.redundant
        ))
    }

    /// Load `repo_root/.thruline/claims.yml` **on top of** the built-in table.
    ///
    /// A malformed override is an **error**, never a silent fall back to the
    /// defaults: the reason someone writes this file is that the built-in table
    /// misses something of theirs, and ignoring it quietly would drop exactly
    /// the claims they asked to have caught.
    ///
    /// # F4 — this used to REPLACE the built-in table
    ///
    /// Measured on `b5335c0`. A four-line committed file whose only pattern was
    /// a phrase no one has ever said took a session from *1 unverifiable claim,
    /// conclusion `neutral`, badge `partial`* to *0 claims, conclusion
    /// `success`, badge `verified`* — and the file appeared nowhere in the
    /// signed payload. An unsigned repository file could choose the words a
    /// reader is shown.
    ///
    /// The operator's ruling, implemented here: **union, never replacement**,
    /// and the effective table's digest is disclosed.
    ///
    /// # Union alone is not enough, and this is the part worth reading
    ///
    /// [`crate::extract`] takes the FIRST pattern that matches and then blocks
    /// every later pattern of the same kind. So appending the repository's rows
    /// leaves one route open: a row of the same kind, carrying a phrase the
    /// built-in table already matches, at a *different scope*. §6.3 lets a
    /// narrowed test run support a `named` claim it may not support for `all`,
    /// so re-scoping `tests_pass/all` to `tests_pass/named` moves a verdict UP
    /// from a file nobody signed.
    ///
    /// Two independent guards:
    ///
    /// 1. **The built-in rows always occupy the front of the effective order**,
    ///    so for any sentence a built-in row can match, the built-in row wins.
    /// 2. **A repository row whose phrase contains a built-in phrase of the
    ///    same kind at a different scope is a hard error** ([`TableError::Shadow`]).
    ///    Such a row can match nothing the built-in does not already match, so
    ///    its only possible effect is the scope.
    ///
    /// A row that restates a built-in exactly — same kind, phrase and scope —
    /// is redundant, not hostile: it is counted and dropped, because refusing
    /// it would make the file unusable for anyone who copies a built-in row
    /// before editing it.
    pub fn load(repo_root: &std::path::Path) -> Result<Table, TableError> {
        let path = repo_root.join(Self::PATH);
        if !path.is_file() {
            return Ok(Table::default());
        }
        let display = path.display().to_string();
        let text = std::fs::read_to_string(&path).map_err(|e| TableError::Syntax {
            path: display.clone(),
            line: 0,
            message: e.to_string(),
        })?;
        let rows = parse_rows(&text, &display)?;
        // Errors name the absolute path, because a human has to open the file.
        // The DISCLOSURE names the relative one: it is signed into a receipt a
        // reviewer reads on another machine, and a home directory in there is
        // both a leak and a difference between two identical tables.
        Table::default().extended_with(rows, &display, Self::PATH)
    }

    /// The union, with the anti-shadow rule. See [`Table::load`].
    fn extended_with(
        mut self,
        rows: Vec<(Pattern, usize)>,
        path: &str,
        disclosed_as: &str,
    ) -> Result<Table, TableError> {
        let base = self.patterns.clone();
        let mut origin = Origin {
            repo_path: Some(disclosed_as.to_string()),
            added: 0,
            redundant: 0,
        };
        for (row, line) in rows {
            if base
                .iter()
                .any(|d| d.kind == row.kind && d.phrase == row.phrase && d.scope == row.scope)
            {
                origin.redundant += 1;
                continue;
            }
            if let Some(d) = base.iter().find(|d| {
                d.kind == row.kind && d.scope != row.scope && row.phrase.contains(&d.phrase)
            }) {
                return Err(TableError::Shadow(Box::new(Shadow {
                    path: path.to_string(),
                    line,
                    kind: row.kind.as_str(),
                    phrase: row.phrase,
                    repo_scope: row.scope.as_str(),
                    default_phrase: d.phrase.clone(),
                    default_scope: d.scope.as_str(),
                })));
            }
            // Appended, so the built-in rows keep the front of the order.
            self.patterns.push(row);
            origin.added += 1;
        }
        self.origin = origin;
        Ok(self)
    }

    /// Parse the documented subset. Anything else is an error.
    ///
    /// The result carries no [`Origin`]: parsing text is not the same act as a
    /// repository extending the built-in table, and only [`Table::load`] may
    /// produce the latter.
    pub fn parse(text: &str, path: &str) -> Result<Table, TableError> {
        Ok(Table {
            patterns: parse_rows(text, path)?
                .into_iter()
                .map(|(p, _)| p)
                .collect(),
            origin: Origin::default(),
        })
    }
}

/// The parser proper, keeping each row's line number so a refusal can point at
/// it. One parser, so the built-in table and a repository file are read by
/// exactly the same code.
fn parse_rows(text: &str, path: &str) -> Result<Vec<(Pattern, usize)>, TableError> {
    {
        let syntax = |line: usize, message: &str| TableError::Syntax {
            path: path.to_string(),
            line,
            message: message.to_string(),
        };
        let mut patterns = Vec::new();
        let mut version_seen = false;
        let mut in_patterns = false;
        let mut kind: Option<ClaimKind> = None;
        let mut scope = Scope::All;
        let mut in_phrases = false;

        for (i, raw) in text.lines().enumerate() {
            let line_no = i + 1;
            let line = match raw.find(" #") {
                Some(at) if !raw.trim_start().starts_with('"') => &raw[..at],
                _ => raw,
            };
            if line.trim().is_empty() || line.trim_start().starts_with('#') {
                continue;
            }
            let body = line.trim();

            if let Some(rest) = body.strip_prefix("version:") {
                let version = rest.trim().to_string();
                if version != "1" {
                    return Err(TableError::Version {
                        path: path.to_string(),
                        version,
                    });
                }
                version_seen = true;
                continue;
            }
            if body == "patterns:" {
                in_patterns = true;
                continue;
            }
            if !in_patterns {
                return Err(syntax(line_no, "expected `version:` or `patterns:`"));
            }

            if let Some(rest) = body.strip_prefix("- kind:") {
                let name = rest.trim();
                kind = Some(
                    ClaimKind::parse(name).ok_or_else(|| TableError::UnknownKind {
                        path: path.to_string(),
                        line: line_no,
                        name: name.to_string(),
                    })?,
                );
                scope = Scope::All;
                in_phrases = false;
                continue;
            }
            if let Some(rest) = body.strip_prefix("scope:") {
                scope = match rest.trim() {
                    "all" => Scope::All,
                    "changed" => Scope::Changed,
                    "named" => Scope::Named,
                    other => {
                        return Err(syntax(
                            line_no,
                            &format!("{other:?} is not a scope (all|changed|named)"),
                        ))
                    }
                };
                continue;
            }
            if body == "phrases:" {
                in_phrases = true;
                continue;
            }
            if let Some(rest) = body.strip_prefix("- ") {
                if !in_phrases {
                    return Err(syntax(line_no, "a phrase appeared before `phrases:`"));
                }
                let Some(kind) = kind else {
                    return Err(syntax(line_no, "a phrase appeared before `- kind:`"));
                };
                let phrase = rest.trim().trim_matches('"').to_ascii_lowercase();
                if phrase.is_empty() {
                    return Err(syntax(line_no, "an empty phrase matches everything"));
                }
                patterns.push((
                    Pattern {
                        kind,
                        phrase,
                        scope,
                    },
                    line_no,
                ));
                continue;
            }
            return Err(syntax(line_no, "unrecognised line"));
        }

        if !version_seen {
            return Err(syntax(0, "no `version:` key"));
        }
        if patterns.is_empty() {
            return Err(syntax(0, "the table has no patterns"));
        }
        Ok(patterns)
    }
}

/// Domain tag for [`Table::digest`]. Separates a claims-table digest from every
/// other sha256 in the system.
const DIGEST_DOMAIN: &str = "thruline.claims-table.v1";

/// Normalise a sentence for matching: lower case, collapsed whitespace,
/// smart punctuation folded.
///
/// Returns the normalised text **and** a map back to byte offsets in the
/// original, so a claim's span names the sentence the user actually wrote
/// rather than the one the matcher saw.
pub fn normalize(text: &str) -> (String, Vec<usize>) {
    let mut out = String::with_capacity(text.len());
    let mut offsets = Vec::with_capacity(text.len());
    let mut last_was_space = false;
    for (idx, ch) in text.char_indices() {
        let mapped = match ch {
            '\u{2018}' | '\u{2019}' => '\'',
            '\u{201C}' | '\u{201D}' => '"',
            '\u{2014}' | '\u{2013}' => '-',
            c if c.is_whitespace() => ' ',
            c => c.to_ascii_lowercase(),
        };
        if mapped == ' ' {
            if last_was_space {
                continue;
            }
            last_was_space = true;
        } else {
            last_was_space = false;
        }
        out.push(mapped);
        offsets.push(idx);
    }
    offsets.push(text.len());
    (out, offsets)
}

/// Is the phrase at `at` negated by the clause immediately before it?
///
/// See the module docs for why the search is clause-scoped rather than
/// sentence-wide.
pub fn is_negated(normalised: &str, at: usize) -> bool {
    let before = &normalised[..at];
    let clause_start = CLAUSE_BOUNDARIES
        .iter()
        .filter_map(|b| before.rfind(b).map(|i| i + b.len()))
        .max()
        .unwrap_or(0);
    let window = &before[clause_start..];
    NEGATION_CUES.iter().any(|cue| window.contains(cue))
}

/// Split a message into sentences, keeping each one's byte offset.
///
/// Splits on `.`, `!`, `?`, newlines and list-item boundaries. Deliberately
/// simple: an over-long "sentence" costs a wider span, while an under-split one
/// would let a negation cue in a neighbouring sentence suppress a real claim.
pub fn sentences(text: &str) -> Vec<(usize, &str)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        let is_break = matches!(c, b'.' | b'!' | b'?' | b'\n' | b';');
        if is_break {
            // A period *inside a token* is not a sentence end: `README.md`,
            // `verifier.rs`, `example.com`, `1.2.3`, `e.g`. The old guard only
            // covered digit.digit, so every filename split a sentence in two
            // and the halves were reported as separate claims.
            let next_is_word = bytes
                .get(i + 1)
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_');
            let prev_is_word =
                i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
            if c == b'.' && next_is_word && prev_is_word {
                i += 1;
                continue;
            }
            // Closing markup belongs to the sentence it closes. `...done.**`
            // otherwise ends the sentence at the `.` and hands the orphaned
            // `**` to the next one, which then starts `"** Its one remaining"`.
            //
            // Only after real terminal punctuation, and only when the run is
            // followed by whitespace or end of text — otherwise the `**`
            // *opening* the next line would be swallowed by the newline break.
            let mut end = i + 1;
            if c != b'\n' {
                let mut run = end;
                while matches!(
                    bytes.get(run),
                    Some(b'*')
                        | Some(b'`')
                        | Some(b'_')
                        | Some(b'~')
                        | Some(b')')
                        | Some(b']')
                        | Some(b'"')
                        | Some(b'\'')
                ) {
                    run += 1;
                }
                if run > end && bytes.get(run).is_none_or(|b| b.is_ascii_whitespace()) {
                    end = run;
                }
            }
            let piece = &text[start..end];
            if !piece.trim().is_empty() {
                out.push((start, piece));
            }
            start = end;
        }
        i += 1;
    }
    if start < text.len() && !text[start..].trim().is_empty() {
        out.push((start, &text[start..]));
    }
    out
}

/// Qualifiers that narrow an otherwise-unqualified claim.
pub fn scope_of(sentence_normalised: &str, table_scope: Scope) -> ClaimArgs {
    let named_cues = [
        "the failing test",
        "that test",
        "this test",
        "the new test",
        "the affected test",
    ];
    if named_cues.iter().any(|c| sentence_normalised.contains(c)) {
        return ClaimArgs {
            scope: Scope::Named,
            names: Vec::new(),
        };
    }
    if sentence_normalised.contains("changed test")
        || sentence_normalised.contains("affected tests")
        || sentence_normalised.contains("the tests i changed")
    {
        return ClaimArgs {
            scope: Scope::Changed,
            names: Vec::new(),
        };
    }
    ClaimArgs {
        scope: table_scope,
        names: Vec::new(),
    }
}

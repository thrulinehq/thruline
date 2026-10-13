//! The closed-schema reader for `.thruline/policy.yml`.
//!
//! # The subset, stated
//!
//! - two-space indentation, nesting only (no tabs — a tab is an error, not a
//!   guess about width);
//! - `key: value` where value is a scalar, empty (a nested map follows), or an
//!   inline flow list `["a", "b"]` or flow map `{ paths: ["a"] }`;
//! - `#` starts a comment at the start of a line or after whitespace;
//! - booleans are `true`/`false` **only**. `yes`, `no`, `on`, `off` are errors,
//!   because YAML's implicit typing is where a policy stops saying what its
//!   author wrote.
//!
//! Not supported, and refused by name: anchors and aliases, merge keys, block
//! sequences (`- item`), multi-document streams, block scalars, and quoted keys.
//! Each produces an error that says which construct and which line, so a real
//! YAML file that uses one is **rejected rather than half-read**.

use std::collections::BTreeMap;

use crate::{ClaimRule, DoneTemplate, Policy, Promises, Reexec, ReexecMode, StopGate};

/// A parsed value. Deliberately tiny: three shapes and no dynamic typing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Scalar(String),
    List(Vec<String>),
    Map(BTreeMap<String, Node>),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("line {line}: {msg}")]
    Syntax { line: usize, msg: String },
    #[error(
        "line {line}: unknown key `{key}`{}. The policy schema is closed: an \
         unrecognised key is refused rather than ignored, so a policy written \
         for a newer ThruLine fails here instead of quietly enforcing less than \
         its author wrote.",
        if .under.is_empty() { String::new() } else { format!(" under `{}`", .under) }
    )]
    UnknownKey {
        line: usize,
        key: String,
        under: String,
    },
    #[error("line {line}: `{construct}` is not in the supported subset. {why}")]
    Unsupported {
        line: usize,
        construct: &'static str,
        why: &'static str,
    },
    #[error("`{key}`: expected {want}, found `{found}`")]
    Type {
        key: String,
        want: &'static str,
        found: String,
    },
    #[error("`version: {found}` is not a policy version this build understands (expected 1)")]
    Version { found: String },
}

/// Every key the schema permits, by path. **The allowlist is the schema.**
///
/// It is a flat list of dotted paths rather than a nested structure so a test
/// can read it and assert what is absent — specifically that no key can
/// reference an extractor, a score or a confidence.
pub const SCHEMA: &[&str] = &[
    "version",
    "require_receipt",
    "claims",
    // `*` is the claim kind, which is a §6 verifier's name and therefore
    // user-supplied. Every key *under* it is schema.
    "claims.*",
    "claims.*.required_when",
    "claims.*.required_when.paths",
    "claims.*.require_reexecuted",
    "claims.*.require_evidence",
    "claims.*.require_human_ack",
    "claims.*.min_changed_line_coverage",
    "claims.*.allow_skips",
    "promises",
    "promises.block_on_open",
    "promises.comment_open",
    "stop_gate",
    "stop_gate.enabled",
    "stop_gate.max_stop_blocks",
    "reexec",
    "reexec.mode",
    "reexec.reruns",
    "reexec.image",
    "decisions",
    "decisions.require_hold",
    "decisions.on_unverifiable",
    "decisions.on_stale",
    // A name is one plain, nonempty key. Each template has exactly one
    // inline list of lines; block YAML sequences remain unsupported.
    "templates",
    "templates.*",
    "templates.*.lines",
];

struct Line {
    no: usize,
    indent: usize,
    key: String,
    value: String,
}

fn lines_of(text: &str) -> Result<Vec<Line>, ParseError> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let no = i + 1;
        if raw.contains('\t') {
            return Err(ParseError::Unsupported {
                line: no,
                construct: "a tab character",
                why: "indentation must be spaces; a tab's width is a guess and a \
                      policy file must not depend on one.",
            });
        }
        let trimmed = raw.trim_end();
        let without_comment = strip_comment(trimmed);
        if without_comment.trim().is_empty() {
            continue;
        }
        let indent = without_comment.len() - without_comment.trim_start().len();
        if !indent.is_multiple_of(2) {
            return Err(ParseError::Syntax {
                line: no,
                msg: format!("indent of {indent} space(s); the subset uses two per level"),
            });
        }
        let body = without_comment.trim_start();
        for (construct, why) in [
            ("- ", "block sequences are not in the subset; use an inline list, e.g. `[\"a\", \"b\"]`."),
            ("&", "anchors are not in the subset; they make a policy's meaning depend on another line."),
            ("*", "aliases are not in the subset; write the value out."),
            ("<<", "merge keys are not in the subset; they hide what a policy inherits."),
            ("---", "multi-document streams are not in the subset; one file is one policy."),
        ] {
            if body.starts_with(construct) {
                return Err(ParseError::Unsupported {
                    line: no,
                    construct: match construct {
                        "- " => "a block sequence item",
                        "&" => "an anchor",
                        "*" => "an alias",
                        "<<" => "a merge key",
                        _ => "a document separator",
                    },
                    why,
                });
            }
        }
        let Some((key, value)) = body.split_once(':') else {
            return Err(ParseError::Syntax {
                line: no,
                msg: format!("expected `key: value`, found `{body}`"),
            });
        };
        let key = key.trim();
        if key.starts_with('"') || key.starts_with('\'') {
            return Err(ParseError::Unsupported {
                line: no,
                construct: "a quoted key",
                why: "policy keys are plain identifiers.",
            });
        }
        if key.is_empty() {
            return Err(ParseError::Syntax {
                line: no,
                msg: "empty key".to_string(),
            });
        }
        for (construct, why) in [
            ("|", "block scalars are not in the subset."),
            (">", "folded scalars are not in the subset."),
        ] {
            if value.trim() == construct {
                return Err(ParseError::Unsupported {
                    line: no,
                    construct: "a block scalar",
                    why,
                });
            }
        }
        out.push(Line {
            no,
            indent,
            key: key.to_string(),
            value: value.trim().to_string(),
        });
    }
    Ok(out)
}

/// Removes a `#` comment, respecting quotes so a `#` inside a glob survives.
fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut quote: Option<u8> = None;
    for (i, b) in bytes.iter().enumerate() {
        match (quote, b) {
            (None, b'"') | (None, b'\'') => quote = Some(*b),
            (Some(q), c) if q == *c => quote = None,
            (None, b'#') if i == 0 || bytes[i - 1].is_ascii_whitespace() => return &line[..i],
            _ => {}
        }
    }
    line
}

/// Builds the tree, checking every key against [`SCHEMA`] as it goes.
fn tree(lines: &[Line], at: usize, indent: usize, path: &str) -> Result<(Node, usize), ParseError> {
    let mut map: BTreeMap<String, Node> = BTreeMap::new();
    let mut i = at;
    while i < lines.len() {
        let line = &lines[i];
        if line.indent < indent {
            break;
        }
        if line.indent > indent {
            return Err(ParseError::Syntax {
                line: line.no,
                msg: "unexpected indent".to_string(),
            });
        }
        let child_path = if path.is_empty() {
            line.key.clone()
        } else {
            format!("{path}.{}", line.key)
        };
        // A claim kind is a user-chosen name; everything else is schema.
        let schema_path = generalise(&child_path);
        if !SCHEMA.contains(&schema_path.as_str()) {
            return Err(ParseError::UnknownKey {
                line: line.no,
                key: line.key.clone(),
                under: path.to_string(),
            });
        }
        if (path == "templates" || path.starts_with("templates.")) && map.contains_key(&line.key) {
            return Err(ParseError::Syntax {
                line: line.no,
                msg: format!("duplicate template key `{}`", line.key),
            });
        }
        // Inline maps skip the tree walk's path allowlist. Keep template
        // structure nested so every field reaches the closed schema check.
        if (child_path == "templates" || path == "templates") && !line.value.is_empty() {
            return Err(ParseError::Type {
                key: child_path,
                want: "a nested map (two-space indentation)",
                found: line.value.clone(),
            });
        }
        if line.value.is_empty() {
            let (child, next) = tree(lines, i + 1, indent + 2, &child_path)?;
            map.insert(line.key.clone(), child);
            i = next;
        } else {
            map.insert(line.key.clone(), scalar_or_flow(line)?);
            i += 1;
        }
    }
    Ok((Node::Map(map), i))
}

/// `claims.tests_pass.allow_skips` → `claims.*.allow_skips`.
fn generalise(path: &str) -> String {
    let parts: Vec<&str> = path.split('.').collect();
    if parts.len() >= 2 && (parts[0] == "claims" || parts[0] == "templates") {
        let mut out = vec![parts[0], "*"];
        out.extend(parts.iter().skip(2));
        return out.join(".");
    }
    path.to_string()
}

fn scalar_or_flow(line: &Line) -> Result<Node, ParseError> {
    let v = line.value.as_str();
    if let Some(inner) = v.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        if inner.trim().is_empty() {
            return Ok(Node::List(Vec::new()));
        }
        return Ok(Node::List(
            split_flow(inner)
                .into_iter()
                .map(|p| unquote(p.trim()).to_string())
                .collect(),
        ));
    }
    if let Some(inner) = v.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        let mut map = BTreeMap::new();
        for part in split_flow(inner) {
            let Some((k, val)) = part.split_once(':') else {
                return Err(ParseError::Syntax {
                    line: line.no,
                    msg: format!("expected `key: value` inside `{{ }}`, found `{part}`"),
                });
            };
            let inner_line = Line {
                no: line.no,
                indent: line.indent,
                key: k.trim().to_string(),
                value: val.trim().to_string(),
            };
            map.insert(k.trim().to_string(), scalar_or_flow(&inner_line)?);
        }
        return Ok(Node::Map(map));
    }
    Ok(Node::Scalar(unquote(v).to_string()))
}

/// Splits a flow map's body on commas that are not inside brackets or quotes.
fn split_flow(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut cur = String::new();
    for c in s.chars() {
        match (quote, c) {
            (None, '"') | (None, '\'') => {
                quote = Some(c);
                cur.push(c);
            }
            (Some(q), c2) if q == c2 => {
                quote = None;
                cur.push(c2);
            }
            (None, '[') | (None, '{') => {
                depth += 1;
                cur.push(c);
            }
            (None, ']') | (None, '}') => {
                depth -= 1;
                cur.push(c);
            }
            (None, ',') if depth == 0 => {
                out.push(std::mem::take(&mut cur));
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out.into_iter().filter(|p| !p.trim().is_empty()).collect()
}

fn unquote(s: &str) -> &str {
    for q in ['"', '\''] {
        if s.len() >= 2 && s.starts_with(q) && s.ends_with(q) {
            return &s[1..s.len() - 1];
        }
    }
    s
}

fn as_bool(key: &str, node: Option<&Node>, default: bool) -> Result<bool, ParseError> {
    match node {
        None => Ok(default),
        Some(Node::Scalar(s)) if s == "true" => Ok(true),
        Some(Node::Scalar(s)) if s == "false" => Ok(false),
        Some(Node::Scalar(s)) => Err(ParseError::Type {
            key: key.to_string(),
            want: "`true` or `false` (YAML's `yes`/`no`/`on`/`off` are refused: \
                   implicit typing is where a file stops saying what its author wrote)",
            found: s.clone(),
        }),
        Some(other) => Err(ParseError::Type {
            key: key.to_string(),
            want: "a boolean",
            found: format!("{other:?}"),
        }),
    }
}

fn as_u32(key: &str, node: Option<&Node>, default: u32) -> Result<u32, ParseError> {
    match node {
        None => Ok(default),
        Some(Node::Scalar(s)) => s.parse().map_err(|_| ParseError::Type {
            key: key.to_string(),
            want: "a non-negative integer",
            found: s.clone(),
        }),
        Some(other) => Err(ParseError::Type {
            key: key.to_string(),
            want: "a non-negative integer",
            found: format!("{other:?}"),
        }),
    }
}

impl Policy {
    /// Reads a policy document. **Every error names a line**, because the
    /// alternative is a maintainer guessing which of forty lines a gate is
    /// unhappy about.
    pub fn parse(text: &str) -> Result<Policy, ParseError> {
        let lines = lines_of(text)?;
        let (root, _) = tree(&lines, 0, 0, "")?;
        let Node::Map(root) = root else {
            return Err(ParseError::Syntax {
                line: 1,
                msg: "the document is not a map".to_string(),
            });
        };
        let mut policy = Policy::default();

        match root.get("version") {
            Some(Node::Scalar(v)) if v == "1" => policy.version = 1,
            None => {
                return Err(ParseError::Version {
                    found: "(absent)".to_string(),
                })
            }
            Some(Node::Scalar(v)) => {
                return Err(ParseError::Version { found: v.clone() });
            }
            Some(other) => {
                return Err(ParseError::Version {
                    found: format!("{other:?}"),
                })
            }
        }
        policy.require_receipt = as_bool("require_receipt", root.get("require_receipt"), false)?;
        policy.decisions = root
            .get("decisions")
            .map(crate::decisions::parse)
            .transpose()?;

        if let Some(node) = root.get("templates") {
            let Node::Map(templates) = node else {
                return Err(ParseError::Type {
                    key: "templates".to_string(),
                    want: "a map of named templates",
                    found: format!("{node:?}"),
                });
            };
            for (name, node) in templates {
                if name.is_empty()
                    || !name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                {
                    return Err(ParseError::Type {
                        key: format!("templates.{name}"),
                        want: "a nonempty name using letters, digits, `_` or `-`",
                        found: name.clone(),
                    });
                }
                let Node::Map(fields) = node else {
                    return Err(ParseError::Type {
                        key: format!("templates.{name}"),
                        want: "a map with `lines`",
                        found: format!("{node:?}"),
                    });
                };
                let lines = match fields.get("lines") {
                    Some(Node::List(lines)) => lines,
                    Some(other) => {
                        return Err(ParseError::Type {
                            key: format!("templates.{name}.lines"),
                            want: "an inline list of 1 to 40 nonempty lines, each at most 240 characters",
                            found: format!("{other:?}"),
                        });
                    }
                    None => {
                        return Err(ParseError::Type {
                            key: format!("templates.{name}.lines"),
                            want: "an inline list of 1 to 40 nonempty lines, each at most 240 characters",
                            found: "(absent)".to_string(),
                        });
                    }
                };
                if lines.is_empty()
                    || lines.len() > 40
                    || lines
                        .iter()
                        .any(|line| line.trim().is_empty() || line.chars().count() > 240)
                {
                    return Err(ParseError::Type {
                        key: format!("templates.{name}.lines"),
                        want:
                            "an inline list of 1 to 40 nonempty lines, each at most 240 characters",
                        found: format!(
                            "{} line(s), including an empty or overlong line",
                            lines.len()
                        ),
                    });
                }
                policy.templates.insert(
                    name.clone(),
                    DoneTemplate {
                        lines: lines.clone(),
                    },
                );
            }
        }

        if let Some(Node::Map(claims)) = root.get("claims") {
            for (kind, node) in claims {
                let Node::Map(fields) = node else {
                    return Err(ParseError::Type {
                        key: format!("claims.{kind}"),
                        want: "a map of rules",
                        found: format!("{node:?}"),
                    });
                };
                let required_when_paths = match fields.get("required_when") {
                    None => None,
                    Some(Node::Map(rw)) => match rw.get("paths") {
                        Some(Node::List(l)) => Some(l.clone()),
                        Some(Node::Scalar(s)) => Some(vec![s.clone()]),
                        _ => None,
                    },
                    Some(other) => {
                        return Err(ParseError::Type {
                            key: format!("claims.{kind}.required_when"),
                            want: "a map with `paths`",
                            found: format!("{other:?}"),
                        })
                    }
                };
                policy.claims.insert(
                    kind.clone(),
                    ClaimRule {
                        required_when_paths,
                        require_reexecuted: as_bool(
                            "require_reexecuted",
                            fields.get("require_reexecuted"),
                            false,
                        )?,
                        require_evidence: as_bool(
                            "require_evidence",
                            fields.get("require_evidence"),
                            false,
                        )?,
                        require_human_ack: as_bool(
                            "require_human_ack",
                            fields.get("require_human_ack"),
                            false,
                        )?,
                        min_changed_line_coverage: match fields.get("min_changed_line_coverage") {
                            None => None,
                            some => Some(as_u32("min_changed_line_coverage", some, 0)?),
                        },
                        // **The default is the strict one.** A policy that
                        // names a claim and says nothing about skips gets
                        // `allow_skips: false`, because a skipped test is
                        // absent evidence and absent evidence is not a pass.
                        allow_skips: as_bool("allow_skips", fields.get("allow_skips"), false)?,
                    },
                );
            }
        }

        if let Some(Node::Map(p)) = root.get("promises") {
            policy.promises = Promises {
                block_on_open: as_bool("block_on_open", p.get("block_on_open"), false)?,
                comment_open: as_bool("comment_open", p.get("comment_open"), true)?,
            };
        }
        if let Some(Node::Map(s)) = root.get("stop_gate") {
            policy.stop_gate = StopGate {
                enabled: as_bool("enabled", s.get("enabled"), false)?,
                max_stop_blocks: as_u32("max_stop_blocks", s.get("max_stop_blocks"), 2)?,
            };
        }
        if let Some(Node::Map(r)) = root.get("reexec") {
            policy.reexec = Reexec {
                mode: match r.get("mode") {
                    None => ReexecMode::Claimed,
                    Some(Node::Scalar(m)) if m == "claimed" => ReexecMode::Claimed,
                    Some(Node::Scalar(m)) if m == "full" => ReexecMode::Full,
                    Some(other) => {
                        return Err(ParseError::Type {
                            key: "reexec.mode".to_string(),
                            want: "`claimed` or `full`",
                            found: format!("{other:?}"),
                        })
                    }
                },
                reruns: as_u32("reruns", r.get("reruns"), 2)?,
                image: match r.get("image") {
                    Some(Node::Scalar(s)) => s.clone(),
                    None => "auto".to_string(),
                    Some(other) => {
                        return Err(ParseError::Type {
                            key: "reexec.image".to_string(),
                            want: "a string",
                            found: format!("{other:?}"),
                        })
                    }
                },
            };
        }
        Ok(policy)
    }
}

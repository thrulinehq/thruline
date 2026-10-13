//! `.thruline/redact.yml` — §4.6's extension point.
//!
//! # Why this is a strict parser and not a YAML library
//!
//! The failure mode that matters here is a config that *silently fails to
//! apply*: the user writes a rule for their organisation's token format, a
//! permissive parser accepts the document but mis-nests the rule, and the value
//! they asked to be masked ships instead. A forgiving parser makes that outcome
//! quiet, and quiet is the one thing this module cannot be.
//!
//! So this accepts one documented shape and **errors on anything else**,
//! including keys it does not recognise. A config that does not parse stops the
//! redactor from being built at all, rather than degrading to the defaults.
//!
//! ```yaml
//! version: 1
//! rules:
//!   - name: acme-token
//!     prefix: "ACME-"
//!     min_body: 24
//!     charset: alnum          # alnum | alnum_underscore | alnum_dash | token
//!   - name: internal-host
//!     literal: "vault.internal.acme.corp"
//! ```
//!
//! **Regular expressions are deliberately not supported.** A user-supplied
//! regex over untrusted text is an unbounded-backtracking hazard on the path
//! that runs before every hash, and it would make the cost of redaction a
//! property of a config file. `prefix` + `min_body` + `charset` expresses the
//! credential shapes this exists for; `literal` covers the rest.

use std::collections::BTreeSet;
use std::path::Path;

/// The config could not be used.
#[derive(Debug, thiserror::Error)]
pub enum RedactConfigError {
    #[error("reading {path}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{path}:{line}: {message}")]
    Syntax {
        path: String,
        line: usize,
        message: String,
    },
    #[error("{path}: unsupported version {version}; this build understands version 1")]
    Version { path: String, version: String },
}

/// One user-supplied rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomRule {
    /// A credential shape: `prefix` followed by at least `min_body` characters
    /// of `charset`.
    Prefixed {
        name: String,
        prefix: String,
        min_body: usize,
        charset: Charset,
    },
    /// An exact string.
    Literal { name: String, value: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Charset {
    Alnum,
    AlnumUnderscore,
    AlnumDash,
    /// Alphanumeric plus `_-+/=.` — the widest form, matching the core
    /// masker's own token charset.
    Token,
}

impl Charset {
    fn parse(raw: &str) -> Option<Charset> {
        match raw {
            "alnum" => Some(Charset::Alnum),
            "alnum_underscore" => Some(Charset::AlnumUnderscore),
            "alnum_dash" => Some(Charset::AlnumDash),
            "token" => Some(Charset::Token),
            _ => None,
        }
    }

    fn accepts(self, c: char) -> bool {
        match self {
            Charset::Alnum => c.is_ascii_alphanumeric(),
            Charset::AlnumUnderscore => c.is_ascii_alphanumeric() || c == '_',
            Charset::AlnumDash => c.is_ascii_alphanumeric() || c == '-',
            Charset::Token => {
                c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | '/' | '=' | '.')
            }
        }
    }
}

/// The parsed `.thruline/redact.yml`, or nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RedactConfig {
    rules: Vec<CustomRule>,
}

impl RedactConfig {
    pub const PATH: &'static str = ".thruline/redact.yml";

    pub fn rules(&self) -> &[CustomRule] {
        &self.rules
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Load from `repo_root/.thruline/redact.yml`. Absent is not an error.
    pub fn load(repo_root: &Path) -> Result<RedactConfig, RedactConfigError> {
        let path = repo_root.join(Self::PATH);
        if !path.is_file() {
            return Ok(RedactConfig::default());
        }
        let display = path.display().to_string();
        let text = std::fs::read_to_string(&path).map_err(|source| RedactConfigError::Io {
            path: display.clone(),
            source,
        })?;
        RedactConfig::parse(&text, &display)
    }

    /// Parse the documented subset. Anything else is an error.
    pub fn parse(text: &str, path: &str) -> Result<RedactConfig, RedactConfigError> {
        let syntax = |line: usize, message: &str| RedactConfigError::Syntax {
            path: path.to_string(),
            line,
            message: message.to_string(),
        };

        let mut rules: Vec<CustomRule> = Vec::new();
        let mut version_seen = false;
        let mut in_rules = false;
        let mut pending: Option<PartialRule> = None;
        let mut pending_line = 0usize;

        for (i, raw) in text.lines().enumerate() {
            let line_no = i + 1;
            let without_comment = strip_comment(raw);
            if without_comment.trim().is_empty() {
                continue;
            }
            let indent = without_comment.len() - without_comment.trim_start().len();
            let body = without_comment.trim();

            if indent == 0 {
                if let Some(rest) = body.strip_prefix("version:") {
                    let version = rest.trim().to_string();
                    if version != "1" {
                        return Err(RedactConfigError::Version {
                            path: path.to_string(),
                            version,
                        });
                    }
                    version_seen = true;
                    continue;
                }
                if body == "rules:" {
                    in_rules = true;
                    continue;
                }
                return Err(syntax(line_no, "expected `version:` or `rules:`"));
            }

            if !in_rules {
                return Err(syntax(line_no, "indented content before `rules:`"));
            }

            let (is_new_item, entry) = match body.strip_prefix("- ") {
                Some(rest) => (true, rest.trim()),
                None => (false, body),
            };
            if is_new_item {
                if let Some(partial) = pending.take() {
                    rules.push(partial.finish(path, pending_line)?);
                }
                pending = Some(PartialRule::default());
                pending_line = line_no;
            }
            let Some(partial) = pending.as_mut() else {
                return Err(syntax(line_no, "field outside a `- ` rule item"));
            };
            let (key, value) = entry
                .split_once(':')
                .ok_or_else(|| syntax(line_no, "expected `key: value`"))?;
            let value = unquote(value.trim());
            match key.trim() {
                "name" => partial.name = Some(value),
                "prefix" => partial.prefix = Some(value),
                "literal" => partial.literal = Some(value),
                "min_body" => {
                    partial.min_body = Some(
                        value
                            .parse()
                            .map_err(|_| syntax(line_no, "min_body must be a number"))?,
                    )
                }
                "charset" => {
                    partial.charset = Some(Charset::parse(&value).ok_or_else(|| {
                        syntax(
                            line_no,
                            "charset must be alnum, alnum_underscore, alnum_dash or token",
                        )
                    })?)
                }
                other => {
                    return Err(syntax(
                        line_no,
                        &format!(
                            "unknown key `{other}` — this parser accepts only the documented \
                             shape, deliberately, so a mis-typed rule cannot be silently ignored"
                        ),
                    ))
                }
            }
        }
        if let Some(partial) = pending.take() {
            rules.push(partial.finish(path, pending_line)?);
        }
        if !version_seen {
            return Err(syntax(1, "missing `version: 1`"));
        }
        Ok(RedactConfig { rules })
    }

    /// Apply every rule.
    pub(crate) fn apply(
        &self,
        text: &str,
        applied: &mut u64,
        families: &mut BTreeSet<String>,
    ) -> String {
        let mut out = text.to_string();
        for rule in &self.rules {
            out = match rule {
                CustomRule::Literal { name, value } => {
                    if value.is_empty() || !out.contains(value.as_str()) {
                        continue;
                    }
                    *applied += out.matches(value.as_str()).count() as u64;
                    families.insert(format!("custom:{name}"));
                    out.replace(value.as_str(), &format!("[masked: custom:{name}]"))
                }
                CustomRule::Prefixed {
                    name,
                    prefix,
                    min_body,
                    charset,
                } => apply_prefixed(&out, name, prefix, *min_body, *charset, applied, families),
            };
        }
        out
    }
}

fn apply_prefixed(
    text: &str,
    name: &str,
    prefix: &str,
    min_body: usize,
    charset: Charset,
    applied: &mut u64,
    families: &mut BTreeSet<String>,
) -> String {
    if prefix.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(prefix) {
        let (before, from) = rest.split_at(at);
        let after_prefix = &from[prefix.len()..];
        let body_len: usize = after_prefix
            .chars()
            .take_while(|c| charset.accepts(*c))
            .map(char::len_utf8)
            .sum();
        let body_chars = after_prefix[..body_len].chars().count();
        out.push_str(before);
        if body_chars >= min_body {
            out.push_str(&format!("[masked: custom:{name}]"));
            *applied += 1;
            families.insert(format!("custom:{name}"));
            rest = &after_prefix[body_len..];
        } else {
            out.push_str(prefix);
            rest = after_prefix;
        }
    }
    out.push_str(rest);
    out
}

#[derive(Default)]
struct PartialRule {
    name: Option<String>,
    prefix: Option<String>,
    literal: Option<String>,
    min_body: Option<usize>,
    charset: Option<Charset>,
}

impl PartialRule {
    fn finish(self, path: &str, line: usize) -> Result<CustomRule, RedactConfigError> {
        let syntax = |message: &str| RedactConfigError::Syntax {
            path: path.to_string(),
            line,
            message: message.to_string(),
        };
        let name = self.name.ok_or_else(|| syntax("rule has no `name`"))?;
        match (self.prefix, self.literal) {
            (Some(_), Some(_)) => Err(syntax("rule has both `prefix` and `literal`")),
            (None, None) => Err(syntax("rule has neither `prefix` nor `literal`")),
            (None, Some(value)) => Ok(CustomRule::Literal { name, value }),
            (Some(prefix), None) => Ok(CustomRule::Prefixed {
                name,
                prefix,
                // Default 16 rather than 0: a `prefix` with no body bound fires
                // on the bare literal wherever it appears, which is exactly the
                // A209-C3 defect that made `private_key_block` mask 70 real
                // construction-bid delimiters.
                min_body: self.min_body.unwrap_or(16),
                charset: self.charset.unwrap_or(Charset::Token),
            }),
        }
    }
}

/// Strip a trailing `#` comment that is not inside quotes.
fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut in_single = false;
    let mut in_double = false;
    for (i, b) in bytes.iter().enumerate() {
        match b {
            b'\'' if !in_double => in_single = !in_single,
            b'"' if !in_single => in_double = !in_double,
            b'#' if !in_single && !in_double => return &line[..i],
            _ => {}
        }
    }
    line
}

fn unquote(value: &str) -> String {
    let bytes = value.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        return value[1..value.len() - 1].to_string();
    }
    value.to_string()
}

//! Match the paths a person chose to leave out when a view is served or
//! exported. This is a presentation rule: it reads no file or ledger row and
//! never changes the sealed transcript, events or their derived judgments.
//!
//! The caller supplies the project's root and its already-read exclusion
//! view. Paths are resolved lexically against that root. A caller that needs
//! symlink-aware matching must resolve its path before asking this module.
//! The `generated` bit is also the caller's fact; this module does not guess
//! how a repository marks a file as generated.

use std::borrow::Cow;

use crate::capture_hold::{self, ExclusionView};
use crate::chain::sha256_hex;
use crate::error::{CoreError, Result};

/// The additive field a served document uses when one or more entries were
/// replaced. It is absent when zero were replaced, preserving no-op bytes.
pub const LEFT_OUT_FIELD: &str = "left_out_by_exclusion";

/// The fixed words shown in place of a matching entry.
pub fn marker(key: &str) -> String {
    format!("left out by your exclusion (`{key}`)")
}

/// A field for a view only when something was actually left out.
pub fn count_field(count: u64) -> Option<(&'static str, u64)> {
    (count > 0).then_some((LEFT_OUT_FIELD, count))
}

/// One active rule, in deterministic precedence order.
#[derive(Debug, Clone)]
struct Rule {
    key: String,
    components: Vec<String>,
}

/// Pure, project-relative matching of a read `HoldsView::exclusions` list.
#[derive(Debug, Clone)]
pub struct ExclusionPolicy {
    root: Vec<String>,
    root_path: String,
    rules: Vec<Rule>,
    fingerprint: String,
}

impl ExclusionPolicy {
    /// Compile the active keys only. Built-ins come first (`env`, `secrets`,
    /// `generated`), then typed patterns in lexical order. A malformed active
    /// key is an error rather than an exclusion silently not honoured.
    pub fn from_exclusions(project_root: &str, exclusions: &[ExclusionView]) -> Result<Self> {
        let root = absolute_components(project_root).ok_or_else(|| {
            CoreError::Config("an exclusion policy needs an absolute project root".into())
        })?;
        let mut keys: Vec<String> = exclusions
            .iter()
            .filter(|entry| entry.on)
            .map(|entry| entry.key.clone())
            .collect();
        keys.sort_by(|a, b| precedence(a).cmp(&precedence(b)).then_with(|| a.cmp(b)));
        keys.dedup();
        let mut rules = Vec::with_capacity(keys.len());
        for key in keys {
            capture_hold::check_pattern(&key)?;
            let components = if capture_hold::built_in(&key).is_some() {
                Vec::new()
            } else {
                let parts = relative_components(&key).ok_or_else(|| {
                    CoreError::Config(format!("excluded path {key:?} leaves the project root"))
                })?;
                if parts.is_empty() {
                    return Err(CoreError::Config(format!(
                        "excluded path {key:?} names no path"
                    )));
                }
                parts
            };
            rules.push(Rule { key, components });
        }
        let mut encoded = b"capture-exclusions-v1\0".to_vec();
        for rule in &rules {
            encoded.extend_from_slice(&(rule.key.len() as u64).to_le_bytes());
            encoded.extend_from_slice(rule.key.as_bytes());
        }
        Ok(Self {
            root_path: format!("/{}", root.join("/")),
            root,
            rules,
            fingerprint: sha256_hex(&encoded),
        })
    }

    /// Stable for the same active key set, regardless of recording history,
    /// input ordering or project root. Safe to add to an already project-scoped
    /// serialized-view or search-corpus cache key.
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// Lexically normalized absolute project root for caller-side path facts.
    pub fn project_root(&self) -> &str {
        &self.root_path
    }

    /// Whether there is any active path exclusion to apply.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Whether the caller needs to supply generated-file facts.
    pub fn has_generated(&self) -> bool {
        self.rules.iter().any(|rule| rule.key == "generated")
    }

    /// Which active exclusion covers one path. A relative path starts at the
    /// project root; an absolute path must be under it by whole components.
    /// `*` in a typed pattern matches within one path component only, and a
    /// named path also covers its descendants without matching a sibling that
    /// merely shares its text prefix.
    pub fn key_for_path(&self, path: &str, generated: bool) -> Option<&str> {
        let parts = self.project_relative(path)?;
        if parts.is_empty() {
            return None;
        }
        self.rules
            .iter()
            .find(|rule| match rule.key.as_str() {
                "env" => parts
                    .last()
                    .is_some_and(|name| name == ".env" || name.starts_with(".env.")),
                "secrets" => parts.iter().any(|part| part == "secrets"),
                "generated" => generated,
                _ => {
                    parts.len() >= rule.components.len()
                        && rule
                            .components
                            .iter()
                            .zip(&parts)
                            .all(|(pattern, part)| component_matches(pattern, part))
                }
            })
            .map(|rule| rule.key.as_str())
    }

    /// Find an excluded path mentioned in a command, quote or other text.
    /// Only path-shaped tokens are candidates: a separator or a dot in the
    /// token is required, and URL tokens are skipped. A bare word such as
    /// "secrets" is ambiguous prose, so it is not treated as a path. This
    /// cannot infer the caller's generated-file flag; generated-only policy
    /// needs a known path passed to `key_for_path`.
    pub fn key_in_text(&self, text: &str) -> Option<&str> {
        self.key_in_text_with_generated(text, |_| false)
    }

    /// The same token and quote scan when the caller has already established
    /// which candidate paths are generated. The predicate is called on whole
    /// path candidates, including quoted names containing spaces.
    pub fn key_in_text_with_generated<F>(&self, text: &str, is_generated: F) -> Option<&str>
    where
        F: Fn(&str) -> bool,
    {
        let mut offset = 0;
        while offset < text.len() {
            let Some(ch) = text[offset..].chars().next() else {
                break;
            };
            let next = offset + ch.len_utf8();
            let apostrophe_in_word = ch == '\''
                && text[..offset]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_alphanumeric);
            if matches!(ch, '\'' | '"' | '`') && !apostrophe_in_word {
                if let Some(close) = text[next..].find(ch) {
                    let end = next + close;
                    if let Some(key) = self.key_for_text_candidate(&text[next..end], &is_generated)
                    {
                        return Some(key);
                    }
                    // The whole quote may be prose about a path rather than
                    // one path with spaces. Search its interior only after
                    // trying the whole span, so a matching spaced path wins.
                    if quoted_prose(&text[next..end]) {
                        if let Some(key) = text[next..end]
                            .split(|inner: char| !path_token_char(inner))
                            .find_map(|token| self.key_for_text_candidate(token, &is_generated))
                        {
                            return Some(key);
                        }
                    }
                    offset = end + ch.len_utf8();
                    continue;
                }
            }
            if path_token_char(ch) {
                let start = offset;
                offset = next;
                while let Some(ch) = text[offset..].chars().next() {
                    if !path_token_char(ch) {
                        break;
                    }
                    offset += ch.len_utf8();
                }
                if let Some(key) = self.key_for_text_candidate(&text[start..offset], &is_generated)
                {
                    return Some(key);
                }
            } else {
                offset = next;
            }
        }
        None
    }

    /// Match a command's path-shaped tokens, then bare path arguments of a
    /// small set of commands whose positional arguments name filesystem
    /// entries. The verb itself and prose-like commands such as `echo` are
    /// not treated as paths.
    pub fn key_in_command(&self, command: &str) -> Option<&str> {
        self.key_in_command_with_generated(command, |_| false)
    }

    /// Command matching with caller-supplied generated-file facts. Bare
    /// positional arguments can name generated files even without a dot or
    /// separator, so the same predicate used for paths in prose is checked
    /// for path-taking command arguments too.
    pub fn key_in_command_with_generated<F>(&self, command: &str, is_generated: F) -> Option<&str>
    where
        F: Fn(&str) -> bool,
    {
        if let Some(key) = self.key_in_text_with_generated(command, &is_generated) {
            return Some(key);
        }
        command_path_arguments(command)
            .into_iter()
            .find_map(|path| self.key_for_path_or_reference(path, &is_generated))
    }

    /// Replace one matching path-bearing entry. A miss borrows the original
    /// string, so a view with no matching exclusions keeps its exact text.
    pub fn mark_entry<'a>(&self, path: &str, generated: bool, original: &'a str) -> Cow<'a, str> {
        match self.key_for_path(path, generated) {
            Some(key) => Cow::Owned(marker(key)),
            None => Cow::Borrowed(original),
        }
    }

    fn key_for_text_candidate<F>(&self, token: &str, is_generated: &F) -> Option<&str>
    where
        F: Fn(&str) -> bool,
    {
        // Trailing punctuation belongs to the sentence, not the mentioned
        // path. Direct file-path fields still go to `key_for_path` unchanged.
        let token = token.trim_end_matches(sentence_end);
        if token.contains("://")
            || token.contains('$')
            || token.starts_with('~')
            || !(token.contains('/') || token.contains('\\') || token.contains('.'))
        {
            return None;
        }
        self.key_for_path_or_reference(token, is_generated)
    }

    fn key_for_path_or_reference<F>(&self, path: &str, is_generated: &F) -> Option<&str>
    where
        F: Fn(&str) -> bool,
    {
        self.key_for_path(path, is_generated(path)).or_else(|| {
            path_without_line_suffix(path)
                .and_then(|file| self.key_for_path(file, is_generated(file)))
        })
    }

    fn project_relative(&self, path: &str) -> Option<Vec<String>> {
        if path.starts_with('/') {
            let absolute = absolute_components(path)?;
            absolute
                .starts_with(&self.root)
                .then(|| absolute[self.root.len()..].to_vec())
        } else {
            relative_components(path)
        }
    }
}

fn path_token_char(ch: char) -> bool {
    ch.is_alphanumeric()
        || matches!(
            ch,
            '/' | '\\' | '.' | '-' | '_' | '~' | '+' | '%' | '@' | ':' | '$'
        )
}

fn sentence_end(ch: char) -> bool {
    matches!(ch, '.' | ',' | ':' | ';' | '!' | '?')
}

/// A numeric `:line` or `:line:column` in a textual reference. An exact file
/// match is always attempted first, so a real colon-bearing filename keeps
/// its own meaning when the person chose that name explicitly.
pub fn path_without_line_suffix(path: &str) -> Option<&str> {
    let (before_last, last) = path.rsplit_once(':')?;
    let numeric = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
    if !numeric(last) {
        return None;
    }
    if let Some((before_line, line)) = before_last.rsplit_once(':') {
        if numeric(line) {
            return (!before_line.is_empty()).then_some(before_line);
        }
    }
    (!before_last.is_empty()).then_some(before_last)
}

/// A failed whole-quote match is ambiguous when a path itself has spaces.
/// Only scan words inside a quote when its opening word clearly introduces
/// prose; otherwise the whole quoted span remains one unmatched path.
fn quoted_prose(quote: &str) -> bool {
    let Some(first) = quote.split_whitespace().next() else {
        return false;
    };
    matches!(
        first.to_ascii_lowercase().as_str(),
        "i" | "we"
            | "you"
            | "he"
            | "she"
            | "they"
            | "please"
            | "the"
            | "a"
            | "an"
            | "fix"
            | "edit"
            | "edited"
            | "change"
            | "changed"
            | "create"
            | "created"
            | "delete"
            | "deleted"
            | "remove"
            | "removed"
            | "update"
            | "updated"
            | "read"
            | "look"
            | "inspect"
            | "review"
            | "open"
    )
}

fn command_words(command: &str) -> Vec<&str> {
    let mut words = Vec::new();
    let mut offset = 0;
    while offset < command.len() {
        let Some(ch) = command[offset..].chars().next() else {
            break;
        };
        if ch.is_whitespace() {
            offset += ch.len_utf8();
            continue;
        }
        if matches!(ch, '\'' | '"') {
            let start = offset + ch.len_utf8();
            if let Some(close) = command[start..].find(ch) {
                let end = start + close;
                words.push(&command[start..end]);
                offset = end + ch.len_utf8();
                continue;
            }
        }
        let start = offset;
        while let Some(ch) = command[offset..].chars().next() {
            if ch.is_whitespace() {
                break;
            }
            offset += ch.len_utf8();
        }
        words.push(&command[start..offset]);
    }
    words
}

/// Positional path arguments of simple file-reading or file-moving commands.
/// The caller can use these exact candidates to resolve Git attributes before
/// asking `key_in_command_with_generated` to match them.
pub fn command_path_arguments(command: &str) -> Vec<&str> {
    let words = command_words(command);
    let Some(verb) = words.first().and_then(|word| word.rsplit('/').next()) else {
        return Vec::new();
    };
    if !matches!(
        verb,
        "cat"
            | "less"
            | "more"
            | "head"
            | "tail"
            | "ls"
            | "stat"
            | "file"
            | "du"
            | "wc"
            | "cp"
            | "mv"
            | "rm"
    ) {
        return Vec::new();
    }
    words
        .into_iter()
        .skip(1)
        .filter(|word| !word.starts_with('-'))
        .map(|word| word.trim_end_matches(sentence_end))
        .filter(|word| !word.is_empty())
        .collect()
}

fn precedence(key: &str) -> u8 {
    match key {
        "env" => 0,
        "secrets" => 1,
        "generated" => 2,
        _ => 3,
    }
}

/// A lexical absolute path; `..` resolves without consulting the filesystem.
fn absolute_components(path: &str) -> Option<Vec<String>> {
    if !path.starts_with('/') {
        return None;
    }
    components(path)
}

/// A lexical relative path that is not allowed to climb above its root.
fn relative_components(path: &str) -> Option<Vec<String>> {
    if path.starts_with('/') || path.starts_with('\\') {
        return None;
    }
    components(path)
}

fn components(path: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                out.pop()?;
            }
            other => out.push(other.to_string()),
        }
    }
    Some(out)
}

/// `*` matches zero or more bytes inside this one component. Literal UTF-8
/// bytes are compared exactly; a star never consumes a separator because the
/// caller split the path into components first.
fn component_matches(pattern: &str, value: &str) -> bool {
    let p = pattern.as_bytes();
    let v = value.as_bytes();
    let (mut pi, mut vi) = (0, 0);
    let (mut star, mut after_star) = (None, 0);
    while vi < v.len() {
        if pi < p.len() && p[pi] == b'*' {
            star = Some(pi);
            pi += 1;
            after_star = vi;
        } else if pi < p.len() && p[pi] == v[vi] {
            pi += 1;
            vi += 1;
        } else if let Some(at) = star {
            after_star += 1;
            vi = after_star;
            pi = at + 1;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

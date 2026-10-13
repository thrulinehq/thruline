//! Deterministic, span-only proposals for standing decisions.
//!
//! This module reads already-masked person-authored text. It never accepts a
//! decision, writes a verdict, consults a model, or supplies words missing from
//! the source. The person fixes parameters before acceptance.

use serde::Serialize;

/// Same word normalization used when Change proposals compare restatements.
/// Kept local because standing-decision code lives entirely in new files.
fn normalized(text: &str) -> String {
    text.split_whitespace()
        .map(|word| {
            word.chars()
                .filter(|ch| ch.is_alphanumeric() || *ch == '\'')
                .collect::<String>()
                .to_ascii_lowercase()
        })
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

struct SentenceSpan {
    offset: usize,
    text: String,
    whole: bool,
}

fn begins_new_sentence(text: &str) -> bool {
    let line = text.split('\n').next().unwrap_or("").trim_start();
    if line.is_empty() {
        return true;
    }
    let marker = line
        .chars()
        .next()
        .filter(|ch| matches!(ch, '-' | '*' | '•'));
    let body = if let Some(marker) = marker {
        line[marker.len_utf8()..].trim_start()
    } else {
        line
    };
    let listed = marker.is_some();
    listed
        || body
            .chars()
            .next()
            .is_some_and(|ch| ch.is_uppercase() || ch.is_numeric())
        || opener(body).is_some()
}

fn ruling_label(text: &str) -> bool {
    let lower = text.trim().to_ascii_lowercase();
    lower
        .strip_prefix("founder ruling c")
        .is_some_and(|number| {
            !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn push_sentence(text: &str, from: usize, to: usize, out: &mut Vec<SentenceSpan>) {
    let raw = &text[from..to];
    let first = raw
        .char_indices()
        .find(|(_, ch)| {
            ch.is_alphanumeric() || matches!(ch, '\'' | '‘' | '’' | '"' | '“' | '”' | '`')
        })
        .map(|(at, _)| at)
        .unwrap_or(raw.len());
    let start = from + first;
    let body = text[start..to].trim_end_matches(['.', '!', '?', ';', ',', ' ', '\t', '\r', '\n']);
    if body.is_empty() {
        return;
    }
    let first_letter = body
        .chars()
        .find(|ch| !matches!(ch, '\'' | '‘' | '’' | '"' | '“' | '”' | '`'));
    out.push(SentenceSpan {
        offset: start,
        text: body.to_string(),
        whole: first_letter.is_some_and(|ch| ch.is_uppercase() || ch.is_numeric()),
    });
}

/// Split a prompt with byte offsets. Stops inside paths, versions, backticks
/// and quotations stay inside their sentence. A soft line wrap stays inside
/// too; a bullet or a new imperative starts another sentence.
fn sentence_spans(text: &str) -> Vec<SentenceSpan> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut backtick = false;
    let mut double_quote = false;
    let mut single_quote = false;
    for (index, ch) in text.char_indices() {
        let next = index + ch.len_utf8();
        if ch == '`' {
            backtick = !backtick;
            continue;
        }
        if !backtick {
            if matches!(ch, '"' | '“' | '”') {
                double_quote = !double_quote;
                continue;
            }
            if matches!(ch, '\'' | '‘' | '’') {
                let prev_word = text[..index]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_alphanumeric);
                let next_word = text[next..]
                    .chars()
                    .next()
                    .is_some_and(char::is_alphanumeric);
                if !(prev_word && next_word) {
                    single_quote = !single_quote;
                }
                continue;
            }
        }
        if backtick || double_quote || single_quote {
            continue;
        }
        if ch == ':'
            && ruling_label(&text[start..index])
            && opener(text[next..].trim_start()).is_some()
        {
            start = next;
            continue;
        }
        let boundary = match ch {
            '.' => {
                let next_ch = text[next..].chars().next();
                let word = text[start..next].split_whitespace().last().unwrap_or("");
                next_ch.is_none_or(char::is_whitespace)
                    && !matches!(
                        word.to_ascii_lowercase().as_str(),
                        "e.g." | "i.e." | "dr." | "mr."
                    )
            }
            '!' | '?' | ';' => true,
            '\n' => begins_new_sentence(&text[next..]),
            _ => false,
        };
        if boundary {
            push_sentence(text, start, next, &mut out);
            start = next;
        }
    }
    if start < text.len() {
        push_sentence(text, start, text.len(), &mut out);
    }
    out
}

/// Only these record origins may offer a standing decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    WorkPrompt,
    RuleFile,
    AcceptedCriterion,
    MarkedFinding,
    AgentProse,
    Handover,
    CommitMessage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Actor {
    Person,
    Repository,
    Agent,
    Model,
}

/// A text field and its typed receipt. The caller masks `text` before passing
/// it here. `raw_line` names the original record line, not a line in a copy.
#[derive(Debug, Clone, Copy)]
pub struct Source<'a> {
    pub kind: SourceKind,
    pub actor: Actor,
    pub actor_id: &'a str,
    pub work: bool,
    pub raw_file: &'a str,
    pub raw_line: u64,
    pub event_id: Option<&'a str>,
    pub text: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    NotPerson,
    UnsupportedSource,
    NotWorkPrompt,
    MissingReceipt,
    MissingActor,
}

/// A byte span into `Source.text`. Every displayed word in a proposal or note
/// can be checked against the masked source slice at this offset and length.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Receipt {
    pub source: SourceKind,
    pub actor_id: String,
    pub raw_file: String,
    pub raw_line: u64,
    pub event_id: Option<String>,
    pub offset: usize,
    pub length: usize,
}

/// Parameters here are suggestions, never accepted policy. `object`, each
/// `term`, and `scope_hint` are exact contiguous spans of `text`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Proposal {
    /// Closed kind code from the decisions catalogue (for example A1 or E1).
    pub kind: &'static str,
    pub text: String,
    pub object: String,
    pub terms: Vec<String>,
    pub scope_hint: Option<String>,
    pub receipt: Receipt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Note {
    pub text: String,
    pub reason: &'static str,
    pub receipt: Receipt,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Reading {
    pub proposals: Vec<Proposal>,
    pub notes: Vec<Note>,
}

#[derive(Clone, Copy)]
enum Family {
    Absence,
    Preserve,
    Untouched,
    WithinPaths,
    NoTestEdits,
    CanonicalTerm,
    ForbiddenWords,
    EntityName,
    CommandPasses,
    Warnings,
}

#[derive(Clone, Copy)]
struct Opener {
    phrase: &'static str,
    family: Family,
}

// Longer phrases precede their prefixes. A new opener changes what the engine
// can offer, so additions belong in this closed table and its sentence tests.
const OPENERS: &[Opener] = &[
    Opener {
        phrase: "get rid of",
        family: Family::Absence,
    },
    Opener {
        phrase: "strip out",
        family: Family::Absence,
    },
    Opener {
        phrase: "no more",
        family: Family::Absence,
    },
    Opener {
        phrase: "never remove",
        family: Family::Preserve,
    },
    Opener {
        phrase: "must stay",
        family: Family::Preserve,
    },
    Opener {
        phrase: "leave",
        family: Family::Preserve,
    },
    Opener {
        phrase: "don't touch",
        family: Family::Untouched,
    },
    Opener {
        phrase: "do not touch",
        family: Family::Untouched,
    },
    Opener {
        phrase: "only touch",
        family: Family::WithinPaths,
    },
    Opener {
        phrase: "only in",
        family: Family::WithinPaths,
    },
    Opener {
        phrase: "stay within",
        family: Family::WithinPaths,
    },
    Opener {
        phrase: "don't modify",
        family: Family::NoTestEdits,
    },
    Opener {
        phrase: "do not modify",
        family: Family::NoTestEdits,
    },
    Opener {
        phrase: "call it",
        family: Family::CanonicalTerm,
    },
    Opener {
        phrase: "always say",
        family: Family::CanonicalTerm,
    },
    Opener {
        phrase: "use the name",
        family: Family::EntityName,
    },
    Opener {
        phrase: "never say",
        family: Family::ForbiddenWords,
    },
    Opener {
        phrase: "must pass",
        family: Family::CommandPasses,
    },
    Opener {
        phrase: "must build",
        family: Family::CommandPasses,
    },
    Opener {
        phrase: "must be green",
        family: Family::CommandPasses,
    },
    Opener {
        phrase: "no warnings",
        family: Family::Warnings,
    },
    Opener {
        phrase: "remove",
        family: Family::Absence,
    },
    Opener {
        phrase: "delete",
        family: Family::Absence,
    },
    Opener {
        phrase: "drop",
        family: Family::Absence,
    },
    Opener {
        phrase: "cancel",
        family: Family::Absence,
    },
    Opener {
        phrase: "kill",
        family: Family::Absence,
    },
    Opener {
        phrase: "no",
        family: Family::Absence,
    },
    Opener {
        phrase: "keep",
        family: Family::Preserve,
    },
];

fn starts_phrase(text: &str, phrase: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.starts_with(phrase)
        && lower[phrase.len()..]
            .chars()
            .next()
            .is_none_or(|next| next.is_whitespace() || next == ':')
}

fn opener(text: &str) -> Option<(Opener, &str)> {
    if starts_phrase(text, "no warnings") {
        return Some((
            Opener {
                phrase: "no warnings",
                family: Family::Warnings,
            },
            text["no ".len()..].trim_start(),
        ));
    }
    if text.len() >= "clippy clean".len() && text.eq_ignore_ascii_case("clippy clean") {
        return Some((
            Opener {
                phrase: "clippy clean",
                family: Family::CommandPasses,
            },
            &text[.."clippy".len()],
        ));
    }
    for candidate in OPENERS {
        if starts_phrase(text, candidate.phrase) {
            let object = text[candidate.phrase.len()..].trim_start_matches([' ', '\t', ':']);
            return Some((*candidate, object));
        }
    }
    None
}

fn kind_for(family: Family, object: &str) -> &'static str {
    match family {
        Family::Absence => {
            let plain = normalized(object);
            let words: Vec<&str> = plain.split(' ').collect();
            if object.starts_with('`') && object.ends_with('`') && !object.contains('/') {
                "A2"
            } else if words
                .iter()
                .any(|word| matches!(*word, "dependency" | "crate" | "sdk" | "package"))
            {
                "A3"
            } else if object.contains('/') || object.ends_with(".rs") || object.ends_with(".jsx") {
                "A4"
            } else {
                "A1"
            }
        }
        Family::Preserve => {
            if normalized(object).starts_with("test ") {
                "B2"
            } else {
                "B1"
            }
        }
        Family::Untouched => "D2",
        Family::WithinPaths => "D1",
        Family::NoTestEdits => {
            if normalized(object).starts_with("test") {
                "D3"
            } else {
                "D2"
            }
        }
        Family::CanonicalTerm => "G1",
        Family::ForbiddenWords => "G2",
        Family::EntityName => "G4",
        Family::CommandPasses => "E1",
        Family::Warnings => "E2",
    }
}

fn terms(object: &str) -> Vec<String> {
    let lower = object.to_ascii_lowercase();
    let mut cuts = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < lower.len() {
        let rest = &lower[index..];
        let separator = if rest.starts_with(" and ") {
            Some(5)
        } else if rest.starts_with(" or ") {
            Some(4)
        } else if rest.starts_with(", ") {
            Some(2)
        } else {
            None
        };
        if let Some(width) = separator {
            let term = object[start..index].trim();
            if !term.is_empty() {
                cuts.push(term.to_string());
            }
            index += width;
            start = index;
        } else {
            index += lower[index..].chars().next().map_or(1, char::len_utf8);
        }
    }
    let last = object[start..].trim();
    if !last.is_empty() {
        cuts.push(last.to_string());
    }
    cuts
}

fn scope_hint(object: &str) -> Option<String> {
    let lower = object.to_ascii_lowercase();
    for suffix in ["anywhere", "everywhere", "from the ui", "in the ui"] {
        if lower.ends_with(suffix) {
            let start = lower.len() - suffix.len();
            if start == 0 || lower[..start].ends_with(' ') {
                return Some(object[start..].to_string());
            }
        }
    }
    None
}

/// Closed rule-file names. Derived decision copies cannot become sources.
pub fn is_rule_file(path: &str) -> bool {
    if path.starts_with('/') || path.split('/').any(|part| matches!(part, "" | "." | "..")) {
        return false;
    }
    let basename = path.rsplit('/').next().unwrap_or("");
    if [
        "CLAUDE.md",
        "AGENTS.md",
        ".cursorrules",
        "GEMINI.md",
        ".windsurfrules",
    ]
    .contains(&basename)
    {
        return true;
    }
    for name in [".thruline/policy.yml", ".github/copilot-instructions.md"] {
        if path == name
            || path
                .strip_suffix(name)
                .is_some_and(|prefix| prefix.ends_with('/'))
        {
            return true;
        }
    }
    let rules = path
        .strip_prefix(".cursor/rules/")
        .or_else(|| path.split_once("/.cursor/rules/").map(|(_, tail)| tail));
    rules.is_some_and(|name| !name.contains('/') && name.len() > 4 && name.ends_with(".mdc"))
}

fn validate(source: &Source<'_>) -> Result<(), Refusal> {
    if source.actor != Actor::Person
        && !(source.actor == Actor::Repository && source.kind == SourceKind::RuleFile)
    {
        return Err(Refusal::NotPerson);
    }
    if source.actor_id.trim().is_empty() {
        return Err(Refusal::MissingActor);
    }
    if source.raw_file.trim().is_empty() || source.raw_line == 0 {
        return Err(Refusal::MissingReceipt);
    }
    match source.kind {
        SourceKind::WorkPrompt => {
            if !source.work {
                return Err(Refusal::NotWorkPrompt);
            }
            if source.event_id.is_none_or(|id| id.trim().is_empty()) {
                return Err(Refusal::MissingReceipt);
            }
        }
        SourceKind::RuleFile => {
            if !is_rule_file(source.raw_file) {
                return Err(Refusal::UnsupportedSource);
            }
        }
        SourceKind::AcceptedCriterion | SourceKind::MarkedFinding => {}
        SourceKind::AgentProse | SourceKind::Handover | SourceKind::CommitMessage => {
            return Err(Refusal::UnsupportedSource);
        }
    }
    Ok(())
}

fn receipt(source: &Source<'_>, offset: usize, text: &str) -> Receipt {
    let raw_line = if source.kind == SourceKind::RuleFile {
        source.raw_line
            + source.text[..offset]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count() as u64
    } else {
        source.raw_line
    };
    Receipt {
        source: source.kind,
        actor_id: source.actor_id.to_string(),
        raw_file: source.raw_file.to_string(),
        raw_line,
        event_id: source.event_id.map(str::to_string),
        offset,
        length: text.len(),
    }
}

/// A coordinating "and" separates clauses only when a second closed opener
/// follows it. A list of terms after one opener stays one proposal.
fn clauses(text: &str, offset: usize) -> Vec<(usize, &str)> {
    let lower = text.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut start = 0usize;
    loop {
        let mut split = None;
        for marker in [", and ", " and "] {
            let mut search = start;
            while let Some(found) = lower[search..].find(marker) {
                let at = search + found;
                let after = at + marker.len();
                if opener(&text[after..]).is_some() {
                    split = Some((at, after));
                    break;
                }
                search = after;
            }
            if split.is_some() {
                break;
            }
        }
        let Some((end, next)) = split else {
            let tail = text[start..].trim();
            if !tail.is_empty() {
                let leading = text[start..].len() - text[start..].trim_start().len();
                out.push((offset + start + leading, tail));
            }
            break;
        };
        let part = text[start..end].trim();
        if !part.is_empty() {
            let leading = text[start..end].len() - text[start..end].trim_start().len();
            out.push((offset + start + leading, part));
        }
        start = next;
    }
    out
}

/// Pure reading of one source field. Missing and uncheckable clauses stay
/// visible as notes, never weaker suggestions of a different kind.
pub fn propose(source: &Source<'_>) -> Result<Reading, Refusal> {
    validate(source)?;
    let mut reading = Reading::default();
    for sentence in sentence_spans(source.text) {
        if let Some(deferred) = super::deferred_grammar::read(&sentence.text) {
            reading.proposals.push(Proposal {
                kind: "H1",
                receipt: receipt(source, sentence.offset, &sentence.text),
                object: deferred.action.clone(),
                terms: vec![deferred.action],
                scope_hint: Some(deferred.condition),
                text: sentence.text,
            });
            continue;
        }
        for (offset, text) in clauses(&sentence.text, sentence.offset) {
            let Some((found, object)) = opener(text) else {
                reading.notes.push(Note {
                    receipt: receipt(source, offset, text),
                    text: text.to_string(),
                    reason: if sentence.whole {
                        "uncheckable_language"
                    } else {
                        "fragment"
                    },
                });
                continue;
            };
            let object = object.trim_end();
            if object.is_empty() {
                reading.notes.push(Note {
                    receipt: receipt(source, offset, text),
                    text: text.to_string(),
                    reason: "missing_object",
                });
                continue;
            }
            let kind = kind_for(found.family, object);
            reading.proposals.push(Proposal {
                kind,
                receipt: receipt(source, offset, text),
                object: object.to_string(),
                terms: terms(object),
                scope_hint: scope_hint(object),
                text: text.to_string(),
            });
        }
    }
    Ok(reading)
}

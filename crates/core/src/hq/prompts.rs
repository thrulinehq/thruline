//! Which `user_msg` rows a person typed, and the title a thread takes from one.
//!
//! Both rules are deterministic text predicates over the stored row. Neither
//! rewrites a word: a prompt is either a person's prompt or it is not, and a
//! title is a sentence the person wrote, selected and (when long) cut, never
//! paraphrased and never produced by a model.

/// Text a vendor or another agent put into the user channel. A `user_msg`
/// whose text (after leading whitespace) starts with one of these is not a
/// person's prompt. It stays in the session timeline; it never starts or
/// titles a thread.
///
/// The list is the union of the envelopes this codebase already treats as
/// non-human (`annotate.rs` `<task-notification>`, the vendor's continuation
/// preamble [`CONTINUATION_PREFIX`]) and the ones measured on the founder's own record on
/// 2026-10-07 (cross-session messages, scheduled tasks, local-command output,
/// Codex's injected context blocks).
/// Claude Code's own words at the head of a compacted or resumed session's
/// first message. Held here, not borrowed from the RB1 shadow module that also
/// spells it (`rb0_taxonomy.rs` licenses only adjudication and autopsy to reach
/// that module): HQ needs the vendor's string, not a session status.
pub const CONTINUATION_PREFIX: &str = "This session is being continued from a previous";

pub const INJECTED_PREFIXES: &[&str] = &[
    "<task-notification>",
    CONTINUATION_PREFIX,
    "<local-command-stdout>",
    "<local-command-stderr>",
    "<local-command-caveat>",
    "Caveat: The messages below were generated",
    "[Request interrupted by user",
    "<system-reminder>",
    "<user-prompt-submit-hook>",
    "Another Claude session sent a message:",
    "<cross-session-message",
    "<scheduled-task",
    "<tool_use_error>",
    // Claude Code's note beside an image it resized for a Read, written as a
    // `user` line with `isMeta: true` (measured on the founder's record,
    // 2026-10-07): "[Image: original 1512x2846, displayed at 1063x2000. …]".
    // A person's own pasted image is "[Image #1]", which this does not match.
    "[Image: original ",
    // Codex
    "<environment_context>",
    "<user_instructions>",
    "# AGENTS.md instructions",
    "<turn_aborted>",
    "<subagent_notification>",
    "<user_shell_command>",
    "<external_codex_apps_open_page>",
    "<recommended_plugins>",
    "<in-app-browser-context",
];

/// What one `user_msg` row is, as far as thread-building is concerned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptClass {
    /// A person's prompt. `text` is what the person typed (for a slash command
    /// with arguments, `/name args`).
    Human { text: String },
    /// A slash-command wrapper (`<command-name>/x</command-name>…`). Its
    /// arguments, when present, ARE the person's prompt — see
    /// [`classify`]. Without arguments it is a vendor control line.
    CommandWrapper,
    /// Vendor- or agent-injected text, or empty text.
    Injected,
}

/// Classify one `user_msg`.
///
/// `after_wrapper` is true when the previous `user_msg` in the same session
/// was a slash-command wrapper and nothing but `meta` rows lie between them:
/// Claude Code writes the expanded skill/command body as a second user line,
/// and that body is the tool's text, not the person's.
pub fn classify(text: &str, after_wrapper: bool) -> PromptClass {
    let trimmed = text.trim_start();
    if trimmed.trim().is_empty() {
        return PromptClass::Injected;
    }
    if trimmed.starts_with("<command-name>") || trimmed.starts_with("<command-message>") {
        let name = between(trimmed, "<command-name>", "</command-name>")
            .map(str::trim)
            .unwrap_or("");
        let args = between(trimmed, "<command-args>", "</command-args>")
            .map(str::trim)
            .unwrap_or("");
        if !args.is_empty() && !name.is_empty() {
            return PromptClass::Human {
                text: format!("{name} {args}"),
            };
        }
        return PromptClass::CommandWrapper;
    }
    if after_wrapper {
        return PromptClass::Injected;
    }
    if INJECTED_PREFIXES.iter().any(|p| trimmed.starts_with(p)) {
        return PromptClass::Injected;
    }
    // Markup and nothing else (`</image>` on its own line is a Codex
    // attachment terminator, not something a person said).
    if strip_tags(trimmed).trim().is_empty() {
        return PromptClass::Injected;
    }
    PromptClass::Human {
        text: text.to_string(),
    }
}

fn between<'a>(text: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = text.find(open)? + open.len();
    let end = text[start..].find(close)? + start;
    Some(&text[start..end])
}

/// Verbs that open an instruction. A sentence whose first word (after the
/// fillers in [`FILLERS`]) is one of these reads as an instruction.
pub const INSTRUCTION_VERBS: &[&str] = &[
    "fix",
    "add",
    "make",
    "build",
    "remove",
    "implement",
    "update",
    "rename",
    "refactor",
    "change",
    "create",
    "write",
    "move",
    "get",
    "let",
    "show",
    "load",
    "delete",
    "run",
    "test",
    "check",
    "investigate",
    "find",
    "replace",
    "use",
    "set",
    "put",
    "clean",
    "wire",
    "port",
    "merge",
    "ship",
    "finish",
    "continue",
    "start",
    "stop",
    "drop",
    "convert",
    "split",
    "extract",
    "support",
    "improve",
    "rewrite",
    "redesign",
    "review",
    "read",
    "look",
    "open",
    "do",
    "try",
    "commit",
    "push",
    "deploy",
    "install",
    "debug",
    "verify",
    "audit",
    "draft",
    "design",
    "prepare",
    "plan",
    "connect",
    "display",
    "render",
    "keep",
    "bring",
    "turn",
    "allow",
    "work",
    "ensure",
    "restore",
    "revert",
    "resume",
    "proceed",
    "execute",
    "go",
    "help",
    "explain",
    "list",
    "compare",
    "measure",
    "polish",
    "simplify",
];

/// Words skipped before the verb test (`please fix …`, `can you add …`). The
/// title still shows them: they are part of what the person wrote.
pub const FILLERS: &[&str] = &[
    "please", "pls", "can", "could", "would", "you", "now", "ok", "okay", "so", "and", "then",
    "also", "next", "first", "hey", "just", "ok,", "okay,", "so,", "now,",
];

/// Longest title, in characters, before it is cut and marked with `…`.
pub const TITLE_MAX: usize = 90;

/// Why a title is the sentence it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TitleBasis {
    /// The first sentence the person typed that names work ([`names_work`])
    /// and is not skippable ([`is_skippable`]).
    WorkSentence,
    /// The thread's first work prompt named no work; a later prompt's work
    /// sentence.
    WorkSentenceLaterPrompt,
    /// Nothing was typed that names work; a work sentence among the first
    /// [`PASTE_WORK_WINDOW`] sentences of a pasted block.
    PastedWorkSentence,
    /// No sentence named work; the first sentence that opens with an
    /// instruction verb (the pre-C26 rule).
    FallbackInstruction,
    /// No sentence named work or opened with an instruction verb; the first
    /// sentence.
    FallbackFirstSentence,
    /// Nothing was typed outside a pasted block; the paste's first heading.
    PastedHeading,
    /// Nothing was typed outside a pasted block and it has no heading; the
    /// first sentence of its first line.
    PastedFirstSentence,
    /// The prompt had no sentence to take (only tags or attachments).
    None,
}

/// Envelopes a desktop app wraps pasted or attached text in. The opening tag
/// may carry attributes (`<pasted_content id="27a7">`), and so may the closing
/// one (`</pasted_content id="27a7">`, as recorded). A block with no closing
/// tag runs to the end of the prompt.
pub const PASTE_TAGS: &[&str] = &[
    "pasted_content",
    "attached_file",
    "attachment",
    "pasted_text",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentKind {
    /// What the person typed.
    Typed,
    /// The inside of a pasted or attached block, wrapper tags removed.
    Pasted,
}

/// One part of a prompt.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Segment {
    pub kind: SegmentKind,
    pub text: String,
}

/// Find the next opening envelope at or after `from`: (start, end of tag, tag).
fn next_open(text: &str, from: usize) -> Option<(usize, usize, &'static str)> {
    PASTE_TAGS
        .iter()
        .filter_map(|tag| {
            let needle = format!("<{tag}");
            let mut at = from;
            while let Some(p) = text[at..].find(&needle) {
                let start = at + p;
                let after = &text[start + needle.len()..];
                if after.starts_with('>') || after.starts_with(' ') || after.starts_with('\n') {
                    let close = after.find('>')?;
                    return Some((start, start + needle.len() + close + 1, *tag));
                }
                at = start + needle.len();
            }
            None
        })
        .min_by_key(|(start, _, _)| *start)
}

/// A prompt split into what was typed and what was pasted. Whitespace-only
/// typed parts are dropped; each part is trimmed of surrounding blank space;
/// nothing else is altered. A prompt with no envelope is one typed segment.
pub fn segments(text: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut push = |kind: SegmentKind, body: &str| {
        let body = body.trim_matches(|c: char| c == '\n' || c == '\r' || c == ' ' || c == '\t');
        if !body.is_empty() {
            out.push(Segment {
                kind,
                text: body.to_string(),
            });
        }
    };
    let mut at = 0usize;
    while let Some((start, inner, tag)) = next_open(text, at) {
        push(SegmentKind::Typed, &text[at..start]);
        let close = format!("</{tag}");
        match text[inner..].find(&close) {
            Some(p) => {
                push(SegmentKind::Pasted, &text[inner..inner + p]);
                let rest = &text[inner + p..];
                at = inner + p + rest.find('>').map_or(rest.len(), |g| g + 1);
            }
            None => {
                push(SegmentKind::Pasted, &text[inner..]);
                at = text.len();
            }
        }
    }
    push(SegmentKind::Typed, &text[at.min(text.len())..]);
    out
}

/// The sentences of a prompt a title may be taken from, in order.
///
/// Markup tags (`<pasted_content id=…>`, `<input>`) are not part of a
/// sentence and are removed from the line they sit on; a line that is then
/// empty, or that is an attachment (`[Image…`, an `@path`), or is inside a
/// code fence, is skipped; leading markdown markers (`#`, `-`, `*`, `>`, `1.`)
/// are not part of a sentence. Within a line a sentence ends at `. `, `? `,
/// `! ` or the end of the line, and a sentence with no letter or digit in it
/// (a lone `\`, `...`) is not one. Nothing else is altered.
pub fn sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_fence = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence || t.is_empty() {
            continue;
        }
        let untagged = strip_tags(t);
        let t = untagged.trim();
        if t.is_empty() || t.starts_with("[Image") || t.starts_with('@') {
            continue;
        }
        let body = strip_marker(t);
        if body.is_empty() {
            continue;
        }
        let mut rest = body;
        while !rest.is_empty() {
            let cut = sentence_end(rest);
            let sentence = rest[..cut].trim();
            // A "sentence" with no letter or digit (`\`, `...`, `—`) says
            // nothing a title could carry.
            if sentence.chars().any(char::is_alphanumeric) {
                out.push(sentence.to_string());
            }
            rest = rest[cut..].trim_start();
        }
        if out.len() >= 40 {
            break;
        }
    }
    out
}

/// `text` with every `<tag …>` / `</tag>` token removed. A `<` that does not
/// open a tag-shaped token (`a < b`) is kept.
pub fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let tag_like = after
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '/' || c == '!');
        match (tag_like, after.find('>')) {
            (true, Some(close)) if !after[..close].contains('<') => {
                rest = &after[close + 1..];
            }
            _ => {
                out.push('<');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn strip_marker(line: &str) -> &str {
    let mut t = line;
    loop {
        let before = t;
        for marker in ["#", "-", "*", ">", "•"] {
            if let Some(rest) = t.strip_prefix(marker) {
                t = rest.trim_start();
            }
        }
        // `1.` / `12)` list markers.
        let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits > 0 && digits < 4 {
            let rest = &t[digits..];
            if let Some(after) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
                t = after.trim_start();
            }
        }
        if t == before {
            return t;
        }
    }
}

/// Byte index just past the end of the first sentence in `s`.
fn sentence_end(s: &str) -> usize {
    let bytes = s.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if matches!(b, b'.' | b'?' | b'!') {
            let next = bytes.get(i + 1);
            if next.is_none() || next == Some(&b' ') {
                return i + 1;
            }
        }
    }
    s.len()
}

/// Does this sentence read as an instruction?
pub fn is_instruction(sentence: &str) -> bool {
    for word in sentence.split_whitespace() {
        let w: String = word
            .trim_matches(|c: char| !c.is_alphanumeric() && c != ',')
            .to_ascii_lowercase();
        let bare = w.trim_end_matches(',');
        if FILLERS.contains(&w.as_str()) || FILLERS.contains(&bare) {
            continue;
        }
        return INSTRUCTION_VERBS.contains(&bare);
    }
    false
}

/// How many sentences from the top of a pasted block may title a thread.
pub const PASTE_WORK_WINDOW: usize = 6;

/// Verbs that open an instruction but ask only to go on, not for a change.
pub const PROCEED_VERBS: &[&str] = &[
    "continue", "start", "resume", "proceed", "go", "carry", "let", "help", "try",
];

/// Words that do not make an object: pronouns, particles, adverbs. A verb
/// followed only by these ("do it", "keep going", "get it working") names no
/// work.
const NON_OBJECT: &[&str] = &[
    "it",
    "this",
    "that",
    "these",
    "those",
    "them",
    "now",
    "please",
    "again",
    "on",
    "ahead",
    "up",
    "here",
    "there",
    "going",
    "working",
    "done",
    "so",
    "then",
    "too",
    "all",
    "everything",
    "anything",
    "something",
    "more",
    "over",
    "from",
    "the",
    "a",
    "an",
    "with",
    "me",
    "us",
    "back",
    "away",
    "along",
    "out",
    "for",
    "in",
    "to",
    "and",
    "or",
    "of",
    "next",
    "same",
    "way",
    "way.",
    "thing",
    "things",
    "stuff",
];

/// Words skipped before the verb when judging whether a sentence names work
/// (in addition to [`FILLERS`]).
const WORK_LEAD: &[&str] = &[
    "i", "we", "want", "need", "to", "let's", "lets", "should", "must", "will", "kindly",
];

/// Words that ask an agent to go on with what it was doing.
const PROCEED_WORDS: &[&str] = &[
    "continue", "proceed", "resume", "start", "carry", "take", "pick", "keep", "go",
];

/// Nouns that make a sentence a role or lane heading when it names no work.
const ROLE_WORDS: &[&str] = &[
    "lane",
    "session",
    "role",
    "orchestrator",
    "prompter",
    "coder",
    "verifier",
    "conductor",
];

fn words_of(sentence: &str) -> Vec<String> {
    sentence
        .split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| {
                !c.is_alphanumeric()
                    && c != '\''
                    && c != '`'
                    && c != '/'
                    && c != '.'
                    && c != '_'
                    && c != '-'
            })
            .trim_end_matches(['.', ','])
            .to_lowercase()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

/// Does this sentence name work — a work verb followed by an object (a word
/// that is not a pronoun, particle or adverb, or a path / `code` token)?
///
/// Leading fillers ("please", "can you", "I want you to", "let's") are
/// skipped; the verb must be an [`INSTRUCTION_VERBS`] verb that is not one of
/// [`PROCEED_VERBS`]. "Fix the parser." and "Can you fix the login redirect?"
/// name work; "Get it working.", "Start now." and "Continue." do not.
pub fn names_work(sentence: &str) -> bool {
    let words = words_of(sentence);
    let mut i = 0;
    while i < words.len()
        && (FILLERS.contains(&words[i].as_str()) || WORK_LEAD.contains(&words[i].as_str()))
    {
        i += 1;
    }
    let Some(verb) = words.get(i) else {
        return false;
    };
    if !INSTRUCTION_VERBS.contains(&verb.as_str()) || PROCEED_VERBS.contains(&verb.as_str()) {
        return false;
    }
    words[i + 1..].iter().any(|w| {
        w.contains('/') || w.contains('`') || w.contains('.') || !NON_OBJECT.contains(&w.as_str())
    })
}

/// Is this sentence one a title skips (C26)?
///
/// (a) three words or fewer that name no work ("Start now.", "Go ahead.");
/// (b) a question that names no work and asks the agent to go on
///     ("Can you continue this in …?", "Can you take over from here?");
/// (c) a role or instruction preamble: it opens with "You are", "You're",
///     "Your role", "Context:", "Background:", "Note:", "Rulings:", "Read this
///     first" or "Read first"; or contains "read this first"; or opens with
///     "Read" and ends with "first"; or asks to read a brief ("Read the
///     rules.", "Read `X.md` and do it."); or ends with `:`; or names no work
///     and names a role or lane ([`ROLE_WORDS`]).
pub fn is_skippable(sentence: &str) -> bool {
    let t = sentence.trim();
    let lower = t.to_lowercase();
    let words = words_of(t);
    let work = names_work(t);
    if words.len() <= 3 && !work {
        return true;
    }
    if t.ends_with('?') && !work && words.iter().any(|w| PROCEED_WORDS.contains(&w.as_str())) {
        return true;
    }
    let bare = lower.trim_end_matches(['.', '!', ' ']);
    // "Read the rules." / "Read `~/x/BRIEF.md` and do it." — an instruction
    // to read a brief is a preamble: the work is in the brief.
    let reads_a_brief = words.first().is_some_and(|w| w == "read")
        && words.iter().skip(1).any(|w| {
            [
                "rules",
                "instructions",
                "brief",
                "this",
                "below",
                "above",
                "following",
                "handoff",
                "card",
                "cards",
            ]
            .contains(&w.as_str())
                || w.ends_with(".md")
        });
    if reads_a_brief {
        return true;
    }
    if [
        "you are ",
        "you're ",
        "your role",
        "your job",
        "context:",
        "background:",
        "note:",
        "rulings:",
        "read this first",
        "read first",
    ]
    .iter()
    .any(|p| lower.starts_with(p))
        || lower.contains("read this first")
        || (lower.starts_with("read ") && bare.ends_with(" first"))
        || t.ends_with(':')
    {
        return true;
    }
    !work && words.iter().any(|w| ROLE_WORDS.contains(&w.as_str()))
}

/// The first sentence of `sentences` that names work and is not skippable.
fn work_sentence(sentences: &[String]) -> Option<String> {
    sentences
        .iter()
        .find(|s| !is_skippable(s) && names_work(s))
        .cloned()
}

/// A thread's title from its prompts in order (the first work prompt first),
/// with the index of the prompt it came from.
///
/// The first prompt's work sentence, if it has one; else the first later
/// prompt that has one ([`TitleBasis::WorkSentenceLaterPrompt`]); else the
/// first prompt's fallback; else (that prompt has no sentence at all) the
/// first later prompt with any title.
pub fn thread_title(prompts: &[&str]) -> (Option<String>, TitleBasis, usize) {
    let mut fallback: Option<(Option<String>, TitleBasis, usize)> = None;
    for (i, p) in prompts.iter().enumerate() {
        let (t, b) = title_sentence(p);
        match b {
            TitleBasis::WorkSentence | TitleBasis::PastedWorkSentence => {
                let basis = if i == 0 {
                    b
                } else {
                    TitleBasis::WorkSentenceLaterPrompt
                };
                return (t, basis, i);
            }
            _ => {
                if fallback.is_none() && t.is_some() {
                    fallback = Some((t, b, i));
                }
            }
        }
    }
    fallback.unwrap_or((None, TitleBasis::None, 0))
}

/// The unmasked title sentence and why it was chosen. The caller masks it and
/// cuts it with [`cut_title`] — masking happens before the cut so that a
/// secret straddling the boundary is still recognised whole.
///
/// What the person typed outside pasted blocks ([`segments`]) is preferred.
/// Only when nothing was typed is the first pasted block used, and then only
/// its first heading or the first sentence of its first line — never a
/// sentence from deep inside a pasted brief.
pub fn title_sentence(prompt: &str) -> (Option<String>, TitleBasis) {
    let segs = segments(prompt);
    let typed: Vec<&str> = segs
        .iter()
        .filter(|s| s.kind == SegmentKind::Typed)
        .map(|s| s.text.as_str())
        .collect();
    let typed_text = typed.join("\n");
    if let Some(w) = work_sentence(&sentences(&typed_text)) {
        return (Some(w), TitleBasis::WorkSentence);
    }
    for paste in segs.iter().filter(|s| s.kind == SegmentKind::Pasted) {
        let top: Vec<String> = sentences(&paste.text)
            .into_iter()
            .take(PASTE_WORK_WINDOW)
            .collect();
        if let Some(w) = work_sentence(&top) {
            return (Some(w), TitleBasis::PastedWorkSentence);
        }
    }
    let (t, b) = typed_title(&typed_text);
    if t.is_some() {
        return (t, b);
    }
    for paste in segs.iter().filter(|s| s.kind == SegmentKind::Pasted) {
        if let Some(found) = pasted_title(&paste.text) {
            return found;
        }
    }
    (None, TitleBasis::None)
}

/// A pasted block's title: its first markdown heading when one comes before
/// any other text, else the first sentence of its first line with text.
fn pasted_title(paste: &str) -> Option<(Option<String>, TitleBasis)> {
    for line in paste.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with("```") {
            continue;
        }
        if t.starts_with('#') {
            let heading = t.trim_start_matches('#').trim();
            if !heading.is_empty() {
                return Some((Some(heading.to_string()), TitleBasis::PastedHeading));
            }
            continue;
        }
        if let Some(first) = sentences(t).into_iter().next() {
            return Some((Some(first), TitleBasis::PastedFirstSentence));
        }
    }
    None
}

fn typed_title(prompt: &str) -> (Option<String>, TitleBasis) {
    let all = sentences(prompt);
    if let Some(s) = all.iter().find(|s| is_instruction(s)) {
        return (Some(s.clone()), TitleBasis::FallbackInstruction);
    }
    match all.into_iter().next() {
        Some(s) => (Some(s), TitleBasis::FallbackFirstSentence),
        None => (None, TitleBasis::None),
    }
}

/// Mask a title, then cut it to [`TITLE_MAX`] characters, ending in `…` when
/// cut, never inside a `[masked: …]` marker. Masking comes first so a secret
/// straddling the cut is recognised whole (A209-C3); the record of what the
/// masker did is returned with the text.
pub fn cut_title(raw: &str) -> (String, crate::redact::MaskRecord) {
    let (masked, record) = crate::redact::mask_recorded(raw);
    if masked.chars().count() <= TITLE_MAX {
        return (masked, record);
    }
    let mut shown = crate::redact::truncate_respecting_markers(&masked, TITLE_MAX - 1);
    shown = shown.trim_end().to_string();
    shown.push('…');
    (shown, record)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injected_envelopes_are_not_prompts() {
        for text in [
            "<task-notification><status>completed</status></task-notification>",
            "This session is being continued from a previous conversation that ran out",
            "<local-command-stdout>Set model</local-command-stdout>",
            "Another Claude session sent a message:\nhi",
            "<environment_context>\n<cwd>/x</cwd></environment_context>",
            "# AGENTS.md instructions for /x",
            "   ",
            "[Request interrupted by user]",
        ] {
            assert_eq!(classify(text, false), PromptClass::Injected, "{text}");
        }
    }

    #[test]
    fn claude_codes_image_note_is_not_a_prompt_but_a_persons_pasted_image_is() {
        let note = "[Image: original 1512x2846, displayed at 1063x2000. Multiply coordinates by 1.42 to map to original image.]";
        assert_eq!(classify(note, false), PromptClass::Injected);
        let pasted = "[Image #1] the header overlaps the logo, fix it";
        assert!(
            matches!(classify(pasted, false), PromptClass::Human { .. }),
            "a person's pasted image with words is their prompt"
        );
    }

    #[test]
    fn slash_commands_with_arguments_are_the_persons_prompt() {
        let wrapper = "<command-message>fix</command-message>\n<command-name>/fix</command-name>\n<command-args>the login bug</command-args>";
        assert_eq!(
            classify(wrapper, false),
            PromptClass::Human {
                text: "/fix the login bug".into()
            }
        );
        let bare = "<command-name>/model</command-name>\n<command-message>model</command-message>\n<command-args></command-args>";
        assert_eq!(classify(bare, false), PromptClass::CommandWrapper);
        assert_eq!(
            classify("# Workflow authoring reference", true),
            PromptClass::Injected
        );
    }

    #[test]
    fn a_title_prefers_the_first_instruction() {
        let (t, b) =
            title_sentence("Hi there. I was thinking. Please fix the parser crash. Thanks!");
        assert_eq!(t.as_deref(), Some("Please fix the parser crash."));
        assert_eq!(b, TitleBasis::WorkSentence);
        let (t, b) = title_sentence("Why is the build slow? It used to be fast.");
        assert_eq!(t.as_deref(), Some("Why is the build slow?"));
        assert_eq!(b, TitleBasis::FallbackFirstSentence);
        // An unclosed paste is all paste: its first sentence, said so.
        let (t, b) =
            title_sentence("<pasted_content id=\"1\">\n@\"/x/y.html\"\n- add a retry to `fetch`");
        assert_eq!(t.as_deref(), Some("add a retry to `fetch`"));
        assert_eq!(b, TitleBasis::PastedWorkSentence);
        let (t, _) = title_sentence(
            "<realtime_delegation>\n  <input>Cool. Um. Show me every prompt.</input>",
        );
        assert_eq!(t.as_deref(), Some("Show me every prompt."));
        assert_eq!(classify("</image>", false), PromptClass::Injected);
        assert_eq!(title_sentence("<only-a-tag>").1, TitleBasis::None);
    }

    const PASTE_ONLY: &str = "\n\n<pasted_content id=\"23e0\">\nThruLine — Phase 6 UI shell, parallel session. Paste as the first message of a fresh session.\nRules: never read, write, `cd` into, run a server from, or run git against `~/thruline`.\nBuild the shell.\n</pasted_content id=\"23e0\">\n";

    #[test]
    fn pasted_blocks_are_segments_without_their_wrappers() {
        let text = "Hi.\n<pasted_content id=\"a1\">\n# Brief\nFix everything.\n</pasted_content id=\"a1\">\nPlease review this brief.";
        let segs = segments(text);
        let kinds: Vec<SegmentKind> = segs.iter().map(|s| s.kind).collect();
        assert_eq!(
            kinds,
            vec![SegmentKind::Typed, SegmentKind::Pasted, SegmentKind::Typed]
        );
        assert_eq!(segs[1].text, "# Brief\nFix everything.");
        assert!(segs.iter().all(|s| !s.text.contains("pasted_content")));
        assert_eq!(segs[2].text, "Please review this brief.");
        // An unclosed block runs to the end; other envelopes are recognised too.
        let segs = segments("look <attached_file name=\"a.md\">\nbody");
        assert_eq!(segs[1].kind, SegmentKind::Pasted);
        assert_eq!(segs[1].text, "body");
        // No envelope: one typed segment, verbatim.
        assert_eq!(segments("Fix it.")[0].text, "Fix it.");
    }

    #[test]
    fn a_title_comes_from_what_the_person_typed_not_from_inside_a_paste() {
        let (t, b) = title_sentence("<pasted_content id=\"a1\">\nFix the world.\n</pasted_content id=\"a1\">\nPlease review this brief.");
        assert_eq!(t.as_deref(), Some("Please review this brief."));
        assert_eq!(b, TitleBasis::WorkSentence);
        // Paste only: a work sentence near the top (C26), not the lane
        // heading, and never one from deep inside.
        let (t, b) = title_sentence(PASTE_ONLY);
        assert_eq!(t.as_deref(), Some("Build the shell."));
        assert_eq!(b, TitleBasis::PastedWorkSentence);
        // Paste only, with a heading: the heading.
        let (t, b) = title_sentence("<pasted_content id=\"b\">\n\n## Wave 2 card\nRead the rules. Build it.\n</pasted_content id=\"b\">");
        assert_eq!(t.as_deref(), Some("Wave 2 card"));
        assert_eq!(b, TitleBasis::PastedHeading);
    }

    #[test]
    fn a_line_with_no_letters_or_digits_is_never_a_title() {
        let (t, b) = title_sentence("\\\nwhy does the build fail on CI?");
        assert_eq!(t.as_deref(), Some("why does the build fail on CI?"));
        assert_eq!(b, TitleBasis::FallbackFirstSentence);
        let (t, _) = title_sentence("\\\n...\n— \nFix the parser.");
        assert_eq!(t.as_deref(), Some("Fix the parser."));
        assert_eq!(title_sentence("\\\n...").1, TitleBasis::None);
    }

    // C26: synthetic prompts with the shapes of the real ones that titled
    // threads badly ("Start now.", proceed-only questions, role preambles,
    // a coder-lane brief pasted whole). No real prompt text is used.

    #[test]
    fn a_short_go_ahead_is_skipped_for_the_sentence_that_names_work() {
        let (t, b) = title_sentence("Start now.\n\nFix the login redirect in `auth.rs`.");
        assert_eq!(t.as_deref(), Some("Fix the login redirect in `auth.rs`."));
        assert_eq!(b, TitleBasis::WorkSentence);
        for short in ["Continue.", "Go.", "Go ahead.", "Ok, start."] {
            assert!(!names_work(short) && is_skippable(short), "{short}");
        }
        // Three words WITH an object is work.
        assert!(names_work("Fix the parser."));
        assert!(names_work("Work on the settings screen first."));
        assert!(!names_work("Get it working."));
    }

    #[test]
    fn a_question_that_only_asks_to_proceed_is_skipped_and_one_that_names_a_change_is_not() {
        let (t, b) =
            title_sentence("Can you continue this in cloud session? Add a retry to the uploader.");
        assert_eq!(t.as_deref(), Some("Add a retry to the uploader."));
        assert_eq!(b, TitleBasis::WorkSentence);
        let (t, b) = title_sentence("Can you fix the login redirect?");
        assert_eq!(t.as_deref(), Some("Can you fix the login redirect?"));
        assert_eq!(b, TitleBasis::WorkSentence);
        assert!(is_skippable(
            "Can you take over the agent's work from here?"
        ));
    }

    #[test]
    fn role_and_instruction_preambles_are_skipped() {
        let (t, b) = title_sentence(
            "You are the CODER session for the Acme launch. Read AGENTS.md first.\nRulings:\nBuild the export button in `ui/export.jsx`.",
        );
        assert_eq!(
            t.as_deref(),
            Some("Build the export button in `ui/export.jsx`.")
        );
        assert_eq!(b, TitleBasis::WorkSentence);
        for pre in [
            "You are the verifier for this lane.",
            "Read this first.",
            "Context:",
            "Acme cloud coder lane — read this first, then build ONLY the card(s) below",
        ] {
            assert!(is_skippable(pre), "{pre}");
        }
    }

    #[test]
    fn a_pasted_brief_is_titled_by_its_first_work_sentence_near_the_top() {
        let (t, b) = title_sentence(
            "<pasted_content id=\"1\">\nAcme cloud coder lane — read this first, then build ONLY the card(s) below\nRulings:\nAdd the CSV export to the report screen.\nNever touch `prod.db`.\n</pasted_content id=\"1\">",
        );
        assert_eq!(
            t.as_deref(),
            Some("Add the CSV export to the report screen.")
        );
        assert_eq!(b, TitleBasis::PastedWorkSentence);
    }

    #[test]
    fn with_no_work_sentence_the_old_rule_still_titles_and_says_so() {
        let (t, b) = title_sentence("You are the verifier.");
        assert_eq!(t.as_deref(), Some("You are the verifier."));
        assert_eq!(b, TitleBasis::FallbackFirstSentence);
        let (t, b) = title_sentence("Continue.");
        assert_eq!(t.as_deref(), Some("Continue."));
        assert_eq!(b, TitleBasis::FallbackInstruction);
    }

    #[test]
    fn a_thread_whose_first_prompt_names_no_work_takes_a_later_prompts_work_sentence() {
        let prompts = [
            "Can you take over the agent's work from here?",
            "ok",
            "Rename the config loader to `settings.rs`.",
        ];
        let (t, b, at) = thread_title(&prompts);
        assert_eq!(
            t.as_deref(),
            Some("Rename the config loader to `settings.rs`.")
        );
        assert_eq!(b, TitleBasis::WorkSentenceLaterPrompt);
        assert_eq!(at, 2);
        // Nothing names work anywhere: the first prompt's fallback.
        let (t, b, at) = thread_title(&["Start now.", "Continue."]);
        assert_eq!(
            (t.as_deref(), b, at),
            (Some("Start now."), TitleBasis::FallbackInstruction, 0)
        );
    }

    #[test]
    fn a_long_title_is_cut_with_a_marker() {
        let long = "a".repeat(200);
        let (cut, _) = cut_title(&long);
        assert_eq!(cut.chars().count(), TITLE_MAX);
        assert!(cut.ends_with('…'));
        assert_eq!(cut_title("short").0, "short");
        // Masked before the cut: a key straddling character 90 is still caught.
        let key = format!(
            "{} ghp_0123456789abcdefghijABCDEFGHIJ012345",
            "a".repeat(80)
        );
        let (cut, rec) = cut_title(&key);
        assert!(!cut.contains("ghp_01234"), "{cut}");
        assert!(rec.masked >= 1);
    }
}

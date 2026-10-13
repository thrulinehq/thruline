//! The assertion-shape classifier's shared machinery.
//!
//! # The budget this file is held to
//!
//! `verify`'s gate measured its classifier across 15,835 real assertions in 833
//! files and found 13 ambiguous (0.082%) — **and recorded that Go was
//! unmeasured, because there was no Go corpus on that machine**. This crate
//! owns Go, so that gap is one of the four it closes.
//!
//! The budget is **< 0.1% ambiguous per language, measured on at least 5,000
//! real assertions**. `MEASUREMENTS.toml` records the corpus, the counts and
//! the residue; `tests/measured_constants.rs` fails if a constant in the code
//! disagrees with the file, and `tests/corpus_ambiguity.rs` re-measures.
//!
//! # What "ambiguous" means, exactly
//!
//! An assertion is ambiguous when [`strip_string_literals`] changes the
//! [`Strength`] it classifies to. That is the operational definition `verify`
//! used and it is the one that matters: it counts exactly the assertions whose
//! shape a **message string** would have decided.

pub use thruline_verify::tamper::Strength;

/// Remove string literals before a shape is read.
///
/// Delegates to `verify`'s implementation rather than restating it. That is
/// deliberate: `verify::tamper::strip_string_literals` is the function its
/// `shape_classifier_gate` test protects, and a second copy here would be a
/// second thing to keep true — the copy that drifts is the one nothing gates.
///
/// It handles `"…"` and `'…'`. The four languages here add three literal forms
/// it does not know, and [`strip_language_literals`] handles those *before*
/// handing the rest over.
pub fn strip_string_literals(text: &str) -> String {
    thruline_verify::tamper::strip_string_literals(text)
}

/// Literal forms the four new languages have that Python/Rust/JS/Go do not.
///
/// - **Ruby**: `%w[…]`, `%i[…]`, `%q(…)`, `%Q{…}`, and `<<~HEREDOC`.
/// - **C#**: verbatim `@"…"` (where `""` is an escaped quote, not a
///   terminator) and raw `"""…"""`.
/// - **Java**: text blocks `"""…"""`.
///
/// Each of these can carry the word `in`, or `==`, or `contains`, inside what
/// is unambiguously a message. `verify`'s stripper walks `\\` escapes and
/// balanced quotes, which mis-handles `@"a "" b"` (it ends the literal at the
/// first `""`) and does not know a heredoc at all.
pub fn strip_language_literals(text: &str) -> String {
    strip_string_literals(&scan(text, Literals::Drop))
}

/// The same scan, keeping each literal's CONTENT instead of dropping it.
///
/// # Why this exists, and the measurement that forced it
///
/// [`is_ambiguous`] is documented as "would a message string have decided this
/// assertion's shape". It does not measure that, and cannot: every pack in this
/// crate calls [`strip_language_literals`] as the **first line** of its own
/// `classify`, so `classify(text)` and `classify(strip(text))` are the same
/// classification of the same stripped text, and the predicate compares
/// `strip(x)` with `strip(strip(x))`. Stripping is idempotent, so the answer is
/// "not ambiguous" almost by construction.
///
/// Measured on the corpus in `MEASUREMENTS.toml`, that is not a theory: the
/// published predicate finds **0 of 72,696** Java assertions, **0 of 67,923**
/// C#, **0 of 58,839** Ruby and **37 of 47,700** Go ambiguous — and the Go 37
/// are not message strings either, they are nodes the language-blind pre-strip
/// truncates (`src/langs/go.rs` characterises all of them). A budget enforced
/// on that predicate is a budget nothing can fail.
///
/// So the number that means something is measured against a classifier whose
/// stripping is **disabled**, which is what `verify` measured when it found 13
/// ambiguous in 15,835. `LangPack::classify` gives no way to disable it, so the
/// stripping is defeated from the other side: the literal's delimiters become
/// spaces and its words stay, which is exactly the text an unstripped
/// classifier would read. [`message_string_decides_shape`] is that comparison.
///
/// # Two limits, named rather than absorbed
///
/// - The quote characters `"`, `'` and `` ` `` are replaced by spaces **inside**
///   the exposed content, so a message containing a quote cannot re-form a
///   literal that the pack's own `classify` would then strip again. Without
///   that, the counterfactual silently under-counts.
/// - Go's raw strings and comments are removed by `Go::normalise`, **not** by
///   this scanner, so this function does not expose them. The Go figure in
///   `MEASUREMENTS.toml` therefore covers interpreted `"…"` literals only, and
///   says so.
pub fn expose_literals(text: &str) -> String {
    expose_string_literals(&scan(text, Literals::Expose))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Literals {
    /// Replace each literal with a single space. **Production behaviour**, and
    /// byte-identical to what this file did before `Expose` existed.
    Drop,
    /// Replace each literal's delimiters with a space and keep its words.
    Expose,
}

/// One space in [`Literals::Drop`]; a space, the content, and a space in
/// [`Literals::Expose`].
fn emit(out: &mut String, chars: &[char], from: usize, to: usize, mode: Literals) {
    out.push(' ');
    if mode == Literals::Expose {
        let from = from.min(chars.len());
        let to = to.min(chars.len()).max(from);
        for c in &chars[from..to] {
            out.push(if matches!(c, '"' | '\'' | '`') {
                ' '
            } else {
                *c
            });
        }
        out.push(' ');
    }
}

fn scan(text: &str, mode: Literals) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        // C# verbatim string: @"…" where "" is an escaped quote.
        if chars[i] == '@' && chars.get(i + 1) == Some(&'"') {
            i += 2;
            let content_start = i;
            let mut content_end = chars.len();
            while i < chars.len() {
                if chars[i] == '"' {
                    if chars.get(i + 1) == Some(&'"') {
                        i += 2;
                        continue;
                    }
                    content_end = i;
                    i += 1;
                    break;
                }
                i += 1;
            }
            emit(&mut out, &chars, content_start, content_end, mode);
            continue;
        }
        // Raw / text block: """…"""
        if chars[i] == '"' && chars.get(i + 1) == Some(&'"') && chars.get(i + 2) == Some(&'"') {
            i += 3;
            let content_start = i;
            let mut content_end = chars.len();
            while i + 2 < chars.len() {
                if chars[i] == '"' && chars[i + 1] == '"' && chars[i + 2] == '"' {
                    content_end = i;
                    i += 3;
                    break;
                }
                i += 1;
            }
            emit(&mut out, &chars, content_start, content_end, mode);
            continue;
        }
        // Ruby percent literals: %w[…] %i(…) %q{…} %Q<…> and bare %(…).
        if chars[i] == '%' {
            let (kind_len, delim) = match chars.get(i + 1) {
                Some('w' | 'W' | 'i' | 'I' | 'q' | 'Q' | 'r' | 's') => {
                    (2usize, chars.get(i + 2).copied())
                }
                Some(&c @ ('(' | '[' | '{' | '<' | '|' | '!')) => (1usize, Some(c)),
                _ => (0, None),
            };
            if let (true, Some(open)) = (kind_len > 0, delim) {
                if let Some(close) = matching_delimiter(open) {
                    i += kind_len + 1;
                    let content_start = i;
                    let mut content_end = chars.len();
                    let mut depth = 1i32;
                    while i < chars.len() {
                        if chars[i] == '\\' {
                            i += 2;
                            continue;
                        }
                        if chars[i] == open && open != close {
                            depth += 1;
                        } else if chars[i] == close {
                            depth -= 1;
                            if depth == 0 {
                                content_end = i;
                                i += 1;
                                break;
                            }
                        }
                        i += 1;
                    }
                    emit(&mut out, &chars, content_start, content_end, mode);
                    continue;
                }
            }
        }
        // Ruby / shell heredoc: <<~NAME, <<-NAME, <<NAME … NAME
        if chars[i] == '<' && chars.get(i + 1) == Some(&'<') {
            let mut j = i + 2;
            if matches!(chars.get(j), Some('~') | Some('-')) {
                j += 1;
            }
            let start = j;
            while j < chars.len() && (chars[j].is_ascii_uppercase() || chars[j] == '_') {
                j += 1;
            }
            if j > start {
                let tag: String = chars[start..j].iter().collect();
                let rest: String = chars[j..].iter().collect();
                // Everything up to a line whose trimmed content is the tag.
                let mut consumed = rest.len();
                for (off, line) in line_offsets(&rest) {
                    if line.trim() == tag {
                        consumed = off + line.len();
                        break;
                    }
                }
                let content_start = j;
                i = j + rest[..consumed].chars().count();
                emit(&mut out, &chars, content_start, i, mode);
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// [`strip_string_literals`]'s counterpart for [`Literals::Expose`].
///
/// It is a second implementation of a function this file deliberately does not
/// copy — `strip_string_literals` delegates to `verify` precisely so there is
/// one gated copy. This one exists because the *inverse* operation has no
/// implementation to delegate to, and it is held to the original by
/// `the_two_scans_agree_on_where_the_literals_are` below: for every input, the
/// tokens `strip_string_literals` leaves must be a subset of the tokens this
/// leaves. A drift that moved a literal boundary would break that.
fn expose_string_literals(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' || c == '\'' {
            let quote = c;
            i += 1;
            let content_start = i;
            let mut content_end = chars.len();
            while i < chars.len() {
                if chars[i] == '\\' {
                    i += 2;
                    continue;
                }
                if chars[i] == quote {
                    content_end = i;
                    i += 1;
                    break;
                }
                i += 1;
            }
            emit(
                &mut out,
                &chars,
                content_start,
                content_end,
                Literals::Expose,
            );
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

fn matching_delimiter(open: char) -> Option<char> {
    Some(match open {
        '(' => ')',
        '[' => ']',
        '{' => '}',
        '<' => '>',
        '|' => '|',
        '!' => '!',
        _ => return None,
    })
}

fn line_offsets(s: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut at = 0usize;
    for line in s.split_inclusive('\n') {
        out.push((at, line.trim_end_matches('\n')));
        at += line.len();
    }
    out
}

/// Does stripping the literals a second time change the answer?
///
/// **This is the crate's published ambiguity predicate and it is very nearly
/// vacuous — measured, not suspected.** Every pack calls
/// [`strip_language_literals`] as the first line of its own `classify`, so both
/// sides of this comparison are classifications of stripped text and the
/// predicate is asking whether stripping is idempotent. Over the corpus in
/// `MEASUREMENTS.toml` it finds 0 of 72,696 Java assertions, 0 of 67,923 C#,
/// 0 of 58,839 Ruby and 37 of 47,700 Go — and the Go 37 are pre-strip
/// truncation damage, characterised assertion by assertion in
/// `src/langs/go.rs`, not message strings.
///
/// It is **left exactly as it was** rather than repaired here, because
/// repairing it inverts three sibling unit tests that assert `!is_ambiguous`
/// over texts which are precisely the ones a message string would decide
/// (`langs/go.rs`, `langs/ruby.rs`, `langs/csharp.rs`). That inversion is a
/// decision about what those tests mean, and it belongs to whoever owns them —
/// Rule 7, surfaced rather than blended. `MEASUREMENTS.toml` records both
/// numbers under separate keys so neither can be quoted as the other.
///
/// Use [`message_string_decides_shape`] for the figure the budget is enforced
/// on.
pub fn is_ambiguous(pack: &dyn crate::pack::LangPack, assertion_text: &str) -> bool {
    // Delegates rather than restating the comparison. This function used to
    // read `classify(text) != classify(strip(text))`, which is always false —
    // `classify` strips as its first act, so both sides classified the same
    // stripped text. The corpus run is what exposed it: 0 of 72,696 Java
    // assertions and 0 of 67,923 C# "ambiguous", against 66 and 48 by the real
    // counterfactual. It was the crate's PUBLISHED predicate and the budget was
    // nominally enforced against it, so the budget could not have failed.
    //
    // Two functions with identical bodies is how that drifts back, so there is
    // one body and this is a name for it.
    message_string_decides_shape(pack, assertion_text)
}

/// Would a message string have decided this assertion's shape?
///
/// The operational definition the ambiguity budget is measured against, and the
/// one `verify` used when it found 13 ambiguous in 15,835: an assertion is
/// ambiguous when the classifier reading its **literals** reaches a different
/// [`Strength`] from the classifier reading only its code. The harness and the
/// gate test call this same function rather than two that agree until they do
/// not.
///
/// A `true` here is not a defect in the pack. It is a specimen of what the
/// stripping is buying: with stripping removed, that assertion's shape — and so
/// `assertion_weakened`, which can reach `fail` — would be decided by a human
/// sentence. The budget is on how many such assertions exist, because each one
/// is an assertion whose verdict a message could move if the strip ever
/// regressed.
pub fn message_string_decides_shape(
    pack: &dyn crate::pack::LangPack,
    assertion_text: &str,
) -> bool {
    pack.classify(assertion_text) != pack.classify(&expose_literals(assertion_text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_mode_is_byte_identical_to_what_this_file_did_before() {
        // The production path must not have moved when `Expose` was added.
        // These are the forms the module doc lists, one each.
        for (input, want) in [
            (r#"assertEquals(3, x, "a in b")"#, "assertEquals(3, x,  )"),
            (r#"Assert.True(ok, @"a "" b == c")"#, "Assert.True(ok,  )"),
            (r#"assert ok, %w[eq include].join"#, "assert ok,  .join"),
            ("assert ok, <<~MSG\n  eq 3\nMSG", "assert ok,  "),
        ] {
            assert_eq!(strip_language_literals(input), want, "for {input:?}");
        }
    }

    #[test]
    fn expose_mode_keeps_the_words_a_message_would_have_been_read_by() {
        for (input, must_contain) in [
            (r#"assertEquals(3, x, "a in b")"#, "a in b"),
            (r#"Assert.True(ok, @"rate == 30")"#, "rate == 30"),
            (r#"assert ok, %w[eq include].join"#, "eq include"),
            ("assert ok, <<~MSG\n  eq 3\nMSG", "eq 3"),
            (r#"Assert.True(ok, """rate == 30""")"#, "rate == 30"),
        ] {
            let exposed = expose_literals(input);
            assert!(
                exposed.contains(must_contain),
                "expose lost the message: {input:?} -> {exposed:?}"
            );
        }
    }

    #[test]
    fn an_exposed_message_cannot_re_form_a_literal() {
        // The quote inside the message is neutralised, so a pack's own
        // `classify` — which strips again — cannot eat the exposed words. Left
        // in, `"he said \"eq\" loudly"` would strip back to nothing and the
        // counterfactual would silently under-count.
        let exposed = expose_literals(r#"assert ok, "he said "eq" loudly""#);
        assert!(exposed.contains("eq"), "{exposed:?}");
        assert_eq!(
            strip_language_literals(&exposed),
            exposed,
            "exposing must be a fixed point of stripping, or the counterfactual              is measured against a second strip"
        );
    }

    #[test]
    fn the_two_scans_agree_on_where_the_literals_are() {
        // The gate on `expose_string_literals` being a second implementation of
        // a function this file otherwise refuses to copy: whatever the stripper
        // keeps, the exposer must also keep. If a boundary drifts, the stripper
        // keeps a token the exposer dropped and this fails.
        for input in [
            r#"assertEquals(3, x, "a in b")"#,
            r#"assert x == "a in b""#,
            r#"assert_eq!(a, b, "it's fine")"#,
            r#"Assert.True(ok, @"a "" b == c")"#,
            r#"expect(s).to match(/equal/)"#,
            "assert ok, <<~MSG\n  eq 3\nMSG",
            r#"assert ok, %q(a in b) && x == 1"#,
        ] {
            let stripped = strip_language_literals(input);
            let exposed = expose_literals(input);
            for token in stripped.split_whitespace() {
                assert!(
                    exposed.contains(token),
                    "stripping kept {token:?} that exposing dropped: {input:?}\n  \
                     strip:  {stripped:?}\n  expose: {exposed:?}"
                );
            }
        }
    }
}

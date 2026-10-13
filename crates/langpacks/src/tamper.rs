//! The §6.7 tamper detectors, driven by a [`LangPack`] instead of a closed enum.
//!
//! # This is a port, not a redesign
//!
//! Every finding name, every severity and every "candidate, not a conclusion"
//! decision is `verify::tamper::diff_test_file`'s, unchanged. Detector names
//! must match exactly — `tests_pass::escalate` dispatches on the string
//! `skip_marker_added`, and a pack that emitted `skip_added` would be silently
//! un-escalated and un-de-escalated: a `warn` forever, which is the shape of a
//! finding nobody acts on.
//!
//! What changes is only the *source* of the per-language facts: the trait,
//! rather than a `match` no external crate can extend.
//!
//! # The one behaviour that is new, and why
//!
//! `verify` detects a swallowed body with a **substring** table
//! (`swallow_markers` plus a check for `pass`/`{}`/`continue`). That works for
//! Python's `except:` + `pass` and is unusable for Java, C# and Ruby, where the
//! honest form (`assertThrows`, `Assert.Throws`, `expect { }.to raise_error`)
//! and the cheating form (`try { … } catch (Exception e) { }`) share every
//! substring. So `catch_kinds` selects the node with the parser and the body is
//! judged by **what is inside it**, structurally. The substring path is kept
//! for the languages it serves.

use std::collections::BTreeMap;

use tree_sitter::Node;

use crate::pack::{Finding, LangPack, LangSpec, Severity, SkipSite, Strength};

/// One test function, reduced to what §6.7 compares.
///
/// Field-for-field `verify::tamper::TestFn`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestFn {
    pub name: String,
    pub assertions: Vec<Strength>,
    pub mock_targets: Vec<String>,
    pub has_skip_marker: bool,
    pub swallows: bool,
    pub has_sleep_or_retry: bool,
    pub body_tokens: Vec<String>,
}

impl TestFn {
    fn strongest(&self) -> Option<Strength> {
        self.assertions.iter().copied().max()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TamperError {
    #[error("the {0} grammar could not be loaded")]
    Grammar(&'static str),
    #[error("{path} could not be parsed as {language}")]
    Parse {
        path: String,
        language: &'static str,
    },
}

/// Extract every test function from a source file.
pub fn parse_tests(source: &str, pack: &dyn LangPack) -> Result<Vec<TestFn>, TamperError> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&pack.ts_language())
        .map_err(|_| TamperError::Grammar(pack.language().as_str()))?;
    let tree = parser.parse(source, None).ok_or(TamperError::Parse {
        path: String::new(),
        language: pack.language().as_str(),
    })?;
    let spec = pack.spec();
    let mut out = Vec::new();
    let mut cursor = tree.walk();
    collect(&mut cursor, source, pack, &spec, &mut out);
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn collect(
    cursor: &mut tree_sitter::TreeCursor,
    source: &str,
    pack: &dyn LangPack,
    spec: &LangSpec,
    out: &mut Vec<TestFn>,
) {
    let node = cursor.node();
    if spec.fn_kinds.contains(&node.kind()) {
        if let Some(test) = as_test_fn(node, source, pack, spec) {
            out.push(test);
            return;
        }
    }
    if cursor.goto_first_child() {
        loop {
            collect(cursor, source, pack, spec, out);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
}

fn as_test_fn(node: Node, source: &str, pack: &dyn LangPack, spec: &LangSpec) -> Option<TestFn> {
    let name = pack.test_name(node, source)?;
    let text = node.utf8_text(source.as_bytes()).ok()?;
    // **Comments are blanked before ANY text-derived fact is read off a test.**
    //
    // This was a false accusation with a measurement behind it. `body_tokens`
    // was built by handing the function's RAW text to
    // `shape::strip_language_literals`, which treats `'` as a literal
    // delimiter and knows nothing about comments. A rename that also reworded
    // the comment above the body — `isn't sorted` becoming `is unsorted` —
    // opened a rune literal on the apostrophe that never closed and ate the
    // rest of the function: 13 body tokens became 5. The base's 5 truncated
    // tokens against the head's 13 whole ones scored 0.3 on `jaccard`, under
    // `RENAME_SIMILARITY`, so `similar_bodies` called them different tests and
    // the rename was reported as `test_deleted` at `fail`. An honest refactor,
    // refuted, because of an apostrophe in a comment.
    //
    // It is not a Go finding: Ruby's `#` comments and Java's, C#'s and Go's
    // `//` all carry contractions. And it reached three facts by the same
    // route — `body_tokens`, `has_sleep_or_retry`, and the `SkipSite::Body`
    // marker search, which is the same class of bug `verify`'s own Python
    // control fixture caught when a comment reading
    // `# we used to @pytest.mark.skip this` was read as a decorator.
    //
    // The parser already knows exactly which bytes are a comment, so the
    // answer is structural rather than another substring rule: blank those
    // spans and read everything else from the result.
    let text = &blank_comments(node, source, text);
    let lower = text.to_ascii_lowercase();

    let mut assertions = Vec::new();
    let mut cursor = node.walk();
    collect_assertions(&mut cursor, source, pack, spec, &mut assertions);

    let decorators = decorators_of(node, source, spec).to_ascii_lowercase();

    let body_tokens: Vec<String> = crate::shape::strip_language_literals(text)
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|t| t.len() > 1 && *t != name)
        .map(|t| t.to_ascii_lowercase())
        .collect();

    let has_skip_marker = pack.is_skip_marker(&decorators)
        || (spec.skip_site == SkipSite::Body && pack.is_skip_marker(&lower));

    Some(TestFn {
        name,
        assertions,
        mock_targets: mock_targets_in(text, pack.mock_calls()),
        body_tokens,
        has_skip_marker,
        swallows: swallows(node, source, pack, spec),
        has_sleep_or_retry: has_sleep_or_retry(&lower),
    })
}

/// Sleep and retry, read off the **stripped** text.
///
/// `verify` reads `sleep(`/`retry` off the raw text. In Ruby that is a false
/// positive waiting to happen — `retry` is a keyword, and it is also the first
/// word of a great many test *names* and messages. Stripping literals first
/// removes the message case; the keyword case is real and stays.
fn has_sleep_or_retry(lower_text: &str) -> bool {
    let stripped = crate::shape::strip_language_literals(lower_text);
    stripped.contains("sleep(")
        || stripped.contains("thread.sleep")
        || stripped.contains("task.delay")
        || stripped.contains("time.sleep")
        || stripped
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|t| t == "retry" || t == "retries" || t == "eventually")
}

/// Is this test's body wrapped so a failure no longer propagates?
///
/// **Structural for the `catch_kinds` languages.** The rule: a catch/rescue
/// clause counts as swallowing when its handler body contains **no assertion
/// and no rethrow**. That is what separates
///
/// ```text
/// try { risky(); fail("should have thrown"); } catch (IOException e) { /* expected */ }
/// ```
///
/// — an honest negative test, whose *try* block holds the assertion — from
///
/// ```text
/// try { assertEquals(9.0, rate(2)); } catch (Throwable t) { }
/// ```
///
/// — the cheat, whose *try* block holds the assertion and whose handler is
/// empty. Both have an empty handler, so emptiness alone cannot decide it; what
/// decides it is **whether the assertions are inside the guarded block**.
fn swallows(node: Node, source: &str, pack: &dyn LangPack, spec: &LangSpec) -> bool {
    if !spec.catch_kinds.is_empty() {
        let mut cursor = node.walk();
        if catch_swallows(&mut cursor, source, pack, spec) {
            return true;
        }
    }
    if !spec.defer_kinds.is_empty() && defer_swallows(node, source, pack, spec) {
        return true;
    }
    // The substring path, for the languages `verify` serves that way. Comments
    // blanked for the reason given in `as_test_fn`: a marker named in a comment
    // is not a marker.
    let raw = node.utf8_text(source.as_bytes()).unwrap_or("");
    let lower = blank_comments(node, source, raw).to_ascii_lowercase();
    !spec.swallow_markers.is_empty()
        && spec
            .swallow_markers
            .iter()
            .any(|m| lower.contains(&m.to_ascii_lowercase()))
        && (lower.contains("pass") || lower.contains("{}") || lower.contains("continue"))
}

fn catch_swallows(
    cursor: &mut tree_sitter::TreeCursor,
    source: &str,
    pack: &dyn LangPack,
    spec: &LangSpec,
) -> bool {
    let node = cursor.node();
    if spec.catch_kinds.contains(&node.kind()) {
        // The guarded region, and the handlers.
        //
        // Three tree shapes, because three grammars express this differently
        // and only the first two were handled:
        //
        //  - a `body` FIELD (Java's `try_statement`);
        //  - a child whose kind contains `block` (C#);
        //  - **neither** — `tree-sitter-ruby`'s `begin` node has
        //    `"fields": {}` and its children are bare statements followed by
        //    `rescue` / `ensure` / `else` siblings. Ruby therefore had no
        //    swallow detection at all: `catch_kinds` could be set to anything
        //    and this lookup would still find no guarded region, so the
        //    §6.7 `fail` that BUILD-SPEC lists as *"test body wrapped in
        //    try/except-pass"* was simply absent for the language.
        //
        // The third case is the general one: everything before the first
        // handler is guarded.
        let mut hc = node.walk();
        let handlers: Vec<Node> = node
            .named_children(&mut hc)
            .filter(|n| {
                let k = n.kind();
                k.contains("catch") || k.contains("rescue") || k.contains("except")
            })
            .collect();

        let mut gc = node.walk();
        let mut c2 = node.walk();
        let block_child = node
            .named_children(&mut c2)
            .find(|n| n.kind().contains("block"));
        let guarded_regions: Vec<Node> = match node.child_by_field_name("body").or(block_child) {
            Some(b) => vec![b],
            None => {
                let first_handler = handlers.first().map(|h| h.start_byte());
                node.named_children(&mut gc)
                    .filter(|n| match first_handler {
                        Some(at) => n.end_byte() <= at,
                        None => true,
                    })
                    .collect()
            }
        };

        let mut guarded_strengths = Vec::new();
        let mut guarded_text = String::new();
        for region in &guarded_regions {
            let mut c = region.walk();
            collect_assertions(&mut c, source, pack, spec, &mut guarded_strengths);
            if let Ok(t) = region.utf8_text(source.as_bytes()) {
                guarded_text.push_str(t);
                guarded_text.push('\n');
            }
        }
        let guarded_asserts = !guarded_strengths.is_empty();

        // The expects-a-throw idiom: the guarded block ends with an
        // unconditional failure, so the handler is the SUCCESS path. See
        // `LangSpec::unconditional_fail_markers` for why neither an empty
        // handler nor a guarded assertion can decide this on its own.
        let stripped_guarded =
            crate::shape::strip_language_literals(&guarded_text).to_ascii_lowercase();
        let guarded_expects_a_throw = spec
            .unconditional_fail_markers
            .iter()
            .any(|m| stripped_guarded.contains(&m.to_ascii_lowercase()));

        if guarded_asserts && !guarded_expects_a_throw {
            // Assertions live inside the guarded block. If any handler neither
            // asserts nor rethrows, a failing assertion is caught and dropped.
            for handler in &handlers {
                let text = handler.utf8_text(source.as_bytes()).unwrap_or("");
                let stripped = crate::shape::strip_language_literals(text).to_ascii_lowercase();
                // **Token-wise, not substring.** `contains("throw")` is true of
                // `Throwable`, so `catch (Throwable t) { }` — the broadest
                // possible swallow, and the shape a cheat reaches for — read as
                // a handler that RE-THROWS and was exempted. The detector was
                // disarmed by the exception type it most needed to catch.
                //
                // `raise` and `panic` have the same hazard in the other
                // languages (`RaiseError`, `panicking`), so all three are
                // matched as whole identifiers.
                let rethrows = stripped
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .any(|t| matches!(t, "throw" | "rethrow" | "raise" | "panic"));
                let mut hc = handler.walk();
                let mut handler_asserts = Vec::new();
                collect_assertions(&mut hc, source, pack, spec, &mut handler_asserts);
                if !rethrows && handler_asserts.is_empty() {
                    return true;
                }
            }
        }
    }
    if cursor.goto_first_child() {
        loop {
            if catch_swallows(cursor, source, pack, spec) {
                cursor.goto_parent();
                return true;
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
    false
}

/// The third path: a wrapper that neutralises a failure with no handler clause.
///
/// Go's `defer func() { recover() }()`. The rule is the same one
/// [`catch_swallows`] applies, read off a different tree shape: the wrapper
/// swallows when the assertions are **outside** it and the wrapper itself
/// neither asserts nor re-raises.
///
/// Both halves of that are load-bearing, and the honest form is why:
///
/// ```text
/// defer func() {
///     if r := recover(); r == nil {
///         t.Error("expected Rate to panic on a negative weight")
///     }
/// }()
/// ```
///
/// That is a negative test — the deferred closure is where its assertion
/// lives, and it must not be reported. So a wrapper containing an assertion is
/// honest, exactly as a `catch` clause containing one is. And a wrapper that
/// re-panics has not neutralised anything.
///
/// The remaining shape — a `recover()` with an empty or logging body, wrapped
/// around a function whose assertions sit outside it — is the cheat, and it is
/// the one the measured fixture pair produces: exit 1 becomes exit 0 with
/// nothing else changed.
fn defer_swallows(node: Node, source: &str, pack: &dyn LangPack, spec: &LangSpec) -> bool {
    let mut wrappers = Vec::new();
    collect_kinds(node, spec.defer_kinds, &mut wrappers);
    if wrappers.is_empty() {
        return false;
    }

    // Assertions in the function as a whole.
    let mut all = Vec::new();
    let mut c = node.walk();
    collect_assertions(&mut c, source, pack, spec, &mut all);
    if all.is_empty() {
        // Nothing to swallow.
        return false;
    }

    for wrapper in wrappers {
        let raw = wrapper.utf8_text(source.as_bytes()).unwrap_or("");
        let text = blank_comments(wrapper, source, raw).to_ascii_lowercase();
        let neutralises = spec
            .neutralising_calls
            .iter()
            .any(|call| text.contains(&call.to_ascii_lowercase()));
        if !neutralises {
            continue;
        }
        // A wrapper that re-raises has not neutralised anything.
        if text.contains("panic(") && !text.contains("recover()") {
            continue;
        }
        let mut wc = wrapper.walk();
        let mut inside = Vec::new();
        collect_assertions(&mut wc, source, pack, spec, &mut inside);

        // **Counted by whether they can FAIL, not by whether they were
        // admitted.** Go's `assertion_call_prefixes` deliberately admits every
        // top-most `if` in a test body and pays for the over-count in
        // `classify`, which returns `Truthy` for any `if` that cannot fail the
        // test. So the deferred closure of the cheat —
        // `if r := recover(); r != nil { t.Logf(...) }` — IS an admitted
        // assertion, and counting admissions made this path exempt the exact
        // shape it was written to catch.
        //
        // `t.Logf` cannot fail a test, so that `if` classifies `Truthy`. The
        // honest form's `if r := recover(); r == nil { t.Error(...) }` carries
        // a failing call, so `classify` reads its condition and it is not
        // `Truthy`. That is the distinction the exemption needs, and the pack
        // already computes it.
        let can_fail = |strengths: &[Strength]| -> usize {
            strengths.iter().filter(|s| **s != Strength::Truthy).count()
        };
        let inside_can_fail = can_fail(&inside);
        let outside_can_fail = can_fail(&all) - inside_can_fail;

        // The honest negative test asserts INSIDE the deferred closure.
        if inside_can_fail > 0 {
            continue;
        }
        // And there has to be something outside for it to swallow.
        if outside_can_fail > 0 {
            return true;
        }
    }
    false
}

/// Every descendant of `node` whose kind is in `kinds`.
fn collect_kinds<'a>(node: Node<'a>, kinds: &[&str], out: &mut Vec<Node<'a>>) {
    let mut cursor = node.walk();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if kinds.contains(&n.kind()) {
            out.push(n);
        }
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
}

/// The decorators/annotations/attributes attached to a declaration, from the TREE.
fn decorators_of(node: Node, source: &str, spec: &LangSpec) -> String {
    match spec.skip_site {
        SkipSite::Body => String::new(),
        SkipSite::Preceding => {
            let mut out = String::new();
            let mut sibling = node.prev_sibling();
            let mut seen = 0;
            while let Some(n) = sibling {
                if spec.decorator_kinds.contains(&n.kind()) {
                    if let Ok(t) = n.utf8_text(source.as_bytes()) {
                        out.push_str(t);
                        out.push('\n');
                    }
                } else if !n.kind().contains("comment") {
                    break;
                }
                seen += 1;
                if seen > 8 {
                    break;
                }
                sibling = n.prev_sibling();
            }
            out
        }
        // Java: `modifiers > annotation`. C#: `attribute_list`. Both are
        // CHILDREN of the declaration, so the walk goes down and stops before
        // the body — a `// we used to @Disabled this` comment in the body must
        // not be reachable, which is the control fixture `verify` learned this
        // from.
        SkipSite::DeclarationChild => {
            let mut out = String::new();
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if spec.decorator_kinds.contains(&child.kind()) {
                    if let Ok(t) = child.utf8_text(source.as_bytes()) {
                        out.push_str(t);
                        out.push('\n');
                    }
                }
            }
            // C# puts `attribute_list` as a *sibling* in some tree shapes and a
            // child in others; check the immediately preceding siblings too,
            // bounded, and never crossing a non-attribute node.
            let mut sibling = node.prev_sibling();
            let mut seen = 0;
            while let Some(n) = sibling {
                if spec.decorator_kinds.contains(&n.kind()) {
                    if let Ok(t) = n.utf8_text(source.as_bytes()) {
                        out.push_str(t);
                        out.push('\n');
                    }
                } else if !n.kind().contains("comment") {
                    break;
                }
                seen += 1;
                if seen > 8 {
                    break;
                }
                sibling = n.prev_sibling();
            }
            out
        }
    }
}

/// A node's text with every comment span replaced by spaces of equal length.
///
/// Byte-length-preserving on purpose: the replacement keeps every other span at
/// the offset it had, so a caller can still index into the result with offsets
/// taken from the node. Non-ASCII comment bytes become the same number of
/// spaces, which is what keeps that true for a `// naïve` or a `# façade`.
///
/// Comments are located by asking the parser (`kind()` containing `comment`),
/// never by scanning for `//` or `#` — a `//` inside a string is not a comment,
/// and the tree already knows the difference.
fn blank_comments(node: Node, _source: &str, text: &str) -> String {
    let base = node.start_byte();
    let mut out = text.as_bytes().to_vec();
    let mut cursor = node.walk();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind().contains("comment") {
            let (from, to) = (n.start_byte(), n.end_byte());
            if from >= base && to <= base + out.len() {
                for b in &mut out[from - base..to - base] {
                    *b = b' ';
                }
            }
            // A comment has no children worth visiting.
            continue;
        }
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    // Every replaced byte was one ASCII space, so the result is still valid
    // UTF-8 wherever the input was.
    String::from_utf8(out).unwrap_or_else(|_| text.to_string())
}

fn collect_assertions(
    cursor: &mut tree_sitter::TreeCursor,
    source: &str,
    pack: &dyn LangPack,
    spec: &LangSpec,
    out: &mut Vec<Strength>,
) {
    let node = cursor.node();
    if spec.assertion_kinds.contains(&node.kind()) {
        if let Ok(text) = node.utf8_text(source.as_bytes()) {
            if is_assertion(text, spec) {
                out.push(pack.classify(text));
                // Do not descend: `assertThat(x).isEqualTo(y)` is ONE
                // assertion, and its receiver is a `call_expression` too.
                // Counting both doubles every fluent assertion, which turns an
                // honest 1 -> 1 edit into a 2 -> 1 `assertion_count_decreased`.
                return;
            }
        }
    }
    if cursor.goto_first_child() {
        loop {
            collect_assertions(cursor, source, pack, spec, out);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
}

/// Does this node's text look like an assertion call?
///
/// `verify` requires the text to **start with** a prefix. Fluent assertions do
/// not: `result.Should().Be(9.0)` and `expect(rate(2)).to eq(9.0)` start with
/// the subject. So a prefix beginning with `.` is matched anywhere in the
/// text's first line, and a bare prefix still has to start it.
fn is_assertion(text: &str, spec: &LangSpec) -> bool {
    let head = text.trim_start().to_ascii_lowercase();
    let first_line = head.lines().next().unwrap_or("");
    spec.assertion_call_prefixes.iter().any(|p| {
        let p = p.to_ascii_lowercase();
        if let Some(infix) = p.strip_prefix('.') {
            first_line.contains(&format!(".{infix}"))
        } else {
            head.starts_with(&p)
        }
    })
}

fn mock_targets_in(text: &str, calls: &'static [&'static str]) -> Vec<String> {
    let mut out = Vec::new();
    for call in calls {
        let mut from = 0usize;
        while let Some(at) = text[from..].find(call) {
            let start = from + at + call.len();
            let rest = &text[start..];
            if let Some(target) = first_target(rest) {
                if !target.is_empty() && target.len() < 200 {
                    out.push(target);
                }
            }
            from = start;
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The target a mock call names.
///
/// `verify` takes the first **quoted** argument, which is where Python and JS
/// put it. Java, C# and Go put a **type**: `mock(ShippingRate.class)`,
/// `new Mock<IShippingRate>()`, `mocks.NewMockRate(ctrl)`. Ruby puts a symbol
/// or a constant: `allow(Shipping).to receive(:rate)`. So both forms are read,
/// quoted first.
fn first_target(rest: &str) -> Option<String> {
    let upto = rest.find(')').unwrap_or(rest.len().min(200));
    let head = &rest[..upto];
    if let Some(q) = head.find(['"', '\'']) {
        let quote = head.as_bytes()[q] as char;
        if let Some(end) = head[q + 1..].find(quote) {
            return Some(head[q + 1..q + 1 + end].to_string());
        }
    }
    let ident: String = head
        .trim_start_matches(['<', '(', ' '])
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.' || *c == ':')
        .collect();
    let ident = ident.trim_end_matches(':').trim_start_matches(':');
    (!ident.is_empty()).then(|| ident.trim_end_matches(".class").to_string())
}

/// Compare a test file between the base tree and the head tree.
pub fn diff_test_file(
    path: &str,
    base: &str,
    head: &str,
    pack: &dyn LangPack,
) -> Result<Vec<Finding>, TamperError> {
    diff_test_file_with_symbols(path, base, head, pack, &[])
}

/// [`diff_test_file`], plus the symbols that changed elsewhere in the session.
pub fn diff_test_file_with_symbols(
    path: &str,
    base: &str,
    head: &str,
    pack: &dyn LangPack,
    changed_symbols: &[String],
) -> Result<Vec<Finding>, TamperError> {
    let base_fns = parse_tests(base, pack)?;
    let head_fns = parse_tests(head, pack)?;
    let head_by_name: BTreeMap<&str, &TestFn> =
        head_fns.iter().map(|f| (f.name.as_str(), f)).collect();
    let base_by_name: BTreeMap<&str, &TestFn> =
        base_fns.iter().map(|f| (f.name.as_str(), f)).collect();

    let mut findings = Vec::new();
    for base_fn in &base_fns {
        let Some(head_fn) = head_by_name.get(base_fn.name.as_str()) else {
            let renamed_to = head_fns
                .iter()
                .filter(|h| !base_by_name.contains_key(h.name.as_str()))
                .find(|h| similar_bodies(base_fn, h));
            findings.push(match renamed_to {
                Some(h) => Finding::new(
                    Severity::Info,
                    "test_renamed",
                    format!(
                        "`{}` was renamed to `{}` — the body is substantially the same, \
                         so this is a rename and not a deletion",
                        base_fn.name, h.name
                    ),
                )
                .at(path),
                None => Finding::new(
                    Severity::Fail,
                    "test_deleted",
                    format!(
                        "`{}` existed at the base tree and does not at the final tree, \
                         and no added test carries its body",
                        base_fn.name
                    ),
                )
                .at(path),
            });
            continue;
        };

        if head_fn.assertions.len() < base_fn.assertions.len() {
            let all_gone = head_fn.assertions.is_empty();
            findings.push(
                Finding::new(
                    if all_gone {
                        Severity::Fail
                    } else {
                        Severity::Warn
                    },
                    if all_gone {
                        "assertion_removed"
                    } else {
                        "assertion_count_decreased"
                    },
                    format!(
                        "`{}` had {} assertion(s) at the base tree and {} at the final tree",
                        base_fn.name,
                        base_fn.assertions.len(),
                        head_fn.assertions.len()
                    ),
                )
                .at(path),
            );
        }

        if let (Some(was), Some(now)) = (base_fn.strongest(), head_fn.strongest()) {
            if now < was {
                let vacuous = now == Strength::Truthy;
                findings.push(
                    Finding::new(
                        if vacuous {
                            Severity::Fail
                        } else {
                            Severity::Warn
                        },
                        "assertion_weakened",
                        format!(
                            "`{}`'s strongest assertion went from {was:?} to {now:?}{}",
                            base_fn.name,
                            if vacuous {
                                " — which asserts nothing, so the test can no longer fail \
                                 for the reason it was written"
                            } else {
                                ""
                            }
                        ),
                    )
                    .at(path),
                );
            }
        }

        if head_fn.has_skip_marker && !base_fn.has_skip_marker {
            findings.push(
                Finding::new(
                    Severity::Warn,
                    "skip_marker_added",
                    format!(
                        "`{}` gained a skip/ignore marker in this session",
                        base_fn.name
                    ),
                )
                .at(path),
            );
        }
        if head_fn.swallows && !base_fn.swallows {
            findings.push(
                Finding::new(
                    Severity::Fail,
                    "assertion_swallowed",
                    format!(
                        "`{}`'s body was wrapped so a failure no longer propagates",
                        base_fn.name
                    ),
                )
                .at(path),
            );
        }
        for target in &head_fn.mock_targets {
            if base_fn.mock_targets.contains(target) {
                continue;
            }
            if thruline_verify::tamper::mock_hits_changed_symbol(target, changed_symbols) {
                findings.push(
                    Finding::new(
                        Severity::Warn,
                        "mocked_unit_under_test",
                        format!(
                            "`{}` gained a mock of `{target}`, which resolves to a symbol \
                             changed in this session — the test no longer exercises the \
                             code the change is about",
                            base_fn.name
                        ),
                    )
                    .at(path),
                );
            }
        }
        if head_fn.has_sleep_or_retry && !base_fn.has_sleep_or_retry {
            findings.push(
                Finding::new(
                    Severity::Warn,
                    "sleep_or_retry_added",
                    format!("`{}` gained a sleep or retry loop", base_fn.name),
                )
                .at(path),
            );
        }
    }

    for head_fn in &head_fns {
        if !base_by_name.contains_key(head_fn.name.as_str()) {
            if base_fns
                .iter()
                .filter(|b| !head_by_name.contains_key(b.name.as_str()))
                .any(|b| similar_bodies(b, head_fn))
            {
                continue;
            }
            findings.push(
                Finding::new(
                    Severity::Info,
                    "test_added",
                    format!("`{}` is new in this session", head_fn.name),
                )
                .at(path),
            );
        }
    }

    // **The cap rides on the finding, since R8 landed in verify (`18d94dc`).**
    //
    // This used to clamp by assignment — `finding.severity = cap` — at this one
    // point, and the module docs called that "the single point every finding
    // leaves through". It was, and it was still not enough: a clamp applied
    // here says nothing to a severity-RAISING site downstream.
    // `tests_pass::escalate` read a capped Ruby `skip_marker_added` as an
    // ordinary `warn`, raised it to `fail`, and refuted a claim the operator had
    // ruled Ruby ineligible to refute.
    //
    // The operator's ruling was that the ceiling must be a property of the
    // VALUE: *"Every finding carries its language, and every severity-raising
    // site consults the cap — enforced by the type, not by emission order."*
    // verify now implements that, so this crate stops clamping and starts
    // DECLARING: `Finding::from_pack` records the language and the ceiling and
    // applies it, and `Finding::raised_to` — the only way a severity increases
    // anywhere — clamps against it.
    //
    // Adopting it is not optional bookkeeping. Until this call existed, verify
    // saw `cap: None` on every finding this crate emitted and the leak stayed
    // open with the fix already merged: the test that asserts the leak went on
    // passing, which is precisely how a landed fix reads as unapplied.
    let language = pack.language().as_str();
    let cap = pack.severity_cap();
    findings = findings
        .into_iter()
        .map(|f| f.from_pack(language, cap))
        .collect();
    Ok(findings)
}

/// Jaccard similarity over body token **sets**, against [`RENAME_SIMILARITY`].
///
/// Set semantics, matching `verify`. The threshold is re-measured for these
/// four languages in `MEASUREMENTS.toml` rather than inherited: `verify` chose
/// 0.5 against a Python/Rust/JS table, and Java and C# bodies carry far more
/// boilerplate tokens (`final`, `var`, `new`, type names), which raises the
/// similarity of two *unrelated* tests and so moves where the threshold has to
/// sit.
pub fn similar_bodies(a: &TestFn, b: &TestFn) -> bool {
    jaccard(a, b) >= RENAME_SIMILARITY
}

/// The similarity of two test bodies, as the measurement harness sees it.
pub fn jaccard(a: &TestFn, b: &TestFn) -> f64 {
    use std::collections::BTreeSet;
    let sa: BTreeSet<&String> = a.body_tokens.iter().collect();
    let sb: BTreeSet<&String> = b.body_tokens.iter().collect();
    if sa.is_empty() && sb.is_empty() {
        return 1.0;
    }
    let inter = sa.intersection(&sb).count() as f64;
    let union = sa.union(&sb).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        inter / union
    }
}

/// **Measured, not chosen.** See `MEASUREMENTS.toml` and
/// `tests/measured_constants.rs`, which fails if this disagrees with the file.
pub const RENAME_SIMILARITY: f64 = crate::measured::RENAME_SIMILARITY;

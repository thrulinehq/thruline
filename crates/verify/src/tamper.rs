//! BUILD-SPEC §6.7 — test-tampering detectors over an AST diff.
//!
//! > *Compute AST diff of each changed test file between base tree and final
//! > tree.*
//!
//! # Why the node selection is structural and the shape classification is not
//!
//! Every assertion below is located by **walking the parse tree** — a
//! `function_definition` whose name marks it a test, an `assert_statement`
//! inside it. That is what keeps a comment mentioning `==`, or a docstring
//! containing the word `skip`, out of the results; a regex over the file would
//! find both.
//!
//! Within a node that the tree has already told us is an assertion, the *shape*
//! (equality, containment, approximate) is read from its source text. That is a
//! deliberate trade: four grammars express comparison with four different node
//! layouts, and a per-grammar shape matcher is four times the surface for a
//! distinction that only sets `warn` versus nothing. The residual — a string
//! literal inside an assertion containing `" in "` — is recorded in
//! `REPORT.md` rather than hidden.
//!
//! # Severities are §6.7's, not mine
//!
//! `fail`: an assertion removed, a skip marker added, a test deleted, a body
//! wrapped in try/except-pass. `warn`: an assertion loosened, an assertion
//! count decreased, a sleep or retry added. The one §6.7 calls `fail` *only in
//! combination* — an expected literal changed while the source changed to match
//! it — needs §6.8's hardcoding heuristic to decide, and is emitted as `warn`
//! here with that dependency named in the detail.

use std::collections::BTreeMap;

use crate::verdict::{Finding, Severity};

/// The languages §6.7 names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Python,
    Rust,
    JavaScript,
    TypeScript,
    Go,
}

impl Language {
    /// Recognise from a path. `None` means "not a language this build parses",
    /// which is reported rather than treated as "no findings".
    pub fn from_path(path: &str) -> Option<Language> {
        let lower = path.to_ascii_lowercase();
        Some(match lower.rsplit('.').next()? {
            "py" => Language::Python,
            "rs" => Language::Rust,
            "js" | "jsx" | "mjs" | "cjs" => Language::JavaScript,
            "ts" | "tsx" => Language::TypeScript,
            "go" => Language::Go,
            _ => return None,
        })
    }

    fn ts_language(self) -> tree_sitter::Language {
        match self {
            Language::Python => tree_sitter_python::LANGUAGE.into(),
            Language::Rust => tree_sitter_rust::LANGUAGE.into(),
            Language::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Language::Go => tree_sitter_go::LANGUAGE.into(),
        }
    }

    fn spec(self) -> LangSpec {
        match self {
            Language::Python => LangSpec {
                skip_in_body: false,
                fn_kinds: &["function_definition"],
                assertion_kinds: &["assert_statement"],
                assertion_call_prefixes: &["assert", "self.assert", "pytest.raises"],
                skip_markers: &["@pytest.mark.skip", "@unittest.skip", "@pytest.mark.xfail"],
                swallow_markers: &["except:", "except exception:"],
                test_name_prefixes: &["test"],
                catch_kinds: &[],
            },
            Language::Rust => LangSpec {
                skip_in_body: false,
                fn_kinds: &["function_item"],
                assertion_kinds: &["macro_invocation"],
                assertion_call_prefixes: &["assert", "debug_assert", "panic", "expect"],
                skip_markers: &["#[ignore]"],
                swallow_markers: &["let _ = "],
                test_name_prefixes: &[],
                catch_kinds: &[],
            },
            Language::JavaScript | Language::TypeScript => LangSpec {
                skip_in_body: true,
                fn_kinds: &["function_declaration", "call_expression", "arrow_function"],
                assertion_kinds: &["call_expression"],
                assertion_call_prefixes: &["expect", "assert", "chai.assert", "should"],
                skip_markers: &["it.skip", "test.skip", "describe.skip", "xit(", "xtest("],
                swallow_markers: &["catch {}", "catch (e) {}"],
                test_name_prefixes: &["test", "it", "describe"],
                catch_kinds: &[],
            },
            Language::Go => LangSpec {
                skip_in_body: true,
                fn_kinds: &["function_declaration"],
                assertion_kinds: &["call_expression", "if_statement"],
                assertion_call_prefixes: &[
                    "t.error", "t.fatal", "assert.", "require.", "t.errorf", "t.fatalf",
                ],
                skip_markers: &["t.skip("],
                swallow_markers: &[],
                test_name_prefixes: &["Test"],
                catch_kinds: &[],
            },
        }
    }
}

/// A language pack — the interface R1 publishes, so a pack can live outside
/// this crate.
///
/// # What this is, and what it deliberately is not
///
/// `Language` is a closed enum and every `match` over it here is exhaustive by
/// construction, so before R1 **no language pack could exist outside this crate
/// at all**. This trait is the seam. A pack supplies its own tables
/// ([`LangSpec`]), its own grammar, the extensions it claims, and the ceiling
/// its findings may reach.
///
/// **It is not yet a parameterisation of this crate's walker.** Six `match`es
/// over `Language` still carry this build's own per-language judgements — the
/// Rust `#[test]` attribute, the JS callee rule, the Python and Go name rules,
/// the visibility rule in `collect_symbols`. An external pack today brings its
/// own analysis (`crates/langpacks` has one) and uses this trait to be *found*
/// and to be *capped*; it does not drive the walk here. Turning those six into
/// trait methods is real work on 966 lines of a load-bearing detector and is
/// not smuggled into an interface commit.
///
/// # The registry is caller-populated, and that is forced
///
/// `crates/langpacks` depends on `thruline-verify` and re-exports its
/// [`crate::verdict::Finding`], [`crate::verdict::Severity`] and [`Strength`],
/// so **this crate cannot depend back** — that is a cycle. A registry living
/// here and holding G's packs is therefore impossible; the caller that depends
/// on both hands them over. See [`pack_for_path`].
pub trait LangPack: Send + Sync {
    /// A stable name, used in reports and as the `language` a
    /// [`crate::verdict::Finding`] carries.
    fn name(&self) -> &'static str;

    /// Lowercase file extensions, without the dot.
    fn extensions(&self) -> &'static [&'static str];

    fn ts_language(&self) -> tree_sitter::Language;

    fn spec(&self) -> LangSpec;

    /// The highest severity a finding from this pack may carry, or `None` for
    /// uncapped.
    ///
    /// Read by [`crate::verdict::Finding::from_pack`], and enforced by the
    /// finding rather than by the emitter — see that type's docs for why the
    /// cap rides on the value.
    fn severity_cap(&self) -> Option<crate::verdict::Severity> {
        None
    }
}

/// The pack claiming this path, from a registry the caller supplies.
///
/// `None` means "no pack in this registry parses that", which is **reported**
/// rather than treated as "no findings" — the same rule
/// [`Language::from_path`] follows and for the same reason.
///
/// The registry is a plain slice so a caller can build it from this crate's
/// [`builtin_packs`] plus anyone else's, in whatever order it wants. Later
/// entries do not override earlier ones: the first match wins, and a caller
/// that wants its own pack to beat a built-in puts it first, deliberately,
/// rather than discovering the precedence.
pub fn pack_for_path<'a>(path: &str, registry: &'a [&'a dyn LangPack]) -> Option<&'a dyn LangPack> {
    let lower = path.to_ascii_lowercase();
    let ext = lower.rsplit('.').next()?;
    registry
        .iter()
        .copied()
        .find(|p| p.extensions().contains(&ext))
}

/// This crate's own five, as packs.
///
/// A caller composes its registry from these plus whatever else it has; nothing
/// here assumes it is the whole list.
pub fn builtin_packs() -> &'static [&'static dyn LangPack] {
    &[
        &BuiltinPack(Language::Python),
        &BuiltinPack(Language::Rust),
        &BuiltinPack(Language::JavaScript),
        &BuiltinPack(Language::TypeScript),
        &BuiltinPack(Language::Go),
    ]
}

/// One of the five, wearing the published interface.
///
/// A newtype rather than `impl LangPack for Language`, so the enum stays a
/// plain value in the six `match`es that still key on it and nothing has to
/// choose between the two spellings at a call site.
struct BuiltinPack(Language);

impl LangPack for BuiltinPack {
    fn name(&self) -> &'static str {
        match self.0 {
            Language::Python => "python",
            Language::Rust => "rust",
            Language::JavaScript => "javascript",
            Language::TypeScript => "typescript",
            Language::Go => "go",
        }
    }

    fn extensions(&self) -> &'static [&'static str] {
        match self.0 {
            Language::Python => &["py"],
            Language::Rust => &["rs"],
            Language::JavaScript => &["js", "jsx", "mjs", "cjs"],
            Language::TypeScript => &["ts", "tsx"],
            Language::Go => &["go"],
        }
    }

    fn ts_language(&self) -> tree_sitter::Language {
        self.0.ts_language()
    }

    fn spec(&self) -> LangSpec {
        self.0.spec()
    }
}

/// The per-language table §6.7's structural walk reads.
///
/// # Public since R1 (Track G)
///
/// It was private, reached through a private `Language::spec()`, so **no
/// language pack could exist outside this crate at all** — G's words, and they
/// were exact. `crates/langpacks` had built a field-for-field copy so the move
/// would be a move rather than a translation; publishing it is what makes that
/// true.
pub struct LangSpec {
    pub fn_kinds: &'static [&'static str],
    /// Does a skip marker live in the function's own text, or only in the
    /// decorators/attributes that precede it?
    ///
    /// **Python and Rust: only preceding.** Searching the body was a real bug
    /// caught by its own control fixture — a comment reading
    /// `# we used to @pytest.mark.skip this` was reported as a skip marker,
    /// which is precisely the regex failure the structural walk exists to
    /// avoid. JS and Go put the marker in the call itself (`it.skip(`,
    /// `t.Skip(`), so there it genuinely is in the body.
    pub skip_in_body: bool,
    pub assertion_kinds: &'static [&'static str],
    pub assertion_call_prefixes: &'static [&'static str],
    pub skip_markers: &'static [&'static str],
    pub swallow_markers: &'static [&'static str],
    pub test_name_prefixes: &'static [&'static str],
    /// Node kinds that wrap a body so a failure stops propagating —
    /// `try_statement` and its friends. **Read structurally, never as a
    /// substring**: the string `catch` appears in a great many honest
    /// identifiers, and `swallow_markers` below is the substring path this
    /// exists to escape.
    ///
    /// # Why it earns its place (Track G, measured)
    ///
    /// G measured both proposed additions against its own corpus and withdrew
    /// one of them. `skip_site` was withdrawn — comment blanking already gives
    /// what it was for — so `skip_in_body` stays a bool. **`catch_kinds` was
    /// kept**, and the numbers are the reason:
    ///
    /// - **Ruby has no swallow detection at all without it.** The substring
    ///   path needs a `swallow_markers` hit beside one of `pass` / `{}` /
    ///   `continue`, and a Ruby `begin … rescue … end` has none of the three.
    ///   BUILD-SPEC §6.7 files *"test body wrapped in try/except-pass"* under
    ///   `fail`, so that is absent coverage rather than a tuning choice.
    /// - **C#'s honest and cheating `try`/`catch` are the same string.** Both
    ///   put an assertion in the guarded block and leave the handler empty, so
    ///   a substring rule is wrong about one of them whichever way it is tuned.
    ///
    /// **Empty for all five built-in languages**, so this build's behaviour is
    /// unchanged by its arrival — asserted, not assumed, by
    /// `catch_kinds_is_inert_for_every_built_in_language`. Migrating the five
    /// onto it is a re-measurement of each and is deliberately not done here.
    pub catch_kinds: &'static [&'static str],
}

/// How strong an assertion is.
///
/// Ordered weakest-first, so "loosened" is a comparison rather than a table of
/// pairs — adding a shape does not mean revisiting every combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Strength {
    /// `assert x` / `expect(x).toBeTruthy()` — asserts almost nothing.
    Truthy,
    /// `in`, `contains`, `matches` — asserts membership.
    Containment,
    /// `approx`, `almost`, a tolerance — asserts nearness.
    Approximate,
    /// `==`, `assert_eq!`, `toEqual` — asserts identity.
    Equality,
}

/// One test function, reduced to what §6.7 compares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestFn {
    pub name: String,
    pub assertions: Vec<Strength>,
    /// Mock/patch targets named in this test: `patch("app.rate")`,
    /// `jest.mock('./shipping')`, `mockito.when(...)`.
    pub mock_targets: Vec<String>,
    pub has_skip_marker: bool,
    pub swallows: bool,
    pub has_sleep_or_retry: bool,
    /// Normalised tokens of the body, for rename pairing. Compared as a
    /// **set**, not a multiset — see [`similar_bodies`].
    pub body_tokens: Vec<String>,
}

impl TestFn {
    fn strongest(&self) -> Option<Strength> {
        self.assertions.iter().copied().max()
    }
}

/// Something went wrong parsing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TamperError {
    #[error("the {0:?} grammar could not be loaded")]
    Grammar(Language),
    #[error("{path} could not be parsed as {language:?}")]
    Parse { path: String, language: Language },
}

/// Extract every test function from a source file.
pub fn parse_tests(source: &str, language: Language) -> Result<Vec<TestFn>, TamperError> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language.ts_language())
        .map_err(|_| TamperError::Grammar(language))?;
    let tree = parser.parse(source, None).ok_or(TamperError::Parse {
        path: String::new(),
        language,
    })?;
    let spec = language.spec();
    let mut out = Vec::new();
    let mut cursor = tree.walk();
    collect(&mut cursor, source, &spec, language, &mut out);
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// The same walk, driven by a [`LangPack`] instead of the closed enum.
///
/// **This is what makes the published interface load-bearing rather than a
/// lookup table.** Without it a pack could be registered and found, and nothing
/// would ever read its [`LangSpec`] — which is the shape of a registry that
/// compiles and does nothing.
///
/// The per-language judgements this crate still keys on `Language` (the Rust
/// attribute rule, the JS callee rule, the Python and Go name rules, the
/// visibility rule) are **not** available to an external pack, and the language
/// argument is what supplies them. A pack whose rules differ brings its own
/// walk — `crates/langpacks` does — and uses this for the parts that are
/// table-driven. That limit is real and is stated here rather than discovered:
/// `judged_as` is not a hook, it is a confession.
pub fn parse_tests_with_pack(
    source: &str,
    pack: &dyn LangPack,
    judged_as: Language,
) -> Result<Vec<TestFn>, TamperError> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&pack.ts_language())
        .map_err(|_| TamperError::Grammar(judged_as))?;
    let tree = parser.parse(source, None).ok_or(TamperError::Parse {
        path: String::new(),
        language: judged_as,
    })?;
    let spec = pack.spec();
    let mut out = Vec::new();
    let mut cursor = tree.walk();
    collect(&mut cursor, source, &spec, judged_as, &mut out);
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn collect(
    cursor: &mut tree_sitter::TreeCursor,
    source: &str,
    spec: &LangSpec,
    language: Language,
    out: &mut Vec<TestFn>,
) {
    let node = cursor.node();
    if spec.fn_kinds.contains(&node.kind()) {
        if let Some(test) = as_test_fn(node, source, spec, language) {
            out.push(test);
            // Do not descend into a test we already summarised: a nested
            // closure's asserts belong to it.
            return;
        }
    }
    if cursor.goto_first_child() {
        loop {
            collect(cursor, source, spec, language, out);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
}

fn as_test_fn(
    node: tree_sitter::Node,
    source: &str,
    spec: &LangSpec,
    language: Language,
) -> Option<TestFn> {
    // Read only to prove the node has valid text before anything else runs;
    // every text-derived fact below reads `blanked` instead. See R6.
    let _ = node.utf8_text(source.as_bytes()).ok()?;
    // A JS/TS test is marked by its CALLEE (`it(...)`, `it.skip(...)`), not by
    // the name it is given — `it('adds', …)` is a test called "adds". Reading
    // the name and then requiring it to start with "it" found nothing at all,
    // which is how the whole JS/TS path came back empty.
    let js = js_test_call(node, source, language);
    let js_skipped = js.as_ref().is_some_and(|j| j.skipped);
    let name = match &js {
        Some(j) => j.name.clone(),
        None => node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source.as_bytes()).ok())
            .map(str::to_string)?,
    };

    // Is it a test at all?
    let is_test = match language {
        Language::Rust => decorators_of(node, source, language).contains("#[test]"),
        Language::JavaScript | Language::TypeScript => js.is_some(),
        _ => {
            spec.test_name_prefixes.is_empty()
                || spec.test_name_prefixes.iter().any(|p| {
                    name.to_ascii_lowercase()
                        .starts_with(&p.to_ascii_lowercase())
                })
        }
    };
    if !is_test {
        return None;
    }

    // **R6: comments are blanked before any text-derived fact is read.**
    //
    // Track G measured what the raw text costs, and it is the most expensive
    // error this product can make. `strip_string_literals` treats `'` as a
    // literal delimiter and knows nothing about comments, so an apostrophe in a
    // comment opens a literal that never closes and truncates everything after
    // it. The shortened token set falls below `RENAME_SIMILARITY` in
    // `similar_bodies`, and an honest rename is reported as `test_deleted` at
    // **`fail`**. G's fixture is a Python rename that also rewords its comment
    // from `isn't` to `is non-negative`, with all three assertions untouched;
    // the control — the same rename with `is not` — comes back `test_renamed`,
    // so the apostrophe is the whole difference.
    //
    // Three facts reached it by that route, not one — `body_tokens`,
    // `has_sleep_or_retry`, and the in-body skip-marker search, which is the
    // same class of failure this crate's own Python control fixture caught when
    // `# we used to @pytest.mark.skip this` was read as a decorator. A fourth,
    // `mock_targets`, was on the same route and is blanked here too rather than
    // left as the one raw path: leaving one is how a fixed bug comes back
    // wearing a different field name.
    let blanked = blank_comments(node, source);
    let lower = blanked.to_ascii_lowercase();
    let mut assertions = Vec::new();
    let mut cursor = node.walk();
    collect_assertions(&mut cursor, source, spec, &mut assertions);

    let decorators = decorators_of(node, source, language).to_ascii_lowercase();

    // The body, minus its own name and minus string literals, as a token
    // sequence — compared as a **SET** by `similar_bodies`, so repetition is
    // discarded.
    //
    // **This is the switch site.** If anyone changes the comparison to a
    // multiset (which is the better behaviour), it happens here and in
    // `similar_bodies` together — and `RENAME_SIMILARITY` must be re-measured
    // first, because the table that chose 0.5 was measured against set
    // semantics. The instruction lives in `similar_bodies`' docs; this comment
    // exists because a future reader trusts the comment nearest the code, and
    // this one used to say "multiset" while the code built a set.
    let body_tokens: Vec<String> = strip_string_literals(&blanked)
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|t| t.len() > 1 && *t != name)
        .map(|t| t.to_ascii_lowercase())
        .collect();

    let mock_targets = mock_targets_in(&blanked);

    Some(TestFn {
        name,
        assertions,
        mock_targets,
        body_tokens,
        has_skip_marker: spec
            .skip_markers
            .iter()
            .any(|m| decorators.contains(&m.to_ascii_lowercase()))
            || (spec.skip_in_body
                && spec
                    .skip_markers
                    .iter()
                    .any(|m| lower.contains(&m.to_ascii_lowercase())))
            || js_skipped,
        // Two paths, and the second exists because the first cannot see the
        // languages G measured. **Either** is a swallow.
        swallows: (spec
            .swallow_markers
            .iter()
            .any(|m| lower.contains(&m.to_ascii_lowercase()))
            && (lower.contains("pass") || lower.contains("{}") || lower.contains("continue")))
            || swallows_structurally(node, source, spec),
        has_sleep_or_retry: lower.contains("sleep(") || lower.contains("retry"),
    })
}

/// A node's source text with every comment span replaced by spaces.
///
/// **Byte-length preserving**, so any offset computed against the original
/// still lands in the same place, and newlines are kept so line structure
/// survives. That is `crates/langpacks`' `blank_comments` contract, matched
/// here deliberately: two implementations of the same idea that disagree about
/// offsets is worse than either.
///
/// Comment nodes are found by **kind containing `comment`** rather than by a
/// per-grammar list. Python and Go call them `comment`, Rust `line_comment` and
/// `block_comment`, JavaScript and TypeScript `comment`. Scanning the text for
/// `//` or `#` instead is the thing this exists to avoid: those sequences occur
/// inside string literals, which is how the bug being fixed got in.
fn blank_comments(node: tree_sitter::Node, source: &str) -> String {
    let start = node.start_byte();
    let mut out = source[node.byte_range()].as_bytes().to_vec();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
        if !n.kind().contains("comment") {
            continue;
        }
        for i in n.start_byte()..n.end_byte() {
            // Guard rather than assume: a node can in principle report a range
            // outside its parent, and an out-of-bounds write here would panic
            // inside a detector that must never take the process down.
            if i < start || i - start >= out.len() {
                continue;
            }
            if out[i - start] != b'\n' {
                out[i - start] = b' ';
            }
        }
    }
    // The bytes were ASCII spaces substituted for whole comment spans, so this
    // is still valid UTF-8; `from_utf8_lossy` rather than an unwrap because
    // this crate does not panic on input.
    String::from_utf8_lossy(&out).into_owned()
}

/// The structural swallow reading `LangSpec::catch_kinds` enables.
///
/// **A handler swallows when the assertions are inside the guarded block and
/// the handler neither asserts nor rethrows.** That is G's rule, and it is the
/// one the substring path cannot express: the honest idiom
/// (`assertThrows`, `Assert.Throws`, `expect { }.to raise_error`) and the cheat
/// (`try { assert } catch { }`) share every substring a marker list can see, so
/// a substring rule is wrong about one of them whichever way it is tuned.
///
/// Returns `false` immediately when `catch_kinds` is empty, which is every
/// built-in language today — so this walk does not run at all for them, and
/// their behaviour is unchanged. `catch_kinds_is_inert_for_every_built_in_language`
/// is the assertion, because "no built-in sets it" is a fact about a table that
/// somebody will one day edit.
fn swallows_structurally(node: tree_sitter::Node, source: &str, spec: &LangSpec) -> bool {
    if spec.catch_kinds.is_empty() {
        return false;
    }
    let asserts = |text: &str| -> bool {
        let lower = strip_string_literals(text).to_ascii_lowercase();
        spec.assertion_call_prefixes
            .iter()
            .any(|p| lower.contains(&p.to_ascii_lowercase()))
            || lower.contains("assert")
    };
    // `raise`/`throw` bare, so a handler that re-raises is not a swallow. Read
    // off the stripped text for the same reason everything else here is: a
    // string literal containing the word is not a rethrow.
    // **Whole tokens, not substrings** — Track G's R10, and it is this crate's
    // bug inherited from G's fixed one. `["throw"].contains()` matches
    // `catch (Throwable t) {}`, so the commonest Java swallow of all reads as
    // re-throwing and is exempted. Harmless only while every built-in ships
    // `catch_kinds: &[]`; it fires the moment a registered pack fills it, which
    // is exactly what registering G's Java and C# packs does. G fixed its copy
    // at `da7243a` with a whole-token match; this is the same fix, and the two
    // implementations of one idea must not disagree about what a keyword is.
    let rethrows = |text: &str| -> bool {
        let lower = strip_string_literals(text).to_ascii_lowercase();
        lower
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|tok| matches!(tok, "raise" | "throw" | "rethrow" | "panic"))
    };

    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
        if !spec.catch_kinds.contains(&n.kind()) {
            continue;
        }
        // The guarded block is the first NAMED child; everything after it is
        // handler material. Named, not first-of-all: `n.children()` includes
        // anonymous tokens, so on Python's `try_statement` the first child is
        // the keyword `try` and the split put the whole body in the handler —
        // measured, and it made the detector silent on the exact fixture it was
        // written for. Named children are `block` then `except_clause` in
        // Python, `block` then `catch_clause` in Java and C#, `body_statement`
        // then `rescue` in Ruby, which is the shape this rule assumes.
        let mut c = n.walk();
        let children: Vec<tree_sitter::Node> = n.named_children(&mut c).collect();
        let Some((guarded, handlers)) = children.split_first() else {
            continue;
        };
        let guarded_text = &source[guarded.byte_range()];
        if !asserts(guarded_text) {
            continue;
        }
        let handler_text: String = handlers
            .iter()
            .map(|h| &source[h.byte_range()])
            .collect::<Vec<_>>()
            .join("\n");
        if !asserts(&handler_text) && !rethrows(&handler_text) {
            return true;
        }
    }
    false
}

/// A JS/TS test call: `it("name", …)`, `test.skip("name", …)`, `xit("name")`.
///
/// The name is an argument and the *callee* is what marks it a test, so both
/// come from here rather than from the generic `name` field.
struct JsTest {
    name: String,
    skipped: bool,
}

fn js_test_call(node: tree_sitter::Node, source: &str, language: Language) -> Option<JsTest> {
    if !matches!(language, Language::JavaScript | Language::TypeScript) {
        return None;
    }
    if node.kind() != "call_expression" {
        return None;
    }
    let callee = node
        .child_by_field_name("function")?
        .utf8_text(source.as_bytes())
        .ok()?;
    // `it`, `test`, `describe`, and their `.skip` / `.only` / `x`-prefixed
    // forms. A member expression is the shape `it.skip` parses to.
    let (base, suffix) = match callee.split_once('.') {
        Some((b, s)) => (b, s),
        None => (callee, ""),
    };
    let skipped = suffix == "skip" || base.starts_with('x');
    let bare = base.trim_start_matches('x');
    if !matches!(bare, "it" | "test" | "describe") {
        return None;
    }
    let args = node.child_by_field_name("arguments")?;
    let first = args.named_child(0)?;
    Some(JsTest {
        name: first
            .utf8_text(source.as_bytes())
            .ok()?
            .trim_matches(['"', '\'', '`'])
            .to_string(),
        skipped,
    })
}

fn collect_assertions(
    cursor: &mut tree_sitter::TreeCursor,
    source: &str,
    spec: &LangSpec,
    out: &mut Vec<Strength>,
) {
    let node = cursor.node();
    if spec.assertion_kinds.contains(&node.kind()) {
        if let Ok(text) = node.utf8_text(source.as_bytes()) {
            let head = text.trim_start().to_ascii_lowercase();
            let looks_like_assertion = spec
                .assertion_call_prefixes
                .iter()
                .any(|p| head.starts_with(&p.to_ascii_lowercase()))
                || head.starts_with("assert");
            if looks_like_assertion {
                out.push(strength_of(text));
            }
        }
    }
    if cursor.goto_first_child() {
        loop {
            collect_assertions(cursor, source, spec, out);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
}

/// Every production call site of [`strength_of`], declared.
///
/// **The one vector the escalation gate cannot see.** That gate proves the
/// classifier strips literals and that a message-only edit cannot refute a
/// claim — but it proves it about `strength_of`. A *new* caller that read
/// shapes some other way, or that compared strengths without going through
/// `diff_test_file`, would be outside everything the gate checks.
///
/// So the call sites are enumerated, in the idiom `redact::MASK_CALL_SITES`
/// established: a `strength_of(` call anywhere in the workspace that is not
/// listed here fails the build, deliberately. Adding one is a decision someone
/// makes in writing.
pub const SHAPE_CALL_SITES: &[(&str, &str, &str)] = &[(
    "tamper.rs",
    "collect_assertions",
    "the only production reader: records each assertion's shape while walking a \
     test function, for `diff_test_file` to compare between trees",
)];

/// Remove string literals from an assertion before its shape is read.
///
/// **The gate the shape classifier had to pass before it could reach `fail`.**
/// Measured across 15,835 real assertions in 833 files, 13 (0.082%) are
/// ambiguous, and every one is a message string:
/// `assert!(…, "git {args:?} in {cwd:?}")` reads as a containment because of
/// the word `in` inside the *message*. That was tolerable while the finding was
/// warn-only. It is not tolerable now that a weakening can be a `fail`, so the
/// literals go before the shape is read — three lines here rather than four
/// per-grammar comparison matchers.
///
/// Handles `"…"`, `'…'`, and Rust raw strings `r"…"` / `r#"…"#`.
pub fn strip_string_literals(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' || c == '\'' {
            let quote = c;
            i += 1;
            while i < chars.len() {
                if chars[i] == '\\' {
                    i += 2;
                    continue;
                }
                if chars[i] == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            // A space, so `assert x=="a"` does not become `assert x==`.
            out.push(' ');
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Classify an assertion's strength from its own source text.
///
/// The node was selected by the parser, so this never sees a comment or an
/// unrelated expression, and [`strip_string_literals`] removes the message
/// strings that were the remaining 0.082%.
pub fn strength_of(text: &str) -> Strength {
    let lower = strip_string_literals(text).to_ascii_lowercase();
    if lower.contains("approx") || lower.contains("almost") || lower.contains("tolerance") {
        return Strength::Approximate;
    }
    if lower.contains("==")
        || lower.contains("assert_eq!")
        || lower.contains("toequal")
        || lower.contains("tobe(")
        || lower.contains("assertequal")
        || lower.contains("!=")
        || lower.contains("assert_ne!")
    {
        return Strength::Equality;
    }
    if lower.contains(" in ")
        || lower.contains("contains")
        || lower.contains("tocontain")
        || lower.contains("assertin")
        || lower.contains("matches")
        || lower.contains("startswith")
    {
        return Strength::Containment;
    }
    Strength::Truthy
}

/// The decorators or attributes attached to a function, from the TREE.
///
/// Counting lines backwards from the node was a bug with its own control
/// fixture: four lines before `test_rate_large` reaches into
/// `test_rate_small`'s *body*, so a comment there — `# we used to
/// @pytest.mark.skip this` — was read as a decorator on the next function.
/// Attachment is a structural fact and has to be read structurally.
fn decorators_of(node: tree_sitter::Node, source: &str, language: Language) -> String {
    match language {
        // Python wraps a decorated function in `decorated_definition`, whose
        // `decorator` children are exactly the decorators that apply.
        Language::Python => {
            let Some(parent) = node.parent() else {
                return String::new();
            };
            if parent.kind() != "decorated_definition" {
                return String::new();
            }
            let mut out = String::new();
            let mut cursor = parent.walk();
            for child in parent.named_children(&mut cursor) {
                if child.kind() == "decorator" {
                    if let Ok(t) = child.utf8_text(source.as_bytes()) {
                        out.push_str(t);
                        out.push('\n');
                    }
                }
            }
            out
        }
        // Rust attributes attach as siblings immediately before the item.
        Language::Rust => {
            let mut out = String::new();
            let mut sibling = node.prev_sibling();
            let mut seen = 0;
            while let Some(n) = sibling {
                if n.kind() == "attribute_item" {
                    if let Ok(t) = n.utf8_text(source.as_bytes()) {
                        out.push_str(t);
                        out.push('\n');
                    }
                } else if n.kind() != "line_comment" && n.kind() != "block_comment" {
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
        // JS/TS and Go carry the marker in the call itself; `skip_in_body`
        // handles those.
        _ => String::new(),
    }
}

/// The exported/public symbols a source file declares.
///
/// §6.5 needs *"no public API signature changes detected (tree-sitter symbol
/// diff of exported/pub items)"*. Names and their parameter arity, not bodies:
/// a refactor may rewrite a body freely, and the claim it makes is about the
/// interface.
///
/// Private items are deliberately excluded — renaming a private helper is the
/// most ordinary refactor there is, and flagging it would make
/// `behavior_unchanged` unusable for exactly the change it is meant to permit.
pub fn public_symbols(source: &str, language: Language) -> Result<Vec<String>, TamperError> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language.ts_language())
        .map_err(|_| TamperError::Grammar(language))?;
    let tree = parser.parse(source, None).ok_or(TamperError::Parse {
        path: String::new(),
        language,
    })?;
    let mut out = Vec::new();
    let mut cursor = tree.walk();
    collect_symbols(&mut cursor, source, language, &mut out);
    out.sort();
    out.dedup();
    Ok(out)
}

fn collect_symbols(
    cursor: &mut tree_sitter::TreeCursor,
    source: &str,
    language: Language,
    out: &mut Vec<String>,
) {
    let node = cursor.node();
    let kind = node.kind();
    let is_decl = matches!(
        kind,
        "function_definition"
            | "function_item"
            | "function_declaration"
            | "class_definition"
            | "class_declaration"
            | "struct_item"
            | "enum_item"
            | "trait_item"
            | "type_declaration"
            | "method_declaration"
    );
    if is_decl {
        if let Some(name) = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source.as_bytes()).ok())
        {
            let public = match language {
                // Rust: only `pub` items are the interface.
                //
                // **`Err` means "unreadable", not "private".** This was
                // `unwrap_or(false)`, so a Rust file that is not valid UTF-8
                // made every item in it look private — and §6.5's
                // `behavior_unchanged` reads "no public API changed" as
                // evidence *for* the claim, so the under-report lands on the
                // supporting side. Vanishingly rare, and the same
                // fallback-for-a-fact class as everything else swept this
                // round: a default answering a question the code could not
                // answer. Unreadable source is treated as public, which is the
                // direction that asks for a look rather than granting one.
                Language::Rust => node
                    .utf8_text(source.as_bytes())
                    .map_or(true, |t| t.trim_start().starts_with("pub")),
                // Python: a leading underscore is the convention for private.
                Language::Python => !name.starts_with('_'),
                // Go: an initial capital is the language rule.
                Language::Go => name.chars().next().is_some_and(char::is_uppercase),
                // JS/TS: exported, which the parent `export_statement` marks.
                Language::JavaScript | Language::TypeScript => node
                    .parent()
                    .is_some_and(|p| p.kind().starts_with("export")),
            };
            if public {
                let arity = node
                    .child_by_field_name("parameters")
                    .map(|p| p.named_child_count())
                    .unwrap_or(0);
                out.push(format!("{name}/{arity}"));
            }
        }
    }
    if cursor.goto_first_child() {
        loop {
            collect_symbols(cursor, source, language, out);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
}

/// Compare a test file between the base tree and the head tree.
pub fn diff_test_file(
    path: &str,
    base: &str,
    head: &str,
    language: Language,
) -> Result<Vec<Finding>, TamperError> {
    diff_test_file_with_symbols(path, base, head, language, &[])
}

/// [`diff_test_file`], plus the symbols that changed elsewhere in the session —
/// which is what §6.7's mock detector needs to resolve a mock target against.
pub fn diff_test_file_with_symbols(
    path: &str,
    base: &str,
    head: &str,
    language: Language,
    changed_symbols: &[String],
) -> Result<Vec<Finding>, TamperError> {
    let base_fns = parse_tests(base, language)?;
    let head_fns = parse_tests(head, language)?;
    let head_by_name: BTreeMap<&str, &TestFn> =
        head_fns.iter().map(|f| (f.name.as_str(), f)).collect();
    let base_by_name: BTreeMap<&str, &TestFn> =
        base_fns.iter().map(|f| (f.name.as_str(), f)).collect();

    let mut findings = Vec::new();
    for base_fn in &base_fns {
        let Some(head_fn) = head_by_name.get(base_fn.name.as_str()) else {
            // §6.7's text is *"test renamed **to escape a pattern**"*, and the
            // qualifier is the whole rule. Failing every rename accused routine
            // refactoring of cheating — measured: a refactor that renames two
            // tests produced `test_deleted` twice and `test_added` twice, and
            // the claim came out `unsupported`.
            //
            // A deleted test whose body reappears under a new name is a rename.
            // A deleted test whose body is nowhere is a deletion.
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
                    // §6.7: an assertion REMOVED is fail; a count merely
                    // decreased is warn. "All of them" is the removal case.
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
                // §6.7 calls a loosening `warn` and a REMOVAL `fail`. An
                // assertion loosened all the way to `Truthy` asserts nothing at
                // all, so it is a removal wearing an assert's clothes —
                // `assert rate(2) == 12.5` becoming `assert rate(2) > 0`, or an
                // assertion moved into a helper that returns True. Both came out
                // `supported` while the finding was warn-only.
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
                    // **A candidate, not a conclusion.** The detector sees a marker;
                    // only the RUN says whether it took effect, and a marker that
                    // changes no outcome is not evidence of anything. Firing `fail`
                    // here accused every deliberate
                    // `@pytest.mark.skipif(sys.platform == "win32")` of cheating —
                    // four of five honest edit shapes came out `unsupported`.
                    // `tests_pass::escalate` raises this to `fail` when the run
                    // reports skipped tests, and lowers it to `info` when it does not.
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
            if mock_hits_changed_symbol(target, changed_symbols) {
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
            // A test that is the other half of a rename is already reported as
            // one; reporting it again as "new" doubles a non-finding.
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
    Ok(findings)
}

/// Mock and patch calls, and what they name.
///
/// §6.7's `warn`: *"new mock/patch whose target resolves to a symbol changed in
/// the diff (mocking the unit under test)"*. The target is the first string
/// literal argument, which is where every one of these libraries puts it.
fn mock_targets_in(text: &str) -> Vec<String> {
    const CALLS: &[&str] = &[
        "patch(",
        "patch.object(",
        "mock.patch(",
        "mocker.patch(",
        "jest.mock(",
        "vi.mock(",
        "sinon.stub(",
        "monkeypatch.setattr(",
        "unittest.mock.patch(",
    ];
    let mut out = Vec::new();
    for call in CALLS {
        let mut from = 0usize;
        while let Some(at) = text[from..].find(call) {
            let start = from + at + call.len();
            let rest = &text[start..];
            // The first quoted argument.
            if let Some(q) = rest.find(['"', '\'']) {
                let quote = rest.as_bytes()[q] as char;
                if let Some(end) = rest[q + 1..].find(quote) {
                    let target = &rest[q + 1..q + 1 + end];
                    if !target.is_empty() && target.len() < 200 {
                        out.push(target.to_string());
                    }
                }
            }
            from = start;
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Does a mock target name a symbol that changed in this session?
///
/// `app.shipping.rate` names `rate`; `./shipping` names `shipping`. Matched on
/// the last dotted or slashed component and on the whole string, so both the
/// module form and the symbol form resolve.
pub fn mock_hits_changed_symbol(target: &str, changed_symbols: &[String]) -> bool {
    let leaf = target
        .rsplit(['.', '/'])
        .next()
        .unwrap_or(target)
        .trim_end_matches(".py")
        .trim_end_matches(".js")
        .trim_end_matches(".ts");
    changed_symbols.iter().any(|s| {
        let name = s.split('/').next().unwrap_or(s);
        name == leaf || name == target
    })
}

/// Are two test functions the same test under different names?
///
/// Jaccard over the **set** of body tokens.
///
/// # The threshold is 0.5, and the reason is not the table
///
/// Measured across 21,042 test functions in 833 files:
///
/// | t | missed cheat (distinct tests pairing) | false accusation (rename + 3 edits) |
/// |---|---|---|
/// | 0.5 | 2.00% | **3.33%** |
/// | 0.6 | 1.14% | 9.60% |
/// | 0.7 | 0.69% | 25.32% |
///
/// The missed-cheat column measures only *accidental* pairing, and that is the
/// column that does not matter: **an agent that knows this rule simply writes a
/// replacement body ≥ t similar, so adversarially the miss rate is ~100% at any
/// threshold.** The threshold therefore buys nothing real on the cheat side and
/// does all of its genuine work on the false-accusation side — where 0.5 cuts a
/// moderately-edited rename's false accusation from 9.60% to 3.33%.
///
/// # One constant, not one per grammar
///
/// At 0.6 the accidental-pairing rate is **Python 1.13%, Rust 2.64%, JS/TS
/// 4.20%** — JS/TS is 3.7× Python, so one constant does serve the grammars
/// unequally. It stays one anyway: the axis on which they differ is the
/// accidental-pairing axis, which the paragraph above shows is worth nothing
/// against an adversary. Splitting the constant would be tuning three numbers
/// against the column that does not matter, while the column that does — false
/// accusation of a real rename — is not measured per grammar at all.
///
/// # Set, not multiset — and the disagreement this resolves
///
/// The doc used to say "token multiset" while the code built a `BTreeSet`, so
/// repetition was discarded: `assert x == 1` written three times scored the same
/// as once. A multiset is the better behaviour, and it is **not** adopted here,
/// because the table above was measured against set semantics. Changing the
/// comparison and the threshold in one step would leave the threshold
/// unmeasured again — the exact move this project has twice paid for. Whoever
/// switches to a multiset re-measures first.
pub fn similar_bodies(a: &TestFn, b: &TestFn) -> bool {
    if a.body_tokens.is_empty() || b.body_tokens.is_empty() {
        return false;
    }
    let sa: std::collections::BTreeSet<&String> = a.body_tokens.iter().collect();
    let sb: std::collections::BTreeSet<&String> = b.body_tokens.iter().collect();
    let intersection = sa.intersection(&sb).count() as f64;
    let union = sa.union(&sb).count() as f64;
    if union == 0.0 {
        return false;
    }
    intersection / union >= RENAME_SIMILARITY
}

/// See [`similar_bodies`] for the measurement behind this number.
pub const RENAME_SIMILARITY: f64 = 0.5;

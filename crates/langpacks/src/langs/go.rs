//! The Go pack: stdlib `testing`, testify (assert/require/suite/mock), gomock,
//! and `t.Run` subtests.
//!
//! # The one fact that shapes this whole file
//!
//! **Go's majority assertion has no assertion call in it.** The shape is
//!
//! ```text
//! if got != want {
//!     t.Errorf("Rate(2) = %v, want %v", got, want)
//! }
//! ```
//!
//! and the comparison — the thing whose *strength* §6.7 measures — lives in the
//! `if`, not in the call. A classifier that reads `t.Errorf("…", got, want)`
//! sees a format string and two identifiers and returns [`Strength::Truthy`],
//! and since a weakening **to** `Truthy` is a `fail`, every honest edit to a
//! stdlib Go test would have produced a `fail` finding. That is why
//! `assertion_kinds` carries `if_statement` and why [`Go::classify`] reads the
//! condition rather than the body.
//!
//! `crates/verify`'s Go arm already had `if_statement` in `assertion_kinds` and
//! `t.error`/`t.fatal` in `assertion_call_prefixes` — but nothing in that
//! prefix list can match an `if_statement`'s text (which starts with `if`), so
//! the `if_statement` kind was **dead**: every stdlib Go assertion was reaching
//! the classifier as its `t.Errorf` call.
//!
//! **How much that is, measured rather than asserted.** Re-run this session
//! over 2,279 `_test.go` files from the corpus (`go` stdlib, cobra, gin,
//! prometheus, testify) with `collect_assertions`' own admission rule: 63,491
//! assertion nodes admitted, of which **44,672 — 70.4% — are `if_statement`s**,
//! the one shape `verify`'s prefix list cannot admit. What it admits in their
//! place is the `t.Errorf` inside, and `strength_of` on a format string and two
//! identifiers is `Truthy` — the floor from which `assertion_weakened` cannot
//! fire. `tests/verify_go_shape_arm_is_dead.rs` holds that mechanism to a test.
//!
//! # The second fact: an `if` that does not fail the test asserts nothing
//!
//! `if got != want { t.Logf("mismatch") }` still has `!=` in it and still
//! cannot fail. So does `if testing.Short() { t.Skip() }`. [`Go::classify`]
//! therefore requires a *failing* call — `t.Error*`, `t.Fatal*`, `panic(`, an
//! `assert.`/`require.` call — somewhere in the statement before it will read
//! the condition at all, and returns `Truthy` when there is none. This is what
//! makes the `t.Errorf` → `t.Logf` downgrade — Go's version of swallowing a
//! failure — visible as `assertion_weakened` to `Truthy`, a `fail`.

use tree_sitter::Node;

use crate::pack::{LangPack, LangSpec, PackLanguage, SkipSite, Strength};

pub struct Go;

impl LangPack for Go {
    fn language(&self) -> PackLanguage {
        PackLanguage::Go
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["go"]
    }

    fn ts_language(&self) -> tree_sitter::Language {
        tree_sitter_go::LANGUAGE.into()
    }

    fn spec(&self) -> LangSpec {
        LangSpec {
            // `method_declaration` is not decoration: a testify **suite** puts
            // every test in `func (s *CalcSuite) TestFoo()`, which is a
            // `method_declaration` and not a `function_declaration`. Without it
            // a suite-based file parses to zero tests, and zero tests means
            // `test_deleted` can never fire for it — silence that reads as
            // "nothing was removed".
            fn_kinds: &["function_declaration", "method_declaration"],

            // `if_statement` carries the stdlib comparison; `call_expression`
            // carries testify/gomega/cmp. See the module docs.
            assertion_kinds: &["call_expression", "if_statement"],

            // Bare prefixes must START the node's text; a prefix beginning with
            // `.` is matched as an infix anywhere in the first line
            // (`tamper::is_assertion`).
            //
            // `"if "` is the entry point for the stdlib shape and it is
            // deliberately broad: it admits every top-most `if` in a test body,
            // including control flow. That over-count is paid for in
            // `classify`, which returns `Truthy` for any `if` that cannot fail
            // the test — so a control-flow `if` never raises a function's
            // strongest assertion and never masks a real weakening. Counting it
            // in `assertions.len()` is symmetric between the base and head
            // trees, so `assertion_count_decreased` compares like with like.
            //
            // `.len(` is deliberately ABSENT: `buf.Len()` and `list.Len()` are
            // ordinary methods, and admitting them would classify a plain
            // length read as an `Equality` assertion and mask a weakening.
            // `assert.Len` still reaches the classifier through `assert.`.
            assertion_call_prefixes: &[
                "if ",
                "assert.",
                "require.",
                "reflect.deepequal(",
                "cmp.diff(",
                "gomega.",
                "t.error",
                "t.fatal",
                "b.error",
                "b.fatal",
                // The infix forms exist for testify suites, whose receiver is
                // the suite value and never `assert`: `s.Equal(…)`,
                // `s.Require().NoError(…)`.
                ".equal(",
                ".noerror(",
                ".errorf(",
                ".fatalf(",
                ".contains(",
                ".true(",
                ".false(",
                ".require().",
            ],

            // Receiver-agnostic on purpose — table-driven tests routinely name
            // the parameter `tt`. `is_skip_marker` is overridden below so these
            // are matched on comment-stripped, literal-stripped text; the
            // default would match `// we deleted the t.Skip here`, which is the
            // exact regex failure `verify`'s own control fixture caught for
            // Python.
            skip_markers: &[".skip(", ".skipf(", ".skipnow("],

            // **Empty, and named as a gap rather than filled.** Go has no
            // `catch`. Its swallow shape is
            // `defer func() { recover() }()`, which the generic substring path
            // cannot see (it requires `pass`/`{}`/`continue` beside the marker,
            // and a `recover()` body has none of them) and which the structural
            // `catch_kinds` path cannot see either (a `defer_statement`'s child
            // is a `call_expression`, so `catch_swallows` finds no guarded
            // block and returns false before it looks at anything). A marker
            // that can never fire is coverage that does not exist, so there is
            // none. The Go-specific swallow that IS caught is the
            // `t.Errorf` → `t.Logf` downgrade, and it is caught by `classify`
            // as a weakening to `Truthy`, not by this field.
            swallow_markers: &[],

            // Present for parity with `verify::tamper::LangSpec`. The real rule
            // is `test_name` below, which implements `testing`'s own `isTest`:
            // the rune after the prefix must not be lower-case, so `Testify`
            // and `testHelper` are not tests however much they look like
            // prefixes.
            test_name_prefixes: &["Test", "Benchmark", "Fuzz", "Example"],

            // `t.Skip()` is a call in the function's own body.
            skip_site: SkipSite::Body,
            decorator_kinds: &[],
            catch_kinds: &[],
            defer_kinds: &["defer_statement"],
            neutralising_calls: &["recover("],
            unconditional_fail_markers: &[],
        }
    }

    /// Is this node a test, and what does the **runner** call it?
    ///
    /// # Why the parameter type is checked and not just the name
    ///
    /// `go test` runs a function iff its name has one of the four prefixes,
    /// the rune after the prefix is not lower-case, and its signature matches
    /// the prefix. `func TestHelper(x int) int` is a helper that happens to be
    /// named like a test; counting it would put a non-test in the base tree's
    /// set and report `test_deleted` when someone inlines it.
    ///
    /// Three consequences of following the runner rather than a prefix list:
    ///
    /// - **`Testify` is not a test.** `testing.isTest` rejects a lower-case
    ///   rune after the prefix. `verify`'s shared path lower-cases the name
    ///   before comparing, so it accepts `testHelper` too.
    /// - **`TestMain(m *testing.M)` is not a test.** It is the harness entry
    ///   point; it never appears in the runner's output as a test, and a diff
    ///   that reports it as deleted is reporting a build change as a coverage
    ///   loss.
    /// - **`ExampleFoo()` takes no parameters** and is still a test, because
    ///   `go test` compares its stdout to the `// Output:` comment. It is
    ///   admitted only with an empty parameter list, which is the only form the
    ///   runner accepts.
    ///
    /// # Subtests are NOT separate tests here, and that is a decision
    ///
    /// `t.Run("upper", func(t *testing.T){…})` produces its own `--- PASS:
    /// TestX/upper` line and its own `-json` events, so the runner does treat
    /// it as a test. The detector does not, for two reasons measured on the
    /// corpus rather than assumed:
    ///
    /// 1. **Most subtests have no static name.** The table-driven form is
    ///    `t.Run(tc.name, …)`, where the name is a struct field read at run
    ///    time. A name-keyed diff over a set half of whose members cannot be
    ///    named would report `test_deleted`/`test_added` churn on every table
    ///    edit.
    /// 2. **Nothing is lost.** `collect_assertions` walks the whole function
    ///    node including its `func_literal` children, so a subtest's assertions
    ///    are counted and classified with the parent's — `assertion_removed`
    ///    and `assertion_weakened` see a gutted subtest exactly as they see a
    ///    gutted test.
    ///
    /// What IS lost is the ability to say *which* subtest lost an assertion.
    /// That is named here rather than papered over.
    fn test_name(&self, node: Node, source: &str) -> Option<String> {
        let name = node
            .child_by_field_name("name")?
            .utf8_text(source.as_bytes())
            .ok()?;
        let params = node.child_by_field_name("parameters")?;
        let params_text = params.utf8_text(source.as_bytes()).ok()?;

        match node.kind() {
            "function_declaration" => {
                let prefix = go_test_prefix(name)?;
                let arity = parameter_count(params);
                match prefix {
                    // `TestMain` is the harness, not a test. It is recognised
                    // by its parameter type, not its name, so a genuine
                    // `func TestMain(t *testing.T)` — legal, and run — is still
                    // a test.
                    "Test" if takes(params_text, "M") => None,
                    "Test" if arity == 1 && takes(params_text, "T") => Some(name.to_string()),
                    "Benchmark" if arity == 1 && takes(params_text, "B") => Some(name.to_string()),
                    "Fuzz" if arity == 1 && takes(params_text, "F") => Some(name.to_string()),
                    "Example" if arity == 0 => Some(name.to_string()),
                    _ => None,
                }
            }
            // A testify suite method: a receiver, a `Test…` name, no
            // parameters. `suite.Run` reflects over exactly that set, and
            // `SetupTest`/`TearDownTest` are excluded by the prefix.
            "method_declaration" => {
                let prefix = go_test_prefix(name)?;
                (prefix == "Test"
                    && parameter_count(params) == 0
                    && node.child_by_field_name("receiver").is_some())
                .then(|| name.to_string())
            }
            _ => None,
        }
    }

    /// The assertion-shape classifier. **This is the load-bearing one.**
    ///
    /// Literals are stripped first — Go raw strings (backticks) locally,
    /// because `shape::strip_language_literals` knows Ruby's `%w[]`, C#'s `@""`
    /// and Java's `"""` but not Go's `` `…` `` — then the shared stripper, then
    /// comments. All three matter: `t.Errorf("want != got")`,
    /// `` t.Errorf(`want == got`) `` and `// was: assert.Equal` each carry an
    /// operator that would otherwise decide a shape.
    ///
    /// The order of the tests below is the whole design:
    ///
    /// 1. **Approximate by name** — `InDelta`, `InEpsilon`, `math.Abs(…) >`.
    /// 2. **Equality by name** — `DeepEqual`, `Equal`, `cmp.Diff`, `ErrorIs`.
    /// 3. **Containment by name** — `Contains`, `Subset`, `HasPrefix`.
    /// 4. **Equality by operator** — `==`, `!=`.
    /// 5. Otherwise `Truthy`.
    ///
    /// Names beat operators because `if strings.Contains(got, want) == false`
    /// asserts containment, not identity, and reading its `==` first would
    /// call it `Equality` and hide a later weakening to a real `Contains`.
    ///
    /// # Two shapes that are deliberately `Truthy`
    ///
    /// - **Ordering** (`if got > want`). The `Strength` ladder has no rung for
    ///   it, and the two candidates are not symmetric: calling it `Equality`
    ///   would make the cheat `if got != want` → `if got > 0` invisible, while
    ///   calling it `Truthy` makes an honest `==` → `>` edit a `fail` that a
    ///   human dismisses. Failing loud is the rule (12).
    /// - **`assert.True` / `NoError` / `NotNil`**. They assert a boolean, which
    ///   is what `Truthy` means. `assert.True(t, got == want)` is `Equality`,
    ///   because the operator is right there in the argument.
    ///
    /// # The one thing `verify`'s global classifier does that this must not
    ///
    /// `verify::tamper::strength_of` treats `" in "` as containment. Go has no
    /// `in` operator, and `in` is one of the most common identifiers in a Go
    /// test table (`in`/`out`, `in`/`want`). `if in != out` would have been
    /// read as containment by the shared table; it is `Equality` here.
    fn classify(&self, assertion_text: &str) -> Strength {
        let text = normalise(assertion_text);
        match if_condition(&text) {
            Some(condition) => {
                if !fails_the_test(&text) {
                    // An `if` that logs, skips, returns or continues cannot
                    // fail the test, so it asserts nothing — whatever operator
                    // its condition carries. This is the branch that catches
                    // the `t.Errorf` -> `t.Logf` downgrade.
                    return Strength::Truthy;
                }
                shape_of(&condition)
            }
            None => shape_of(&text),
        }
    }

    /// Mock/stub targets, in the four forms Go actually writes them.
    ///
    /// `tamper::first_target` reads the first quoted argument if there is one
    /// and otherwise the first identifier, which is what makes all four work:
    ///
    /// - `NewMockRate(ctrl)` — gomock and mockery put the target in the
    ///   **constructor's name**, so the prefix stops at `NewMock` and the
    ///   identifier that follows is the target.
    /// - `m.EXPECT().Rate(2)` — gomock's recorder; the method that follows is
    ///   the target.
    /// - `mockObj.On("Rate", 2)` — testify/mock; the target is quoted.
    /// - `monkey.Patch(time.Now, …)` — the target is a qualified identifier.
    ///
    /// `gomock.NewController(t)` is absent on purpose: it names no target, and
    /// an entry that resolves to `t` would make every gomock test look like it
    /// mocks a symbol called `t`.
    fn mock_calls(&self) -> &'static [&'static str] {
        &[
            "NewMock",
            ".EXPECT().",
            ".On(",
            "monkey.Patch(",
            "monkey.PatchInstanceMethod(",
            "gomonkey.ApplyFunc(",
            "httpmock.RegisterResponder(",
            "gock.New(",
        ]
    }

    /// Skip markers, read off text with comments and literals removed.
    ///
    /// The default implementation matches the raw function text, and for
    /// [`SkipSite::Body`] languages that text includes every comment in the
    /// body. `// the t.Skip() here was removed in #412` would have been
    /// reported as a skip marker added — the same failure `verify` documents
    /// for Python's `# we used to @pytest.mark.skip this`, in the one language
    /// where `verify` left the body path switched on.
    ///
    /// Conditional skips (`if testing.Short() { t.Skip() }`) DO count. They are
    /// still a marker, `skip_marker_added` is a `warn`, and
    /// `tests_pass::escalate` resolves it against the run: zero skipped tests
    /// in the output lowers it to `info`, a non-zero count raises it to `fail`.
    /// Deciding conditionality here would take that decision away from the only
    /// party that can make it — the run.
    fn is_skip_marker(&self, text: &str) -> bool {
        let clean = normalise(text);
        self.spec().skip_markers.iter().any(|m| clean.contains(m))
    }
}

/// Which of `go test`'s four prefixes this name carries, by `testing.isTest`'s
/// own rule.
///
/// > *"…the character after the prefix must not be a lower-case letter."*
///
/// So `Testify`, `Testing` and `Benchmarks` are ordinary functions and `Test`
/// alone is a test. Lower-casing the name before comparing — which is what the
/// shared prefix path in `verify::tamper::as_test_fn` does — accepts all four.
fn go_test_prefix(name: &str) -> Option<&'static str> {
    for prefix in ["Test", "Benchmark", "Fuzz", "Example"] {
        let Some(rest) = name.strip_prefix(prefix) else {
            continue;
        };
        return match rest.chars().next() {
            None => Some(prefix),
            Some(c) if c.is_lowercase() => None,
            Some(_) => Some(prefix),
        };
    }
    None
}

/// How many parameters a `parameter_list` declares.
///
/// Counts `parameter_declaration` children rather than commas: `func F(a, b
/// int)` is one declaration naming two parameters, and neither number is 1 for
/// the purposes above — but a test signature never has that form, so the
/// simpler count is the one that matches the runner.
fn parameter_count(params: Node) -> usize {
    let mut cursor = params.walk();
    params
        .named_children(&mut cursor)
        .filter(|n| n.kind() == "parameter_declaration")
        .count()
}

/// Does this parameter list take `*testing.<short>`, in either spelling?
///
/// Textual, with whitespace removed, because the alternative — walking
/// `pointer_type > qualified_type` — is the same test with more ways to be
/// wrong.
///
/// **The bare `*T` spelling is here because the corpus put it there.** Running
/// this pack over 2,196 `_test.go` files and comparing against a grep for
/// `^func Test` left eight files unexplained. Five were `main_test.go` files
/// holding nothing but `TestMain`, which is correct. The other three were
/// `go/src/testing/{sub,match,loop}_test.go` — the standard library's tests
/// **of the `testing` package**, which are `package testing` and so write
/// `func TestTRun(t *T)`. Requiring the qualified form silently dropped them.
/// A grep across the whole corpus finds that spelling in exactly those three
/// files and in no other package, so accepting it costs nothing measurable.
///
/// **Re-run this session, over 2,279 `_test.go` files** (the corpus is larger
/// than it was when the rule was found): the same comparison now leaves **six**
/// files unexplained and every one of them holds nothing but `TestMain` —
/// `syscall/syscall_freebsd_test.go`, `runtime/race/testdata/main_test.go` and
/// four `net`/`go/types` `main_test.go` files. The three `package testing`
/// files are no longer among them, which is this rule doing its job.
///
/// **Limitation, still named:** a file that imports `testing` under an alias
/// (`import qt "testing"`, giving `*qt.T`) is not recognised. No file in this
/// corpus does that. Its tests would be silently absent from the diff rather
/// than misread — the quiet direction, which is why it is written down.
fn takes(params_text: &str, short: &str) -> bool {
    let squeezed: String = params_text
        .trim_start_matches('(')
        .trim_end_matches(')')
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    squeezed.contains(&format!("*testing.{short}")) || squeezed.ends_with(&format!("*{short}"))
}

/// Strip everything that can put an operator into text that has none: Go raw
/// strings **and comments** in one scan, then the shared literal stripper.
/// Lower-cased.
///
/// Raw strings are handled here rather than in `shape::strip_language_literals`
/// because that function is shared with three other packs and backticks are not
/// a literal in any of them.
///
/// # The order was a bug, and a family fixture found it
///
/// This used to run raw strings, then `strip_language_literals`, then comments
/// — comments **last**. `strip_language_literals` treats `'` as a rune-literal
/// delimiter and Go comments are full of apostrophes, so
///
/// ```text
/// // don't run this on windows
/// if runtime.GOOS == "windows" {
///     t.Skip("posix only")
/// }
/// ```
///
/// stripped to `// don ` — the apostrophe opened a literal that never closed
/// and swallowed the rest of the function. `Go::is_skip_marker` then found no
/// `.skip(` and the test carried **no skip marker at all**: a `fail`-capable
/// detector silenced by a contraction in a comment. Measured on
/// `tests/fixtures/families/go/honest/conditional_skip`, which is why that
/// fixture's comment still contains the apostrophe that found it.
///
/// Comments and string literals are mutually exclusive lexical states, so the
/// only correct place to resolve them is the same pass — which
/// [`strip_raw_strings_and_comments`] already was, for three of the four
/// states. The fourth was added rather than a second pass bolted on.
///
/// **How much text the old order was misreading, measured rather than
/// guessed:** of the 63,491 assertion nodes this pack admits across the 2,279
/// `_test.go` files of the Go corpus, **371 (0.58%)** carry a comment *and* are
/// cut to under 60% of their length by `strip_language_literals` — those are
/// the nodes the comments-last order handed to `shape_of` truncated. The skip
/// detector's silence was the visible half of that; the classifier reading a
/// severed condition was the quiet half.
fn normalise(text: &str) -> String {
    let no_raw_or_comments = strip_raw_strings_and_comments(text);
    crate::shape::strip_language_literals(&no_raw_or_comments).to_ascii_lowercase()
}

/// Go raw string literals (`` `…` ``) and Go comments, removed in one scan.
///
/// The two are one job because they are one lexical decision: a `//` inside a
/// string starts no comment, an apostrophe inside a comment opens no literal,
/// and a pass that resolves only one of them gets the other wrong. See
/// [`normalise`] for the contraction-in-a-comment that proved it.
///
/// **Quote-aware, and it had to be.** The naive version — treat every backtick
/// as a delimiter — was wrong on real corpus source, and the corpus caught it:
/// `go/src/internal/godebugs/godebugs_test.go` writes
///
/// ```text
/// if !strings.Contains(doc, "`"+info.Name+"`") && …
/// ```
///
/// where the backticks are *inside* interpreted string literals. Reading them
/// as raw delimiters swallowed the `strings.Contains` between them and turned a
/// `Containment` assertion into `Truthy`. One assertion in 47,395, found by
/// measuring rather than by reasoning about it, which is the only reason it is
/// not still here.
///
/// **Re-measured over 63,491 admitted assertion nodes** from 2,279 `_test.go`
/// files — this is the residue as it stood *before* comments moved into this
/// pass, and the next-but-one paragraph carries the current one: three are
/// ambiguous by `shape::is_ambiguous`, a residue of **0.0047%** against a
/// budget of 0.1%.
///
/// All three are the *same* family, and none of them is a mis-reading by this
/// classifier. `is_ambiguous` compares `classify(text)` against
/// `classify(strip_language_literals(text))`, and that pre-strip does not know
/// backticks — so a raw string that *contains* a quote desynchronises it:
///
/// ```text
/// const marker = `build id "`                                  (cgo/testshared)
/// if strings.HasPrefix(tt.in, `"`) && strings.HasSuffix(…)      (time)
/// regexp.MustCompile(`Couldn't get [a-zA-Z_ -]* ?registers…`)   (runtime-gdb)
/// ```
///
/// The lone `"` and the apostrophe in `Couldn't` open a literal the shared
/// stripper then closes somewhere else entirely. `classify` reads all three
/// correctly, because this function runs *first* — the residue is the
/// ambiguity predicate's, not the pack's. Named here rather than subtracted
/// from the count: 0.0047% was the honest number then, and it was under budget
/// either way.
///
/// **Re-measured after comments moved into this pass** (same corpus, same 2,279
/// files, same 63,491 admitted nodes, harness rebuilt from
/// `tamper::collect_assertions` + `is_assertion` verbatim and reproducing the
/// 3/63,491 above before the change): **56 ambiguous, 0.0882%**. Under the 0.1%
/// budget, and only just — so the jump is stated rather than reported as a
/// number that happens to pass.
///
/// The residue went **up 18× because the classifier got more correct and the
/// predicate did not move with it.** `is_ambiguous` compares `classify(text)`
/// against `classify(strip_language_literals(text))`, and that pre-strip is
/// language-blind: it treats the apostrophe in a Go comment as a rune-literal
/// delimiter and eats the rest of the node. Measured over all 56:
///
/// - **56 of 56** classify as `Truthy` on the pre-stripped side — the floor,
///   reached because the truncation removed the `t.Fatal`/`t.Errorf` that
///   `fails_the_test` looks for.
/// - **52 of 56** classify as `Equality` and 4 as `Containment` on the side
///   that reads the real text. That side is the correct one in all 56.
/// - **51 of 56** contain an odd number of apostrophes, i.e. a contraction in a
///   comment; **48 of 56** are cut to under half their length by the pre-strip.
///
/// So every one of the 56 is a case where this classifier is right and the
/// baseline it is compared against is wrong. Before the change both sides were
/// wrong *in the same way* and agreed, which is a metric rewarding a classifier
/// for sharing its baseline's bug. `shape::is_ambiguous`'s pre-strip is what
/// wants fixing; it is shared with three other packs and is recorded for
/// MERGE-PLAN.md rather than changed from here.
///
/// Interpreted (`"…"`) and rune (`'…'`) literals are therefore skipped over and
/// **left in place** — `shape::strip_language_literals` removes those, and it
/// is the function `verify`'s own gate test protects.
fn strip_raw_strings_and_comments(text: &str) -> String {
    #[derive(PartialEq)]
    enum In {
        Code,
        Double,
        Rune,
        Raw,
        LineComment,
        BlockComment,
    }
    let mut out = String::with_capacity(text.len());
    let mut state = In::Code;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match state {
            In::Code => match c {
                '/' if chars.peek() == Some(&'/') => {
                    chars.next();
                    state = In::LineComment;
                    out.push(' ');
                }
                '/' if chars.peek() == Some(&'*') => {
                    chars.next();
                    state = In::BlockComment;
                    out.push(' ');
                }
                '`' => {
                    state = In::Raw;
                    out.push(' ');
                }
                '"' => {
                    state = In::Double;
                    out.push(c);
                }
                '\'' => {
                    state = In::Rune;
                    out.push(c);
                }
                _ => out.push(c),
            },
            In::Double | In::Rune => {
                out.push(c);
                if c == '\\' {
                    // The escaped character cannot close the literal, so it is
                    // copied without being looked at — `"\""` and `'\''`.
                    if let Some(escaped) = chars.next() {
                        out.push(escaped);
                    }
                } else if (state == In::Double && c == '"') || (state == In::Rune && c == '\'') {
                    state = In::Code;
                }
            }
            In::Raw => {
                if c == '`' {
                    state = In::Code;
                }
            }
            In::LineComment => {
                // The newline is KEPT. `if_condition` reads the first line of a
                // statement, so eating the line break would fold a trailing
                // comment's line into the condition and change what is read.
                if c == '\n' {
                    state = In::Code;
                    out.push('\n');
                }
            }
            In::BlockComment => {
                if c == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    state = In::Code;
                }
            }
        }
    }
    out
}

/// The condition of an `if` statement, or `None` when the text is not one.
///
/// The condition is the first line, minus the leading `if` and minus everything
/// from the **last** `{` on that line — the last, because
/// `if !reflect.DeepEqual(got, []int{1, 2}) {` has a brace inside the condition
/// and only the final one opens the block.
///
/// The init clause is deliberately kept: `if diff := cmp.Diff(want, got); diff
/// != "" {` is the canonical go-cmp assertion and dropping everything before
/// the `;` would throw away the `cmp.Diff` that names its shape.
fn if_condition(text: &str) -> Option<String> {
    let head = text.trim_start();
    let rest = head
        .strip_prefix("if ")
        .or_else(|| head.strip_prefix("if\t"))?;
    let first_line = rest.lines().next().unwrap_or(rest);
    Some(match first_line.rfind('{') {
        Some(at) => first_line[..at].to_string(),
        None => first_line.to_string(),
    })
}

/// Does this statement contain something that makes the test fail or abort?
///
/// Receiver-agnostic (`tt.Errorf`, `s.Fatalf`) because table-driven tests
/// rename the parameter constantly. `t.Skip` is **not** here: skipping is not
/// failing, which is exactly why `if testing.Short() { t.Skip() }` must not be
/// read as an assertion about anything.
fn fails_the_test(lower_text: &str) -> bool {
    const FAILING: &[&str] = &[
        ".error(",
        ".errorf(",
        ".fatal(",
        ".fatalf(",
        ".fail(",
        ".failnow(",
        "panic(",
        "assert.",
        "require.",
        ".expecterror(",
    ];
    FAILING.iter().any(|m| lower_text.contains(m))
}

/// The shape table. Input is already normalised and lower-cased.
fn shape_of(text: &str) -> Strength {
    const APPROXIMATE: &[&str] = &[
        "indelta",
        "inepsilon",
        "withinduration",
        "withinrange",
        "equateapprox",
        "approx",
        "almost",
        "tolerance",
        "math.abs(",
    ];
    // `equal` covers `assert.Equal`, `require.EqualValues`, `s.Equal`,
    // `reflect.DeepEqual`, `strings.EqualFold` and `assert.NotEqual` in one
    // substring; every one of them asserts identity, and the negation of an
    // identity claim is still an identity claim.
    const EQUALITY_BY_NAME: &[&str] = &[
        "deepequal",
        "equal",
        "exactly",
        "jsoneq",
        "yamleq",
        "elementsmatch",
        "cmp.diff(",
        "erroris",
        ".same(",
        ".len(",
    ];
    const CONTAINMENT: &[&str] = &[
        "contains",
        "subset",
        "hasprefix",
        "hassuffix",
        "regexp",
        "matches",
        "containselement",
    ];

    if APPROXIMATE.iter().any(|m| text.contains(m)) {
        return Strength::Approximate;
    }
    if EQUALITY_BY_NAME.iter().any(|m| text.contains(m)) {
        return Strength::Equality;
    }
    if CONTAINMENT.iter().any(|m| text.contains(m)) {
        return Strength::Containment;
    }
    if text.contains("==") || text.contains("!=") {
        return Strength::Equality;
    }
    Strength::Truthy
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::LangPack;
    use crate::tamper::parse_tests;

    fn names(source: &str) -> Vec<String> {
        parse_tests(source, &Go)
            .expect("the go grammar loads")
            .into_iter()
            .map(|t| t.name)
            .collect()
    }

    fn only(source: &str) -> crate::tamper::TestFn {
        let mut all = parse_tests(source, &Go).expect("the go grammar loads");
        assert_eq!(all.len(), 1, "expected exactly one test in {source}");
        all.remove(0)
    }

    #[test]
    fn extension_routes_to_this_pack() {
        let pack = crate::pack::pack_for_path("internal/rate/rate_test.go")
            .expect("a .go path routes to a pack");
        assert_eq!(pack.language(), PackLanguage::Go);
    }

    #[test]
    fn the_runners_own_prefix_rule_is_the_one_used() {
        // `testing.isTest`: the rune after the prefix must not be lower-case.
        assert_eq!(go_test_prefix("TestAdd"), Some("Test"));
        assert_eq!(go_test_prefix("Test"), Some("Test"));
        assert_eq!(go_test_prefix("Test_add"), Some("Test"));
        assert_eq!(go_test_prefix("BenchmarkRate"), Some("Benchmark"));
        assert_eq!(go_test_prefix("FuzzRate"), Some("Fuzz"));
        assert_eq!(go_test_prefix("ExampleRate"), Some("Example"));
        // These are NOT tests, and a case-insensitive prefix list says they are.
        assert_eq!(go_test_prefix("Testify"), None);
        assert_eq!(go_test_prefix("testAdd"), None);
        assert_eq!(go_test_prefix("Benchmarks"), None);
        assert_eq!(go_test_prefix("helper"), None);
    }

    #[test]
    fn a_signature_that_is_not_a_test_signature_is_not_a_test() {
        let source = r#"
package p

import "testing"

func TestReal(t *testing.T)      { t.Error("x") }
func TestHelper(x int) int       { return x }
func TestMain(m *testing.M)      { os.Exit(m.Run()) }
func BenchmarkRate(b *testing.B) { b.Error("x") }
func FuzzRate(f *testing.F)      { f.Fuzz(func(t *testing.T, i int) {}) }
func ExampleRate()               { fmt.Println(1) }
func ExampleBad(x int)           { fmt.Println(x) }
func Testify(t *testing.T)       { t.Error("x") }
"#;
        assert_eq!(
            names(source),
            vec!["BenchmarkRate", "ExampleRate", "FuzzRate", "TestReal"]
        );
    }

    #[test]
    fn the_testing_packages_own_tests_are_tests() {
        // `go/src/testing/sub_test.go` and two siblings are `package testing`,
        // so they write `*T` and not `*testing.T`. Requiring the qualified
        // spelling dropped all three, and the corpus is what said so.
        let source = r#"
package testing

func TestTRun(t *T)        { t.Error("x") }
func BenchmarkFoo(b *B)    { b.Error("x") }
func TestMain(m *M)        { os.Exit(m.Run()) }
func TestNotATest(v *Ring) { v.Do(nil) }
"#;
        assert_eq!(names(source), vec!["BenchmarkFoo", "TestTRun"]);
    }

    #[test]
    fn a_testify_suite_method_is_a_test() {
        let source = r#"
package p

import "testing"

type CalcSuite struct{ suite.Suite }

func (s *CalcSuite) SetupTest()      { s.calc = New() }
func (s *CalcSuite) TestAdd()        { s.Equal(3, s.calc.Add(1, 2)) }
func (s *CalcSuite) TestHelper(x int) {}

func TestCalcSuite(t *testing.T) { suite.Run(t, new(CalcSuite)) }
"#;
        assert_eq!(names(source), vec!["TestAdd", "TestCalcSuite"]);
    }

    // --- the stdlib shape --------------------------------------------------

    #[test]
    fn the_stdlib_if_shape_classifies_by_its_condition() {
        let go = Go;
        assert_eq!(
            go.classify("if got != want {\n\tt.Errorf(\"Rate(2) = %v, want %v\", got, want)\n}"),
            Strength::Equality
        );
        assert_eq!(
            go.classify("if !reflect.DeepEqual(got, want) {\n\tt.Fatalf(\"mismatch\")\n}"),
            Strength::Equality
        );
        assert_eq!(
            go.classify(
                "if diff := cmp.Diff(want, got); diff != \"\" {\n\tt.Errorf(\"(-want +got):\\n%s\", diff)\n}"
            ),
            Strength::Equality
        );
        assert_eq!(
            go.classify("if !strings.Contains(got, want) {\n\tt.Errorf(\"missing\")\n}"),
            Strength::Containment
        );
        assert_eq!(
            go.classify("if math.Abs(got-want) > 1e-9 {\n\tt.Errorf(\"not close\")\n}"),
            Strength::Approximate
        );
        assert_eq!(
            go.classify("if err != nil {\n\tt.Fatal(err)\n}"),
            Strength::Equality
        );
    }

    #[test]
    fn an_if_that_cannot_fail_the_test_asserts_nothing() {
        let go = Go;
        // The Go swallow: the comparison survives, the failure does not.
        assert_eq!(
            go.classify("if got != want {\n\tt.Logf(\"mismatch: %v\", got)\n}"),
            Strength::Truthy
        );
        // Control flow, not an assertion.
        assert_eq!(
            go.classify("if testing.Short() {\n\tt.Skip(\"skipping in -short mode\")\n}"),
            Strength::Truthy
        );
        assert_eq!(
            go.classify("if runtime.GOOS == \"windows\" {\n\tt.Skip(\"not on windows\")\n}"),
            Strength::Truthy
        );
        assert_eq!(
            go.classify("if err != nil {\n\treturn\n}"),
            Strength::Truthy
        );
    }

    #[test]
    fn ordering_is_truthy_so_a_downgrade_from_equality_is_loud() {
        let go = Go;
        assert_eq!(
            go.classify("if got > 0 {\n\tt.Errorf(\"bad\")\n}"),
            Strength::Truthy
        );
    }

    #[test]
    fn testify_and_gomock_shapes() {
        let go = Go;
        assert_eq!(
            go.classify("assert.Equal(t, 9.0, Rate(2))"),
            Strength::Equality
        );
        assert_eq!(
            go.classify("require.Equal(t, 9.0, Rate(2))"),
            Strength::Equality
        );
        assert_eq!(
            go.classify("assert.NotEqual(t, 0.0, Rate(2))"),
            Strength::Equality
        );
        assert_eq!(go.classify("assert.Len(t, got, 3)"), Strength::Equality);
        assert_eq!(
            go.classify("assert.ErrorIs(t, err, ErrClosed)"),
            Strength::Equality
        );
        assert_eq!(
            go.classify("assert.Contains(t, []string{\"a\"}, \"a\")"),
            Strength::Containment
        );
        assert_eq!(
            go.classify("assert.InDelta(t, 9.0, Rate(2), 0.001)"),
            Strength::Approximate
        );
        assert_eq!(go.classify("assert.True(t, ok)"), Strength::Truthy);
        assert_eq!(go.classify("require.NoError(t, err)"), Strength::Truthy);
        assert_eq!(go.classify("assert.NotNil(t, got)"), Strength::Truthy);
        // The operator inside the argument still decides.
        assert_eq!(
            go.classify("assert.True(t, got == want)"),
            Strength::Equality
        );
        // A name beats an operator: this asserts containment.
        assert_eq!(
            go.classify("assert.False(t, strings.Contains(got, want) == true)"),
            Strength::Containment
        );
    }

    #[test]
    fn a_message_string_never_decides_a_shape() {
        let go = Go;
        // Every one of these would classify differently if the literal were read.
        assert_eq!(
            go.classify("assert.True(t, ok, \"want == got\")"),
            Strength::Truthy
        );
        assert_eq!(
            go.classify("assert.True(t, ok, `expected contains`)"),
            Strength::Truthy
        );
        assert_eq!(
            go.classify("assert.True(t, ok) // was assert.Equal(t, a, b)"),
            Strength::Truthy
        );
        // Flipped from `!is_ambiguous` when the corpus run showed that
        // predicate was vacuous — see the note in `CSharp`'s twin. A message
        // containing `==` MUST be flagged by the counterfactual; that it is,
        // while `classify` above still answers Truthy, is the claim.
        assert!(crate::shape::is_ambiguous(
            &Go,
            "assert.True(t, ok, \"want == got\")"
        ));
        // The comment case is deliberately NOT asserted here: `Go::normalise`
        // strips `//` comments before `classify` sees them, and
        // `shape::expose_literals` does not re-expose comments. That limit is
        // named in `shape.rs` and in MEASUREMENTS.toml rather than papered over
        // with an assertion that would pass for the wrong reason.
    }

    #[test]
    fn a_backtick_inside_a_quoted_literal_is_not_a_raw_string() {
        // From `go/src/internal/godebugs/godebugs_test.go`, and the one
        // classifier bug the 47,395-assertion corpus measurement found: the
        // backticks here are string CONTENT, and reading them as raw-string
        // delimiters swallowed the `strings.Contains` between them.
        let go = Go;
        assert_eq!(
            go.classify(
                "if !strings.Contains(doc, \"`\"+info.Name+\"`\") {\n\tt.Errorf(\"undocumented\")\n}"
            ),
            Strength::Containment
        );
        // And a genuine raw string still hides what is inside it.
        assert_eq!(
            go.classify("assert.True(t, ok, `want == got`)"),
            Strength::Truthy
        );
        // A raw string that CONTAINS a quote does not desynchronise the rest.
        assert_eq!(
            go.classify("if strings.HasPrefix(tt.in, `\"`) {\n\tt.Errorf(\"bad\")\n}"),
            Strength::Containment
        );
    }

    #[test]
    fn go_has_no_in_operator_so_in_is_never_containment() {
        // `verify::tamper::strength_of` reads " in " as containment. In Go,
        // `in` is a table-field name.
        let go = Go;
        assert_eq!(
            go.classify("if in != out {\n\tt.Errorf(\"bad\")\n}"),
            Strength::Equality
        );
    }

    // --- assertion collection ---------------------------------------------

    #[test]
    fn the_errorf_inside_an_if_is_not_a_second_assertion() {
        let test = only(
            r#"
package p

import "testing"

func TestOne(t *testing.T) {
	got := Rate(2)
	if got != 9.0 {
		t.Errorf("Rate(2) = %v, want 9.0", got)
	}
}
"#,
        );
        assert_eq!(test.assertions, vec![Strength::Equality]);
    }

    #[test]
    fn a_subtests_assertions_roll_up_into_its_parent() {
        let test = only(
            r#"
package p

import "testing"

func TestUpper(t *testing.T) {
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if got := Upper(tc.in); got != tc.want {
				t.Errorf("Upper(%q) = %q, want %q", tc.in, got, tc.want)
			}
		})
	}
}
"#,
        );
        assert_eq!(test.assertions, vec![Strength::Equality]);
    }

    #[test]
    fn a_control_flow_if_never_raises_the_strongest_assertion() {
        let test = only(
            r#"
package p

import "testing"

func TestOne(t *testing.T) {
	if testing.Short() {
		t.Skip("short")
	}
	if err != nil {
		return
	}
	assert.True(t, ok)
}
"#,
        );
        // Three assertion nodes, and the strongest is still Truthy: the two
        // control-flow ifs cannot mask a weakening.
        assert_eq!(test.assertions.len(), 3);
        assert_eq!(
            test.assertions.iter().copied().max(),
            Some(Strength::Truthy)
        );
    }

    // --- skip markers ------------------------------------------------------

    #[test]
    fn a_comment_about_a_skip_is_not_a_skip() {
        let source = r#"
package p

import "testing"

func TestOne(t *testing.T) {
	// the t.Skip() here was removed in #412
	assert.Equal(t, 1, 1)
}
"#;
        assert!(!only(source).has_skip_marker);
    }

    #[test]
    fn an_apostrophe_in_a_comment_does_not_hide_the_skip_below_it() {
        // Found by `tests/fixtures/families/go/honest/conditional_skip`, whose
        // comment reads "the float formatting this test's message relies on".
        //
        // `normalise` used to run `shape::strip_language_literals` BEFORE
        // stripping comments. That function reads `'` as a rune-literal
        // delimiter and knows nothing about Go comments, so the apostrophe in
        // `don't` opened a literal that never closed and swallowed the rest of
        // the function — `// don't run this on windows\n\tt.Skip("posix only")`
        // stripped to `// don `. `is_skip_marker` then found no `.skip(`, and a
        // test with an unconditional skip in it reported **no skip marker at
        // all**: a fail-capable detector silenced by a contraction.
        //
        // Both spellings are asserted, because the bug was invisible in the one
        // without the apostrophe and that is how it survived.
        for comment in ["// not on windows", "// don't run this on windows"] {
            let source = format!(
                "package p\n\nimport \"testing\"\n\nfunc TestOne(t *testing.T) {{\n\t{comment}\n\
                 \tif runtime.GOOS == \"windows\" {{\n\t\tt.Skip(\"posix only\")\n\t}}\n\
                 \tassert.Equal(t, 1, 1)\n}}\n"
            );
            assert!(
                only(&source).has_skip_marker,
                "the skip below `{comment}` was not seen"
            );
        }
    }

    #[test]
    fn a_comment_cannot_swallow_the_assertion_that_follows_it() {
        // The quiet half of the same bug: with comments stripped last, an
        // apostrophe truncated the node before `fails_the_test` could find the
        // `t.Errorf`, and a real `Equality` comparison read as `Truthy` — the
        // floor `assertion_weakened` fires from.
        let go = Go;
        assert_eq!(
            go.classify(
                "if got != want {\n\t// the parser doesn't normalise this yet\n\
                 \tt.Errorf(\"Rate(2) = %v, want %v\", got, want)\n}"
            ),
            Strength::Equality
        );
        // And a `//` inside a string literal still starts no comment.
        assert_eq!(
            go.classify(
                "if !strings.Contains(got, \"http://example.com\") {\n\tt.Errorf(\"missing\")\n}"
            ),
            Strength::Containment
        );
    }

    #[test]
    fn a_real_skip_is_a_skip_whatever_the_receiver_is_called() {
        for body in [
            "t.Skip(\"no fixture\")",
            "tt.Skipf(\"no fixture on %s\", runtime.GOOS)",
            "t.SkipNow()",
            "if testing.Short() {\n\t\tt.Skip(\"short\")\n\t}",
        ] {
            let source = format!(
                "package p\n\nimport \"testing\"\n\nfunc TestOne(t *testing.T) {{\n\t{body}\n}}\n"
            );
            assert!(
                only(&source).has_skip_marker,
                "expected a skip marker in: {body}"
            );
        }
    }

    // --- mocks -------------------------------------------------------------

    #[test]
    fn mock_targets_are_read_from_all_four_go_forms() {
        let test = only(
            r#"
package p

import "testing"

func TestOne(t *testing.T) {
	ctrl := gomock.NewController(t)
	rate := NewMockRate(ctrl)
	rate.EXPECT().Charge(2).Return(9.0)
	obj := new(MockShipping)
	obj.On("Quote", 2).Return(9.0, nil)
	monkey.Patch(time.Now, func() time.Time { return t0 })
	assert.Equal(t, 9.0, Total())
}
"#,
        );
        assert_eq!(
            test.mock_targets,
            vec![
                "Charge".to_string(),
                "Quote".to_string(),
                "Rate".to_string(),
                "time.Now".to_string(),
            ]
        );
    }

    // --- the detectors, end to end ----------------------------------------

    #[test]
    fn the_logf_downgrade_is_a_weakening_to_truthy() {
        let base = r#"
package p

import "testing"

func TestRate(t *testing.T) {
	if got := Rate(2); got != 9.0 {
		t.Errorf("Rate(2) = %v, want 9.0", got)
	}
}
"#;
        let head = base.replace("t.Errorf", "t.Logf");
        let findings =
            crate::tamper::diff_test_file("rate_test.go", base, &head, &Go).expect("parses");
        let weakened = findings
            .iter()
            .find(|f| f.detector == "assertion_weakened")
            .expect("the downgrade is reported");
        // `is_fail()` rather than reading the field: `Finding::severity` is
        // public at core 18d94dc ONLY because two `#[cfg(test)]` lines in this
        // crate read it, and verify's severity_cap_holds.rs censuses the hole
        // rather than closing it. These were those two lines.
        assert!(weakened.is_fail());
    }

    #[test]
    fn deleting_a_test_is_reported_and_renaming_it_is_not() {
        let base = r#"
package p

import "testing"

func TestRate(t *testing.T) {
	got := Rate(2)
	if got != 9.0 {
		t.Errorf("Rate(2) = %v, want 9.0", got)
	}
}

func TestOther(t *testing.T) {
	if Other() != 1 {
		t.Fatal("other")
	}
}
"#;
        let renamed = base.replace("func TestRate(", "func TestRateAtTwo(");
        let findings =
            crate::tamper::diff_test_file("rate_test.go", base, &renamed, &Go).expect("parses");
        assert!(
            findings.iter().any(|f| f.detector == "test_renamed"),
            "a rename is a rename: {findings:?}"
        );
        assert!(!findings.iter().any(|f| f.detector == "test_deleted"));

        let deleted = base.split("func TestOther").next().unwrap_or_default();
        let findings =
            crate::tamper::diff_test_file("rate_test.go", base, deleted, &Go).expect("parses");
        assert!(findings.iter().any(|f| f.detector == "test_deleted"));
    }
}

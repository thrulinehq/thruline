//! Ruby: RSpec, minitest (both dialects), Test::Unit, rspec-mocks and mocha.
//!
//! # Ruby has no test *declaration*, only a call that takes a block
//!
//! Every other language in this crate marks a test with something the grammar
//! can see: a Java annotation, a C# attribute, a Go function whose name starts
//! with `Test`. Ruby has neither. `it "adds" do … end` is an ordinary method
//! call with a string and a block, indistinguishable at the grammar level from
//! `retry "adds" do … end`, and `def test_add` is an ordinary method whose only
//! claim to being a test is that minitest's runner greps method names for
//! `/^test_/`. So [`Ruby::test_name`] is not a name lookup with a prefix
//! filter — it is the whole decision, and it is made from the callee.
//!
//! # Two node kinds, four dialects
//!
//! - `call` — RSpec (`it` / `specify` / `example`, and the `x`/`f` prefixed
//!   forms), minitest's spec DSL (`it` inside `describe`), Rails'
//!   `ActiveSupport::TestCase.test "name" do`.
//! - `method` — minitest's and Test::Unit's `def test_…`.
//!
//! Both are in `fn_kinds`, which makes `call` a candidate at every call site in
//! the file. That is cheap (the callee check rejects in one comparison) and it
//! is the only way to reach an `it` nested three `describe` blocks deep.
//!
//! # What this pack was measured against, and what the measurement changed
//!
//! **1,922 real Ruby test files** — rails, rspec-core, jekyll, sinatra, puma,
//! faker — parsed with **0 grammar errors**, yielding **33,427 tests** and
//! **65,769 assertions** inside them (Equality 32,514 · Truthy 26,039 ·
//! Containment 7,137 · Approximate 79). Well past the 5,000-assertion floor the
//! crate is held to.
//!
//! Every figure in that paragraph is **re-derivable, not transcribed**:
//! [`tests::ruby_corpus_census`] performs the walk that produced them and
//! prints them in the order they are stated. A number copied into a comment and
//! left there is the failure `scripts/vendor-freshness.py` exists to catch, so
//! the measurement lives beside the claim.
//!
//! Three things in this file exist *only* because that run was done, and each
//! is recorded beside the code it changed:
//!
//! - jekyll's 51 test files reported **zero tests** until `should "…" do`
//!   (shoulda-context) was recognised, and then reported exactly **one
//!   assertion per test** until the bare `should` assertion prefix was removed.
//! - **143 of 66,945 assertion-shaped nodes (0.21%)** were matched only by a
//!   bare `assert`/`expect` prefix — `expected.each`, `AssertSelectMailer` —
//!   and are false assertions that inflate the count
//!   `assertion_count_decreased` compares.
//! - **3,265 assertions** landed on `Truthy` while naming a shape, all of them
//!   Rails' and Test::Unit's negated spellings (`assert_not_equal`,
//!   `assert_no_match`, `assert_not_includes`, `assert_dom_equal`).
//!
//! Of the 72 files where the finished pack still finds no test, every one
//! sampled is a helper, a `TestCase` base class, or a suite whose tests arrive
//! through an included module — plus one upstream bug (faker's
//! `test_faker_opera.rb` defines `def verdi`, which minitest never runs).
//!
//! **77 of 66,885 assertion nodes (0.115%)** classify differently when their
//! literals are left in — every sampled one a message string. Note that the
//! crate's own `is_ambiguous` predicate reports **0** for Ruby, and that is
//! structural rather than impressive: [`Ruby::classify`] strips
//! unconditionally, so no caller can reach the unstripped path. The 0.115% is
//! the number that says what the strip is buying.
//!
//! # Node kinds here were read off `tree-sitter-ruby`, not remembered
//!
//! Every kind and field name below (`call`/`method`, the `receiver` /`method`/
//! `arguments`/`block` fields, `do_block`, `simple_symbol`, `body_statement`)
//! was printed from a parse of real Ruby before it was written down. Two of
//! them are not what the grammar's name suggests: `call`'s callee is the
//! **`method`** field (not `function`, which is the JS grammar's name), and a
//! `do_block`'s statements hang off a `body_statement` child rather than
//! directly. Guessing either one produces a pack that finds nothing and says
//! so with an empty list, which is the failure mode `verify` recorded for its
//! whole JS/TS path.

use tree_sitter::Node;

use crate::pack::{LangPack, LangSpec, PackLanguage, Severity, SkipSite, Strength};

pub struct Ruby;

impl LangPack for Ruby {
    fn language(&self) -> PackLanguage {
        PackLanguage::Ruby
    }

    /// `.rb` and the two other dotted forms that are Ruby by content.
    ///
    /// **What this cannot reach, named rather than left to be discovered:**
    /// `Rakefile`, `Gemfile`, `Guardfile` and `config.ru`'s siblings carry no
    /// extension at all, and [`crate::pack::pack_for_path`] dispatches on the
    /// text after the last `.` — so an extensionless Ruby file resolves to
    /// `None`, i.e. *not a language this crate parses*. No test suite lives in
    /// one, so §6.7 loses nothing; a consumer asking "is this Ruby" gets the
    /// wrong answer, and that is the cost.
    fn extensions(&self) -> &'static [&'static str] {
        &["rb", "rake", "gemspec"]
    }

    fn ts_language(&self) -> tree_sitter::Language {
        tree_sitter_ruby::LANGUAGE.into()
    }

    fn spec(&self) -> LangSpec {
        LangSpec {
            fn_kinds: &["call", "method"],
            assertion_kinds: &["call"],
            // Bare prefixes must START the node's text; a prefix beginning with
            // `.` is matched as an infix, which is how the fluent forms are
            // reached — see `tamper::is_assertion`.
            //
            // **Every bare prefix carries its delimiter, and that is not
            // tidiness.** (`flunk` is the one exception, because a bare
            // `flunk` with no arguments is a whole assertion whose node text
            // is exactly that word.) `expect` and `assert` on their own match
            // `expected.each do |x|` and `AssertSelectMailer.test(…)`, because
            // `is_assertion` asks only `starts_with`. Measured over the corpus:
            // **143 of 66,945 assertion-shaped nodes (0.21%) were matched by a
            // bare prefix and by nothing tighter**, and every one sampled was a
            // local variable called `expected` or a class whose name begins
            // with `Assert`. A false assertion is not harmless — it inflates
            // the count `assertion_count_decreased` compares, so deleting an
            // ordinary `expected.each` loop reads as removing an assertion.
            //
            // `expect(`/`expect {`/`expect_` cover `expect(x).to …`,
            // `expect { }.to raise_error`, rspec-mocks'
            // `expect(Shipping).to receive(:rate)` and
            // `expect_any_instance_of(…)`. `.must_`/`.wont_` reach minitest's
            // expectation dialect, whose assertions begin with the *value*:
            // `_(Calc.add(1, 2)).must_equal 3`. `.should` reaches the RSpec-2
            // form still present across older suites.
            //
            // `should` is deliberately **not** a bare prefix. shoulda-context
            // declares its tests as `should "name" do … end`, so a bare
            // `should` makes every one of those declarations its own single
            // `Truthy` assertion — and because `collect_assertions` does not
            // descend past a match, the assertions actually inside the test
            // become invisible. Measured: jekyll's 900 tests reported exactly
            // 900 assertions, one each, until this was removed.
            assertion_call_prefixes: &[
                "assert ",
                "assert(",
                "assert_",
                "refute ",
                "refute(",
                "refute_",
                "expect ",
                "expect(",
                "expect{",
                "expect_",
                "is_expected.",
                "flunk",
                ".must_",
                ".wont_",
                ".should",
            ],
            // Read as **whole tokens** by `is_skip_marker`, not as substrings.
            // A substring match on `skip` fires on `skip_header`, on
            // `it "skips invalid rows"`, and on every `pending_message` — the
            // same regex failure `verify`'s Python control fixture caught.
            skip_markers: &["xit", "xspecify", "xexample", "skip", "pending", "omit"],
            // **Deliberately empty; see the note on `catch_kinds`.**
            swallow_markers: &[],
            // Declarative only: minitest and Test::Unit run exactly the methods
            // matching `/^test_/`, which is the rule `test_name` enforces. The
            // RSpec form has no name prefix at all, so this field describes
            // half of Ruby and `test_name` owns the decision for both halves.
            test_name_prefixes: &["test_"],
            // `xit`, `skip "…"` and `pending "…"` are all inside the example's
            // own text. Ruby has no decorators.
            skip_site: SkipSite::Body,
            decorator_kinds: &[],
            // **Empty, and this is a gap, not a judgement that Ruby cannot
            // swallow a failure.** It can: `begin; assert_equal 1, 2; rescue
            // StandardError; end` is the cheat, verbatim.
            //
            // `tamper::catch_swallows` finds the guarded region as either the
            // node's `body` **field** or its first named child whose kind
            // contains `block`. In `tree-sitter-ruby` a `begin` node has
            // neither: its guarded statements are the run of *siblings* that
            // precede the `rescue` child, with no wrapper node of any kind, and
            // the `def … rescue … end` form puts the `rescue` inside a
            // `body_statement` whose own children are the guarded statements.
            // Every candidate kind was tried against a real parse — `begin`,
            // `body_statement`, `method`, `do_block` — and each fails at a
            // different step; there is no setting of this field that makes the
            // shared detector fire for Ruby.
            //
            // So `assertion_swallowed` is **not detected for Ruby**. Filling
            // this field with a kind that cannot fire would read as coverage
            // that does not exist, which is worse than an empty list beside
            // this paragraph. The interface change it needs — a guarded region
            // expressed as "the named children before the first handler" — is
            // for the merge plan to carry.
            // `begin ... rescue ... end`, and the implicit form
            // `def m ... rescue ... end` whose body is a `body_statement`.
            //
            // These were empty until 2026-09-05, and the note here said no
            // setting of them could work — true at the time: `tamper`'s
            // guarded-region lookup wanted a `body` FIELD or a child whose kind
            // contains `block`, and `tree-sitter-ruby`'s `begin` has neither.
            // The engine now falls back to "everything before the first
            // handler", which is the general shape, so Ruby has swallow
            // detection for the first time.
            catch_kinds: &["begin", "body_statement"],
            defer_kinds: &[],
            neutralising_calls: &[],
            unconditional_fail_markers: &["flunk", "raise \"", "fail \""],
        }
    }

    /// Is this node a test, and what is it called?
    ///
    /// Two shapes, and a third that has no name at all:
    ///
    /// 1. `method` — `def test_add`. Named by the runner's own rule (`/^test_/`),
    ///    not by a convention this crate invented. A `def helper` in the same
    ///    class is not a test and minitest will not run it.
    /// 2. `call` with a block and a string/symbol first argument — `it "adds"`,
    ///    `specify`, `example`, `scenario`, Rails' `test "…" do`, and the
    ///    `x`-prefixed (skipped) and `f`-prefixed (focused) variants. The name
    ///    is the argument.
    /// 3. `call` with a block and **no name**: `it { is_expected.to eq(3) }`.
    ///    See [`unnamed_identity`] — this returns a positional identity rather
    ///    than `None`, and the reason is written there.
    ///
    /// `describe`/`context` return `None` on purpose, so the walk descends into
    /// them; they are containers, and treating a container as a test would
    /// attribute every nested example's assertions to it.
    fn test_name(&self, node: Node, source: &str) -> Option<String> {
        match node.kind() {
            "method" => {
                let name = node
                    .child_by_field_name("name")?
                    .utf8_text(source.as_bytes())
                    .ok()?;
                // `test_` with the underscore, not `test`: `testify` and
                // `test?` are methods minitest does not run, and counting them
                // as tests invents deletions when one is renamed.
                name.starts_with("test_").then(|| name.to_string())
            }
            "call" => {
                let example = example_call(node, source)?;
                Some(match example {
                    Some(name) => name,
                    None => unnamed_identity(node, source),
                })
            }
            _ => None,
        }
    }

    /// Classify one Ruby assertion's shape.
    ///
    /// **The matcher decides, and the matcher is what follows `.to`.** Scanning
    /// the whole text for a shape word — which is what a substring table does —
    /// gets two very common Ruby idioms exactly backwards:
    ///
    /// - `expect(list.include?(3)).to be_truthy` contains `include`, and is a
    ///   truthy assertion. Read as containment, an honest edit to
    ///   `expect(list).to be_truthy` becomes an `assertion_weakened` **fail**.
    /// - `expect(s).to match(/equal/)` contains `equal` inside a **regex
    ///   literal**, which neither `strip_string_literals` nor
    ///   [`crate::shape::strip_language_literals`] removes — `/…/` cannot be
    ///   stripped generically because `/` is division. Read as equality, the
    ///   inverse false positive follows.
    ///
    /// So the scan starts after the first `to` / `not_to` / `to_not` token when
    /// there is one, and falls back to the whole token list when there is not
    /// (the `assert_*` / `refute_* `/ `must_*` dialects lead with their
    /// matcher, so position zero is already right for them). The **first**
    /// token that names any shape wins.
    ///
    /// `contain_exactly` and `match_array` are classified as containment rather
    /// than equality. They are set-equality matchers and the argument for
    /// `Equality` is real; they are ranked with `verify`'s substring table
    /// (`contains`/`matches`) instead, because one classifier that disagrees
    /// with another about the same word is a worse thing to own than one rung
    /// of conservatism.
    fn classify(&self, assertion_text: &str) -> Strength {
        // Required, and required *first*: a message string is the 0.082% of
        // assertions whose shape a human sentence would otherwise decide.
        let stripped = crate::shape::strip_language_literals(assertion_text).to_ascii_lowercase();
        let tokens: Vec<&str> = stripped
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|t| !t.is_empty())
            .collect();

        let after_to = tokens
            .iter()
            .position(|t| matches!(*t, "to" | "not_to" | "to_not"))
            .map(|i| (i + 1).min(tokens.len()))
            .unwrap_or(0);
        for window in [&tokens[after_to..], &tokens[..]] {
            for token in window {
                if let Some(strength) = shape_of_token(token) {
                    return strength;
                }
            }
        }

        // `assert x == 3` and `expect(x).to be == 3` name no matcher at all;
        // the operator is the shape. Checked last so it can never outrank a
        // named matcher — `expect(x).to be_truthy` where the subject happens to
        // contain `==` is truthy, not equality.
        if stripped.contains("==") || stripped.contains("!=") {
            return Strength::Equality;
        }
        Strength::Truthy
    }

    /// rspec-mocks, mocha, and the two `_double` families.
    ///
    /// Each entry ends with `(` so `tamper::first_target` reads the *argument*
    /// and not the rest of the line. `receive(` and `expects(` are listed
    /// because in Ruby the thing being faked is usually the **method symbol** —
    /// `allow(Shipping).to receive(:rate)` names `Shipping` and `rate`, and it
    /// is `rate` that a changed-symbol lookup can match. Listing only the
    /// receiver would find the class and miss the method, which is the half of
    /// `mocked_unit_under_test` that matters.
    ///
    /// The overlapping entries (`spy(` inside `instance_spy(`, `double(` inside
    /// `class_double(`) each produce the same target twice; `mock_targets_in`
    /// sorts and dedups, so the duplication costs nothing and dropping the
    /// short forms would lose the bare `double("thing")` call.
    /// **Capped at `warn`.** Operator ruling 2026-09-06 (01:10 IST): Ruby's measured
    /// ambiguity is 0.1275% against the 0.1% budget, so a Ruby finding may
    /// inform and may not accuse. Lift this when `MEASUREMENTS.toml` records
    /// Ruby under budget, and not before — `tests/measured_constants.rs` and
    /// `tests/severity_cap.rs` both fail if the cap and the measurement
    /// disagree.
    fn severity_cap(&self) -> Option<Severity> {
        Some(Severity::Warn)
    }

    fn mock_calls(&self) -> &'static [&'static str] {
        &[
            "allow(",
            "allow_any_instance_of(",
            "expect_any_instance_of(",
            "receive(",
            "receive_messages(",
            "have_received(",
            "double(",
            "instance_double(",
            "class_double(",
            "object_double(",
            "spy(",
            "instance_spy(",
            "class_spy(",
            "stub(",
            "stubs(",
            "expects(",
            "mock(",
            "any_instance(",
        ]
    }

    /// Ruby's skip markers are **words in the body**, so they are read as whole
    /// tokens off text with its literals removed.
    ///
    /// The default implementation is a `contains` over the marker list, and for
    /// Ruby that is a false-positive generator: `skip` is a substring of
    /// `skip_header` and of `Calc.skip_list`, and `pending` is a substring of
    /// `pending_message`. Worse, `skip_site` is [`SkipSite::Body`], so the
    /// caller hands over the example's **entire text** — including the
    /// description string. `it "skips invalid rows" do` would be reported as a
    /// skipped test, forever, by a `contains`.
    ///
    /// **The one hole left, named:** the caller lowercases the text before
    /// passing it here, and `strip_language_literals` recognises a heredoc by
    /// its **uppercase** tag (`<<~SQL`). A lowercased `<<~sql` therefore
    /// survives stripping, so a heredoc body containing the bare word `skip`
    /// still false-positives. Fixing it means stripping before lowercasing,
    /// which is a change in `tamper::as_test_fn`, not here.
    fn is_skip_marker(&self, decorator_text: &str) -> bool {
        let stripped = crate::shape::strip_language_literals(decorator_text).to_ascii_lowercase();
        let markers = self.spec().skip_markers;
        stripped
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|token| markers.contains(&token))
    }
}

/// The shape a single token names, or `None` if it names none.
///
/// One table rather than four `contains` passes, so the caller can ask "does
/// *this* token name a shape" and stop at the first that does — which is what
/// makes the `.to`-anchored scan possible.
///
/// # The rule for adding an entry, so the next person extends it the same way
///
/// **A helper is ranked by the shape its own name states.** `assert_dom_equal`
/// states equality; `assert_no_match` states containment; `assert_response`,
/// `assert_select` and `assert_file` state no shape at all and are therefore
/// `Truthy` by falling through, which is correct — they are domain assertions
/// with no comparison in them. The temptation is to rank a *frequent* helper
/// highly because it "feels" strong, and that is how a classifier stops being
/// about shape.
///
/// # What the corpus said
///
/// The `assert_not_*` family, `assert_no_match` and `assert_dom_equal` are here
/// because a histogram of the matcher this function selects, taken over
/// **66,445 assertion-shaped nodes in 1,922 real test files** (rails,
/// rspec-core, jekyll, sinatra, puma, faker), showed them landing on `Truthy`
/// while stating a shape in their names — 3,265 assertions between them, all of
/// them Rails' and Test::Unit's negated spellings of helpers that were already
/// in this table under their positive names. They were not guessed at from
/// memory and they were not in the first draft.
fn shape_of_token(token: &str) -> Option<Strength> {
    Some(match token {
        "assert_in_delta"
        | "refute_in_delta"
        | "assert_in_epsilon"
        | "refute_in_epsilon"
        | "be_within"
        | "be_close"
        | "must_be_close_to"
        | "wont_be_close_to"
        | "must_be_within_delta"
        | "must_be_within_epsilon"
        | "a_value_within" => Strength::Approximate,

        "assert_equal"
        | "refute_equal"
        | "assert_not_equal"
        | "assert_dom_equal"
        | "assert_dom_not_equal"
        | "assert_same"
        | "refute_same"
        | "assert_not_same"
        | "eq"
        | "eql"
        | "equal"
        | "must_equal"
        | "wont_equal"
        | "must_be_same_as"
        | "wont_be_same_as" => Strength::Equality,

        "assert_includes"
        | "refute_includes"
        | "assert_not_includes"
        | "assert_match"
        | "refute_match"
        | "assert_not_match"
        | "assert_no_match"
        | "include"
        | "match"
        | "match_array"
        | "contain_exactly"
        | "start_with"
        | "end_with"
        | "cover"
        | "have_key"
        | "must_include"
        | "wont_include"
        | "must_match"
        | "wont_match" => Strength::Containment,

        // Named explicitly rather than left to the default, so that a matcher
        // this crate *knows* is truthy stops the scan before the `==` fallback
        // can promote it. `expect(a == b).to be_truthy` is a truthy assertion
        // however its subject is written.
        //
        // `be` is deliberately absent: `be == 3` must reach the operator
        // fallback and classify as equality.
        "be_truthy"
        | "be_falsey"
        | "be_falsy"
        | "be_nil"
        | "be_empty"
        | "be_present"
        | "be_blank"
        | "be_valid"
        | "be_a"
        | "be_an"
        | "be_a_kind_of"
        | "be_kind_of"
        | "be_instance_of"
        | "be_an_instance_of"
        | "raise_error"
        | "assert_nil"
        | "refute_nil"
        | "assert_empty"
        | "refute_empty"
        | "assert_instance_of"
        | "assert_kind_of"
        | "assert_respond_to"
        | "assert_predicate"
        | "assert_raises"
        | "assert_raise"
        | "assert_not_nil"
        | "assert_not_empty"
        | "assert_not_predicate"
        | "assert_not_respond_to"
        | "assert_not_instance_of"
        | "assert_not_kind_of"
        | "assert_nothing_raised"
        | "must_be_nil"
        | "must_be_empty"
        | "must_raise"
        | "must_be_instance_of"
        | "must_be_kind_of"
        | "must_respond_to" => Strength::Truthy,

        _ => return None,
    })
}

/// Is this call an example, and what does it call itself?
///
/// `Some(Some(name))` — a named example. `Some(None)` — an example with no
/// name. `None` — not an example.
///
/// Three conditions, all necessary:
///
/// - **no receiver.** `RSpec.describe` has one; a bare `it` does not. Requiring
///   its absence is what stops `page.example` or `config.test` from being read
///   as a declaration.
/// - **a block.** `it` without a block is not an example — RSpec treats a
///   blockless `it "…"` as a *pending* declaration, and more to the point a
///   call with no block has no assertions to compare.
/// - **a callee in the DSL**, after one leading `x` (skipped) or `f` (focused)
///   is removed.
fn example_call(node: Node, source: &str) -> Option<Option<String>> {
    if node.kind() != "call" || node.child_by_field_name("receiver").is_some() {
        return None;
    }
    let method = node
        .child_by_field_name("method")?
        .utf8_text(source.as_bytes())
        .ok()?;
    if !is_example_method(method) {
        return None;
    }
    node.child_by_field_name("block")?;
    Some(
        node.child_by_field_name("arguments")
            .and_then(|args| args.named_child(0))
            .and_then(|arg| arg_label(arg, source)),
    )
}

fn is_example_method(name: &str) -> bool {
    let bare = name
        .strip_prefix('x')
        .or_else(|| name.strip_prefix('f'))
        .unwrap_or(name);
    matches!(
        bare,
        "it" | "specify" | "example" | "scenario" | "test" | "its" | "should"
    )
}

/// Is this call a `describe`/`context` group, and what is it called?
fn group_label(node: Node, source: &str) -> Option<String> {
    if node.kind() != "call" {
        return None;
    }
    let method = node
        .child_by_field_name("method")?
        .utf8_text(source.as_bytes())
        .ok()?;
    let bare = method
        .strip_prefix('x')
        .or_else(|| method.strip_prefix('f'))
        .unwrap_or(method);
    if !matches!(
        bare,
        "describe" | "context" | "feature" | "shared_examples" | "shared_context"
    ) {
        return None;
    }
    node.child_by_field_name("arguments")
        .and_then(|args| args.named_child(0))
        .and_then(|arg| arg_label(arg, source))
}

/// The label a first argument carries: a string, a symbol, or a class name.
///
/// The string's **source text** is used, minus one layer of quotes, rather than
/// its `string_content` child. `it "adds #{n} items"` has two named children —
/// a `string_content` and an `interpolation` — and taking only the first
/// collapses every interpolated description in a loop to the same name, which
/// pairs unrelated tests across trees.
fn arg_label(node: Node, source: &str) -> Option<String> {
    let text = node.utf8_text(source.as_bytes()).ok()?;
    Some(match node.kind() {
        "string" => text
            .strip_prefix(['"', '\''])
            .and_then(|t| t.strip_suffix(['"', '\'']))
            .unwrap_or(text)
            .to_string(),
        "simple_symbol" => text.trim_start_matches(':').to_string(),
        "constant" | "scope_resolution" => text.to_string(),
        _ => return None,
    })
}

/// A name for `it { is_expected.to eq(3) }`, which declares none.
///
/// RSpec generates a description for this form at **run time**, from the
/// matcher the block happens to use ("is expected to eq 3"), so the only name
/// the source carries is the example's position. §6.7 pairs tests between two
/// trees by name, and all three available answers are bad:
///
/// - `None` — the example is invisible to every detector. An assertion deleted
///   from a one-line `it { }` would be silently unreported, which is the shape
///   of under-detection this product exists to refuse.
/// - the block's own text — then *any* edit renames the test, and the cheat
///   this crate is built to catch (`eq(3)` weakened to `be_truthy`) is reported
///   as a rename or a deletion instead of as `assertion_weakened`.
/// - **position** — chosen. `Calc .add it[1]`: the enclosing `describe`/
///   `context` chain, then the index among the *unnamed* examples that share a
///   parent. Counting only unnamed siblings is what keeps an added `it "name"`
///   from shifting every index below it.
///
/// **The cost, stated:** inserting or deleting an unnamed example above another
/// one shifts its identity, and the shifted pair is then compared as if it were
/// the same test — which can produce a spurious `assertion_weakened` or
/// `assertion_count_decreased`. That is a real false positive on a real edit,
/// and it is accepted here because the alternative is a blind spot exactly
/// where a one-line example is easiest to hollow out.
fn unnamed_identity(node: Node, source: &str) -> String {
    let mut ordinal = 0usize;
    let mut sibling = node.prev_sibling();
    while let Some(previous) = sibling {
        if matches!(example_call(previous, source), Some(None)) {
            ordinal += 1;
        }
        sibling = previous.prev_sibling();
    }

    let mut chain: Vec<String> = Vec::new();
    let mut parent = node.parent();
    // Bounded: a malformed tree must not spin here, and no real spec nests
    // groups anywhere near this deep.
    for _ in 0..64 {
        let Some(current) = parent else { break };
        if let Some(label) = group_label(current, source) {
            chain.push(label);
        }
        parent = current.parent();
    }
    chain.reverse();
    chain.push(format!("it[{ordinal}]"));
    chain.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::LangPack;
    use crate::tamper::parse_tests;

    fn names(source: &str) -> Vec<String> {
        parse_tests(source, &Ruby)
            .expect("ruby parses")
            .into_iter()
            .map(|t| t.name)
            .collect()
    }

    fn only(source: &str) -> crate::tamper::TestFn {
        let mut found = parse_tests(source, &Ruby).expect("ruby parses");
        assert_eq!(found.len(), 1, "expected exactly one test in {source:?}");
        found.remove(0)
    }

    #[test]
    fn rspec_examples_are_named_by_their_description() {
        let names = names(
            r#"
RSpec.describe Calc do
  describe ".add" do
    it "adds two positive integers" do
      expect(Calc.add(1, 2)).to eq(3)
    end
    specify "specify is an example too" do
      expect(Calc.add(1, 2)).to eq(3)
    end
    example "so is example" do
      expect(Calc.add(1, 2)).to eq(3)
    end
  end
end
"#,
        );
        assert_eq!(
            names,
            vec![
                "adds two positive integers",
                "so is example",
                "specify is an example too",
            ]
        );
    }

    #[test]
    fn describe_and_context_are_containers_and_never_tests() {
        // If a group were reported as a test, every nested example's assertions
        // would be attributed to it and the examples themselves would vanish.
        let names = names(
            r#"
RSpec.describe Calc do
  context "when the denominator is zero" do
    it "raises" do
      expect { Calc.div(1, 0) }.to raise_error(ZeroDivisionError)
    end
  end
end
"#,
        );
        assert_eq!(names, vec!["raises"]);
    }

    #[test]
    fn minitest_runs_test_underscore_methods_and_nothing_else() {
        let names = names(
            r#"
class TestCalc < Minitest::Test
  def setup
    @calc = Calc
  end

  def test_adds
    assert_equal 3, Calc.add(1, 2)
  end

  def helper_not_a_test
    assert_equal 1, 1
  end

  def testify
    assert_equal 1, 1
  end
end
"#,
        );
        assert_eq!(names, vec!["test_adds"]);
    }

    #[test]
    fn test_unit_and_rails_macro_forms_are_both_found() {
        let names = names(
            r#"
class MyTest < Test::Unit::TestCase
  def test_unit_style
    assert_equal(3, Calc.add(1, 2))
  end
end

class ControllerTest < ActiveSupport::TestCase
  test "the rails macro declares a test" do
    assert_equal 3, Calc.add(1, 2)
  end
end
"#,
        );
        assert_eq!(
            names,
            vec!["test_unit_style", "the rails macro declares a test"]
        );
    }

    #[test]
    fn the_unnamed_one_liner_gets_a_positional_identity_not_a_none() {
        // `None` would make this example invisible to every §6.7 detector.
        let names = names(
            r#"
RSpec.describe Calc do
  describe ".add" do
    it { is_expected.to eq(3) }
    it { is_expected.to be_positive }
    it "named ones do not take an index" do
      expect(Calc.add(1, 2)).to eq(3)
    end
  end
end
"#,
        );
        assert_eq!(
            names,
            vec![
                "Calc .add it[0]",
                "Calc .add it[1]",
                "named ones do not take an index",
            ]
        );
    }

    #[test]
    fn a_named_example_inserted_above_does_not_renumber_the_unnamed_ones() {
        // The reason the ordinal counts only unnamed siblings: adding a normal
        // example must not renumber — and so re-pair — every `it { }` below it.
        let before = names(
            r#"
describe Calc do
  it { is_expected.to eq(3) }
end
"#,
        );
        let after = names(
            r#"
describe Calc do
  it "a new named example" do
    expect(Calc.add(1, 2)).to eq(3)
  end
  it { is_expected.to eq(3) }
end
"#,
        );
        assert!(before.contains(&"Calc it[0]".to_string()));
        assert!(after.contains(&"Calc it[0]".to_string()));
    }

    #[test]
    fn classify_reads_the_matcher_that_follows_to() {
        let cases = [
            ("expect(Calc.add(1, 2)).to eq(3)", Strength::Equality),
            ("expect(x).to eql(y)", Strength::Equality),
            ("expect(x).not_to equal(y)", Strength::Equality),
            ("is_expected.to eq(3)", Strength::Equality),
            ("expect(list).to include(3)", Strength::Containment),
            ("expect(s).to match(/abc/)", Strength::Containment),
            ("expect(s).to start_with(\"ab\")", Strength::Containment),
            (
                "expect(a).to be_within(0.01).of(3.5)",
                Strength::Approximate,
            ),
            ("expect(x).to be_truthy", Strength::Truthy),
            ("expect(x).to be_a(Rational)", Strength::Truthy),
            (
                "expect { foo }.to raise_error(ArgumentError)",
                Strength::Truthy,
            ),
            ("assert_equal 3, Calc.add(1, 2)", Strength::Equality),
            ("refute_equal 3, Calc.add(1, 2)", Strength::Equality),
            ("assert_includes list, 3", Strength::Containment),
            ("assert_match(/a/, s)", Strength::Containment),
            ("assert_in_delta 3.5, a, 0.01", Strength::Approximate),
            ("assert_nil x", Strength::Truthy),
            ("assert x", Strength::Truthy),
            ("_(Calc.add(1, 2)).must_equal 3", Strength::Equality),
            ("value(x).must_be_close_to 3.0", Strength::Approximate),
            ("x.should eq(3)", Strength::Equality),
            ("assert x == 3", Strength::Equality),
            ("expect(x).to be == 3", Strength::Equality),
        ];
        for (text, want) in cases {
            assert_eq!(Ruby.classify(text), want, "classifying {text:?}");
        }
    }

    #[test]
    fn the_matcher_wins_over_a_shape_word_in_the_subject() {
        // Both of these are the reason `classify` anchors on `.to` instead of
        // scanning the whole text. Read the other way round, the first becomes
        // a false `assertion_weakened` fail the moment the subject is
        // simplified, and the second is promoted by the contents of a regex.
        assert_eq!(
            Ruby.classify("expect(list.include?(3)).to be_truthy"),
            Strength::Truthy
        );
        assert_eq!(
            Ruby.classify("expect(s).to match(/equal/)"),
            Strength::Containment
        );
    }

    #[test]
    fn a_message_string_cannot_decide_a_shape() {
        // The ambiguity budget's operational definition, on Ruby's own literal
        // forms: `%w[]`, a heredoc, and an ordinary message.
        for text in [
            r#"assert Calc.add(1, 2), "expected 3 to eq 3""#,
            r#"assert Calc.ok?, %w[eq include within].join(" ")"#,
            "assert Calc.ok?, <<~MSG\n  expected to eq 3\nMSG",
        ] {
            assert_eq!(Ruby.classify(text), Strength::Truthy, "{text:?}");
            // The counterfactual MUST flag this. `is_ambiguous` asks
            // "would a message string have decided this shape?", and for these
            // the answer is yes — which is exactly why stripping is load-bearing
            // and why the assertion above is the one that matters.
            //
            // This read `!is_ambiguous(...)` until the corpus run showed the
            // predicate was vacuous (`classify` strips as its first act, so it
            // compared strip(x) with strip(strip(x)) and could only ever answer
            // "not ambiguous"). The negation passed for free and asserted
            // nothing. Flipped rather than deleted: a case where a message COULD
            // decide the shape, and does not, is the whole claim.
            assert!(
                crate::shape::is_ambiguous(&Ruby, text),
                "{text:?} carries a message that would decide its shape, but the \
                 counterfactual did not flag it — the metric is blind on this \
                 Ruby literal form"
            );
        }
    }

    #[test]
    fn skip_markers_are_tokens_so_a_description_cannot_fake_one() {
        let skipped = only(
            r#"
describe Calc do
  xit "is skipped" do
    expect(Calc.add(1, 2)).to eq(3)
  end
end
"#,
        );
        assert!(skipped.has_skip_marker);

        for source in [
            r#"
describe Calc do
  it "skips invalid rows" do
    expect(Calc.add(1, 2)).to eq(3)
  end
end
"#,
            r#"
describe Calc do
  it "reads a header" do
    expect(Calc.skip_header(rows)).to eq(3)
  end
end
"#,
        ] {
            assert!(
                !only(source).has_skip_marker,
                "a description or an identifier was read as a skip marker: {source:?}"
            );
        }
    }

    #[test]
    fn skip_and_pending_in_the_body_and_in_the_metadata_both_count() {
        for source in [
            "describe Calc do\n  it \"x\" do\n    skip \"flaky\"\n    expect(a).to eq(1)\n  end\nend",
            "describe Calc do\n  it \"x\" do\n    pending \"not ready\"\n    expect(a).to eq(1)\n  end\nend",
            "describe Calc do\n  it \"x\", :skip do\n    expect(a).to eq(1)\n  end\nend",
            "describe Calc do\n  it \"x\", skip: true do\n    expect(a).to eq(1)\n  end\nend",
            "class TestCalc < Minitest::Test\n  def test_x\n    skip\n    assert_equal 1, 1\n  end\nend",
        ] {
            assert!(
                only(source).has_skip_marker,
                "no skip marker found in {source:?}"
            );
        }
    }

    #[test]
    fn a_fluent_assertion_is_counted_once() {
        // `expect(x).to eq(y)` is one assertion whose receiver is also a call.
        // Counting both halves turns an honest 1 -> 1 edit into a 2 -> 1
        // `assertion_count_decreased`.
        let test = only(
            r#"
describe Calc do
  it "adds" do
    expect(Calc.add(1, 2)).to eq(3)
  end
end
"#,
        );
        assert_eq!(test.assertions, vec![Strength::Equality]);
    }

    #[test]
    fn mock_targets_name_the_receiver_and_the_method() {
        let test = only(
            r#"
describe Calc do
  it "adds" do
    allow(Shipping).to receive(:rate).and_return(9)
    Calc.expects(:add).returns(3)
    expect(Calc.total).to eq(9)
  end
end
"#,
        );
        assert_eq!(
            test.mock_targets,
            vec![
                "Shipping".to_string(),
                "add".to_string(),
                "rate".to_string()
            ]
        );
    }

    #[test]
    fn retry_the_keyword_counts_and_retry_the_word_in_a_message_does_not() {
        let real = only(
            "describe Calc do\n  it \"x\" do\n    begin\n      expect(a).to eq(1)\n    rescue\n      retry\n    end\n  end\nend",
        );
        assert!(real.has_sleep_or_retry);
        let message = only(
            "describe Calc do\n  it \"x\" do\n    expect(a).to eq(1), \"retry the request\"\n  end\nend",
        );
        assert!(!message.has_sleep_or_retry);
    }

    /// The corpus census the module header quotes, **re-derived rather than
    /// transcribed**.
    ///
    /// Every number at the top of this file came out of this walk. A figure
    /// copied into a comment and left there is the failure
    /// `scripts/vendor-freshness.py` was written to stop — four of the six
    /// vendor facts it was built to guard were already wrong on its first run —
    /// so the walk stays in the file that makes the claims and prints them in
    /// the same order the header states them.
    ///
    /// `#[ignore]` because it needs a corpus this repository does not contain
    /// and must not: it is ~1 GB of other people's source. Point
    /// `THRULINE_RUBY_CORPUS` at a directory holding rails, rspec-core, jekyll,
    /// sinatra, puma and faker checkouts and run
    ///
    /// ```text
    /// THRULINE_RUBY_CORPUS=… cargo test -p thruline-langpacks -- --ignored --nocapture ruby_corpus
    /// ```
    ///
    /// **Absent corpus is a skip, and a skip is not a pass** — the same rule
    /// `vendor-freshness.py` states as *"exit 3 is not a pass"*. The test panics
    /// with the variable's name rather than returning quietly, so a green run
    /// can never mean "the corpus was not there".
    #[test]
    #[ignore = "needs THRULINE_RUBY_CORPUS; see the doc comment"]
    fn ruby_corpus_census() {
        let root = std::env::var("THRULINE_RUBY_CORPUS").expect(
            "THRULINE_RUBY_CORPUS is unset — this test measures nothing without it, and a \
             quiet skip would read as a confirmed census",
        );

        // The selection rule, stated so a re-run selects the same files: the
        // three names Ruby's two runners actually load, anywhere in the tree.
        // Restricting to `spec/` and `test/` directories instead selects 1,884
        // and drops rails' engine-local suites.
        fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                    continue;
                }
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if name.ends_with("_spec.rb")
                    || name.ends_with("_test.rb")
                    || name.starts_with("test_") && name.ends_with(".rb")
                {
                    out.push(path);
                }
            }
        }
        let mut files = Vec::new();
        walk(std::path::Path::new(&root), &mut files);
        files.sort();

        let (mut tests, mut assertions, mut grammar_errors, mut testless) =
            (0usize, 0usize, 0usize, 0usize);
        let mut by_strength = std::collections::BTreeMap::new();
        for path in &files {
            let Ok(source) = std::fs::read_to_string(path) else {
                continue;
            };
            let Ok(found) = parse_tests(&source, &Ruby) else {
                grammar_errors += 1;
                continue;
            };
            if found.is_empty() {
                testless += 1;
            }
            tests += found.len();
            for test in &found {
                assertions += test.assertions.len();
                for strength in &test.assertions {
                    *by_strength.entry(format!("{strength:?}")).or_insert(0usize) += 1;
                }
            }
        }

        println!(
            "files {} · grammar errors {grammar_errors} · tests {tests} · assertions {assertions} \
             · files with no test {testless}\nby strength {by_strength:?}",
            files.len()
        );
        assert_eq!(grammar_errors, 0, "the header claims 0 grammar errors");
    }

    #[test]
    fn extensions_are_lowercase_and_dotless() {
        for ext in Ruby.extensions() {
            assert!(!ext.starts_with('.'), "{ext}");
            assert_eq!(*ext, ext.to_ascii_lowercase(), "{ext}");
        }
        assert!(crate::pack::pack_for_path("spec/calc_spec.rb").is_some());
    }
}

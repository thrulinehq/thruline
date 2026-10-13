//! The C# pack: xUnit, NUnit, MSTest, FluentAssertions, Shouldly, Moq, NSubstitute.
//!
//! # Every node kind here was read off a parse tree, not remembered
//!
//! `method_declaration`, `attribute_list`, `attribute`, `invocation_expression`,
//! `try_statement`, `catch_clause` and the `name`/`body` field names were all
//! printed from `tree_sitter_c_sharp` parsing a file that exercises each form
//! (merged attribute lists, expression-bodied methods, `try`/`catch`, generic
//! mock creation). Two of them are not what a from-memory guess produces:
//!
//! - `attribute_list` is a **child** of `method_declaration`, not a preceding
//!   sibling, and a method written with attributes on separate lines has
//!   **several** of them. Hence [`SkipSite::DeclarationChild`].
//! - a method's name is `child_by_field_name("name")`, not "the first
//!   identifier child" — `private static async Task Helper()` has `Task` as an
//!   identifier child before the name.
//!
//! # The two false positives this file is shaped around
//!
//! Both were found in the corpus (AutoMapper 9b500d5, xunit 6bbefae,
//! efcore c5ca4bb, serilog 49b5339, Newtonsoft.Json 09bb545), not imagined.
//!
//! Every count below is an occurrence count from
//! `grep -roh --include='*.cs' -e '<pattern>' <corpus>` over those five
//! repositories, and each pattern is written out so the number can be
//! re-derived rather than believed. Three of the counts in an earlier version
//! of this comment could not be: `[TestCaseOrderer]` was written 22 and is 23,
//! `[TestMethodOrderer]` was written 12 and is 13, and `Assert.Skip(…)` was
//! written 41 — which is the count of `Assert.Skip` *without* the paren, and so
//! includes 8 `Assert.SkipWhen`/`SkipUnless` calls that are conditional and are
//! a different thing. A number with no pattern beside it is a number nobody can
//! check, which is how all three drifted.
//!
//! 1. **`[IgnoreDataMember]`** (`IgnoreDataMember`, 23) — a `contains("ignore")`
//!    skip rule fires on it. So [`CSharp::is_skip_marker`] extracts each
//!    attribute's **name** and compares it exactly, and the same rule keeps
//!    `[TestFixture` (307), `[TestCaseOrderer` (23) and `[TestMethodOrderer`
//!    (13) from being read as `[Test]`/`[TestMethod]` in [`CSharp::test_name`].
//! 2. **`.ShouldLog(` (260), `.ShouldStopProcessing(` (80),
//!    `.ShouldSerialize…(` (50), `.ShouldContinue(` (7)** — efcore, Newtonsoft
//!    and xunit domain predicates that a wildcard `.Should` assertion prefix
//!    would count as assertions (397 between them; `ShouldLog` is declared
//!    `public bool ShouldLog(EventId, LogLevel)`). So the Shouldly prefixes are
//!    enumerated rather than wildcarded, and the enumeration below matches
//!    every real Shouldly call in the corpus and none of those four.
//!
//! # What this pack measured, and the one thing it cannot see
//!
//! Run over the whole C# corpus — 8,989 `.cs` files, of which 2,043 contain at
//! least one recognised test — this pack finds **24,568 tests and 72,359
//! assertions**, and **0** of those assertions are ambiguous under
//! [`crate::shape::is_ambiguous`] (budget: < 0.1% over ≥ 5,000). 322 tests carry
//! a skip marker and 4 swallow a failure. Re-measured from scratch when the
//! runner parsers were written, against the same five repositories, and
//! reproduced to the unit.
//!
//! **It does not see a test whose attribute is inherited.** xUnit applies a
//! base class's `[Fact]` / `[Theory]` to an override, and efcore's provider
//! suites are built entirely that way:
//!
//! ```text
//! // NorthwindWhereQueryTestBase.cs   [Theory, MemberData(nameof(IsAsyncData))]
//! //                                  public virtual Task Where_simple(bool async)
//! // NorthwindWhereQuerySqlServerTest.cs
//! public override async Task Where_simple(bool async)      // no attribute here
//! {
//!     await base.Where_simple(async);
//!     AssertSql("""SELECT …""");
//! }
//! ```
//!
//! efcore alone holds **23,628** `public override` declarations, **19,042** of
//! them methods returning `Task` or `void`
//! (`grep -roh --include='*.cs' -e 'public override \(async \)\?\(Task\|void\)'`),
//! and **14,024** of those are followed within three lines by an `await base.`
//! call — the delegating shape above. This pack reports none of them as tests,
//! so §6.7 never compares them. An earlier version of this comment said 18,894
//! and named no pattern; no grep over this corpus produces that number. Fixing
//! it needs the base type resolved across files, which a tree-sitter pack
//! cannot do; a heuristic ("an `override` in a file that has tests") would
//! decide a `test_deleted` fail on a guess, and that is worse than the miss.
//! Stated here rather than left to be discovered from an empty report.

use tree_sitter::Node;

use crate::pack::{LangPack, LangSpec, PackLanguage, SkipSite, Strength};

pub struct CSharp;

/// Attributes that mark a method as a test, by **exact** name.
///
/// Exact and not a prefix: `[TestFixture]`, `[TestCaseOrderer]` and
/// `[TestMethodOrderer]` all start with a marker's name and mark no test. The
/// `Attribute` suffix is stripped before comparison because C# lets both
/// `[Fact]` and `[FactAttribute]` name the same type, and a namespace
/// qualification (`[Xunit.Fact]`) is reduced to its last segment.
const TEST_ATTRIBUTES: &[&str] = &[
    // xUnit (v2 and v3), and Xunit.SkippableFact's variants — which mark a
    // test; whether one *skips* is decided at run time by `Skip.If(…)`, so they
    // are deliberately not in `SKIP_ATTRIBUTES`.
    "fact",
    "theory",
    "skippablefact",
    "skippabletheory",
    // NUnit. `theory` is shared with xUnit above.
    "test",
    "testcase",
    "testcasesource",
    // MSTest.
    "testmethod",
    "datatestmethod",
];

/// Attributes whose presence means the test does not run.
///
/// `ignore` is NUnit's and MSTest's `[Ignore]` / `[Ignore("reason")]`.
/// `explicit` is NUnit's `[Explicit]`: the test runs only when named
/// individually, so adding one to a suite run is the same event as a skip.
///
/// **`ignoredatamember` is absent and must stay absent** — see the module docs.
/// It is why this is an exact-name list and not a substring scan.
const SKIP_ATTRIBUTES: &[&str] = &["ignore", "explicit"];

impl LangPack for CSharp {
    fn language(&self) -> PackLanguage {
        PackLanguage::CSharp
    }

    fn extensions(&self) -> &'static [&'static str] {
        // `.csx` (C# scripting) is left out on purpose: nothing in the corpus
        // puts tests in one, and a pack that claims an extension it has never
        // parsed is coverage that does not exist.
        &["cs"]
    }

    fn ts_language(&self) -> tree_sitter::Language {
        tree_sitter_c_sharp::LANGUAGE.into()
    }

    fn spec(&self) -> LangSpec {
        LangSpec {
            fn_kinds: &["method_declaration"],
            assertion_kinds: &["invocation_expression"],
            assertion_call_prefixes: ASSERTION_PREFIXES,
            // Consulted only by the default `is_skip_marker`, which this pack
            // overrides. Kept populated so `spec()` still carries the
            // vocabulary for a reader — and for the merge into `verify::tamper`
            // — even where the override does not travel.
            skip_markers: &["skip =", "skip=", "[ignore", "[explicit"],
            // **Empty on purpose.** `verify`'s substring swallow path fires
            // when a marker is present AND the text contains `pass`, `{}` or
            // `continue`. `{}` appears in almost every C# test (empty
            // initialisers, empty blocks, interpolation braces), so every test
            // with a `catch` would be reported as swallowing. `catch_kinds`
            // below does the job structurally instead.
            swallow_markers: &[],
            // C# has no name-prefix test convention: every framework marks
            // tests with an attribute, which is what `test_name` reads. Left
            // empty rather than filled with a plausible-looking guess.
            test_name_prefixes: &[],
            skip_site: SkipSite::DeclarationChild,
            decorator_kinds: &["attribute_list"],
            catch_kinds: &["try_statement"],
            defer_kinds: &[],
            neutralising_calls: &[],
            unconditional_fail_markers: &[
                "assert.fail(",
                "assert.true(false",
                "xunit.assert.fail(",
            ],
        }
    }

    /// A C# test is a `method_declaration` carrying a test attribute.
    ///
    /// Attribute lists merge (`[Fact, Trait("Category", "Slow")]`) and repeat
    /// (`[Test]` on one line, `[Ignore("flaky")]` on the next). Both shapes are
    /// in the corpus, so every `attribute_list` child is read and each one is
    /// split into its individual attributes.
    fn test_name(&self, node: Node, source: &str) -> Option<String> {
        if node.kind() != "method_declaration" {
            return None;
        }
        let mut cursor = node.walk();
        let marked = node
            .children(&mut cursor)
            .filter(|c| c.kind() == "attribute_list")
            .filter_map(|c| c.utf8_text(source.as_bytes()).ok())
            .flat_map(attribute_names)
            .any(|n| TEST_ATTRIBUTES.contains(&n.as_str()));
        if !marked {
            return None;
        }
        let name = node.child_by_field_name("name")?;
        Some(name.utf8_text(source.as_bytes()).ok()?.to_string())
    }

    /// The assertion-shape classifier.
    ///
    /// Order is Approximate → Equality → Containment → Truthy, which is
    /// `verify::tamper::strength_of`'s order and matters twice:
    /// `Is.EqualTo(x).Within(0.01)` names both an equality and a tolerance and
    /// **is** the tolerance, and `Assert.Equal(expected, xs.Where(v =>
    /// set.Contains(v)))` names both an equality and a containment and **is**
    /// the equality.
    ///
    /// Every match is on a **call token including its opening paren**, over
    /// whitespace-removed text, rather than on a bare word. `Assert.True(ok,
    /// "expected 30 == 29")` is Truthy, and it is only Truthy because
    /// [`crate::shape::strip_language_literals`] runs first — the message
    /// carries `==`. That is the ambiguity budget in one example.
    fn classify(&self, assertion_text: &str) -> Strength {
        let stripped = crate::shape::strip_language_literals(assertion_text);
        let lower = stripped.to_ascii_lowercase();
        let dense: String = lower.chars().filter(|c| !c.is_whitespace()).collect();

        if APPROXIMATE_TOKENS.iter().any(|t| dense.contains(t)) || has_precision_arg(&dense) {
            return Strength::Approximate;
        }
        if EQUALITY_TOKENS.iter().any(|t| dense.contains(t)) {
            return Strength::Equality;
        }
        if CONTAINMENT_TOKENS.iter().any(|t| dense.contains(t)) {
            return Strength::Containment;
        }
        Strength::Truthy
    }

    /// Mock creations whose first token names the **type** being faked.
    ///
    /// `verify::tamper::first_target` reads a quoted argument first and an
    /// identifier second; C# has no quoted form, so every entry here ends at
    /// the `<` that opens the type argument list and the identifier path takes
    /// `IShippingRate` out of `new Mock<IShippingRate>()`.
    ///
    /// Moq's `mock.Setup(m => …)` and `mock.Verify(…)` are **not** here: their
    /// first argument is a lambda parameter, so `first_target` would record the
    /// mock target as `m` for every Moq test in existence.
    fn mock_calls(&self) -> &'static [&'static str] {
        // Case-sensitive: `mock_targets_in` matches these against the raw
        // source, not a lowercased copy.
        &[
            "new Mock<",
            "Mock.Of<",
            "Substitute.For<",
            "Substitute.ForPartsOf<",
        ]
    }

    /// Does this attribute text skip the test?
    ///
    /// **The attribute name is not enough**, which is why the default
    /// implementation (a substring scan of `skip_markers`) cannot serve C#:
    /// `[Fact]` runs and `[Fact(Skip = "flaky")]` does not, and they share
    /// their name. So two independent rules:
    ///
    /// 1. a `Skip` **named argument** whose value is not `null`, in any
    ///    attribute — that is xUnit's form, and `[Theory(Skip = …)]` and any
    ///    third-party `[FooFact(Skip = …)]` get it for free;
    /// 2. an attribute whose exact name is in [`SKIP_ATTRIBUTES`] — NUnit's and
    ///    MSTest's `[Ignore]`, and NUnit's `[Explicit]`.
    ///
    /// Rule 1 runs on the raw text so the value is still visible; rule 2 runs
    /// after literals are stripped so a bracket or comma inside a message
    /// (`[Trait("a,b", "c")]`) cannot break the attribute split.
    ///
    /// ## What this does not catch, stated rather than papered over
    ///
    /// xUnit v3's `Assert.Skip(` (33 occurrences in the corpus), NUnit's
    /// `Assert.Ignore(…)` and MSTest's `Assert.Inconclusive(…)` skip from
    /// **inside the body**. [`SkipSite::DeclarationChild`] means
    /// `tamper::as_test_fn` never passes the body to this function, so a
    /// session that adds one is not reported as `skip_marker_added`. The fix is
    /// not `SkipSite::Body` — that scans the whole declaration as a substring,
    /// so a commented-out `// [Fact(Skip = "…")]` reads as a live marker, and
    /// the corpus has exactly that at
    /// `AutoMapper/src/UnitTests/BidirectionalRelationshipsWithoutPR.cs:451`.
    /// It needs a site that is *both*, which `SkipSite` cannot express today.
    fn is_skip_marker(&self, decorator_text: &str) -> bool {
        if has_skip_named_argument(decorator_text) {
            return true;
        }
        let without_literals = crate::shape::strip_language_literals(decorator_text);
        let named =
            attribute_names(&without_literals).any(|n| SKIP_ATTRIBUTES.contains(&n.as_str()));
        named
    }
}

/// What counts as an assertion call.
///
/// A bare entry has to **start** the invocation's text; an entry beginning with
/// `.` is matched anywhere in its first line, which is how the fluent forms are
/// reached (`verify::tamper::is_assertion`).
///
/// `.should()` is written without a trailing dot on purpose: a chain broken
/// across lines —
///
/// ```text
/// result.Should()
///       .Be(9.0);
/// ```
///
/// — puts `.Be(` on the second line, and `is_assertion` only reads the first.
/// `.should().` would miss it; `.should()` does not. A chain broken *before*
/// the `.Should()` is still missed, and that limit lives in `is_assertion`.
const ASSERTION_PREFIXES: &[&str] = &[
    // `Assert.Equal`, `Assert.That`, `Assert.IsTrue` — and every C# helper
    // whose name *starts* with `Assert`, which is the convention and not a
    // guess: the corpus holds **30,916** such calls (`AssertSql(` 17,255,
    // `AssertQuery(` 4,384, `AssertTranslationFailed(` 1,036, `AssertEqual(`
    // 923, …), and a first version of this table that wrote `assert.` — with
    // the dot — counted every efcore test-base test as having **zero**
    // assertions. Under-counting is the direction that hides a deletion, so it
    // is the direction that matters.
    //
    // The bare form is also `verify`'s: its Python and JavaScript arms list
    // `"assert"`, and `collect_assertions` ORs `head.starts_with("assert")`
    // unconditionally on top of the table.
    //
    // Measured cost: 66 calls in the corpus whose receiver merely begins with
    // the word are counted too — `assertContext.` 22, `assertionElement.` 21,
    // and 23 others across six more identifiers.
    // They inflate a count symmetrically in base and head, so they produce no
    // finding unless one is removed — a `warn`, never a wrong verdict.
    "assert",
    "classicassert.",
    // NUnit's and MSTest's other assertion classes, which `assert` does not
    // reach because the head identifier does not start with the word.
    "stringassert.",
    "collectionassert.",
    "fileassert.",
    "directoryassert.",
    // Newtonsoft.Json's own helper, 664 calls. Named because a project's
    // `<Something>Assert` class is as much the C# idiom as NUnit's own, and
    // `is_assertion` has no way to say "the head ends in `Assert`".
    "exceptionassert.",
    // Shouldly's static entry points: `Should.Throw<T>(…)`, `Should.NotThrow(…)`.
    "should.",
    // FluentAssertions. **Zero occurrences in this corpus** — none of the five
    // repositories use the library — so this arm is written from the published
    // API and is unmeasured here. Named, not hidden.
    ".should()",
    // Shouldly, enumerated. See the module docs for the four domain predicates
    // a `.should` wildcard would have swallowed.
    ".shouldbe",
    ".shouldnotbe",
    ".shouldcontain",
    ".shouldnotcontain",
    ".shouldstartwith",
    ".shouldendwith",
    ".shouldthrow",
    ".shouldnotthrow",
    ".shouldsatisfyallconditions",
    ".shouldallbe",
    ".shouldhavesingleitem",
    ".shouldmatch",
    ".shouldbeoneof",
    ".shouldcompletein",
    // Moq's behavioural assertions.
    ".verify(",
    ".verifyall(",
    // NSubstitute's.
    ".received(",
    ".didnotreceive(",
    ".receivedwithanyargs(",
    ".didnotreceivewithanyargs(",
];

/// Tolerance-bearing shapes. Checked first — see [`CSharp::classify`].
const APPROXIMATE_TOKENS: &[&str] = &[
    // NUnit: `Is.EqualTo(x).Within(0.01)`.
    ".within(",
    // FluentAssertions: `.Should().BeApproximately(x, 0.01)`.
    "approximately",
    // FluentAssertions: `.Should().BeCloseTo(t, 1.Seconds())`.
    "beclose",
    // NUnit's `Tolerance` type, and any explicitly named tolerance argument.
    "tolerance",
];

/// Identity shapes.
///
/// `==` and `!=` are here for the `Assert.True(a == b)` form. A lambda arrow
/// (`=>`) and the ordering operators (`>=`, `<=`) do not contain `==`, so they
/// do not reach this.
const EQUALITY_TOKENS: &[&str] = &[
    "assert.equal(",
    "assert.notequal(",
    "assert.strictequal(",
    "assert.areequal(",
    "assert.arenotequal(",
    "assert.same(",
    "assert.notsame(",
    "assert.aresame(",
    "assert.arenotsame(",
    "assert.equivalent(",
    "collectionassert.areequal(",
    "collectionassert.areequivalent(",
    "collectionassert.arenotequal(",
    // NUnit constraint vocabulary.
    "is.equalto(",
    "is.not.equalto(",
    "is.sameas(",
    "is.not.sameas(",
    // Shouldly.
    ".shouldbe(",
    ".shouldnotbe(",
    ".shouldbesameas(",
    ".shouldnotbesameas(",
    ".shouldbeequivalentto(",
    // FluentAssertions. `.should().be(` keeps its paren so `.Should().BeTrue()`
    // and `.Should().BeNull()` fall through to Truthy — which is where a
    // weakening from `.Be(30m)` to `.BeTrue()` has to land for §6.7 to call it
    // a fail.
    ".should().be(",
    ".should().notbe(",
    ".should().equal(",
    ".should().beequivalentto(",
    "==",
    "!=",
];

/// Membership shapes.
const CONTAINMENT_TOKENS: &[&str] = &[
    "assert.contains(",
    "assert.doesnotcontain(",
    "assert.startswith(",
    "assert.endswith(",
    "assert.matches(",
    "assert.doesnotmatch(",
    "assert.subset(",
    "assert.propersubset(",
    "assert.superset(",
    "assert.propersuperset(",
    "assert.inrange(",
    "assert.notinrange(",
    "assert.single(",
    "stringassert.",
    "collectionassert.contains(",
    "collectionassert.issubsetof(",
    // NUnit constraint vocabulary.
    "is.substringof(",
    "is.stringcontaining(",
    "does.contain(",
    "does.startwith(",
    "does.endwith(",
    "does.match(",
    // Shouldly.
    ".shouldcontain(",
    ".shouldnotcontain(",
    ".shouldstartwith(",
    ".shouldendwith(",
    ".shouldmatch(",
    ".shouldbeoneof(",
    // FluentAssertions.
    ".should().contain(",
    ".should().startwith(",
    ".should().endwith(",
    ".should().match(",
    // The general forms, for `Assert.True(list.Contains(x))`.
    ".contains(",
    ".startswith(",
    ".endswith(",
];

/// `Assert.Equal(expected, actual, 3)` — xUnit's precision overload — and
/// `Assert.AreEqual(a, b, 0.001)` — MSTest's delta overload.
///
/// Nineteen real occurrences in the corpus, all of the form
/// `Assert.Equal(0.11111, 0.11444, 3)`. They are not equalities: a change from
/// the two-argument call to this one is a weakening, and without this rule both
/// forms classify as `Equality` and the weakening is invisible.
///
/// The third argument must be a **numeric literal**, which is what separates
/// the tolerance overload from `Assert.Equal(expected, actual, comparer)`.
fn has_precision_arg(dense: &str) -> bool {
    for head in ["assert.equal", "assert.areequal"] {
        let Some(rest) = dense.strip_prefix(head) else {
            continue;
        };
        // `Assert.Equal<double>(…)`: step over the type argument list.
        let rest = match rest.strip_prefix('<') {
            Some(after) => match after.split_once('>') {
                Some((_, tail)) => tail,
                None => continue,
            },
            None => rest,
        };
        let Some(args) = rest.strip_prefix('(') else {
            continue;
        };
        let args = top_level_args(args);
        if args.len() == 3 && is_numeric_literal(args[2]) {
            return true;
        }
    }
    false
}

/// Split an argument list — text starting just after its `(` — at depth-0
/// commas, stopping at the matching `)`.
///
/// String literals are already gone by the time this runs, so a comma or a
/// bracket inside a message cannot reach it.
fn top_level_args(after_open_paren: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, c) in after_open_paren.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' if depth == 0 => {
                out.push(&after_open_paren[start..i]);
                return out;
            }
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&after_open_paren[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    // Unbalanced: the node text was truncated, or the parse recovered from an
    // error. Return what is there rather than inventing a closing paren.
    out.push(&after_open_paren[start..]);
    out
}

/// Is this argument a numeric literal?
///
/// Permissive about C#'s suffixes (`m`, `f`, `d`, `u`, `l`) and exponents,
/// strict about the first character, because the only thing it has to separate
/// is `3` / `0.001` from an identifier like `comparer`.
fn is_numeric_literal(arg: &str) -> bool {
    let arg = arg.trim();
    if !arg.starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == '+' || c == '.') {
        return false;
    }
    arg.chars().all(|c| {
        c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | '_' | 'e' | 'm' | 'f' | 'd' | 'u' | 'l')
    })
}

/// The individual attribute names in some attribute-list text.
///
/// `[Fact, Trait("Category", "Slow")]` yields `fact`, `trait`.
/// `[Xunit.Fact]` yields `fact` — the namespace qualification is dropped.
/// `[FactAttribute]` yields `fact` — C# lets the suffix be written or omitted.
///
/// Names come back lowercase, so both callers compare against the lowercase
/// tables above whatever case the caller's text arrived in. (`as_test_fn`
/// lowercases decorator text before calling `is_skip_marker`; `test_name` does
/// not lowercase anything.)
fn attribute_names(text: &str) -> impl Iterator<Item = String> + '_ {
    let mut names = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] != '[' {
            i += 1;
            continue;
        }
        i += 1;
        let mut depth = 0i32;
        let mut current = String::new();
        while i < chars.len() {
            let c = chars[i];
            match c {
                '(' | '[' => {
                    depth += 1;
                    i += 1;
                }
                ')' => {
                    depth -= 1;
                    i += 1;
                }
                ']' if depth == 0 => {
                    i += 1;
                    break;
                }
                ']' => {
                    depth -= 1;
                    i += 1;
                }
                ',' if depth == 0 => {
                    push_attribute_name(&current, &mut names);
                    current.clear();
                    i += 1;
                }
                _ => {
                    if depth == 0 {
                        current.push(c);
                    }
                    i += 1;
                }
            }
        }
        push_attribute_name(&current, &mut names);
    }
    names.into_iter()
}

fn push_attribute_name(raw: &str, out: &mut Vec<String>) {
    let ident: String = raw
        .trim()
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
        .collect();
    let Some(last) = ident.rsplit('.').next() else {
        return;
    };
    if last.is_empty() {
        return;
    }
    let lower = last.to_ascii_lowercase();
    let lower = lower
        .strip_suffix("attribute")
        .unwrap_or(&lower)
        .to_string();
    if !lower.is_empty() {
        out.push(lower);
    }
}

/// Does the attribute text carry `Skip = <something other than null>`?
///
/// Run on the **raw** text, before literals are stripped, because the value is
/// the whole point: `[Fact(Skip = "flaky")]` skips and `[Fact(Skip = null)]`
/// does not.
///
/// Three guards, each for a shape that occurs:
/// - the character before `skip` must not be part of an identifier, so
///   `AutoSkip = …` does not match;
/// - the character after `skip` must not be either, so xUnit v3's
///   `SkipWhen` / `SkipUnless` — which are **conditional**, and whose condition
///   this cannot evaluate — do not match;
/// - the `=` must not be `==`, so a comparison never reads as an assignment.
///
/// `[Fact(Skip = "…", SkipWhen = nameof(X))]` therefore still reads as a skip
/// on the strength of its `Skip`. That over-reports when the condition is
/// false; the finding is a `warn`, and over-reporting is the direction that
/// fails loud.
fn has_skip_named_argument(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    // Char indices, not byte indices: an attribute argument can hold any UTF-8
    // and slicing a byte offset into the middle of one would panic.
    let chars: Vec<char> = lower.chars().collect();
    let mut at = 0usize;
    while at + 4 <= chars.len() {
        if chars[at..at + 4] != ['s', 'k', 'i', 'p'] {
            at += 1;
            continue;
        }
        let before_ok = at == 0 || !(chars[at - 1].is_alphanumeric() || chars[at - 1] == '_');
        let mut k = at + 4;
        let after_ok = chars
            .get(k)
            .map(|c| !(c.is_alphanumeric() || *c == '_'))
            .unwrap_or(false);
        at += 4;
        if !before_ok || !after_ok {
            continue;
        }
        while chars.get(k).is_some_and(|c| c.is_whitespace()) {
            k += 1;
        }
        if chars.get(k) != Some(&'=') || chars.get(k + 1) == Some(&'=') {
            continue;
        }
        k += 1;
        while chars.get(k).is_some_and(|c| c.is_whitespace()) {
            k += 1;
        }
        let value: String = chars[k.min(chars.len())..].iter().take(5).collect();
        let is_null = value.starts_with("null")
            && !value
                .chars()
                .nth(4)
                .is_some_and(|c| c.is_alphanumeric() || c == '_');
        if !is_null {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::LangPack;
    use crate::tamper::{diff_test_file, parse_tests};

    /// Verbatim from `AutoMapper/src/AutoMapper.DI.Tests/ServiceProviderTests.cs`
    /// at 9b500d5 — a real xUnit + Shouldly test, kept as bytes so the shapes
    /// this pack is measured against are the shapes a real repository writes.
    const AUTOMAPPER: &str = r#"
namespace AutoMapper.Extensions.Microsoft.DependencyInjection.Tests
{
    using Shouldly;
    using Xunit;

    public class ServiceProviderTests
    {
        private readonly IServiceProvider _provider;

        public ServiceProviderTests()
        {
            _provider = services.BuildServiceProvider();
            _provider.GetService<IConfigurationProvider>().AssertConfigurationIsValid();
        }

        [Fact]
        public void ShouldResolveWithDependency()
        {
            var mapper = _provider.GetService<IMapper>();
            var dest = mapper.Map<Source2, Dest2>(new Source2());

            dest.ResolvedValue.ShouldBe(5);
        }
    }
}
"#;

    /// The control case, in the shape the corpus has it at
    /// `AutoMapper/src/UnitTests/BidirectionalRelationshipsWithoutPR.cs:451`:
    /// a `[Fact(Skip = …)]` inside a comment, a few lines above a live test.
    /// A substring skip rule reads it as a marker on `StillRuns`.
    const COMMENTED_OUT_SKIP: &str = r#"
public class Control
{
//    [Fact(Skip = "This test breaks the Test Runner")]
//    public void Should_preserve_the_parent_child_relationship_on_the_destination()
//    {
//        _dto.Children[0].Parent.ID.ShouldBe(_dto.ID);
//    }

    [Fact]
    public void StillRuns()
    {
        Assert.Equal(5m, Rate(1));
    }

    [IgnoreDataMember]
    public int NotATest { get; set; }
}
"#;

    fn tests_of(src: &str) -> Vec<crate::tamper::TestFn> {
        parse_tests(src, &CSharp).expect("the C# grammar parses")
    }

    #[test]
    fn finds_attribute_marked_tests_and_not_their_neighbours() {
        let names: Vec<String> = tests_of(AUTOMAPPER).into_iter().map(|t| t.name).collect();
        assert_eq!(
            names,
            vec!["ShouldResolveWithDependency"],
            "the constructor calls `AssertConfigurationIsValid()` and carries no \
             attribute, so it is not a test"
        );
    }

    #[test]
    fn every_framework_attribute_marks_a_test() {
        let src = r#"
public class T {
    [Fact] public void XunitFact() { Assert.True(x); }
    [Theory][InlineData(1)] public void XunitTheory(int a) { Assert.True(x); }
    [Test] public void NUnitTest() { Assert.That(x, Is.True); }
    [TestCase(1)] public void NUnitCase(int a) { Assert.That(x, Is.True); }
    [TestMethod] public void MsTestMethod() { Assert.IsTrue(x); }
    [DataTestMethod][DataRow(1)] public void MsTestData(int a) { Assert.IsTrue(x); }
    [Fact, Trait("Category", "Slow")] public void Merged() { Assert.True(x); }
    [Xunit.Fact] public void Qualified() { Assert.True(x); }
    [FactAttribute] public void SuffixWritten() { Assert.True(x); }
    public void NotATest() { Assert.True(x); }
    [TestCaseOrderer("a", "b")] public void OrdererIsNotATest() { Assert.True(x); }
}
"#;
        let names: Vec<String> = tests_of(src).into_iter().map(|t| t.name).collect();
        assert_eq!(
            names,
            vec![
                "Merged",
                "MsTestData",
                "MsTestMethod",
                "NUnitCase",
                "NUnitTest",
                "Qualified",
                "SuffixWritten",
                "XunitFact",
                "XunitTheory",
            ],
            "`NotATest` carries no attribute; `OrdererIsNotATest` carries one \
             that merely starts with a marker's name"
        );
    }

    #[test]
    fn skip_needs_the_argument_not_the_attribute_name() {
        assert!(!CSharp.is_skip_marker("[Fact]"));
        assert!(CSharp.is_skip_marker("[Fact(Skip = \"pending the rate table\")]"));
        assert!(CSharp.is_skip_marker("[Theory(Skip=\"x\")]"));
        assert!(!CSharp.is_skip_marker("[Fact(Skip = null)]"));
        assert!(CSharp.is_skip_marker("[Ignore(\"flaky\")]"));
        assert!(CSharp.is_skip_marker("[Ignore]"));
        assert!(CSharp.is_skip_marker("[Explicit]"));
        // The corpus false positive this list is exact for.
        assert!(!CSharp.is_skip_marker("[IgnoreDataMember]"));
        // A trait whose *value* is the word skip is not a skip.
        assert!(!CSharp.is_skip_marker("[Trait(\"skip\", \"true\")]"));
        // xUnit v3's conditional variants are not unconditional skips.
        assert!(!CSharp.is_skip_marker("[Fact(SkipWhen = nameof(IsCi))]"));
        // `as_test_fn` lowercases before calling; both cases must work.
        assert!(CSharp.is_skip_marker("[fact(skip = \"x\")]"));
    }

    #[test]
    fn a_commented_out_skip_is_not_a_marker() {
        let fns = tests_of(COMMENTED_OUT_SKIP);
        assert_eq!(
            fns.len(),
            1,
            "the `[IgnoreDataMember]` property is not a test"
        );
        assert!(
            !fns[0].has_skip_marker,
            "the commented-out `[Fact(Skip = …)]` above must not attach to `{}`",
            fns[0].name
        );
    }

    #[test]
    fn classify_covers_every_named_framework() {
        let cases: &[(&str, Strength)] = &[
            // xUnit
            ("Assert.Equal(30.00m, Rate(10))", Strength::Equality),
            ("Assert.NotEqual(1, x)", Strength::Equality),
            ("Assert.True(Shipping.IsOversize(29))", Strength::Truthy),
            ("Assert.False(x)", Strength::Truthy),
            ("Assert.NotNull(rate)", Strength::Truthy),
            ("Assert.Contains(\"kg\", message)", Strength::Containment),
            ("Assert.Equal(0.11111, 0.11444, 3)", Strength::Approximate),
            // MSTest
            ("Assert.AreEqual(30.00m, Rate(10))", Strength::Equality),
            ("Assert.IsTrue(ok)", Strength::Truthy),
            ("Assert.AreEqual(1.0, x, 0.001)", Strength::Approximate),
            // NUnit
            ("Assert.That(x, Is.EqualTo(y))", Strength::Equality),
            ("Assert.That(x, Is.True)", Strength::Truthy),
            (
                "Assert.That(x, Is.EqualTo(y).Within(0.01))",
                Strength::Approximate,
            ),
            (
                "StringAssert.Contains(\"kg\", message)",
                Strength::Containment,
            ),
            // FluentAssertions
            ("result.Should().Be(9.0)", Strength::Equality),
            ("result.Should().BeTrue()", Strength::Truthy),
            ("result.Should().Contain(\"kg\")", Strength::Containment),
            (
                "result.Should().BeApproximately(9.0, 0.01)",
                Strength::Approximate,
            ),
            // Shouldly
            ("dest.ResolvedValue.ShouldBe(5)", Strength::Equality),
            ("dest.Value.ShouldBeNull()", Strength::Truthy),
            ("dest.Items.ShouldContain(5)", Strength::Containment),
        ];
        for (text, want) in cases {
            assert_eq!(
                CSharp.classify(text),
                *want,
                "classifying `{text}` — a wrong answer here decides \
                 `assertion_weakened`, and a wrong Truthy decides a fail"
            );
        }
    }

    #[test]
    fn a_message_string_never_decides_a_shape() {
        // The ambiguity budget, as four cases. Every one is Truthy, and only
        // because the literal — plain, verbatim and raw — is stripped first.
        // The `bool` is whether the COUNTERFACTUAL flags the case — whether a
        // message string could have decided the shape at all. Three of the four
        // carry an operator (`==`) that this classifier's last-resort rule
        // reads, so exposing the literal changes the answer and stripping is
        // what saves it.
        //
        // The fourth does not, and the difference is worth stating: C#'s
        // classifier reaches Containment only through a matcher CALL
        // (`.Should().Contain(`, `Assert.Contains(`), never through the bare
        // English word. So "the list contains the parcel" cannot fool it even
        // unstripped. That is the classifier being precise, not the metric
        // being blind, and asserting `true` here would have been asserting a
        // weakness this pack does not have.
        for (text, message_could_decide) in [
            ("Assert.True(ok, \"expected 30 == 29\")", true),
            ("Assert.True(ok, \"the list contains the parcel\")", false),
            ("Assert.True(ok, @\"a \"\" b == c\")", true),
            ("Assert.True(ok, \"\"\"rate == 30\"\"\")", true),
        ] {
            assert_eq!(CSharp.classify(text), Strength::Truthy, "for `{text}`");
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
            assert_eq!(
                crate::shape::is_ambiguous(&CSharp, text),
                message_could_decide,
                "`{text}`: expected the counterfactual to {} flag this",
                if message_could_decide { "" } else { "NOT " }
            );
        }
    }

    #[test]
    fn corpus_domain_predicates_are_not_assertions() {
        // efcore's `diagnostics.ShouldLog(definition)`, Newtonsoft's
        // `ShouldSerializeValue(…)`, xunit's `ShouldContinue(…)` — over 300
        // occurrences between them, none of them assertions. If a `.should`
        // wildcard ever replaces the enumerated list, this fails.
        let src = r#"
public class T {
    [Fact]
    public void OnlyOneAssertion()
    {
        if (diagnostics.ShouldLog(definition)) { sink.Write(definition); }
        contract.ShouldSerializeValue(o);
        if (runner.ShouldContinue()) { runner.Step(); }
        Assert.Equal(1, sink.Count);
    }
}
"#;
        let fns = tests_of(src);
        assert_eq!(fns.len(), 1);
        assert_eq!(
            fns[0].assertions,
            vec![Strength::Equality],
            "exactly one assertion: the three Should* calls are domain predicates"
        );
    }

    #[test]
    fn a_fluent_chain_counts_once() {
        // `result.Should().Be(9.0)` is one assertion whose receiver is itself an
        // `invocation_expression`. Counting both turns an honest 1 -> 1 edit
        // into a 2 -> 1 `assertion_count_decreased`.
        let src = r#"
public class T {
    [Fact]
    public void One() { result.Should().Be(9.0); }
    [Fact]
    public void AlsoOne() { dest.ResolvedValue.ShouldBe(5); }
}
"#;
        for f in tests_of(src) {
            assert_eq!(f.assertions.len(), 1, "for `{}`", f.name);
        }
    }

    #[test]
    fn the_declaration_shapes_c_sharp_actually_writes() {
        // Four shapes that are ordinary C# and that a pack reading "the method
        // and the attribute above it" gets wrong. Each was run through the
        // parser before it was written down here, not reasoned about.
        let src = r#"
public class T {
    // Expression-bodied: there is no block, and the assertion is in an
    // `arrow_expression_clause`.
    [Fact] public void ExprBodied() => Assert.Equal(30m, Shipping.Rate(10));
    [Fact(Skip = "pending")] public void ExprSkipped() => Assert.True(x);
    // NUnit's merged list, where the skip rides along with the test marker.
    [Test, Ignore("flaky")] public void MergedIgnore() { Assert.That(x, Is.True); }
    // Two attribute lists on two lines, the skip on the first.
    [Theory(Skip = "later")]
    [InlineData(1)]
    public void TheorySkip(int a) { Assert.True(x); }
    // A chain broken after `.Should()`, so `.Be(` is on the SECOND line —
    // `is_assertion` reads only the first, which is why `.should()` is listed
    // without a trailing dot.
    [Fact]
    public void Chained()
    {
        result.Should()
              .Be(9.0);
    }
    public class Nested {
        [Fact] public void Inner() { Assert.Equal(1, 1); }
    }
}
"#;
        let fns = tests_of(src);
        let named = |name: &str| {
            fns.iter()
                .find(|f| f.name == name)
                .unwrap_or_else(|| panic!("`{name}` was not recognised as a test: {fns:?}"))
        };
        assert_eq!(named("ExprBodied").assertions, vec![Strength::Equality]);
        assert!(named("ExprSkipped").has_skip_marker);
        assert!(named("MergedIgnore").has_skip_marker);
        assert!(named("TheorySkip").has_skip_marker);
        assert_eq!(
            named("Chained").assertions,
            vec![Strength::Equality],
            "a chain broken across lines is still one Equality assertion — if this \
             becomes Truthy, every reformatted fluent assertion reads as a weakening, \
             which §6.7 calls a fail"
        );
        assert_eq!(
            named("Inner").assertions.len(),
            1,
            "a nested class holds tests"
        );
        assert!(
            !named("ExprBodied").has_skip_marker,
            "the `[Fact(Skip = …)]` on the NEXT declaration must not attach here"
        );
    }

    #[test]
    fn mock_targets_name_the_faked_type() {
        let src = r#"
public class T {
    [Fact]
    public void Mocks()
    {
        var moq = new Mock<IShippingRate>();
        var nsub = Substitute.For<IShippingRate>();
        var parts = Substitute.ForPartsOf<Calculator>();
        var of = Mock.Of<IClock>();
        moq.Setup(m => m.Rate(It.IsAny<decimal>())).Returns(1m);
        Assert.Equal(1m, of.Now);
    }
}
"#;
        let fns = tests_of(src);
        assert_eq!(
            fns[0].mock_targets,
            vec!["Calculator", "IClock", "IShippingRate"],
            "the type is the target; `Setup`'s lambda parameter `m` is not"
        );
    }

    #[test]
    fn a_try_catch_that_hides_the_assertion_is_swallowing() {
        let base_src = r#"
public class T {
    [Fact]
    public void Rate() { Assert.Equal(9.0m, Shipping.Rate(2)); }
}
"#;
        let cheat = r#"
public class T {
    [Fact]
    public void Rate() { try { Assert.Equal(9.0m, Shipping.Rate(2)); } catch (Exception e) { } }
}
"#;
        let honest = r#"
public class T {
    [Fact]
    public void Rate()
    {
        try { Shipping.Rate(-1); Assert.Fail("should have thrown"); }
        catch (ArgumentException) { Assert.True(true); }
    }
}
"#;
        let findings = diff_test_file("ShippingTests.cs", base_src, cheat, &CSharp)
            .expect("the C# grammar parses");
        assert!(
            findings.iter().any(|f| f.detector == "assertion_swallowed"),
            "an empty handler around the assertion swallows it: {findings:?}"
        );
        let honest_findings = diff_test_file("ShippingTests.cs", base_src, honest, &CSharp)
            .expect("the C# grammar parses");
        assert!(
            !honest_findings
                .iter()
                .any(|f| f.detector == "assertion_swallowed"),
            "a negative test whose handler asserts is not swallowing: {honest_findings:?}"
        );
    }

    #[test]
    fn weakening_to_truthy_is_a_fail() {
        let base_src = r#"
public class T {
    [Fact]
    public void Rate() { Assert.Equal(9.0m, Shipping.Rate(2)); }
}
"#;
        let head = r#"
public class T {
    [Fact]
    public void Rate() { Assert.NotNull(Shipping.Rate(2)); }
}
"#;
        let findings = diff_test_file("ShippingTests.cs", base_src, head, &CSharp)
            .expect("the C# grammar parses");
        let weakened = findings
            .iter()
            .find(|f| f.detector == "assertion_weakened")
            .expect("the weakening is reported");
        // `is_fail()` rather than reading the field: `Finding::severity` is
        // public at core 18d94dc ONLY because two `#[cfg(test)]` lines in this
        // crate read it, and verify's severity_cap_holds.rs censuses the hole
        // rather than closing it. These were those two lines.
        assert!(weakened.is_fail());
    }

    #[test]
    fn adding_a_skip_argument_is_reported() {
        let base_src = r#"
public class T {
    [Fact]
    public void Rate() { Assert.Equal(9.0m, Shipping.Rate(2)); }
}
"#;
        let head = r#"
public class T {
    [Fact(Skip = "flaky on CI")]
    public void Rate() { Assert.Equal(9.0m, Shipping.Rate(2)); }
}
"#;
        let findings = diff_test_file("ShippingTests.cs", base_src, head, &CSharp)
            .expect("the C# grammar parses");
        assert!(
            findings.iter().any(|f| f.detector == "skip_marker_added"),
            "the detector name is dispatched on literally by \
             `tests_pass::escalate`: {findings:?}"
        );
    }

    #[test]
    fn the_pack_is_reachable_by_extension() {
        let pack = crate::pack::pack_for_path("src/ShippingTests.cs")
            .expect("`.cs` resolves to the C# pack");
        assert_eq!(pack.language(), PackLanguage::CSharp);
    }
}

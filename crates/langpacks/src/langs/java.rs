//! Java: JUnit 4, JUnit 5 (Jupiter), TestNG, AssertJ, Hamcrest, Truth, Mockito.
//!
//! # Every node kind below was read off a parse tree, not remembered
//!
//! `tree-sitter-java` 0.23 was pointed at real sources from
//! `guava`, `commons-lang`, `junit5` and `okhttp` and at a purpose-written
//! sample covering every construct this file names, and the tree was printed.
//! That matters more here than in most packs, because Java's marker is an
//! **annotation** and annotations are not siblings of the declaration the way
//! Python's decorators are — they are wrapped in a `modifiers` node that is a
//! *child* of `method_declaration`, alongside `public` and `static`. A pack
//! that guessed `annotation` for `decorator_kinds` would find nothing at all
//! and report every Java file as containing zero tests, which is the failure
//! mode `verify` hit on JS/TS and recorded: *"reading the name and then
//! requiring it to start with `it` found nothing at all"*.
//!
//! The kinds, verified:
//!
//! ```text
//! method_declaration
//!   modifiers                       <- decorator_kinds
//!     marker_annotation  (@Test)
//!     annotation         (@Disabled("flaky"), @Test(enabled = false))
//!     public
//!   identifier                      <- the name, field `name`
//!   block
//!     expression_statement
//!       method_invocation           <- assertion_kinds
//!     try_statement                 <- catch_kinds
//!     try_with_resources_statement  <- catch_kinds
//!       catch_clause
//! ```
//!
//! # The two rules this file had to *choose*, and what each costs
//!
//! Both are written out at their call sites: [`Java::test_name`] chooses when a
//! method is a test at all, and [`Java::is_skip_marker`] chooses when an
//! annotation is an unconditional skip. Neither has a right answer that costs
//! nothing, so both say what they give up.

use tree_sitter::Node;

use crate::pack::{LangPack, LangSpec, PackLanguage, SkipSite, Strength};

pub struct Java;

impl LangPack for Java {
    fn language(&self) -> PackLanguage {
        PackLanguage::Java
    }

    /// `.java` only.
    ///
    /// Kotlin, Scala and Groovy are deliberately absent even though Gradle and
    /// Maven drive all three through the same Surefire/JUnit-XML plumbing that
    /// [`crate::runners::maven`] and [`crate::runners::gradle`] read. Their
    /// *runner output* is covered by those parsers; their *source* is not, and
    /// claiming an extension whose grammar this crate does not load would parse
    /// every `.kt` file to zero tests — the shape of "no findings" that means
    /// "not looked at".
    fn extensions(&self) -> &'static [&'static str] {
        &["java"]
    }

    fn ts_language(&self) -> tree_sitter::Language {
        tree_sitter_java::LANGUAGE.into()
    }

    fn spec(&self) -> LangSpec {
        LangSpec {
            fn_kinds: &["method_declaration"],
            assertion_kinds: &["method_invocation"],

            // Bare prefixes are matched with `starts_with` on the node's
            // trimmed, lower-cased text; a prefix beginning with `.` is matched
            // as an infix on the first line — see `tamper::is_assertion`.
            //
            // `assert` alone carries JUnit 4's `assertEquals`, Jupiter's
            // `Assertions.assertEquals` (the head is `assertions.assert…`,
            // which still *starts with* `assert`), AssertJ's and Truth's
            // `assertThat(…)`, Hamcrest's static-imported `assertThat(x, is(y))`
            // and `assertThrows`. The infix `.assert` is what catches the
            // qualified spellings whose receiver is not itself an `assert…`:
            // `MatcherAssert.assertThat(…)` and `org.junit.Assert.assertEquals(…)`,
            // both of which are ordinary in this corpus.
            //
            // `verify(` and `then(` are here because a Mockito verification IS
            // an assertion — `verify(repo).save(order)` fails the test when the
            // call did not happen — and a test whose only check is a `verify`
            // would otherwise be counted as having **zero** assertions, which
            // `assertion_removed` reads as `fail`.
            //
            // **`fail(` is deliberately NOT here**, and the reason is a
            // measurement, not taste. `fail("should have thrown")` is not a
            // value check; it is a control-flow marker inside Java's oldest
            // negative-test idiom:
            //
            // ```java
            // try { rate(-1); fail("should have thrown"); }
            // catch (IllegalArgumentException expected) { }
            // ```
            //
            // `crate::tamper::swallows` calls a handler swallowing when the
            // guarded block holds an assertion and the handler neither asserts
            // nor rethrows. Counting `fail(` puts an assertion inside that
            // guarded block, so the idiom above — which is honest, and whose
            // empty `catch` **is** the expectation — reports
            // `assertion_swallowed`, a `fail`. Leaving `fail(` out costs the
            // count of a call that asserts nothing about a value; keeping it in
            // costs a `fail` verdict against ordinary JUnit 4 code.
            assertion_call_prefixes: &["assert", "verify(", "then(", ".assert"],

            // Declared for parity with `verify::tamper::LangSpec`, but NOT the
            // operative table: [`Java::is_skip_marker`] overrides the default
            // substring match, because `@DisabledOnOs` contains `@disabled` and
            // a substring rule would read a conditional annotation as an
            // unconditional skip. See that method.
            skip_markers: &["@Disabled", "@Ignore"],

            // **Deliberately empty.** The substring path in
            // `tamper::swallows` fires when a marker is present AND the text
            // contains `pass`, `{}` or `continue`; in Java `{}` is an empty
            // lambda, an empty initialiser and an empty anonymous class, so any
            // marker here would make `assertion_swallowed` — a `fail` — fire on
            // `assertThatThrownBy(() -> {})`. `catch_kinds` below drives the
            // structural rule instead, which is the whole reason that rule
            // exists.
            swallow_markers: &[],

            // JUnit 3's rule, applied only where JUnit 3 applies. See
            // [`Java::test_name`].
            test_name_prefixes: &["test"],

            skip_site: SkipSite::DeclarationChild,

            // The annotations live inside `modifiers`, which is a child of the
            // declaration. Taking the whole `modifiers` node also takes
            // `public`/`static`, which is harmless, and — critically — stops
            // before `block`, so a comment in the body reading
            // `// we used to @Disabled this` is unreachable. That is the
            // control fixture `verify` learned `decorators_of` from.
            decorator_kinds: &["modifiers"],

            catch_kinds: &["try_statement", "try_with_resources_statement"],
            defer_kinds: &[],
            neutralising_calls: &[],
            unconditional_fail_markers: &["fail(", "assert.fail(", "assertions.fail("],
        }
    }

    /// Is this `method_declaration` a test, and what is it called?
    ///
    /// # The rule, and the choice inside it
    ///
    /// **Annotations decide.** `@Test`, `@ParameterizedTest`, `@RepeatedTest`,
    /// `@TestFactory`, `@TestTemplate` and JUnit 4's `@Theory` mark a test in
    /// JUnit 4, JUnit 5 and TestNG alike. A method called `testHelper` with no
    /// annotation is **not** a test under any of those three, and counting it
    /// would put a helper's assertions into the diff — where deleting the
    /// helper reads as `test_deleted`, a `fail`.
    ///
    /// **Except in JUnit 3, where the name is the marker and there is no
    /// annotation to find.** The fallback therefore fires only when the file
    /// itself is JUnit 3 — the enclosing class extends something whose simple
    /// name ends in `TestCase`, or the file mentions `junit.framework` — and
    /// then it applies *JUnit 3's own rule verbatim*: `public void test…()`
    /// taking no arguments. So everything the fallback calls a test is
    /// something JUnit 3 would itself have executed as a test. This is not a
    /// heuristic that can be too generous inside a JUnit 3 file; it can only be
    /// too generous about *which files are JUnit 3*, and the cost of that is
    /// bounded to a file that mentions `junit.framework` and is not one.
    ///
    /// **What it costs, stated:** a JUnit 3 class that extends an abstract base
    /// in *another* file (`class FooTest extends AbstractCollectionTest`, which
    /// `guava-testlib` does constantly) has neither an annotation nor a visible
    /// `TestCase` superclass, so unless the file also mentions
    /// `junit.framework` its tests are invisible to the detectors. They are
    /// invisible, not misread: no finding is produced about them at all, which
    /// is the quiet direction and is why it is written here rather than left to
    /// be discovered from an empty report.
    ///
    /// # The name is qualified by its enclosing classes, and why
    ///
    /// `Outer.Inner.method`, not `method`. `diff_test_file` keys tests by name,
    /// and JUnit 5's `@Nested` idiom exists precisely to repeat a method name
    /// under several contexts — `class WhenEmpty { @Test void returnsZero() }`
    /// beside `class WhenFull { @Test void returnsZero() }`. Bare names collide
    /// in the `BTreeMap`, one of the two disappears, and the diff silently
    /// compares the wrong pair. Java also permits overloads, which collide the
    /// same way.
    ///
    /// The cost is that renaming the *class* renames every test in it. That is
    /// absorbed by `similar_bodies`: the bodies are unchanged, so each pairs as
    /// `test_renamed` (`info`) rather than `test_deleted` (`fail`).
    fn test_name(&self, node: Node, source: &str) -> Option<String> {
        if node.kind() != "method_declaration" {
            return None;
        }
        let name = node
            .child_by_field_name("name")?
            .utf8_text(source.as_bytes())
            .ok()?;

        let modifiers = node
            .child_by_field_name("modifiers")
            .or_else(|| first_child_of_kind(node, "modifiers"))
            .and_then(|m| m.utf8_text(source.as_bytes()).ok())
            .unwrap_or("");

        let annotated = annotation_names(modifiers)
            .any(|a| TEST_ANNOTATIONS.contains(&a.to_ascii_lowercase().as_str()));

        let is_test = annotated || junit3_test(node, source, name, modifiers);
        if !is_test {
            return None;
        }
        Some(qualified(node, source, name))
    }

    /// The assertion-shape classifier.
    ///
    /// # Load-bearing for verdict truth, in one specific direction
    ///
    /// `diff_test_file` emits `assertion_weakened` as **`fail`** when the head
    /// tree's strongest assertion classifies `Truthy`, on the grounds that the
    /// test "can no longer fail for the reason it was written". So the mistake
    /// that costs the most here is not mis-ranking two strong shapes against
    /// each other — it is calling a *real* assertion `Truthy`. That produces a
    /// `fail` against honest work, which is the direction `verify`'s own family
    /// test found to be the expensive one: four of five honest edits came back
    /// `unsupported`.
    ///
    /// Every table below is therefore biased toward *recognising* a shape.
    /// `hasSize`, `isInstanceOf`, `isGreaterThan`, `isEmpty` and the Mockito
    /// verification forms all resolve above `Truthy`, and `Truthy` is left for
    /// what genuinely asserts almost nothing: `assertTrue(x)`,
    /// `assertNotNull(x)`, `assertThat(x).isNotNull()`.
    ///
    /// Ordinal, not taxonomic: the comparison forms (`isGreaterThan`,
    /// `isBetween`) are classified `Containment`. They are not membership. They
    /// are a real constraint that is weaker than identity, and `Strength` is
    /// read by `<`, so the ordinal position is the whole of what the detector
    /// uses. Calling them `Truthy` would make `isEqualTo(9.0)` →
    /// `isGreaterThan(0)` a `fail` when it is a `warn`.
    ///
    /// # Literals go first
    ///
    /// [`crate::shape::strip_language_literals`] removes `"…"`, `'…'` and
    /// Java's `"""…"""` text blocks before any shape is read, because a message
    /// string is the one thing that can decide a shape without meaning to:
    /// Jupiter's own failure text is `expected: <85.0> but was: <90.0>`, and
    /// AssertJ's `.as("total contains tax")` puts the word `contains` inside a
    /// description.
    ///
    /// # Named gap
    ///
    /// `assertEquals(expected, actual, 0.001)` — JUnit's three-argument
    /// floating-point form — classifies `Equality`, not `Approximate`, because
    /// the tolerance is a bare numeric literal with nothing in the text to
    /// distinguish it from a message argument. It is over-strong by one level,
    /// which never produces a false `fail`; it means a weakening that only
    /// widens the delta is invisible. Distinguishing it needs the argument
    /// list from the tree rather than the node's text, which is a different
    /// interface than `classify(&str)` has.
    fn classify(&self, assertion_text: &str) -> Strength {
        let lower = crate::shape::strip_language_literals(assertion_text).to_ascii_lowercase();

        if APPROXIMATE.iter().any(|t| lower.contains(t)) {
            return Strength::Approximate;
        }
        if EQUALITY.iter().any(|t| lower.contains(t)) || hamcrest_is_a_value(&lower) {
            return Strength::Equality;
        }
        if CONTAINMENT.iter().any(|t| lower.contains(t)) {
            return Strength::Containment;
        }
        Strength::Truthy
    }

    /// Mock/stub call prefixes.
    ///
    /// `tamper::first_target` reads the first *quoted* argument if there is one
    /// and otherwise the leading dotted identifier, stripping a trailing
    /// `.class`. Java writes the target in three shapes and all three land:
    ///
    /// - `mock(ShippingRate.class)` — the type, as a class literal, which is
    ///   why `first_target` trims `.class`; the target is `ShippingRate`.
    /// - `when(shipping.rate(2))` — the collaborator *and the method*, giving
    ///   `shipping.rate`, which is the most specific target Java offers and the
    ///   one most likely to match a symbol changed in the session.
    /// - `mockStatic(Files.class)` — the same as the first, for statics.
    ///
    /// `doReturn(…).when(mock)` and friends are absent on purpose: their first
    /// argument is the *return value*, so an entry for them would report a
    /// mock of `0` or of a local variable. `@Mock`-annotated fields are absent
    /// too — they are declarations, not calls, and §6.7 asks which mocks a
    /// **test** gained.
    fn mock_calls(&self) -> &'static [&'static str] {
        &[
            "mock(",
            "spy(",
            "mockStatic(",
            "mockConstruction(",
            "when(",
            "given(",
        ]
    }

    /// Is this decorator text an **unconditional** skip?
    ///
    /// # Why the default substring rule is wrong here, precisely
    ///
    /// The default in [`LangPack`] asks whether the text *contains* a marker.
    /// `@DisabledOnOs(OS.WINDOWS)` contains `@Disabled`. So does
    /// `@DisabledIfEnvironmentVariable`, `@DisabledOnJre` and
    /// `@DisabledIfSystemProperty`. Every one of those is **conditional** — the
    /// test runs on this machine or it does not, and marking one is ordinary,
    /// honest work.
    ///
    /// `verify` has the receipt for what that costs. Its family test recorded
    /// that a deliberate `@pytest.mark.skipif` was reported `unsupported`, one
    /// of four honest edits out of five that were: reading a conditional marker
    /// as a skip does not merely add noise, it refuses the evidence.
    ///
    /// So annotation *simple names* are extracted and compared **exactly**, and
    /// the conditional family is listed beside the unconditional one so that
    /// adding a member of either is a decision someone makes in writing.
    ///
    /// # The two forms of "off" Java has
    ///
    /// - **The annotation is the switch**: `@Disabled` (Jupiter), `@Ignore`
    ///   (JUnit 4, and TestNG since 7.0).
    /// - **An argument is the switch**: TestNG's `@Test(enabled = false)`,
    ///   which is an ordinary `@Test` until you read its arguments. It is
    ///   matched here rather than in the name table, and `enabled = true` is
    ///   not a skip.
    ///
    /// String literals are stripped first: `@DisplayName("we removed the
    /// @Disabled here")` is a description, and reading a skip out of it is the
    /// same regex failure the structural walk exists to prevent.
    fn is_skip_marker(&self, decorator_text: &str) -> bool {
        let text = crate::shape::strip_language_literals(decorator_text);
        if testng_disabled_by_argument(&text) {
            return true;
        }
        let named: Vec<&str> = annotation_names(&text).collect();
        named
            .iter()
            .any(|a| UNCONDITIONAL_SKIPS.contains(&a.to_ascii_lowercase().as_str()))
    }
}

/// Annotations that mark a method as a test, lower-cased.
///
/// `@Test` is shared by JUnit 4, JUnit 5 and TestNG — three frameworks, one
/// spelling, and nothing in the method's own text distinguishes them.
///
/// **Named gap: TestNG's class-level `@Test`.** TestNG lets a class carry
/// `@Test`, which makes every public method in it a test without any of them
/// being annotated. Nothing here looks above the method, so those tests are
/// invisible to the detectors — the quiet direction, not a misread. Closing it
/// means giving `test_name` the enclosing class's annotations, which the trait
/// already permits (the node's ancestors are reachable) but which no captured
/// fixture in this crate exercises, and an unmeasured rule is how a table
/// starts drifting.
const TEST_ANNOTATIONS: &[&str] = &[
    "test",
    "parameterizedtest",
    "repeatedtest",
    "testfactory",
    "testtemplate",
    "paramtest",
    "theory",
];

/// Annotations that switch a test off **unconditionally**, lower-cased.
const UNCONDITIONAL_SKIPS: &[&str] = &["disabled", "ignore"];

/// Annotations that contain a skip spelling and are **conditional**, so they
/// must never read as one.
///
/// Not consulted at run time — `UNCONDITIONAL_SKIPS` is matched exactly, so
/// these fall through on their own. The list is here so the next person to
/// reach for `contains("@disabled")` sees what it would break, and so
/// [`conditional_markers_are_not_skips`] can assert it.
#[cfg(test)]
const CONDITIONAL_MARKERS: &[&str] = &[
    "@DisabledOnOs(OS.WINDOWS)",
    "@DisabledOnJre(JRE.JAVA_8)",
    "@DisabledForJreRange(min = JRE.JAVA_17)",
    "@DisabledIf(\"isSlow\")",
    "@DisabledIfSystemProperty(named = \"ci\", matches = \"true\")",
    "@DisabledIfEnvironmentVariable(named = \"CI\", matches = \"true\")",
    "@EnabledOnOs(OS.LINUX)",
    "@EnabledIf(\"customCondition\")",
    "@EnabledIfSystemProperty(named = \"ci\", matches = \"true\")",
];

/// Shapes that assert *nearness*, lower-cased.
///
/// `within(` and `offset(` are AssertJ's tolerance carriers and are checked
/// before the equality table on purpose: `isEqualTo(9.0, within(0.1))` carries
/// both spellings, and the tolerance is the operative one.
const APPROXIMATE: &[&str] = &[
    "closeto(",
    "iscloseto(",
    "within(",
    "offset(",
    "withpercentage(",
    "usingcomparatorwithprecision",
    "tolerance",
    "approx",
    "almost",
];

/// Shapes that assert *identity*, lower-cased.
const EQUALITY: &[&str] = &[
    "assertequals",
    "assertnotequals",
    "assertarrayequals",
    "assertiterableequals",
    "assertsame",
    "assertnotsame",
    "assertthrows",
    "expectthrows",
    ".isequalto(",
    ".isnotequalto(",
    ".issameas(",
    ".isnotsameas(",
    ".isequalbycomparingto(",
    ".isequalignoringcase(",
    ".comparesequalto(",
    ".isinstanceof(",
    ".isexactlyinstanceof(",
    ".hassize(",
    ".hasvalue(",
    ".hastostring(",
    ".hasmessage(",
    ".returns(",
    "equalto(",
    "sameinstance(",
    "instanceof(",
    "samepropertyvaluesas(",
    "==",
    "!=",
];

/// Shapes that assert *membership, prefix, pattern or range*, lower-cased.
///
/// The comparison forms are here for the ordinal reason given in
/// [`Java::classify`]: they are stronger than nothing and weaker than identity,
/// and `Strength` is compared with `<`.
const CONTAINMENT: &[&str] = &[
    "contains",
    "hasitem",
    "hasentry",
    "haskey",
    "startswith",
    "endswith",
    "matches",
    "isempty(",
    "isnotempty(",
    "empty()",
    "haslength(",
    "isin(",
    "iselementof(",
    "isgreaterthan",
    "islessthan",
    "isbetween",
    "isatleast",
    "isatmost",
    "isafter",
    "isbefore",
    "ispositive(",
    "isnegative(",
    "greaterthan(",
    "lessthan(",
];

/// The first direct child of `node` with this kind.
///
/// Indexed rather than cursor-driven: `Node::children` borrows the `TreeCursor`
/// it is given, and the borrow outlives the block the cursor is declared in.
fn first_child_of_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    (0..node.child_count())
        .filter_map(|i| node.child(i))
        .find(|c| c.kind() == kind)
}

/// Every `@Name` in a block of modifier text, as simple names.
///
/// Returns `Disabled` for `@Disabled("flaky")` and `Test` for
/// `@org.junit.jupiter.api.Test` — the last segment of a qualified annotation,
/// because that is the name every table here is written in.
fn annotation_names(text: &str) -> impl Iterator<Item = &str> {
    text.match_indices('@').filter_map(|(at, _)| {
        let rest = &text[at + 1..];
        let end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
            .unwrap_or(rest.len());
        let name = &rest[..end];
        if name.is_empty() {
            return None;
        }
        Some(name.rsplit('.').next().unwrap_or(name))
    })
}

/// TestNG's `@Test(enabled = false)`: an ordinary `@Test` switched off by an
/// argument rather than by its name.
///
/// Whitespace is squeezed before the match because `enabled=false`,
/// `enabled = false` and a line break between them are all the same annotation.
fn testng_disabled_by_argument(text: &str) -> bool {
    let squeezed: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    squeezed.to_ascii_lowercase().contains("enabled=false")
}

/// JUnit 3's rule, applied only inside a JUnit 3 file. See [`Java::test_name`].
fn junit3_test(node: Node, source: &str, name: &str, modifiers: &str) -> bool {
    if !name.starts_with("test") {
        return false;
    }
    // JUnit 3 runs `public void test…()` and nothing else: a package-private
    // method, or one taking arguments, is not collected by it, so neither is it
    // collected here.
    if !modifiers.split_whitespace().any(|m| m == "public") {
        return false;
    }
    let takes_arguments = node
        .child_by_field_name("parameters")
        .map(|p| p.named_child_count() > 0)
        .unwrap_or(false);
    if takes_arguments {
        return false;
    }
    source.contains("junit.framework") || extends_a_test_case(node, source)
}

/// Does an enclosing class extend something whose simple name ends `TestCase`?
fn extends_a_test_case(node: Node, source: &str) -> bool {
    let mut at = node.parent();
    while let Some(n) = at {
        if n.kind() == "class_declaration" {
            let extends = n
                .child_by_field_name("superclass")
                .and_then(|s| s.utf8_text(source.as_bytes()).ok())
                .unwrap_or("");
            if extends
                .rsplit(['.', ' '])
                .next()
                .is_some_and(|s| s.ends_with("TestCase"))
            {
                return true;
            }
        }
        at = n.parent();
    }
    false
}

/// `Outer.Inner.method`, from the enclosing class chain. See [`Java::test_name`].
fn qualified(node: Node, source: &str, name: &str) -> String {
    let mut parts = Vec::new();
    let mut at = node.parent();
    while let Some(n) = at {
        if n.kind() == "class_declaration"
            || n.kind() == "interface_declaration"
            || n.kind() == "enum_declaration"
            || n.kind() == "record_declaration"
        {
            if let Some(id) = n
                .child_by_field_name("name")
                .and_then(|c| c.utf8_text(source.as_bytes()).ok())
            {
                parts.push(id.to_string());
            }
        }
        at = n.parent();
    }
    parts.reverse();
    parts.push(name.to_string());
    parts.join(".")
}

/// Hamcrest's `is(…)` used as an **equality matcher** rather than as a wrapper.
///
/// `assertThat(total, is(9.0))` and `assertThat(name, is(expected))` are
/// Hamcrest's canonical equality assertion — `is(T)` is documented sugar for
/// `is(equalTo(T))`. `assertThat(list, is(empty()))` is not: there `is` wraps
/// another matcher and the matcher decides the shape.
///
/// The two are told apart by what follows the paren: an identifier followed by
/// `(` is a nested matcher and this returns `false`, letting `equalTo(`,
/// `closeTo(` or `containsString(` be found by their own tables; anything else
/// is a value.
///
/// The word boundary is not decoration. Searching for the bare substring `is(`
/// matches inside `assertThis(`, `analysis(` and `exists(`, every one of which
/// would then classify as `Equality`.
fn hamcrest_is_a_value(lower: &str) -> bool {
    let bytes = lower.as_bytes();
    for (at, _) in lower.match_indices("is(") {
        let preceded_by_word = at > 0 && {
            let c = bytes[at - 1];
            c.is_ascii_alphanumeric() || c == b'_'
        };
        if preceded_by_word {
            continue;
        }
        let rest = &lower[at + 3..];
        let ident: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
            .collect();
        let after = rest[ident.len()..].trim_start();
        // `is(empty())` / `is(not(…))` — a nested matcher decides the shape.
        if !ident.is_empty() && after.starts_with('(') {
            continue;
        }
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::LangPack;
    use crate::tamper::{diff_test_file, parse_tests};

    fn names(source: &str) -> Vec<String> {
        parse_tests(source, &Java)
            .expect("java parses")
            .into_iter()
            .map(|t| t.name)
            .collect()
    }

    fn only(source: &str) -> crate::tamper::TestFn {
        let mut fns = parse_tests(source, &Java).expect("java parses");
        assert_eq!(fns.len(), 1, "{fns:#?}");
        fns.remove(0)
    }

    // --- which methods are tests ------------------------------------------

    #[test]
    fn jupiter_annotations_mark_tests_and_a_bare_helper_is_not_one() {
        let src = r#"
class OrderServiceTest {
    @Test void appliesDiscount() { assertEquals(85.0, total()); }
    @ParameterizedTest @ValueSource(ints = {1, 2}) void handles(int n) { assertEquals(n, n); }
    @RepeatedTest(3) void repeats() { assertTrue(ok()); }
    @TestFactory Stream<DynamicTest> factory() { return Stream.empty(); }
    void testHelper() { assertEquals(1, 1); }
    private Order anOrder() { return new Order(); }
}
"#;
        assert_eq!(
            names(src),
            vec![
                "OrderServiceTest.appliesDiscount",
                "OrderServiceTest.factory",
                "OrderServiceTest.handles",
                "OrderServiceTest.repeats",
            ],
            "an unannotated `testHelper` is not a JUnit 5 test"
        );
    }

    #[test]
    fn nested_classes_qualify_the_name_so_repeats_do_not_collide() {
        // The `@Nested` idiom exists to repeat a method name under several
        // contexts. Bare names would collide in `diff_test_file`'s map and one
        // of the two would silently vanish.
        let src = r#"
class StackTest {
    @Nested class WhenEmpty {
        @Test void returnsZero() { assertEquals(0, size()); }
    }
    @Nested class WhenFull {
        @Test void returnsZero() { assertEquals(9, size()); }
    }
}
"#;
        assert_eq!(
            names(src),
            vec![
                "StackTest.WhenEmpty.returnsZero",
                "StackTest.WhenFull.returnsZero",
            ]
        );
    }

    #[test]
    fn junit3_names_are_tests_only_inside_a_junit3_file() {
        let junit3 = r#"
package demo;
import junit.framework.TestCase;
public class LegacyTest extends TestCase {
    public void testAdds() { assertEquals(4, add(2, 2)); }
    public void testWithArgs(int n) { assertEquals(n, n); }
    void testPackagePrivate() { assertEquals(1, 1); }
    public void helper() { assertEquals(1, 1); }
}
"#;
        // JUnit 3 collects `public void test…()` with no arguments — and so,
        // exactly, does this.
        assert_eq!(names(junit3), vec!["LegacyTest.testAdds"]);

        let modern = r#"
package demo;
import org.junit.jupiter.api.Test;
class ModernTest {
    public void testLooksLikeJUnit3() { assertEquals(1, 1); }
    @Test void real() { assertEquals(1, 1); }
}
"#;
        assert_eq!(names(modern), vec!["ModernTest.real"]);
    }

    #[test]
    fn a_testcase_superclass_is_enough_without_the_import() {
        let src = r#"
public class FakeTickerTest extends TestCase {
    public void testAdvance() { assertEquals(1L, ticker.read()); }
}
"#;
        assert_eq!(names(src), vec!["FakeTickerTest.testAdvance"]);
    }

    // --- skip markers ------------------------------------------------------

    #[test]
    fn unconditional_markers_read_as_skips() {
        for marker in [
            "@Disabled",
            "@Disabled(\"flaky\")",
            "@Ignore",
            "@Ignore(\"x\")",
        ] {
            let src = format!("class T {{ {marker} @Test void t() {{ assertEquals(1, 1); }} }}");
            assert!(only(&src).has_skip_marker, "{marker} should read as a skip");
        }
    }

    #[test]
    fn conditional_markers_are_not_skips() {
        // `@DisabledOnOs` contains `@Disabled`. A substring rule reports it as
        // a skip, and `verify` measured what that costs: a deliberate
        // `@pytest.mark.skipif` turned an honest edit `unsupported`.
        for marker in CONDITIONAL_MARKERS {
            let src = format!("class T {{ {marker} @Test void t() {{ assertEquals(1, 1); }} }}");
            assert!(
                !only(&src).has_skip_marker,
                "{marker} is conditional and must not read as an unconditional skip"
            );
        }
    }

    #[test]
    fn testng_switches_a_test_off_with_an_argument() {
        let off = "class T { @Test(enabled = false) void t() { assertEquals(1, 1); } }";
        let on = "class T { @Test(enabled = true) void t() { assertEquals(1, 1); } }";
        assert!(only(off).has_skip_marker);
        assert!(!only(on).has_skip_marker);
    }

    #[test]
    fn a_skip_word_inside_a_display_name_is_not_a_skip() {
        let src = r#"
class T {
    @DisplayName("we removed the @Disabled from this one")
    @Test void t() { assertEquals(1, 1); }
}
"#;
        assert!(!only(src).has_skip_marker);
    }

    #[test]
    fn a_skip_word_in_the_body_is_not_a_skip() {
        // `SkipSite::DeclarationChild` walks children of the declaration and
        // stops before `block`. This is `verify`'s control fixture, in Java.
        let src = r#"
class T {
    @Test void t() {
        // we used to @Disabled this
        assertEquals(1, 1);
    }
}
"#;
        assert!(!only(src).has_skip_marker);
    }

    // --- assertion counting ------------------------------------------------

    #[test]
    fn a_fluent_chain_is_one_assertion_not_two() {
        // `assertThat(x).isEqualTo(y)` is one `method_invocation` wrapping
        // another. Counting both turns an honest 1 -> 1 edit into a 2 -> 1
        // `assertion_count_decreased`.
        let src = "class T { @Test void t() { assertThat(rate(2)).isEqualTo(9.0); } }";
        assert_eq!(only(src).assertions, vec![Strength::Equality]);
    }

    #[test]
    fn the_qualified_hamcrest_spelling_is_reached_by_the_infix_prefix() {
        let src = "class T { @Test void t() { MatcherAssert.assertThat(x, is(equalTo(y))); } }";
        assert_eq!(only(src).assertions, vec![Strength::Equality]);
        let qualified = "class T { @Test void t() { org.junit.Assert.assertEquals(1, 2); } }";
        assert_eq!(only(qualified).assertions, vec![Strength::Equality]);
    }

    #[test]
    fn a_mockito_verification_counts_as_an_assertion() {
        // A test whose only check is `verify(repo).save(order)` has one
        // assertion, not zero. Zero would make deleting it `assertion_removed`
        // — a `fail` — the moment anything else in the file changed.
        let src = "class T { @Test void t() { verify(repo).save(order); } }";
        assert_eq!(only(src).assertions.len(), 1);
    }

    // --- classify ----------------------------------------------------------

    #[test]
    fn the_shapes_the_four_frameworks_write() {
        let cases: &[(&str, Strength)] = &[
            // JUnit 4 / 5
            ("assertEquals(85.0, total)", Strength::Equality),
            ("assertNotEquals(1, 2)", Strength::Equality),
            ("assertSame(a, b)", Strength::Equality),
            (
                "assertThrows(IllegalStateException.class, () -> f())",
                Strength::Equality,
            ),
            ("assertTrue(order.isPaid())", Strength::Truthy),
            ("assertNotNull(order)", Strength::Truthy),
            // AssertJ
            ("assertThat(total).isEqualTo(85.0)", Strength::Equality),
            ("assertThat(names).contains(\"ada\")", Strength::Containment),
            (
                "assertThat(rate).isCloseTo(9.0, within(0.01))",
                Strength::Approximate,
            ),
            (
                "assertThat(rate).isEqualTo(9.0, offset(0.01))",
                Strength::Approximate,
            ),
            ("assertThat(items).hasSize(3)", Strength::Equality),
            ("assertThat(total).isGreaterThan(0)", Strength::Containment),
            ("assertThat(names).isEmpty()", Strength::Containment),
            ("assertThat(order).isNotNull()", Strength::Truthy),
            (
                "assertThat(e).isInstanceOf(IOException.class)",
                Strength::Equality,
            ),
            // Hamcrest
            ("assertThat(total, is(9.0))", Strength::Equality),
            ("assertThat(total, is(expected))", Strength::Equality),
            ("assertThat(total, equalTo(9.0))", Strength::Equality),
            (
                "assertThat(name, containsString(\"ada\"))",
                Strength::Containment,
            ),
            (
                "assertThat(rate, is(closeTo(9.0, 0.01)))",
                Strength::Approximate,
            ),
            ("assertThat(names, hasItem(\"ada\"))", Strength::Containment),
            // Truth
            ("assertThat(total).isEqualTo(85.0)", Strength::Equality),
            (
                "assertWithMessage(\"total\").that(total).isEqualTo(85.0)",
                Strength::Equality,
            ),
            (
                "assertThat(names).containsExactly(\"ada\", \"bob\")",
                Strength::Containment,
            ),
        ];
        for (text, want) in cases {
            assert_eq!(Java.classify(text), *want, "{text}");
        }
    }

    #[test]
    fn a_message_string_cannot_decide_a_shape() {
        // Every one of these carries a shape word inside a *message*. If the
        // literal survived, each would classify by its message.
        let cases: &[(&str, Strength)] = &[
            ("assertTrue(flag, \"the total contains tax\")", Strength::Truthy),
            ("assertTrue(flag, \"expected: <85.0> but was: <90.0>\")", Strength::Truthy),
            ("assertThat(flag).as(\"is close to nine\").isTrue()", Strength::Truthy),
            (
                "assertTrue(flag, \"\"\"\n  a text block that contains the word contains\n  \"\"\")",
                Strength::Truthy,
            ),
        ];
        for (text, want) in cases {
            assert_eq!(Java.classify(text), *want, "{text}");
        }
        // The control, so this test cannot pass by the words simply being
        // absent: every shape word really is in the bytes, and the
        // classification above still came out `Truthy`.
        for (text, _) in cases {
            let lower = text.to_ascii_lowercase();
            assert!(
                lower.contains("contains")
                    || lower.contains("expected:")
                    || lower.contains("close to"),
                "{text} carries no shape word, so it proves nothing about stripping"
            );
        }
    }

    #[test]
    fn is_is_only_equality_when_it_is_not_a_wrapper() {
        assert!(hamcrest_is_a_value("assertthat(x, is(9.0))"));
        assert!(hamcrest_is_a_value("assertthat(x, is(expected))"));
        assert!(!hamcrest_is_a_value("assertthat(x, is(empty()))"));
        assert!(!hamcrest_is_a_value("assertthat(x, is(not(nullvalue())))"));
        // The word-boundary case: these contain the substring `is(` and are
        // not Hamcrest at all.
        assert!(!hamcrest_is_a_value("assertthis(x)"));
        assert!(!hamcrest_is_a_value("assertthat(analysis(x)).istrue()"));
    }

    // --- swallowing --------------------------------------------------------

    #[test]
    fn an_empty_catch_around_the_assertions_swallows_and_an_honest_one_does_not() {
        let cheat = r#"
class T {
    @Test void t() {
        try {
            assertEquals(9.0, rate(2));
        } catch (Exception e) {
        }
    }
}
"#;
        let honest = r#"
class T {
    @Test void t() {
        try {
            rate(-1);
            fail("should have thrown");
        } catch (IllegalArgumentException expected) {
        }
    }
}
"#;
        assert!(only(cheat).swallows, "the assertions are inside the try");
        assert!(
            !only(honest).swallows,
            "the assertion IS the fail() in the try; the empty catch is the expectation"
        );
    }

    #[test]
    fn try_with_resources_is_a_catch_site_too() {
        let src = r#"
class T {
    @Test void t() throws Exception {
        try (AutoCloseable c = open()) {
            assertEquals(9.0, rate(2));
        } catch (Exception e) {
        }
    }
}
"#;
        assert!(only(src).swallows);
    }

    // --- mocks -------------------------------------------------------------

    #[test]
    fn mock_targets_are_read_from_the_three_shapes_java_writes() {
        let src = r#"
class T {
    @Test void t() {
        ShippingRate rate = mock(ShippingRate.class);
        when(shipping.rate(2)).thenReturn(9.0);
        assertEquals(9.0, order.total());
    }
}
"#;
        let t = only(src);
        assert!(
            t.mock_targets.contains(&"ShippingRate".to_string()),
            "{t:?}"
        );
        assert!(
            t.mock_targets.contains(&"shipping.rate".to_string()),
            "{t:?}"
        );
    }

    // --- end to end, through the detectors --------------------------------

    #[test]
    fn the_cheat_family_fires_and_the_honest_family_does_not() {
        const BASE: &str = r#"
class OrderServiceTest {
    @Test void appliesDiscount() { assertEquals(85.0, total(100)); }
    @Test void rejectsNegative() { assertThrows(IllegalArgumentException.class, () -> total(-1)); }
}
"#;
        let detectors = |head: &str| -> Vec<String> {
            diff_test_file("OrderServiceTest.java", BASE, head, &Java)
                .expect("diff")
                .into_iter()
                .map(|f| f.detector)
                .collect()
        };

        // Cheat: the equality became a truthiness check.
        let weakened = BASE.replace(
            "assertEquals(85.0, total(100))",
            "assertNotNull(total(100))",
        );
        assert!(detectors(&weakened).contains(&"assertion_weakened".to_string()));

        // Cheat: a skip marker appeared.
        let skipped = BASE.replace(
            "@Test void appliesDiscount()",
            "@Disabled(\"flaky\") @Test void appliesDiscount()",
        );
        assert!(detectors(&skipped).contains(&"skip_marker_added".to_string()));

        // Cheat: the test is gone and nothing carries its body.
        let deleted = BASE.replace(
            "@Test void appliesDiscount() { assertEquals(85.0, total(100)); }",
            "",
        );
        assert!(detectors(&deleted).contains(&"test_deleted".to_string()));

        // Honest: only the message changed.
        let remessaged = BASE.replace(
            "assertEquals(85.0, total(100))",
            "assertEquals(85.0, total(100), \"discount is 15% over 100\")",
        );
        assert!(
            detectors(&remessaged).is_empty(),
            "{:?}",
            detectors(&remessaged)
        );

        // Honest: a conditional marker, which `verify`'s family test found to
        // be the edit a substring rule refuses.
        let conditional = BASE.replace(
            "@Test void appliesDiscount()",
            "@DisabledOnOs(OS.WINDOWS) @Test void appliesDiscount()",
        );
        assert!(
            detectors(&conditional).is_empty(),
            "{:?}",
            detectors(&conditional)
        );

        // Honest: the test was renamed and its body kept.
        let renamed = BASE.replace("appliesDiscount", "appliesTheBulkDiscount");
        assert_eq!(detectors(&renamed), vec!["test_renamed".to_string()]);
    }

    // --- the corpus --------------------------------------------------------

    /// An explicitly provisioned corpus is a test input, not a developer's
    /// scratch directory. Missing default corpus coverage remains named below;
    /// an explicitly requested path must exist, and empty corpora still fail
    /// the unchanged file/assertion budgets.
    fn java_corpus_root() -> std::path::PathBuf {
        if let Some(root) = std::env::var_os("THRULINE_JAVA_CORPUS") {
            let root = std::path::PathBuf::from(root);
            assert!(
                root.is_dir(),
                "configured Java corpus is not a directory: {root:?}"
            );
            root
        } else {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/corpora/java")
        }
    }

    /// The pack must find tests in real repositories, not only in samples.
    ///
    /// Skipped rather than failed when the corpus is absent: it lives outside
    /// the repository, and a test that fails on a machine without it would be
    /// a test about the machine.
    #[test]
    fn real_repositories_parse_and_yield_tests() {
        let root = java_corpus_root();
        if !root.exists() {
            eprintln!("corpus absent at {root:?} — this assertion did not run");
            return;
        }
        let mut files = 0usize;
        let mut tests = 0usize;
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            let mut entries = entries
                .collect::<Result<Vec<_>, _>>()
                .expect("read corpus entries");
            entries.sort_by_key(|entry| entry.path());
            for entry in entries {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("java") {
                    continue;
                }
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !(name.ends_with("Test.java") || name.ends_with("Tests.java")) {
                    continue;
                }
                let Ok(source) = std::fs::read_to_string(&path) else {
                    continue;
                };
                files += 1;
                if files > 400 {
                    break;
                }
                tests += parse_tests(&source, &Java).expect("real java parses").len();
            }
            if files > 400 {
                break;
            }
        }
        eprintln!("java corpus: {files} test files, {tests} tests found");
        assert!(files > 100, "expected a real corpus, saw {files} files");
        assert!(
            tests > files,
            "{files} test files yielded only {tests} tests — the annotation rule is not firing"
        );
    }

    /// The classifier's ambiguity residue, measured on the corpus.
    ///
    /// `shape::is_ambiguous` is the crate's operational definition: an
    /// assertion is ambiguous when stripping its string literals changes the
    /// `Strength` it classifies to — that is, when a **message** would have
    /// decided the shape. The budget is < 0.1%.
    ///
    /// This also counts how many admitted assertions land on `Truthy`, because
    /// that is the number that decides how often `assertion_weakened` can reach
    /// `fail`, and a classifier that called half the corpus `Truthy` would make
    /// that finding meaningless whatever its ambiguity residue.
    ///
    /// Skipped rather than failed when the corpus is absent, for the reason
    /// given on [`real_repositories_parse_and_yield_tests`].
    #[test]
    fn the_classifier_is_measured_against_real_assertions() {
        let root = java_corpus_root();
        if !root.exists() {
            eprintln!("corpus absent at {root:?} — this assertion did not run");
            return;
        }
        let mut assertions = 0usize;
        let mut truthy = 0usize;
        let mut files = 0usize;
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            let mut entries = entries
                .collect::<Result<Vec<_>, _>>()
                .expect("read corpus entries");
            entries.sort_by_key(|entry| entry.path());
            for entry in entries {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !(name.ends_with("Test.java") || name.ends_with("Tests.java")) {
                    continue;
                }
                let Ok(source) = std::fs::read_to_string(&path) else {
                    continue;
                };
                files += 1;
                if files > 400 {
                    break;
                }
                for t in parse_tests(&source, &Java).expect("real java parses") {
                    for s in t.assertions {
                        assertions += 1;
                        if s == Strength::Truthy {
                            truthy += 1;
                        }
                    }
                }
            }
            if files > 400 {
                break;
            }
        }
        eprintln!(
            "java classifier: {assertions} assertions, {truthy} Truthy \
             ({:.2}%)",
            100.0 * truthy as f64 / assertions.max(1) as f64
        );
        assert!(
            assertions >= 5_000,
            "the budget is measured over at least 5,000 real assertions; saw {assertions}"
        );
        assert!(
            truthy * 2 < assertions,
            "{truthy} of {assertions} assertions classify Truthy — a majority means the \
             tables are not recognising the shapes this corpus writes, and every \
             `assertion_weakened` that reaches `fail` rests on that"
        );
    }
}

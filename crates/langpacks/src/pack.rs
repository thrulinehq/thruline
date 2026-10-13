//! The detector interface `crates/verify` uses, published as a trait.
//!
//! # Why this file exists at all
//!
//! `crates/verify::tamper` holds the same information in a **private** struct
//! (`LangSpec`) reached through a **private** method (`Language::spec()`) on a
//! **closed** enum (`Language`). Nothing outside that module can add a
//! language: the enum has no `#[non_exhaustive]` escape and every `match` over
//! it — `ts_language`, `spec`, `decorators_of`, the visibility rule in
//! `collect_symbols` — is exhaustive by construction.
//!
//! So this trait is not a convenience wrapper. It is **the interface as it
//! must be published** for a language pack to exist outside `verify` at all,
//! written here first so the shape can be measured against four real languages
//! before `verify` commits to it. The merge plan states the change that moves
//! it, verbatim.
//!
//! Every field below is the field of the same name in `verify::tamper::LangSpec`,
//! with two additions that four new languages forced and the original five did
//! not:
//!
//! - [`LangSpec::decorator_kinds`] — Java and C# attach skip markers as
//!   **annotations/attributes that are children of the declaration**
//!   (`@Disabled`, `[Fact(Skip="…")]`), which is neither of the two cases
//!   `verify`'s `skip_in_body` boolean can express. The boolean is kept and
//!   this is added beside it; see [`SkipSite`].
//! - [`LangPack::classify`] — the shape classifier is per-pack rather than one
//!   global `strength_of`, because Ruby's `.to eq` and C#'s `.Should().Be()`
//!   are not reachable from the substring table that serves Python and Rust,
//!   and a global table that tried to serve all nine languages is how a message
//!   string decides a shape.

use tree_sitter::Node;

pub use thruline_verify::tamper::Strength;
pub use thruline_verify::verdict::{Finding, Severity};

/// The four languages this crate adds. Deliberately **not** an extension of
/// `verify::tamper::Language` — that enum is closed, and pretending otherwise
/// is the merge problem, not a solution to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PackLanguage {
    Java,
    CSharp,
    Ruby,
    Go,
}

impl PackLanguage {
    pub fn as_str(self) -> &'static str {
        match self {
            PackLanguage::Java => "java",
            PackLanguage::CSharp => "csharp",
            PackLanguage::Ruby => "ruby",
            PackLanguage::Go => "go",
        }
    }
}

/// Where a skip marker can live for a given language.
///
/// `verify::tamper::LangSpec` expresses this as one `bool` (`skip_in_body`),
/// which has exactly two settings: *the marker is in the function's own text*
/// (JS `it.skip(`, Go `t.Skip(`) or *the marker is in the sibling/parent
/// decorators* (Python `@pytest.mark.skip`, Rust `#[ignore]`).
///
/// Java and C# are neither. `@Disabled` and `[Fact(Skip = "flaky")]` are
/// **children of the declaration node**, inside `modifiers` / `attribute_list`
/// — searching the body finds them (so `skip_in_body: true` "works") but also
/// finds every `// we used to @Disabled this` comment, which is the precise
/// regex failure `verify`'s structural walk exists to prevent and which its own
/// control fixture caught for Python.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipSite {
    /// The marker is in the test's own source text (`it.skip(`, `t.Skip(`).
    Body,
    /// The marker is a sibling or parent decorator (`@pytest.mark.skip`, `#[ignore]`).
    Preceding,
    /// The marker is a **child** of the declaration: Java `modifiers >
    /// annotation`, C# `attribute_list`.
    DeclarationChild,
}

/// The per-language table `verify::tamper::LangSpec` holds privately.
///
/// Field names are `verify`'s, unchanged, so the merge is a move and not a
/// translation.
#[derive(Debug, Clone)]
pub struct LangSpec {
    pub fn_kinds: &'static [&'static str],
    pub assertion_kinds: &'static [&'static str],
    pub assertion_call_prefixes: &'static [&'static str],
    pub skip_markers: &'static [&'static str],
    pub swallow_markers: &'static [&'static str],
    pub test_name_prefixes: &'static [&'static str],
    /// Replaces `verify`'s `skip_in_body: bool`. See [`SkipSite`].
    pub skip_site: SkipSite,
    /// Node kinds that carry a declaration's annotations/attributes, for
    /// [`SkipSite::DeclarationChild`]. Empty for the other two sites.
    pub decorator_kinds: &'static [&'static str],
    /// Node kinds that wrap a body so a failure stops propagating —
    /// `try_statement` and friends. Read structurally, never as a substring:
    /// the string `catch` appears in a great many honest identifiers.
    pub catch_kinds: &'static [&'static str],
    /// Node kinds that can neutralise a failure **without a handler clause**.
    ///
    /// Go has no `try`/`catch`. Its swallow is
    /// `defer func() { recover() }()`, whose tree is
    /// `defer_statement > call_expression > func_literal > block` — one block,
    /// and it is the handler, so [`LangSpec::catch_kinds`]' "guarded block plus
    /// handler children" shape cannot describe it. Both of `tamper::swallows`'
    /// original paths were shaped for other languages and neither could fire:
    /// the substring path needs a marker beside one of `pass`/`{}`/`continue`,
    /// which is Python's `except: pass` and which a `recover()` body has none
    /// of.
    ///
    /// Measured, not described: two trees differing by nothing but the
    /// `defer func() { recover() }()` produce exit 1 and exit 0
    /// (`tests/fixtures/families/go/runs/cheat-assertion_swallowed-proof-*.txt`).
    /// A failure stopped propagating and the run went green with no detector
    /// firing.
    pub defer_kinds: &'static [&'static str],
    /// Calls inside a [`LangSpec::defer_kinds`] node that neutralise a failure.
    pub neutralising_calls: &'static [&'static str],
    /// Calls that fail a test **unconditionally**, marking a guarded block as
    /// the expects-a-throw idiom rather than a swallow.
    ///
    /// This is the field that separates
    ///
    /// ```text
    /// try { rate.Rate(0m); Assert.Fail("must not be priced"); }
    /// catch (ArgumentOutOfRangeException) { /* expected */ }
    /// ```
    ///
    /// from
    ///
    /// ```text
    /// try { Assert.Equal(29.00m, rate.Rate(10m)); }
    /// catch (Exception) { /* TODO: re-enable */ }
    /// ```
    ///
    /// Both put an assertion in the guarded block and leave the handler empty,
    /// so neither "the handler is empty" nor "the assertions are guarded" tells
    /// them apart — and `tamper::swallows` reported the honest one as
    /// `assertion_swallowed` at **`fail`**, refuting a common idiom.
    ///
    /// What separates them is that the honest form's guarded block ends with an
    /// UNCONDITIONAL failure. It is reached only when the expected exception
    /// did not arrive; the handler is the success path. A swallow's guarded
    /// block has no such call — its assertions are conditional, and dropping
    /// them is the point.
    pub unconditional_fail_markers: &'static [&'static str],
}

/// One language's detector pack.
///
/// Object-safe on purpose: the registry stores `&'static dyn LangPack`, so a
/// consumer dispatches on a path extension without knowing the set at compile
/// time — which is the whole point of publishing the interface.
pub trait LangPack: Send + Sync {
    fn language(&self) -> PackLanguage;

    /// Lowercase file extensions, without the dot.
    fn extensions(&self) -> &'static [&'static str];

    fn ts_language(&self) -> tree_sitter::Language;

    fn spec(&self) -> LangSpec;

    /// Is this node a test function, and what is it called?
    ///
    /// `verify` decides this inline in `as_test_fn` with a `match` over the
    /// closed enum, mixing three unrelated rules (Rust: an attribute; JS: the
    /// callee; Python/Go: a name prefix). Four more languages add three more
    /// rules — Java and C# mark tests with **annotations that vary by
    /// framework** (`@Test`, `@ParameterizedTest`, `[Fact]`, `[Theory]`), Ruby
    /// marks them with a **block-taking call** whose name is a string
    /// (`it "…" do`) or a method-name prefix (`def test_…`). None of those fit
    /// a prefix list, so the decision is the pack's.
    fn test_name(&self, node: Node, source: &str) -> Option<String>;

    /// Classify one assertion's strength from its own source text.
    ///
    /// The caller has already selected the node with the parser, so this never
    /// sees a comment. It must still strip string literals before reading a
    /// shape — see [`crate::shape::strip_string_literals`] and the ambiguity
    /// budget this crate is held to.
    fn classify(&self, assertion_text: &str) -> Strength;

    /// Mock/stub/patch call prefixes whose first argument names a target.
    fn mock_calls(&self) -> &'static [&'static str];

    /// The highest severity a finding from this pack may carry.
    ///
    /// `None` means uncapped — the detector's own severity stands.
    ///
    /// # Why this exists, and why it is a property of the PACK
    ///
    /// Operator ruling, 2026-09-06 01:10 IST, on Ruby's measured ambiguity of 0.1275%
    /// against the 0.1% budget:
    ///
    /// > *"ship Ruby, but every Ruby finding is capped at warn severity — never
    /// > fail — until ambiguity is under 0.1%, with the cap and the measurement
    /// > recorded beside it. No per-language budget. A detector over budget may
    /// > inform; it may not accuse."*
    ///
    /// That last sentence is the rule, and it is why the cap is not a per-
    /// language budget in disguise. The budget stays one number for every
    /// language (`measured::AMBIGUITY_BUDGET_PCT`, 0.1%). What changes is what a
    /// pack that misses it is *allowed to do*: a shape classifier that is
    /// ambiguous more often than the budget can still tell a reader something
    /// worth reading, but it may not be the thing that refutes a claim.
    ///
    /// `MEASUREMENTS.toml` records the cap beside the measurement that caused
    /// it, so the two cannot drift apart, and
    /// `tests/severity_cap.rs::a_ruby_finding_cannot_reach_fail` proves the cap
    /// holds by construction rather than by inspection.
    ///
    /// **Half of this rule is not enforceable here.**
    /// `verify::tests_pass::escalate` raises `skip_marker_added` from `warn` to
    /// `fail` when the run reports skipped tests, and that happens downstream of
    /// anything this crate emits. Capping at emission cannot prevent it. That
    /// half is interface request R8, agreed with the core.
    fn severity_cap(&self) -> Option<Severity> {
        None
    }

    /// Does this text name a skip marker for this language? Some markers are
    /// **conditional** (`[Fact(Skip=…)]` is not, `@EnabledOnOs` is) and the
    /// distinction is the pack's to make.
    fn is_skip_marker(&self, decorator_text: &str) -> bool {
        let lower = decorator_text.to_ascii_lowercase();
        self.spec()
            .skip_markers
            .iter()
            .any(|m| lower.contains(&m.to_ascii_lowercase()))
    }
}

/// Every pack this crate publishes.
pub fn packs() -> &'static [&'static dyn LangPack] {
    &[
        &crate::langs::java::Java,
        &crate::langs::csharp::CSharp,
        &crate::langs::ruby::Ruby,
        &crate::langs::go::Go,
    ]
}

/// The pack for a path, by extension.
///
/// `None` means *not a language this crate parses*, which the caller must
/// report rather than treat as "no findings" — `verify::tamper::Language::from_path`
/// carries the same warning and for the same reason.
pub fn pack_for_path(path: &str) -> Option<&'static dyn LangPack> {
    let ext = path.to_ascii_lowercase();
    let ext = ext.rsplit('.').next()?.to_string();
    packs()
        .iter()
        .copied()
        .find(|p| p.extensions().contains(&ext.as_str()))
}

/// The pack for a [`PackLanguage`].
pub fn pack_for(language: PackLanguage) -> &'static dyn LangPack {
    packs()
        .iter()
        .copied()
        .find(|p| p.language() == language)
        .expect("every PackLanguage has a pack; packs() and PackLanguage are edited together")
}

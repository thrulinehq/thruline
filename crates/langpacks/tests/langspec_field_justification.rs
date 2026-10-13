//! What `skip_site` and `catch_kinds` buy, measured against the old shape.
//!
//! Track Z asked, for R1 sizing: adopting this crate's `LangSpec` is not an
//! interface publication but a **five-language re-measurement**, because
//! `skip_site` replaces `skip_in_body` and `catch_kinds` is new. So: what does
//! each field buy, and what breaks if `crates/verify` publishes its *current*
//! shape and this crate conforms to that instead?
//!
//! The answer is measured here rather than argued. Each test builds a pack that
//! is identical to a shipping one except for the field under question, set back
//! to what the old shape can express, and runs the real fixtures through it.

use thruline_langpacks::pack::{LangPack, LangSpec, PackLanguage, Severity, SkipSite, Strength};
use thruline_langpacks::{diff_test_file, pack_for};
use tree_sitter::Node;

fn fixture(lang: &str, rel: &str) -> String {
    let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/families")
        .join(lang)
        .join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn fired(fs: &[thruline_verify::verdict::Finding], detector: &str) -> bool {
    fs.iter().any(|f| f.detector == detector)
}

/// A pack that delegates everything to a real one but overrides its `LangSpec`.
struct Altered {
    inner: &'static dyn LangPack,
    spec: LangSpec,
}

impl LangPack for Altered {
    fn language(&self) -> PackLanguage {
        self.inner.language()
    }
    fn extensions(&self) -> &'static [&'static str] {
        self.inner.extensions()
    }
    fn ts_language(&self) -> tree_sitter::Language {
        self.inner.ts_language()
    }
    fn spec(&self) -> LangSpec {
        self.spec.clone()
    }
    fn test_name(&self, node: Node, source: &str) -> Option<String> {
        self.inner.test_name(node, source)
    }
    fn classify(&self, text: &str) -> Strength {
        self.inner.classify(text)
    }
    fn mock_calls(&self) -> &'static [&'static str] {
        self.inner.mock_calls()
    }
    fn severity_cap(&self) -> Option<Severity> {
        self.inner.severity_cap()
    }
}

fn altered(language: PackLanguage, f: impl FnOnce(&mut LangSpec)) -> Altered {
    let inner = pack_for(language);
    let mut spec = inner.spec();
    f(&mut spec);
    Altered { inner, spec }
}

// ---------------------------------------------------------------------------
// `skip_site` — what the old `skip_in_body: bool` cannot express
// ---------------------------------------------------------------------------

/// **`skip_in_body: true` is SAFE for Java now — and was not when `skip_site`
/// was written.** This test measures the correction rather than the claim.
///
/// The original argument for `skip_site` was that a body-text search matches a
/// comment mentioning `@Disabled`. That was true, and it is no longer: this
/// crate's engine blanks comment spans structurally before any text-derived
/// fact is read (`tamper::blank_comments`, added when the same route was found
/// corrupting `body_tokens`). So the comment half of the case for `skip_site`
/// is **obsolete**, and saying so is worth more to R1's sizing than defending
/// it.
#[test]
fn the_comment_false_positive_that_justified_skip_site_is_fixed_by_comment_blanking() {
    let base = "class T {\n  @Test void a() {\n    assertEquals(9.0, rate(2));\n  }\n}\n";
    let head = "class T {\n  @Test void a() {\n    // we used to @Disabled this while the tariff service was down\n    assertEquals(9.0, rate(2));\n  }\n}\n";

    for (label, pack) in [
        ("shipping (DeclarationChild)", None),
        ("old shape (skip_in_body: true)", Some(SkipSite::Body)),
    ] {
        let altered_pack;
        let p: &dyn LangPack = match pack {
            None => pack_for(PackLanguage::Java),
            Some(site) => {
                altered_pack = altered(PackLanguage::Java, |s| s.skip_site = site);
                &altered_pack
            }
        };
        let f = diff_test_file("T.java", base, head, p).expect(label);
        assert!(
            !fired(&f, "skip_marker_added"),
            "{label} reported a COMMENT as a skip marker. If the body search is \
             unsafe again, comment blanking has regressed and BOTH this test and \
             the `body_tokens` guard should be failing: {f:?}"
        );
    }
}

/// So does the old shape work for Java after all? **For the marker, yes. For
/// the CONDITIONAL distinction, no.**
///
/// `skip_in_body: true` finds `@Disabled` because the annotation is inside the
/// declaration's own text. What it cannot do is tell an unconditional
/// `@Disabled` from a conditional `@EnabledOnOs`/`@DisabledIf`, because a body
/// search has no structure to consult — it sees a string either way. That
/// distinction is `LangPack::is_skip_marker`'s, and it needs the decorator text
/// isolated from the body, which is what `decorator_kinds` + `skip_site` give
/// it.
///
/// This is the honest residue of the `skip_site` argument, and it is narrower
/// than the original claim.
#[test]
fn the_old_shape_finds_the_marker_but_cannot_separate_a_conditional_one() {
    let base = fixture("java", "base/ShippingTest.java");
    let conditional = fixture("java", "honest/ineffective_skip_marker/ShippingTest.java");

    let shipping = diff_test_file(
        "ShippingTest.java",
        &base,
        &conditional,
        pack_for(PackLanguage::Java),
    )
    .expect("s");
    let old = altered(PackLanguage::Java, |s| s.skip_site = SkipSite::Body);
    let with_old = diff_test_file("ShippingTest.java", &base, &conditional, &old).expect("old");

    // Whatever the shipping pack decides about this fixture, the point is
    // whether the two shapes AGREE. If they do, `skip_site` buys nothing here
    // and R1 is cheaper; if they differ, the difference is what it buys.
    let a = fired(&shipping, "skip_marker_added");
    let b = fired(&with_old, "skip_marker_added");
    println!("conditional-skip fixture: shipping fired={a}, skip_in_body-true fired={b}");
    assert_eq!(
        a, b,
        "MEASURED DIFFERENCE: the two shapes disagree on a conditional skip \
         marker (shipping={a}, old={b}). That difference is what `skip_site` \
         buys, and this assertion exists to make the number concrete rather \
         than asserted — invert it and record the direction."
    );
}

/// And the other setting misses the real thing.
#[test]
fn skip_in_body_false_misses_a_real_java_disabled_annotation() {
    let base = fixture("java", "base/ShippingTest.java");
    let head = fixture("java", "cheat/skip_marker_added/ShippingTest.java");

    let shipping = diff_test_file(
        "ShippingTest.java",
        &base,
        &head,
        pack_for(PackLanguage::Java),
    )
    .expect("s");
    assert!(
        fired(&shipping, "skip_marker_added"),
        "the shipping pack missed a real @Disabled: {shipping:?}"
    );

    let old = altered(PackLanguage::Java, |s| s.skip_site = SkipSite::Preceding);
    let with_old = diff_test_file("ShippingTest.java", &base, &head, &old).expect("old");
    assert!(
        !fired(&with_old, "skip_marker_added"),
        "`skip_in_body: false` DID find the annotation, so the sibling walk \
         reaches a declaration's children after all and this half of the \
         argument is void: {with_old:?}"
    );
}

// ---------------------------------------------------------------------------
// `catch_kinds` — what the substring path cannot do
// ---------------------------------------------------------------------------

/// Ruby loses swallow detection **entirely** without `catch_kinds`.
///
/// Not "loses precision": the substring path needs a `swallow_markers` hit
/// beside one of `pass` / `{}` / `continue`, and a Ruby `begin … rescue … end`
/// body has none of the three.
#[test]
fn ruby_has_no_swallow_detection_at_all_without_catch_kinds() {
    let base = fixture("ruby", "base_test.rb");
    let head = fixture("ruby", "cheat_assertion_swallowed_test.rb");

    let shipping =
        diff_test_file("t_test.rb", &base, &head, pack_for(PackLanguage::Ruby)).expect("s");
    assert!(
        fired(&shipping, "assertion_swallowed"),
        "the shipping Ruby pack missed its own swallow fixture: {shipping:?}"
    );

    let old = altered(PackLanguage::Ruby, |s| s.catch_kinds = &[]);
    let with_old = diff_test_file("t_test.rb", &base, &head, &old).expect("old");
    assert!(
        !fired(&with_old, "assertion_swallowed"),
        "Ruby detected the swallow with `catch_kinds` EMPTY, so the substring \
         path can serve it after all and R7's Ruby half is void: {with_old:?}"
    );
}

/// The C# case that decides R7a: honest and cheat are **indistinguishable**
/// without the structural path.
///
/// Both put an assertion in the guarded block and leave the handler empty. The
/// substring rule sees the same thing in both, so whichever way it is tuned it
/// is wrong about one of them. With `catch_kinds` the engine reads the guarded
/// block's contents and separates them.
#[test]
fn without_catch_kinds_the_honest_and_cheating_csharp_try_catch_are_the_same_shape() {
    let base = fixture("csharp", "base/ShippingRateTests.cs");
    let honest = fixture(
        "csharp",
        "honest/negative_trycatch_bare/ShippingRateTests.cs",
    );
    let cheat = fixture("csharp", "cheat/assertion_swallowed/ShippingRateTests.cs");

    let pack = pack_for(PackLanguage::CSharp);
    let h = diff_test_file("S.cs", &base, &honest, pack).expect("h");
    let c = diff_test_file("S.cs", &base, &cheat, pack).expect("c");
    assert!(
        !fired(&h, "assertion_swallowed"),
        "the shipping pack accuses the honest expects-a-throw idiom: {h:?}"
    );
    assert!(
        fired(&c, "assertion_swallowed"),
        "the shipping pack misses the real swallow: {c:?}"
    );

    // Now with the structural path removed, as verify's current shape has it.
    let old = altered(PackLanguage::CSharp, |s| s.catch_kinds = &[]);
    let h_old = diff_test_file("S.cs", &base, &honest, &old).expect("h_old");
    let c_old = diff_test_file("S.cs", &base, &cheat, &old).expect("c_old");
    assert_eq!(
        fired(&h_old, "assertion_swallowed"),
        fired(&c_old, "assertion_swallowed"),
        "with `catch_kinds` empty the honest and the cheating try/catch got \
         DIFFERENT answers, so the substring path can separate them and R7a is \
         cheaper than this crate claims: honest={h_old:?} cheat={c_old:?}"
    );
}

// ---------------------------------------------------------------------------
// The fourth raw-text path
// ---------------------------------------------------------------------------

/// A mock named only in a **comment** is not a mock.
///
/// R6's core half found a fourth fact on the raw-text route that this crate's
/// R6 entry named only three of: `mock_targets`, alongside `body_tokens`,
/// `has_sleep_or_retry` and the `skip_in_body` marker search.
///
/// This crate is not affected, and the reason is worth recording because it is
/// the difference between two ways of applying the same fix. `blank_comments`
/// is applied once, at the **binding** — `as_test_fn` rebinds `text` to the
/// blanked copy before reading anything from it — so every fact derived from
/// that binding was covered the moment the first one was, including the one I
/// had not enumerated. Blanking per-fact would have covered exactly the facts
/// someone remembered to list, which is how the fourth went missing from my own
/// account of it.
///
/// Asserted rather than reasoned, because "it should be covered" is what an
/// enumeration says right before it turns out to be short by one.
#[test]
fn a_mock_named_only_in_a_comment_is_not_a_mock_target() {
    let base = "\
describe Shipping do
  it \"prices a small parcel\" do
    expect(Shipping.rate(2)).to eq(9.0)
  end
end
";
    // The head adds a COMMENT mentioning a mock of the changed symbol, and
    // nothing else. `mocked_unit_under_test` must not fire.
    let head = "\
describe Shipping do
  it \"prices a small parcel\" do
    # we could allow(Shipping).to receive(:rate) here, but that would stop
    # exercising the thing under test
    expect(Shipping.rate(2)).to eq(9.0)
  end
end
";
    let changed = vec!["rate".to_string()];
    let findings = thruline_langpacks::diff_test_file_with_symbols(
        "shipping_spec.rb",
        base,
        head,
        pack_for(PackLanguage::Ruby),
        &changed,
    )
    .expect("diff");
    assert!(
        !fired(&findings, "mocked_unit_under_test"),
        "a mock named only inside a comment was read as a real mock — the \
         fourth raw-text path is NOT covered here after all: {findings:?}"
    );

    // Control: the same call in code DOES fire, so the assertion above is not
    // passing because the detector is simply dead.
    let real = "\
describe Shipping do
  it \"prices a small parcel\" do
    allow(Shipping).to receive(:rate).and_return(9.0)
    expect(Shipping.rate(2)).to eq(9.0)
  end
end
";
    let findings = thruline_langpacks::diff_test_file_with_symbols(
        "shipping_spec.rb",
        base,
        real,
        pack_for(PackLanguage::Ruby),
        &changed,
    )
    .expect("diff");
    assert!(
        fired(&findings, "mocked_unit_under_test"),
        "the control did not fire, so the comment case proves nothing: {findings:?}"
    );
}

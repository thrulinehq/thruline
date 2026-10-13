//! R1 (Track G) — the published language-pack interface, and the one field
//! that came with it.
//!
//! G's sentence, and it was exact: *"Without this, no language pack can exist
//! outside `crates/verify` at all."* `Language` is a closed enum, `LangSpec`
//! was private behind a private `Language::spec()`, and every `match` over the
//! enum is exhaustive by construction.
//!
//! What is asserted here is what the interface actually promises today —
//! deliberately less than R1 asked for, and the shortfall is named in
//! `LangPack`'s own documentation rather than implied by an absent test.

use thruline_verify::tamper::{
    builtin_packs, pack_for_path, parse_tests, parse_tests_with_pack, LangPack, LangSpec, Language,
};
use thruline_verify::verdict::Severity;

// ---------------------------------------------------------------------------
// The interface is reachable from outside the crate. That is the whole request.
// ---------------------------------------------------------------------------

#[test]
fn a_pack_can_be_written_outside_this_crate() {
    // This test file IS outside `crates/verify`, so if it compiles, the thing
    // G said was impossible is possible. The type is exercised rather than
    // merely named, so a future `pub(crate)` on any field breaks it.
    struct Elixir;
    impl LangPack for Elixir {
        fn name(&self) -> &'static str {
            "elixir"
        }
        fn extensions(&self) -> &'static [&'static str] {
            &["ex", "exs"]
        }
        fn ts_language(&self) -> tree_sitter::Language {
            // Any grammar; this test is about the interface, not the parser.
            tree_sitter_python::LANGUAGE.into()
        }
        fn spec(&self) -> LangSpec {
            LangSpec {
                fn_kinds: &["function_definition"],
                assertion_kinds: &["assert_statement"],
                assertion_call_prefixes: &["assert"],
                skip_markers: &["@tag :skip"],
                swallow_markers: &[],
                test_name_prefixes: &["test"],
                skip_in_body: false,
                catch_kinds: &["try_statement"],
            }
        }
        fn severity_cap(&self) -> Option<Severity> {
            Some(Severity::Warn)
        }
    }

    let elixir = Elixir;
    assert_eq!(elixir.spec().catch_kinds, &["try_statement"]);
    assert_eq!(elixir.severity_cap(), Some(Severity::Warn));
}

#[test]
fn the_registry_is_caller_populated_and_the_first_match_wins() {
    // The shape forced by the dependency direction: `crates/langpacks` depends
    // on this crate and re-exports its Finding/Severity/Strength, so this crate
    // cannot depend back and cannot hold a registry of anyone else's packs. The
    // caller composes one.
    struct Mine;
    impl LangPack for Mine {
        fn name(&self) -> &'static str {
            "mine"
        }
        fn extensions(&self) -> &'static [&'static str] {
            &["py"] // deliberately collides with the built-in
        }
        fn ts_language(&self) -> tree_sitter::Language {
            tree_sitter_python::LANGUAGE.into()
        }
        fn spec(&self) -> LangSpec {
            builtin_packs()[0].spec()
        }
    }
    let mine = Mine;

    let mut registry: Vec<&dyn LangPack> = vec![&mine];
    registry.extend_from_slice(builtin_packs());
    assert_eq!(
        pack_for_path("app/test_x.py", &registry).map(|p| p.name()),
        Some("mine"),
        "a caller that puts its own pack first did not get it"
    );

    let mut other: Vec<&dyn LangPack> = builtin_packs().to_vec();
    other.push(&mine);
    assert_eq!(
        pack_for_path("app/test_x.py", &other).map(|p| p.name()),
        Some("python"),
        "precedence is not first-match, so a caller cannot control it"
    );
}

#[test]
fn a_path_no_pack_claims_is_none_rather_than_no_findings() {
    let registry: Vec<&dyn LangPack> = builtin_packs().to_vec();
    assert!(pack_for_path("Makefile", &registry).is_none());
    assert!(pack_for_path("app.rb", &registry).is_none());
    // Not a silent empty result: `None` is what the caller reports, the same
    // rule `Language::from_path` follows.
    assert!(pack_for_path("app.PY", &registry).is_some(), "case");
}

#[test]
fn every_built_in_language_is_a_pack_and_they_agree_on_the_extensions() {
    // The two dispatch paths must not disagree. `Language::from_path` is what
    // this crate uses internally; `pack_for_path` is what a caller uses. A pack
    // list that claimed a different extension set would route the same file two
    // ways depending on who asked.
    let registry: Vec<&dyn LangPack> = builtin_packs().to_vec();
    assert_eq!(registry.len(), 5, "a language stopped being a pack");
    for (path, want) in [
        ("a.py", "python"),
        ("a.rs", "rust"),
        ("a.js", "javascript"),
        ("a.jsx", "javascript"),
        ("a.mjs", "javascript"),
        ("a.cjs", "javascript"),
        ("a.ts", "typescript"),
        ("a.tsx", "typescript"),
        ("a.go", "go"),
    ] {
        assert_eq!(
            pack_for_path(path, &registry).map(|p| p.name()),
            Some(want),
            "{path}"
        );
        assert!(
            Language::from_path(path).is_some(),
            "{path} routes through the pack registry and not through the enum"
        );
    }
}

// ---------------------------------------------------------------------------
// `catch_kinds` — the field the coordinator's ruling adopted, and the one it
// did not
// ---------------------------------------------------------------------------

/// **The arrival of `catch_kinds` changes nothing for the five.**
///
/// G measured both proposed fields and withdrew `skip_site`; `catch_kinds` was
/// kept because Ruby has no swallow detection at all without it and C#'s honest
/// and cheating `try`/`catch` are the same string. Neither of those is a
/// built-in language here, so every built-in leaves it empty — and "every
/// built-in leaves it empty" is a fact about a table somebody will edit, so it
/// is asserted rather than trusted.
#[test]
fn catch_kinds_is_inert_for_every_built_in_language() {
    for pack in builtin_packs() {
        assert!(
            pack.spec().catch_kinds.is_empty(),
            "{} sets catch_kinds, which is a re-measurement of that language and \
             not something to acquire by editing a table",
            pack.name()
        );
    }
}

#[test]
fn skip_in_body_survived_as_a_bool() {
    // The other half of the ruling: `skip_site` was NOT adopted, because G
    // measured its justification away — comment blanking already gives what it
    // was for. This pins the decision so the field does not drift back in as a
    // tidy-up.
    let python = builtin_packs()[0].spec();
    assert!(
        !python.skip_in_body,
        "python reads skip markers in decorators"
    );
    let go = builtin_packs()[4].spec();
    assert!(go.skip_in_body, "go's marker is the call itself");
}

/// The structural reading is reachable, and it separates the two cases a
/// substring rule cannot.
///
/// Both sources below contain `try`, `catch` and an assertion. The substring
/// path sees the same thing in both; `catch_kinds` sees that one puts the
/// assertion in the guarded block with an empty handler, and the other's
/// handler asserts.
#[test]
fn catch_kinds_separates_a_swallow_from_an_expects_a_throw() {
    // Both sources contain `try`, `catch`-equivalent and an assertion. A
    // substring rule sees the same thing in both and is wrong about one of them
    // whichever way it is tuned — that is G's C# measurement, reproduced here
    // on a grammar this crate already carries so the test needs no new
    // dependency.
    let cheat =
        "def test_rate():\n    try:\n        assert rate(2) == 5\n    except Exception:\n        x = 1\n";
    let honest =
        "def test_rate():\n    try:\n        rate(None)\n        assert False\n    except ValueError:\n        assert True\n";

    // THE CONTROL, first: through the built-in Python pack `catch_kinds` is
    // empty, so the structural path does not run and neither source is a
    // swallow. Without this, the assertions below could be satisfied by a
    // detector that fires on everything.
    for src in [cheat, honest] {
        let tests = parse_tests(src, Language::Python).expect("parse");
        assert_eq!(tests.len(), 1);
        assert!(
            !tests[0].swallows,
            "the built-in path changed behaviour when catch_kinds arrived"
        );
    }

    // The same sources through a pack that DOES set `catch_kinds`.
    struct WithCatch;
    impl LangPack for WithCatch {
        fn name(&self) -> &'static str {
            "with-catch"
        }
        fn extensions(&self) -> &'static [&'static str] {
            &["py"]
        }
        fn ts_language(&self) -> tree_sitter::Language {
            tree_sitter_python::LANGUAGE.into()
        }
        fn spec(&self) -> LangSpec {
            LangSpec {
                catch_kinds: &["try_statement"],
                ..builtin_packs()[0].spec()
            }
        }
    }
    let pack = WithCatch;

    let cheat_fn = &parse_tests_with_pack(cheat, &pack, Language::Python).expect("parse")[0];
    assert!(
        cheat_fn.swallows,
        "the assertion is in the guarded block and the handler neither asserts \
         nor rethrows — this is the swallow BUILD-SPEC §6.7 files under `fail`, \
         and the substring path cannot see it"
    );

    let honest_fn = &parse_tests_with_pack(honest, &pack, Language::Python).expect("parse")[0];
    assert!(
        !honest_fn.swallows,
        "an expects-a-throw test was called a swallow. §13.1: accusing honest \
         users is the most expensive error this product can make, and this is \
         exactly the false positive `catch_kinds` exists to avoid"
    );
}

#[test]
fn a_handler_that_rethrows_is_not_a_swallow() {
    // The third case, which neither of the two above covers: the handler does
    // not assert, but it re-raises, so the failure still propagates.
    struct WithCatch;
    impl LangPack for WithCatch {
        fn name(&self) -> &'static str {
            "with-catch"
        }
        fn extensions(&self) -> &'static [&'static str] {
            &["py"]
        }
        fn ts_language(&self) -> tree_sitter::Language {
            tree_sitter_python::LANGUAGE.into()
        }
        fn spec(&self) -> LangSpec {
            LangSpec {
                catch_kinds: &["try_statement"],
                ..builtin_packs()[0].spec()
            }
        }
    }
    let src =
        "def test_rate():\n    try:\n        assert rate(2) == 5\n    except Exception:\n        raise\n";
    let f = &parse_tests_with_pack(src, &WithCatch, Language::Python).expect("parse")[0];
    assert!(!f.swallows, "a re-raising handler was called a swallow");
}

/// R10 (Track G) — `catch (Throwable t) {}` is a swallow, not a re-throw.
///
/// The re-throw check matched `"throw"` as a **substring**, so `Throwable`
/// contained it and the commonest Java swallow of all was exempted. This crate
/// inherited the bug from G's, which G had already fixed at `da7243a` with a
/// whole-token match.
///
/// **Harmless only while every built-in ships `catch_kinds: &[]`** — and it
/// fires the moment a registered pack fills that field, which is exactly what
/// registering G's Java and C# packs does. So it is fixed before registration,
/// not after.
#[test]
fn a_catch_of_throwable_is_a_swallow_and_not_a_rethrow() {
    struct WithCatch;
    impl LangPack for WithCatch {
        fn name(&self) -> &'static str {
            "with-catch"
        }
        fn extensions(&self) -> &'static [&'static str] {
            &["py"]
        }
        fn ts_language(&self) -> tree_sitter::Language {
            tree_sitter_python::LANGUAGE.into()
        }
        fn spec(&self) -> LangSpec {
            LangSpec {
                catch_kinds: &["try_statement"],
                ..builtin_packs()[0].spec()
            }
        }
    }

    // The handler binds a name CONTAINING "throw" and does nothing with it.
    // Python stands in for Java's `catch (Throwable t) {}` — the grammar is not
    // the point, the token boundary is.
    let src = "def test_rate():\n    try:\n        assert rate(2) == 5\n    except Throwable:\n        pass\n";
    let f = &parse_tests_with_pack(src, &WithCatch, Language::Python).expect("parse")[0];
    assert!(
        f.swallows,
        "`Throwable` was read as a re-throw, so the commonest Java swallow is \
         exempt from the detector written to catch it"
    );

    // The control: a real re-throw is still a re-throw, or the fix is just
    // "everything swallows now".
    let rethrown = "def test_rate():\n    try:\n        assert rate(2) == 5\n    except Exception:\n        raise\n";
    let f = &parse_tests_with_pack(rethrown, &WithCatch, Language::Python).expect("parse")[0];
    assert!(!f.swallows, "a genuine re-raise was called a swallow");
}

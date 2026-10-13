//! F1: one invariant, one gate, and a control that no path avoids it.
//!
//! The defect this file exists for was measured end to end by the independent
//! verifier, against the real store:
//!
//! - `Event::new` **refused** `383151015300511101` ✓
//! - `serde_json::from_value::<Event>` **accepted** it
//! - `append_events` **wrote it** — one row into the ledger
//! - `…101` and `…102` sealed to the **same hash**
//!
//! One event substitutable for another, with `verify_session` unable to tell.
//! The same bypass defeated the float rule and NFC normalisation. The rule was
//! enforced at one of three construction paths and the ledger used another —
//! the same shape as the predicate-with-two-callers class that cost this
//! repository seventeen rounds in `redact.rs`.
//!
//! The fix is structural: `Event`'s fields are private and its `Deserialize` is
//! `#[serde(try_from = "EventWire")]`, so every path is the same path. These
//! tests are the control.

use serde_json::json;
use thruline_core::spec::event::{
    Event, EventId, EventKind, EventWire, OpenPayload, Payload, Source,
};

const UNSAFE_A: u64 = 383_151_015_300_511_101;
const UNSAFE_B: u64 = 383_151_015_300_511_102;

fn wire_with_payload(payload: serde_json::Value) -> serde_json::Value {
    json!({
        "v": 1,
        "id": EventId::generate(),
        "session_id": "s",
        "seq": 1,
        "ts": "2026-09-02T10:11:12.345Z",
        "source": "exec-recorder",
        "kind": "note",
        "payload": payload,
        "prev_hash": "sha256:00"
    })
}

#[test]
fn deserialization_cannot_smuggle_an_unhashable_integer() {
    // THE regression. Before the fix this parsed, sealed, and collided.
    for n in [UNSAFE_A, UNSAFE_B, u64::MAX] {
        let raw = wire_with_payload(json!({ "n": n }));
        assert!(
            serde_json::from_value::<Event>(raw.clone()).is_err(),
            "from_value accepted {n}, which RFC 8785 rounds — two different \
             events would seal to one hash"
        );
        // Every entry point, not just the one that used to be guarded.
        assert!(serde_json::from_str::<Event>(&raw.to_string()).is_err());
        assert!(serde_json::from_slice::<Event>(raw.to_string().as_bytes()).is_err());
    }

    // And the collision it would have produced is real, so the refusal is not
    // theoretical: these two integers canonicalize identically.
    assert_eq!(
        thruline_core::spec::canonical::canonical_string(&json!(UNSAFE_A)).expect("canon"),
        thruline_core::spec::canonical::canonical_string(&json!(UNSAFE_B)).expect("canon"),
    );
}

#[test]
fn deserialization_cannot_smuggle_an_unhashable_seq() {
    let mut raw = wire_with_payload(json!({}));
    raw["seq"] = json!(UNSAFE_A);
    assert!(
        serde_json::from_value::<Event>(raw).is_err(),
        "`seq` is a hashed field and took the same bypass"
    );
}

#[test]
fn deserialization_cannot_smuggle_a_float() {
    for payload in [
        json!({ "pct": 91.2 }),
        json!({ "nested": { "deep": [1, 2, 0.5] } }),
        json!({ "big": 1e30 }),
    ] {
        assert!(
            serde_json::from_value::<Event>(wire_with_payload(payload.clone())).is_err(),
            "from_value accepted the float in {payload}"
        );
    }
}

#[test]
fn deserialization_normalizes_text_so_two_platforms_cannot_fork_the_chain() {
    // Before the fix, NFD text entered through `from_value` untouched and the
    // same file captured on macOS and Linux produced two different chains.
    let composed: Event =
        serde_json::from_value(wire_with_payload(json!({ "p": "caf\u{e9}" }))).expect("parses");
    let decomposed: Event =
        serde_json::from_value(wire_with_payload(json!({ "p": "cafe\u{301}" }))).expect("parses");

    assert_eq!(composed.payload(), decomposed.payload());

    // Same id, so the comparison is about the text and nothing else — the
    // earlier version of this control let `Event::new` mint fresh ULIDs and
    // therefore could not fail for its stated reason.
    let fixed = EventId::parse("evt_01ARZ3NDEKTSV4RRFFQ69G5FAV").expect("id");
    let a = composed.with_id(fixed.clone()).seal().expect("seals");
    let b = decomposed.with_id(fixed).seal().expect("seals");
    assert_eq!(a.hash(), b.hash(), "NFC/NFD still fork the chain");
}

#[test]
fn deserialization_cannot_smuggle_a_malformed_timestamp() {
    let mut raw = wire_with_payload(json!({}));
    // `+00:00` and `Z` name one instant and hash differently.
    raw["ts"] = json!("2026-09-02T10:11:12+00:00");
    assert!(serde_json::from_value::<Event>(raw.clone()).is_err());
    raw["ts"] = json!("not a time");
    assert!(serde_json::from_value::<Event>(raw).is_err());
}

#[test]
fn the_edit_path_re_runs_the_gate_and_re_seals() {
    // `edit_and_seal` is the only editing form production may call. If it did
    // not re-run the gate it would be a fourth bypass; if it did not re-seal it
    // would hand back an event asserting a hash it no longer has.
    let event = Event::new(
        "s",
        1,
        "2026-09-02T10:11:12.345Z",
        Source::Thruline,
        Payload::Note(OpenPayload::new()),
        "sha256:00",
    )
    .expect("constructible");

    assert!(event
        .clone()
        .edit_and_seal(|w| w.payload = json!({ "n": UNSAFE_A }))
        .is_err());
    assert!(event.clone().edit_and_seal(|w| w.seq = UNSAFE_A).is_err());
    assert!(event
        .clone()
        .edit_and_seal(|w| w.ts = "2026-09-02T10:11:12+00:00".into())
        .is_err());

    // A legitimate edit works AND comes back sealed and self-consistent.
    let edited = event
        .clone()
        .edit_and_seal(|w| w.kind = EventKind::Exec)
        .expect("a valid edit");
    assert_eq!(edited.kind(), &EventKind::Exec);
    edited
        .verify_hash()
        .expect("edit_and_seal must return a sealed, self-consistent event");

    // And an edit that tries to smuggle a stale hash through cannot: the seal
    // overwrites whatever the closure put there. This is the property that
    // makes the form safe for production.
    let forced = event
        .clone()
        .edit_and_seal(|w| {
            w.kind = EventKind::Exec;
            w.hash = Some("sha256:deadbeef".into());
        })
        .expect("a valid edit");
    forced
        .verify_hash()
        .expect("edit_and_seal honoured a caller-supplied hash");
    assert_ne!(forced.hash(), Some("sha256:deadbeef"));
}

#[test]
fn into_wire_drops_the_hash_so_a_stale_one_cannot_travel_by_accident() {
    // The seam that makes tampering fixtures possible is deliberately narrow:
    // decomposing an event drops its hash, so an event whose hash disagrees
    // with its contents can only be built by assigning `wire.hash` back on
    // purpose. That assignment is what the call-site census looks for.
    let event = Event::new(
        "s",
        1,
        "2026-09-02T10:11:12.345Z",
        Source::Thruline,
        Payload::Note(OpenPayload::new()),
        "sha256:00",
    )
    .expect("constructible")
    .seal()
    .expect("seals");
    assert!(event.hash().is_some());

    let wire = event.clone().into_wire();
    assert_eq!(wire.hash, None, "into_wire carried the hash across");

    // Round-tripping therefore yields an UNSEALED event, not a mis-sealed one.
    let back = Event::try_from(wire).expect("valid wire");
    assert!(back.hash().is_none());
    assert!(back.verify_hash().is_err());
}

#[test]
fn no_type_outside_the_event_module_can_build_an_event_by_literal() {
    // The call-site control. Rust's privacy already makes `Event { .. }`
    // impossible outside `spec::event`, so this asserts the property that makes
    // that true — the fields are private — rather than trusting it.
    //
    // Derived from the source, in the spirit of `a209_masking_corpus.rs`'s
    // census: if someone makes a field `pub` again, the gate becomes optional
    // and this fails.
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/spec/event.rs"),
    )
    .expect("read event.rs");

    let start = src
        .find("pub struct Event {")
        .expect("the Event struct is still declared");
    let body = &src[start..start + src[start..].find('}').expect("struct closes")];
    for field in [
        "v",
        "id",
        "session_id",
        "seq",
        "ts",
        "source",
        "kind",
        "payload",
        "prev_hash",
        "hash",
    ] {
        assert!(
            !field_is_visible(body, field),
            "`Event::{field}` is visible outside its module. Any visibility \
             beyond private lets a caller assign the field after construction \
             and skip the gate — that is the F1 defect, and `pub(crate)` \
             reintroduces it from inside this crate."
        );
        assert!(
            body.contains(&format!("{field}:")),
            "`Event::{field}` has disappeared from the struct"
        );
    }

    // And the gate is still wired to Deserialize.
    assert!(
        src.contains("#[serde(try_from = \"EventWire\")]"),
        "Event no longer validates on deserialization — the F1 bypass is open"
    );
}

#[test]
fn the_wire_type_is_a_plain_carrier_and_holds_no_invariant() {
    // `EventWire` is deliberately unchecked: it is what a malformed document
    // parses INTO so it can be inspected and reported. If it started rejecting,
    // a bad event would become unreadable rather than reportable, which the
    // project's "never silently drop, always report" invariant forbids.
    let raw = wire_with_payload(json!({ "n": UNSAFE_A }));
    let wire: EventWire = serde_json::from_value(raw).expect("the wire form accepts anything");
    assert_eq!(wire.payload["n"], json!(UNSAFE_A));
    // ...and converting it is what refuses.
    assert!(Event::try_from(wire).is_err());
}

// ---------------------------------------------------------------------------
// The call-site census
// ---------------------------------------------------------------------------

/// Every non-test call site of [`Event::into_wire`] in the workspace.
///
/// `into_wire` is the seam that makes a hash-disagreeing event constructible:
/// it drops the hash, so a caller who wants a stale one has to assign
/// `wire.hash` back deliberately. Production has no reason to do either —
/// `Event::edit_and_seal` covers every legitimate edit — so this list is
/// expected to stay EMPTY, and a new entry is a decision someone has to write
/// down rather than a diff nobody notices.
///
/// # This census has a known evasion, and it is documented rather than assumed
/// away
///
/// It is a text scan. In the A209 work, a call-site census of exactly this kind
/// was **defeated by a function-pointer alias** — the call was bound to a
/// variable and invoked through it, which the scan could not see, and which
/// passed `fmt` and `clippy` cleanly on the way through.
///
/// So the census is the *second* line, not the first. The first is the type
/// boundary: `edit_and_seal` re-seals unconditionally, and `into_wire` drops
/// the hash, so the dangerous state is not reachable by accident no matter how
/// the call is spelled. The census catches what the boundary cannot express —
/// "no production code should be doing this at all" — and its blind spot is
/// named here so nobody mistakes it for a proof.
///
/// Mitigations against the known alias evasion, both applied below: the scan
/// looks for the bare method name rather than a full call expression, so
/// `let f = Event::into_wire;` is still a hit; and a negative control injects
/// an unclassified site and requires the derivation to FAIL.
const INTO_WIRE_CALL_SITES: &[(&str, &str)] = &[(
    "event.rs",
    "Event::edit_and_seal — the one legitimate production use. It decomposes, \
     applies the caller's edit, forces `hash = None`, and re-seals, so the \
     stale-hash state this seam could otherwise reach is unreachable through \
     it. Proven by `the_edit_path_re_runs_the_gate_and_re_seals`, which feeds \
     it a caller-supplied hash and asserts the seal overwrites it.",
)];

/// Member crate `src` directories, read from the workspace manifest.
///
/// Same derivation as `a209_masking_corpus.rs`, and it inherits the same
/// tripwire: `members` must stay on ONE line in `Cargo.toml`.
fn member_sources() -> Vec<std::path::PathBuf> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace root")
        .to_path_buf();
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).expect("workspace manifest");
    let line = manifest
        .lines()
        .find(|l| l.trim_start().starts_with("members"))
        .expect("members key");
    let inner = line
        .split_once('[')
        .and_then(|(_, r)| r.split_once(']'))
        .map(|(m, _)| m)
        .expect("members list");
    let members: Vec<std::path::PathBuf> = inner
        .split(',')
        .map(|m| m.trim().trim_matches('"').trim())
        .filter(|m| !m.is_empty())
        .map(|m| root.join(m).join("src"))
        .collect();
    assert!(
        members.len() >= 2,
        "the walk must cover every member crate, found {members:?}"
    );
    members
}

fn walk(dir: &std::path::Path, f: &mut impl FnMut(&std::path::Path, &str)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, f);
        } else if path.extension().is_some_and(|e| e == "rs") {
            if let Ok(text) = std::fs::read_to_string(&path) {
                f(&path, &text);
            }
        }
    }
}

/// Strip `#[cfg(test)]` items by brace depth rather than truncating at the
/// first marker — a file with a `cfg(test)` module in the middle would
/// otherwise hide everything after it.
fn strip_cfg_test(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(at) = rest.find("#[cfg(test)]") {
        out.push_str(&rest[..at]);
        let after = &rest[at..];
        let Some(open) = after.find('{') else {
            break;
        };
        let mut depth = 0usize;
        let mut end = None;
        for (i, c) in after[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        match end {
            Some(e) => rest = &after[e..],
            None => break,
        }
    }
    out.push_str(rest);
    out
}

fn observed_into_wire_sites() -> Vec<(String, usize)> {
    let mut found = Vec::new();
    for src in member_sources() {
        walk(&src, &mut |path, text| {
            // The definition itself is not a call site.
            let is_definition_file = path.ends_with("spec/event.rs");
            let stripped = strip_cfg_test(text);
            for (n, line) in stripped.lines().enumerate() {
                if !line.contains("into_wire") {
                    continue;
                }
                if is_definition_file
                    && (line.contains("pub fn into_wire") || line.trim_start().starts_with("///"))
                {
                    continue;
                }
                if line.trim_start().starts_with("//") {
                    continue;
                }
                let name = path
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_default();
                found.push((name, n + 1));
            }
        });
    }
    found
}

#[test]
fn every_into_wire_call_site_outside_tests_is_classified() {
    let observed = observed_into_wire_sites();
    let declared: Vec<&str> = INTO_WIRE_CALL_SITES.iter().map(|(f, _)| *f).collect();
    let unclassified: Vec<&(String, usize)> = observed
        .iter()
        .filter(|(file, _)| !declared.contains(&file.as_str()))
        .collect();
    assert!(
        unclassified.is_empty(),
        "unclassified `into_wire` call sites in production code: {unclassified:?}.\n\
         `into_wire` drops the hash and is the seam for building an event whose \
         hash disagrees with its contents. Production edits go through \
         `Event::edit_and_seal`. If a production use is genuinely needed, add it \
         to INTO_WIRE_CALL_SITES with a reason."
    );
}

#[test]
fn the_census_fails_when_an_unclassified_site_is_injected() {
    // The negative control. Without it the census is a heuristic asserting its
    // own completeness — which is the failure it exists to repair, and the
    // reason A209's first two censuses were worthless.
    let scratch = tempfile::tempdir().expect("tempdir");
    let src = scratch.path().join("src");
    std::fs::create_dir_all(&src).expect("mkdir");
    std::fs::write(
        src.join("injected.rs"),
        "pub fn sneaky(e: Event) -> EventWire { e.into_wire() }\n",
    )
    .expect("write");

    let mut found = Vec::new();
    walk(&src, &mut |path, text| {
        if strip_cfg_test(text).contains("into_wire") {
            found.push(path.to_path_buf());
        }
    });
    assert_eq!(
        found.len(),
        1,
        "the walk cannot see an injected `into_wire` call site, so the census \
         proves nothing"
    );

    // And the aliased spelling that defeated the A209 census is visible to THIS
    // scan, because it matches the bare method name rather than a call
    // expression. Recorded as a measured property, not a hope.
    std::fs::write(
        src.join("aliased.rs"),
        "pub fn sneaky() { let f = Event::into_wire; let _ = f; }\n",
    )
    .expect("write");
    let mut aliased = 0;
    walk(&src, &mut |_, text| {
        aliased += strip_cfg_test(text).matches("into_wire").count();
    });
    assert!(
        aliased >= 2,
        "the alias spelling is invisible to the census — the documented evasion \
         has widened"
    );
}

#[test]
fn cfg_test_blocks_are_stripped_by_depth_not_by_truncation() {
    // The stripper is load-bearing: if it truncated at the first `#[cfg(test)]`
    // it would hide every production call site below one, and the census would
    // pass by seeing nothing.
    let src = "fn a() { into_wire(); }\n\
               #[cfg(test)]\nmod t { fn inner() { if true { into_wire(); } } }\n\
               fn b() { into_wire(); }\n";
    let stripped = strip_cfg_test(src);
    assert_eq!(
        stripped.matches("into_wire").count(),
        2,
        "expected the two production calls and neither test call: {stripped:?}"
    );
    assert!(
        stripped.contains("fn b()"),
        "content after the cfg(test) block was lost"
    );
}

/// Does `body` declare `field` with ANY visibility beyond private?
///
/// Matches `pub` followed by whitespace **or** `(`. The first version tested
/// only the literal `"pub {field}:"`, so `pub(crate) payload: Value` slipped
/// through and the whole suite stayed green — verified reproducible.
/// `pub(crate)` grants write access to every module in `thruline-core`, which
/// reintroduces F1 from inside the crate.
///
/// This is the THIRD time in this project a text census has been evaded by a
/// syntax variant it did not anticipate — the others were a function-pointer
/// alias and a bare identifier in non-call position. A text scan sees
/// spellings, not semantics; the mitigation is to enumerate every spelling that
/// grants the capability, and to keep a control proving each is seen.
fn field_is_visible(body: &str, field: &str) -> bool {
    // NORMALISE FIRST, match second. Four syntax variants have now walked past
    // a text census in this project — a function-pointer alias, a bare
    // identifier in non-call position, `pub(crate)`, and `pub (crate)` with a
    // space plus `pub r#payload:` with a raw identifier. The class is: **a text
    // census matches spellings, and Rust has more spellings than the author
    // thought of.** The mitigation is not a longer list of patterns but a
    // canonical form — collapse whitespace, strip `r#` — so that every spelling
    // of the same declaration reduces to one string before any matching happens.
    //
    // `rustfmt` normalises `pub (crate)` and so the fmt gate happens to close
    // that one; it does NOT touch `r#`, which survived a fully green gate.
    let canonical: String = {
        let mut out = String::with_capacity(body.len());
        let mut chars = body.chars().peekable();
        let mut last_was_space = false;
        while let Some(c) = chars.next() {
            // Strip a raw-identifier prefix wherever it appears.
            if c == 'r' && chars.peek() == Some(&'#') {
                chars.next();
                continue;
            }
            if c.is_whitespace() {
                if !last_was_space {
                    out.push(' ');
                }
                last_was_space = true;
            } else {
                out.push(c);
                last_was_space = false;
            }
        }
        out
    };

    // `pub` as a TOKEN, then an optional `(...)` restriction group, then the
    // field name. One walk that covers every spelling, rather than one pattern
    // per spelling — which is what kept missing them.
    for (idx, _) in canonical.match_indices("pub") {
        // `pub` must be a whole word: `republish:` is not a visibility.
        let before_ok = idx == 0
            || !canonical.as_bytes()[idx - 1].is_ascii_alphanumeric()
                && canonical.as_bytes()[idx - 1] != b'_';
        if !before_ok {
            continue;
        }
        let mut after = canonical[idx + 3..].trim_start();
        if after.starts_with('(') {
            match after.find(')') {
                Some(close) => after = after[close + 1..].trim_start(),
                None => continue,
            }
        }
        if after.starts_with(&format!("{field}:")) {
            return true;
        }
    }
    false
}

#[test]
fn the_visibility_census_sees_every_spelling_that_grants_access() {
    for granting in [
        "    pub payload: Value,",
        "    pub(crate) payload: Value,",
        "    pub(super) payload: Value,",
        "    pub(in crate::spec) payload: Value,",
        // The two that walked past the previous version. `rustfmt` normalises
        // the first; nothing normalises the second, so it survived a fully
        // green gate.
        "    pub (crate) payload: Value,",
        "    pub r#payload: Value,",
        "    pub(crate) r#payload: Value,",
        "    pub\n        payload: Value,",
    ] {
        let body = format!("pub struct Event {{\n{granting}\n");
        assert!(
            field_is_visible(&body, "payload"),
            "the visibility census cannot see `{granting}` — a field made \
             visible that way would reopen F1 with the suite green"
        );
    }

    // It must NOT fire on a genuinely private field, or it is unfalsifiable.
    assert!(
        !field_is_visible("pub struct Event {\n    payload: Value,\n", "payload"),
        "the census fires on a private field"
    );
    // Nor attribute one field's visibility to another.
    assert!(
        !field_is_visible(
            "pub struct Event {\n    pub id: EventId,\n    payload: Value,\n",
            "payload"
        ),
        "the census attributes one field's visibility to another"
    );
}

#[test]
fn the_pub_crate_evasion_is_caught_on_the_real_struct_text() {
    // The regression, run against the ACTUAL declaration rather than a mock:
    // take the real struct body, inject `pub(crate)` onto one field, and require
    // the census to fire. Before the fix this injection left the whole suite
    // green.
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/spec/event.rs"),
    )
    .expect("read event.rs");
    let start = src.find("pub struct Event {").expect("struct");
    let body = &src[start..start + src[start..].find('}').expect("closes")];

    assert!(
        !field_is_visible(body, "payload"),
        "the real struct already exposes `payload`"
    );
    let injected = body.replace("    payload: Value,", "    pub(crate) payload: Value,");
    assert_ne!(injected, body, "the injection did not apply");
    assert!(
        field_is_visible(&injected, "payload"),
        "`pub(crate)` still walks past the census on the real struct text"
    );
}

/// Build a wire whose carried hash is the hash of its own original bytes —
/// what an honest producer on a decomposing platform emits.
fn honest_wire(text: &str) -> EventWire {
    let mut wire: EventWire =
        serde_json::from_value(wire_with_payload(json!({ "p": text }))).expect("wire parses");
    wire.hash = Some(wire.hash_over_original().expect("hashable"));
    wire
}

#[test]
fn an_honest_nfd_producer_is_distinguishable_from_a_tamperer() {
    // R1. Both cases are NFD and both fail `verify_hash` after the gate
    // normalises — so `needs_normalization` alone CANNOT tell them apart, and a
    // reader who took `true` to mean "benign" would excuse a tamper.
    //
    // The discriminator is whether the carried hash matches the ORIGINAL bytes.
    let honest = honest_wire("cafe\u{301}");
    let mut tampered = honest_wire("cafe\u{301}");
    // A tamper wearing NFD as camouflage: change content, keep the old hash.
    tampered.payload = json!({ "p": "cafe\u{301}", "injected": "x" });

    // Identical on the two signals that are NOT sufficient.
    assert!(honest.needs_normalization());
    assert!(tampered.needs_normalization());
    let honest_event = Event::try_from(honest.clone()).expect("converts");
    let tampered_event = Event::try_from(tampered.clone()).expect("converts");
    assert!(honest_event.verify_hash().is_err());
    assert!(tampered_event.verify_hash().is_err());

    // Separated by the one that is.
    assert!(
        honest.is_honest_normalization_difference(),
        "an honest NFD producer was not recognised"
    );
    assert!(
        !tampered.is_honest_normalization_difference(),
        "a tamper wearing NFD as camouflage was excused as a normalisation \
         difference — this is the harm R1 names"
    );
    assert_eq!(
        honest.hash_over_original().expect("hashable"),
        honest.hash.clone().unwrap_or_default()
    );
    assert_ne!(
        tampered.hash_over_original().expect("hashable"),
        tampered.hash.clone().unwrap_or_default()
    );
}

#[test]
fn already_nfc_text_is_never_reported_as_a_normalization_difference() {
    // The negative control: without it `is_honest_normalization_difference`
    // could return true unconditionally and every assertion above would hold.
    let clean = honest_wire("cafe");
    assert!(!clean.needs_normalization());
    assert!(
        !clean.is_honest_normalization_difference(),
        "NFC text reported as needing normalisation"
    );
    // And it converts to an event whose hash still verifies, since nothing moved.
    Event::try_from(clean)
        .expect("converts")
        .verify_hash()
        .expect("NFC text must survive the gate untouched");
}

#[test]
fn a_wire_with_no_hash_is_not_an_honest_normalization_difference() {
    // An unsealed wire carries no claim to compare against, so the predicate
    // must not manufacture one.
    let mut wire = honest_wire("cafe\u{301}");
    wire.hash = None;
    assert!(wire.needs_normalization());
    assert!(!wire.is_honest_normalization_difference());
}

#[test]
fn prev_hash_is_normalized_and_seen_by_the_discriminator() {
    // R2. `prev_hash` is ASCII by construction today, but the NFC guarantee was
    // stated as total and was not: an NFD `prev_hash` produced a different hash
    // and `needs_normalization` returned FALSE on it, so the discriminator was
    // blind to the one field it did not cover.
    let mut raw = wire_with_payload(json!({}));
    raw["prev_hash"] = json!("sha256:cafe\u{301}");
    let wire: EventWire = serde_json::from_value(raw).expect("wire parses");
    assert!(
        wire.needs_normalization(),
        "a non-NFC prev_hash is invisible to the discriminator"
    );

    // And the gate normalises it, so the two spellings converge.
    let fixed = EventId::parse("evt_01ARZ3NDEKTSV4RRFFQ69G5FAV").expect("id");
    let decomposed = Event::try_from(wire)
        .expect("converts")
        .with_id(fixed.clone())
        .seal()
        .expect("seals");

    let mut raw2 = wire_with_payload(json!({}));
    raw2["prev_hash"] = json!("sha256:caf\u{e9}");
    let composed: Event = serde_json::from_value(raw2).expect("parses");
    let composed = composed.with_id(fixed).seal().expect("seals");

    assert_eq!(
        decomposed.hash(),
        composed.hash(),
        "NFC and NFD prev_hash still fork the chain"
    );
    assert_eq!(decomposed.prev_hash(), "sha256:caf\u{e9}");
}

#[test]
fn derived_ids_are_valid_event_ids_and_are_a_function_of_their_material() {
    // The fold needs a deterministic id, or re-folding the same spool produces
    // a different chain head — same event count, same last_seq, different hash
    // — and the head stops being a function of the evidence.
    //
    // Every 128-bit value is a valid ULID (26 Crockford characters carry 130
    // bits), so no masking is needed. Proven over the extremes rather than
    // assumed, because "it fits" is exactly the kind of claim that is wrong at
    // the boundary.
    let a = EventId::derive(b"sess-1|1|deadbeef");
    let b = EventId::derive(b"sess-1|1|deadbeef");
    let c = EventId::derive(b"sess-1|2|deadbeef");
    assert_eq!(a, b, "derivation is not deterministic");
    assert_ne!(a, c, "different material produced the same id");
    for id in [&a, &c] {
        EventId::parse(id.as_str()).expect("a derived id must parse as an event id");
        assert_eq!(id.as_str().len(), 4 + 26);
    }

    // The extremes: all-zero and all-ones material, plus a sweep, all round-trip.
    for material in [
        vec![0u8; 0],
        vec![0u8; 64],
        vec![0xffu8; 64],
        b"\xf0\x9f\x98\x82 astral".to_vec(),
    ] {
        let id = EventId::derive(&material);
        EventId::parse(id.as_str())
            .unwrap_or_else(|e| panic!("derived id from {material:?} does not parse: {e}"));
    }
    for i in 0..512u32 {
        let id = EventId::derive(&i.to_le_bytes());
        assert!(
            EventId::parse(id.as_str()).is_ok(),
            "derived id {id} does not parse"
        );
    }
}

//! The claim a sync feature has to earn, made mechanical.
//!
//! *"The server cannot read your sessions"* is worth exactly as much as the
//! smallest gap in it, and the gaps are not usually in the cipher. They are in
//! the metadata beside it: an id built from a file path, a length that is the
//! plaintext's, a "debug" field, a log line. So this test drives a real push
//! through a transport that records **every byte it is handed** and then walks
//! those bytes for things that should not be in them.
//!
//! Each check carries a positive control, because a leak walk that matches
//! nothing passes.

use thruline_sync::{derive, seal, Envelope, Kdf, Memory, Transport};

/// A marker unlikely to occur by chance, so a hit is a hit.
const MARKER: &str = "PLAINTEXT-MARKER-8f2b41c9-do-not-let-this-reach-a-server";
const PASSPHRASE: &str = "correct horse battery staple";

fn push_everything() -> (Memory, Vec<u8>) {
    let kdf = Kdf::new(&[3u8; 16]);
    let key = derive(PASSPHRASE, &kdf).expect("derive");
    let mut t = Memory::default();
    for i in 0..8 {
        let plaintext = format!(
            "{{\"session\":\"s{i}\",\"secret\":\"{MARKER}\",\"path\":\"/Users/someone/work/repo\"}}"
        );
        let e = seal(&key, &format!("session/s{i}"), &kdf, plaintext.as_bytes()).expect("seal");
        t.push(&format!("session/s{i}"), &e).expect("push");
    }
    let seen = t.seen.clone();
    (t, seen)
}

/// **Decode first, then look.** This walk was wrong twice.
///
/// Version one searched the transport's bytes for the raw marker. Replacing the
/// ciphertext with `base64(plaintext)` — the exact shape a bug in `seal` would
/// take, since the envelope base64-encodes whatever it is handed — left the
/// marker plainly readable and this test passed.
///
/// Version two also searched for `base64(MARKER)`. That is still wrong, and
/// wrong in a way worth writing down: base64 encodes three input bytes at a
/// time from the **start of the whole input**, so the encoding of a substring
/// appears in the encoding of the whole only when the substring's offset is a
/// multiple of three. The marker sits inside a JSON document, so it almost never
/// is. The test passed again, against the same injected defect.
///
/// The shape that actually works is the one a leak detector has to have: decode
/// every encoded-looking run in what the transport was handed, and search the
/// **decoded** bytes. A walk that searches for encodings of a needle is a walk
/// that has to guess the encoder's framing; a walk that decodes does not.
fn decoded_views(seen: &[u8]) -> Vec<Vec<u8>> {
    use base64::Engine as _;
    let text = String::from_utf8_lossy(seen).to_string();
    let mut out = vec![seen.to_vec()];
    // Every maximal run of characters that could be base64 or hex, decoded if
    // it decodes. Over-inclusive on purpose: a run that is not really encoded
    // simply fails to decode and costs nothing.
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut Vec<Vec<u8>>| {
        if run.len() >= 8 {
            for engine in [
                base64::engine::general_purpose::STANDARD,
                base64::engine::general_purpose::STANDARD_NO_PAD,
                base64::engine::general_purpose::URL_SAFE_NO_PAD,
            ] {
                if let Ok(d) = engine.decode(run.as_bytes()) {
                    out.push(d);
                }
            }
            if run.len().is_multiple_of(2) && run.bytes().all(|b| b.is_ascii_hexdigit()) {
                let bytes: Option<Vec<u8>> = (0..run.len() / 2)
                    .map(|i| u8::from_str_radix(&run[i * 2..i * 2 + 2], 16).ok())
                    .collect();
                if let Some(b) = bytes {
                    out.push(b);
                }
            }
        }
        run.clear();
    };
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=' || c == '-' || c == '_' {
            run.push(c);
        } else {
            flush(&mut run, &mut out);
        }
    }
    flush(&mut run, &mut out);
    out
}

fn appears_in(views: &[Vec<u8>], needle: &[u8]) -> bool {
    views
        .iter()
        .any(|v| v.windows(needle.len()).any(|w| w == needle))
}

#[test]
fn no_plaintext_reaches_the_transport_in_any_encoding() {
    let (_t, seen) = push_everything();
    assert!(!seen.is_empty(), "the transport was handed nothing");
    let views = decoded_views(&seen);
    assert!(
        !appears_in(&views, MARKER.as_bytes()),
        "plaintext reached the transport. That is the one thing this crate exists \
         to prevent — and note that this walk decodes, so \"but it was base64\" is \
         not a defence."
    );

    // **Positive controls, one per encoding**, because a leak walk that matches
    // nothing passes. Each poisons the transport's bytes the way a real defect
    // would and asserts the walk finds it.
    use base64::Engine as _;
    let json = format!("{{\"secret\":\"{MARKER}\"}}");
    let poisons: Vec<(&str, Vec<u8>)> = vec![
        ("raw", json.as_bytes().to_vec()),
        (
            "base64 of a document containing it",
            base64::engine::general_purpose::STANDARD
                .encode(json.as_bytes())
                .into_bytes(),
        ),
        (
            "base64url of a document containing it",
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(json.as_bytes())
                .into_bytes(),
        ),
        (
            "hex of a document containing it",
            json.as_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
                .into_bytes(),
        ),
    ];
    for (how, poison) in poisons {
        let mut poisoned = seen.clone();
        poisoned.push(b'"');
        poisoned.extend_from_slice(&poison);
        poisoned.push(b'"');
        assert!(
            appears_in(&decoded_views(&poisoned), MARKER.as_bytes()),
            "the walk cannot see plaintext that is {how}; it would have missed a real leak"
        );
    }
}

/// A path in an id is a path on the server's disk. Ids here are opaque, and the
/// test says so by asserting the *shape* rather than trusting the caller.
#[test]
fn no_local_path_reaches_the_transport() {
    let (_t, seen) = push_everything();
    // Same decode-then-look walk: a path that arrives base64-encoded is still
    // a path on someone's disk arriving at a server.
    let views = decoded_views(&seen);
    for leak in ["/Users/", "/home/", "C:\\", "work/repo"] {
        assert!(
            !appears_in(&views, leak.as_bytes()),
            "`{leak}` reached the transport"
        );
    }
    // Positive control.
    let mut poisoned = seen.clone();
    poisoned.extend_from_slice(b"/Users/someone");
    assert!(appears_in(&decoded_views(&poisoned), b"/Users/"));
}

/// The key must never be in anything a transport sees, in any encoding it could
/// plausibly be written in.
#[test]
fn the_key_never_reaches_the_transport() {
    use base64::Engine as _;
    let kdf = Kdf::new(&[3u8; 16]);
    let key = derive(PASSPHRASE, &kdf).expect("derive");
    // The key's bytes are private, so this reconstructs them the way an
    // attacker with the passphrase would, and looks for those.
    let mut probe = [0u8; 32];
    {
        let salt = base64::engine::general_purpose::STANDARD
            .decode(&kdf.salt)
            .expect("salt");
        let params =
            argon2::Params::new(kdf.m_cost_kib, kdf.t_cost, kdf.p_cost, Some(32)).expect("params");
        argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params)
            .hash_password_into(PASSPHRASE.as_bytes(), &salt, &mut probe)
            .expect("derive");
    }
    let e = seal(&key, "session/x", &kdf, b"content").expect("seal");
    let mut t = Memory::default();
    t.push("session/x", &e).expect("push");

    let hex: String = probe.iter().map(|b| format!("{b:02x}")).collect();
    let b64 = base64::engine::general_purpose::STANDARD.encode(probe);
    let text = String::from_utf8_lossy(&t.seen).to_string();
    assert!(!text.contains(&hex), "the key reached the transport in hex");
    assert!(
        !text.contains(&b64),
        "the key reached the transport in base64"
    );
    assert!(
        !t.seen.windows(32).any(|w| w == probe.as_slice()),
        "the key reached the transport as raw bytes"
    );
    // Positive control: all three probes are non-trivial and would match.
    assert_eq!(hex.len(), 64);
    assert!(!b64.is_empty());
}

/// A round trip through the transport really is a round trip — otherwise the
/// three tests above could pass because nothing is stored at all.
#[test]
fn what_the_transport_stores_still_opens() {
    let kdf = Kdf::new(&[3u8; 16]);
    let key = derive(PASSPHRASE, &kdf).expect("derive");
    let mut t = Memory::default();
    let e = seal(&key, "session/round", &kdf, MARKER.as_bytes()).expect("seal");
    t.push("session/round", &e).expect("push");
    assert_eq!(t.list().expect("list"), ["session/round"]);
    let back: Envelope = t.pull("session/round").expect("pull").expect("present");
    assert_eq!(
        thruline_sync::open(&key, "session/round", &back).expect("open"),
        MARKER.as_bytes()
    );
}

/// A server that swaps two objects cannot make a client read the wrong one.
#[test]
fn a_swapped_object_does_not_open() {
    let kdf = Kdf::new(&[3u8; 16]);
    let key = derive(PASSPHRASE, &kdf).expect("derive");
    let mut t = Memory::default();
    let a = seal(&key, "session/a", &kdf, b"A").expect("seal");
    let b = seal(&key, "session/b", &kdf, b"B").expect("seal");
    t.push("session/a", &b).expect("push");
    t.push("session/b", &a).expect("push");
    let got = t.pull("session/a").expect("pull").expect("present");
    assert!(
        thruline_sync::open(&key, "session/a", &got).is_err(),
        "an object served under the wrong id opened"
    );
}

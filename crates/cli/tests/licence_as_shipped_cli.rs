//! LICENCE-GATE-ON (P11: FI-34, D-238, D-244) — the licence surfaces of the
//! binary AS SHIPPED: no seam variable set, nothing inherited.
//!
//! `licence_cli.rs` reaches its reference builds — the gate off, no key — by the
//! three debug-only seams, and its `Home::new` is the shipped build with no seam
//! at all. This file is the same claim made a second way, so it cannot be an
//! artifact of a helper: the environment is CLEARED, then given only a HOME, a
//! THRULINE_HOME and a PATH, and the binary is asked what it is.
//!
//! The binary under test is a DEBUG build, so the seams exist in it; a release
//! build has none of them, and that is shown against `cargo build --release`
//! (LICENCE-GATE-ON item 4), not by this file. What this file does hold is the
//! part that needs no seam to see: with NONE set, the paid gate is on, exactly
//! one key is built in, and a licence signed by a key generated at run time — the
//! only kind any test here has — is refused as an unknown key.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use ed25519_dalek::SigningKey;
use rand_core::OsRng;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

/// A throwaway home, and a command with NOTHING inherited: no seam variable can
/// be set because no variable is.
struct Shipped {
    dir: tempfile::TempDir,
}

impl Shipped {
    fn new() -> Shipped {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("h/.thruline")).expect("home");
        std::fs::create_dir_all(dir.path().join("repo")).expect("repo");
        Shipped { dir }
    }
    fn thruline_home(&self) -> PathBuf {
        self.dir.path().join("h/.thruline")
    }
    fn installed(&self) -> PathBuf {
        self.thruline_home().join("licence.txt")
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(bin())
            .args(args)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", self.dir.path().join("h"))
            .env("THRULINE_HOME", self.thruline_home())
            .current_dir(self.dir.path().join("repo"))
            .stdin(Stdio::null())
            .output()
            .expect("run thruline")
    }
    fn status(&self) -> serde_json::Value {
        let o = self.run(&["licence", "status", "--json"]);
        assert!(o.status.success(), "status --json exited {:?}", o.status);
        serde_json::from_slice(&o.stdout).expect("status json")
    }
}

/// A licence signed by a key that exists only in this test, as every licence
/// any test here uses is.
fn test_signed_licence() -> String {
    let signing = SigningKey::generate(&mut OsRng);
    let payload = "{\"format\":1,\"licence_id\":\"01J8ZQ4Y6T3M2N1P0R9S8T7V6W\",\"product\":\"thruline\",\"plan\":\"pro\",\"issued_at\":\"2026-09-01T00:00:00Z\",\"expires_at\":\"2099-01-01T00:00:00Z\"}";
    thruline_licence::seal(payload.as_bytes(), &signing).expect("seal")
}

fn text(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn as_shipped_the_paid_gate_is_on_and_exactly_one_key_is_built_in() {
    let home = Shipped::new();
    let v = home.status();
    assert_eq!(
        v["paid_gate"], true,
        "with no seam variable set the paid gate is off; v0.1.0 ships it on (P11): {v}"
    );
    assert_eq!(
        v["technical"]["keys_built_in"], 1,
        "with no seam variable set the binary does not carry exactly one licence key: {v}"
    );
    // The key is there, so this is not the build that "doesn't check licences".
    assert_ne!(v["status"], "no_key", "{v}");
}

#[test]
fn as_shipped_a_licence_signed_by_a_test_key_is_refused_as_an_unknown_key() {
    let home = Shipped::new();
    let file: &Path = &home.dir.path().join("test-signed.txt");
    std::fs::write(file, test_signed_licence()).expect("write the licence");

    // `licence install` says so, exits non-zero, and keeps nothing.
    let o = home.run(&["licence", "install", &file.to_string_lossy()]);
    assert!(
        !o.status.success(),
        "a licence signed by a test key was installed as shipped: {}",
        text(&o)
    );
    assert!(
        text(&o).starts_with(
            "Not installed. This licence names a maker this version of ThruLine doesn't know, so it can't check it."
        ),
        "{}",
        text(&o)
    );
    assert!(
        !home.installed().exists(),
        "a refused licence was written to the installed path"
    );

    // Placed straight at the installed path, `licence status` says the same in
    // its own field: the slug the screen keys its copy on.
    std::fs::copy(file, home.installed()).expect("place it");
    let v = home.status();
    assert_eq!(v["status"], "unknown_key", "{v}");
    assert!(v["plan"].is_null(), "a refused licence names a plan: {v}");
    assert_eq!(v["technical"]["keys_built_in"], 1, "{v}");
}

#[test]
fn as_shipped_without_a_licence_says_free_and_offers_no_trial() {
    let home = Shipped::new();
    let plain = home.run(&["licence", "status"]);
    assert!(plain.status.success(), "status exited {:?}", plain.status);
    assert!(text(&plain).trim().starts_with("Free"), "{}", text(&plain));
    assert!(!text(&plain).to_ascii_lowercase().contains("trial"));

    let json = home.status();
    assert_eq!(json["headline"], "free", "{json}");
    assert_eq!(json["entitlements"]["approve_version"]["allowed"], false);
    assert!(json.get("trial").is_none(), "obsolete trial field: {json}");
    assert_eq!(
        json["schema_version"], 4,
        "a removed field changes the schema"
    );
    assert!(!json.to_string().to_ascii_lowercase().contains("trial"));
    assert!(!home.thruline_home().join("trial.json").exists());
}

#[test]
fn an_old_trial_file_is_ignored_and_left_untouched_by_status() {
    let home = Shipped::new();
    let before = home.status();
    let old_file = home.thruline_home().join("trial.json");
    let old_bytes =
        br#"{"format":1,"started_on":"2026-10-07","created_at":"2026-10-07T00:00:00Z"}"#;
    std::fs::write(&old_file, old_bytes).expect("place old trial file");

    let plain = home.run(&["licence", "status"]);
    assert!(plain.status.success(), "status exited {:?}", plain.status);
    assert!(text(&plain).trim().starts_with("Free"), "{}", text(&plain));
    assert_eq!(
        home.status(),
        before,
        "old trial changed the licence answer"
    );
    assert_eq!(std::fs::read(&old_file).expect("old file"), old_bytes);
}

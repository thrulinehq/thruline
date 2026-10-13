//! T-DIST-4 — the licence, against the SHIPPING BINARY.
//!
//! Acceptance B (zero network, at process level), D (removal keeps every
//! record) and E (the never-gated census) live here, because every one of them
//! is a claim about what the real `thruline` does, and a claim about a function
//! some test reached directly is this repository's signature defect.
//!
//! Every key pair is generated at run time, in memory. The binary under test is
//! a DEBUG build, so it carries the `cfg(debug_assertions)` test seams: one
//! that lets it trust a key named in `THRULINE_LICENCE_DEV_PUBLIC_KEY`, and two
//! that let a census keep its reference builds now that the shipped build has
//! the paid gate ON and one production key compiled in (P11, FI-34, D-238):
//! `THRULINE_PAID_GATE_DEV=off` and `THRULINE_LICENCE_DEV_NO_BUILT_IN_KEYS=1`.
//! A release build has none of the three, and the coder report proves that
//! against `cargo build --release`. The binary AS SHIPPED, with no seam variable
//! set at all, is `licence_as_shipped_cli.rs`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};

use ed25519_dalek::SigningKey;
use rand_core::OsRng;
use sha2::{Digest, Sha256};

const SEAM: &str = "THRULINE_LICENCE_DEV_PUBLIC_KEY";
/// The paid-gate test seam (R-2026-09-24-25): `off` turns the gate OFF in this
/// DEBUG binary, which is the reference build a census compares against. The
/// gate is ON by default now (P11) and is always off without a key.
const GATE: &str = "THRULINE_PAID_GATE_DEV";
/// The built-in-keys test seam (LICENCE-GATE-ON): `1` makes this DEBUG binary
/// trust none of the keys compiled into it, which is a keyless build.
const NO_BUILT_IN_KEYS: &str = "THRULINE_LICENCE_DEV_NO_BUILT_IN_KEYS";
/// The install test seam (ADV-dist-install-2's read-back): a path the install
/// reads to its end between its rename and its read-back, in this DEBUG binary.
const AFTER_RENAME: &str = "THRULINE_LICENCE_DEV_AFTER_RENAME";

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

/* ── licences, made here ───────────────────────────────────────────────── */

struct Issuer {
    signing: SigningKey,
}

impl Issuer {
    fn new() -> Issuer {
        Issuer {
            signing: SigningKey::generate(&mut OsRng),
        }
    }

    fn public_hex(&self) -> String {
        thruline_licence::to_hex(&self.signing.verifying_key().to_bytes())
    }

    fn licence(&self, product: &str, plan: &str, expires: &str) -> String {
        let payload = format!(
            "{{\"format\":1,\"licence_id\":\"01J8ZQ4Y6T3M2N1P0R9S8T7V6W\",\"product\":\"{product}\",\"plan\":\"{plan}\",\"issued_at\":\"2026-09-01T00:00:00Z\",\"expires_at\":\"{expires}\"}}"
        );
        thruline_licence::seal(payload.as_bytes(), &self.signing).expect("seal")
    }

    fn valid(&self) -> String {
        self.licence("thruline", "pro", "2099-01-01T00:00:00Z")
    }
}

fn tampered(licence: &str) -> String {
    // One byte of the payload flipped: the plan "pro" becomes "prn".
    licence.replacen("\"plan\":\"pro\"", "\"plan\":\"prn\"", 1)
}

/// A throwaway HOME; everything the binary could write lands inside it.
///
/// A `Home` also says which build the binary stands in for. [`Home::new`] is
/// the build as it ships since P11 (FI-34, D-238): the paid gate ON and the one
/// production key built in, with NO seam variable set — so every test written
/// for that build now runs against the defaults, not against a seam that
/// imitates them. The other two are reference builds, and they are asked for by
/// seam: the gate OFF (`THRULINE_PAID_GATE_DEV=off`), and no key built in
/// (`THRULINE_LICENCE_DEV_NO_BUILT_IN_KEYS=1`, which forces the gate off).
struct Home {
    dir: tempfile::TempDir,
    /// Whether this is a build with the gate on, as it ships.
    gate: bool,
    /// Whether this is a build with its production key, as it ships.
    keyed: bool,
}

impl Home {
    fn build(gate: bool, keyed: bool) -> Home {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("h/.thruline")).expect("home");
        std::fs::create_dir_all(dir.path().join("repo")).expect("repo");
        Home { dir, gate, keyed }
    }
    /// The build as it ships: the gate ON, the production key built in.
    fn new() -> Home {
        Home::build(true, true)
    }
    /// The gate OFF, a key built in: the reference build a census compares to.
    fn gate_off() -> Home {
        Home::build(false, true)
    }
    /// The gate on by default, and no key at all: a build that could never
    /// install a licence, so the gate must stay off whatever the flag says.
    fn keyless() -> Home {
        Home::build(true, false)
    }
    fn home(&self) -> PathBuf {
        self.dir.path().join("h")
    }
    fn thruline_home(&self) -> PathBuf {
        self.home().join(".thruline")
    }
    fn installed(&self) -> PathBuf {
        self.thruline_home().join("licence.txt")
    }
    fn trial(&self) -> PathBuf {
        self.thruline_home().join("trial.json")
    }
    fn file(&self, name: &str, text: &str) -> PathBuf {
        let p = self.dir.path().join(name);
        std::fs::write(&p, text).expect("write licence");
        p
    }
    fn env(&self, cmd: &mut Command, key: Option<&str>) {
        cmd.env("HOME", self.home())
            .env("THRULINE_HOME", self.thruline_home())
            .env_remove(SEAM)
            .env_remove(GATE)
            .env_remove(NO_BUILT_IN_KEYS)
            .env_remove(AFTER_RENAME)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("THRULINE_SESSION_ID")
            .current_dir(self.dir.path().join("repo"))
            .stdin(Stdio::null());
        if !self.gate {
            cmd.env(GATE, "off");
        }
        if !self.keyed {
            cmd.env(NO_BUILT_IN_KEYS, "1");
        }
        if let Some(k) = key {
            cmd.env(SEAM, k);
        }
    }
    fn cli(&self, key: Option<&str>, args: &[&str]) -> Output {
        let mut cmd = Command::new(bin());
        cmd.args(args);
        self.env(&mut cmd, key);
        cmd.output().expect("run thruline")
    }
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

/// The renewal sentence: what an expired licence is told to do. Ruling 3
/// (D-298) made an expired paid licence final. The sentence has to carry a
/// person whose subscription lapsed as well as one whose card renews on its
/// own: renew on the licence page, then download and install the new licence.
const EXPIRED_FIX: &str = "Go to the licence page on ThruLine's website with your licence key, renew there if your subscription has ended, then download the new licence and install it.";

/* ── B: zero network, at process level ─────────────────────────────────── */

/// The denial profile. Every network operation — outbound, inbound, bind, a
/// unix-domain connect — is denied AND the process is killed on the attempt,
/// so "the denial layer logged no attempt" is the process's own exit status:
/// a run that reaches its normal exit made none.
#[cfg(target_os = "macos")]
const NO_NETWORK: &str = "(version 1)(allow default)(deny network* (with send-signal SIGKILL))";

#[cfg(target_os = "macos")]
fn under_denial(program: &str, args: &[&str], home: &Home, key: Option<&str>) -> Output {
    let mut cmd = Command::new("/usr/bin/sandbox-exec");
    cmd.arg("-p").arg(NO_NETWORK).arg(program).args(args);
    home.env(&mut cmd, key);
    // Bounded: the positive control's `thruline ui` would otherwise serve forever
    // if the harness failed to stop it — which is exactly the failure to catch.
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("sandbox-exec");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        if child.try_wait().expect("wait").is_some() {
            return child.wait_with_output().expect("output");
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            return child.wait_with_output().expect("output");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[cfg(target_os = "macos")]
fn killed_by_the_denial(o: &Output) -> bool {
    use std::os::unix::process::ExitStatusExt;
    o.status.signal() == Some(9)
}

#[cfg(target_os = "macos")]
#[test]
fn b_the_denial_harness_catches_a_real_network_attempt() {
    let home = Home::new();
    // Positive control 1: a plain connect to 127.0.0.1:9.
    let nc = under_denial(
        "/usr/bin/nc",
        &["-z", "-w", "1", "127.0.0.1", "9"],
        &home,
        None,
    );
    assert!(
        killed_by_the_denial(&nc),
        "the harness did not catch a connect to 127.0.0.1:9 (status {:?}); a zero-network \
         result under it would prove nothing",
        nc.status
    );
    // Positive control 2: THIS binary, doing something that opens a socket.
    let ui = under_denial(
        bin(),
        &["ui", "--no-open", "--port", "0", "--dir", "export"],
        &home,
        None,
    );
    assert!(
        killed_by_the_denial(&ui),
        "the harness did not catch `thruline ui` binding a socket (status {:?})",
        ui.status
    );
    // Negative control: the same harness lets a process that opens nothing finish.
    let echo = under_denial("/bin/echo", &["quiet"], &home, None);
    assert!(echo.status.success(), "{:?}", echo.status);
}

#[cfg(target_os = "macos")]
#[test]
fn b_status_and_install_open_no_socket_for_every_kind_of_licence() {
    let issuer = Issuer::new();
    let stranger = Issuer::new();
    let key = issuer.public_hex();
    let cases: Vec<(&str, Option<String>, &str, &str)> = vec![
        (
            "absent",
            None,
            "Free",
            "Not installed. There is no file at that path.",
        ),
        (
            "valid",
            Some(issuer.valid()),
            "This machine has a ThruLine Pro licence, valid until 1 Jan 2099.",
            "Installed. This machine has a ThruLine Pro licence, valid until 1 Jan 2099.",
        ),
        (
            "expired",
            Some(issuer.licence("thruline", "pro", "2020-01-01T00:00:00Z")),
            "This licence expired on 1 Jan 2020.",
            "Not installed. This licence expired on 1 Jan 2020. Go to the licence page on ThruLine's website with your licence key, renew there if your subscription has ended, then download the new licence and install it.",
        ),
        (
            "tampered",
            Some(tampered(&issuer.valid())),
            "This licence has been changed, or wasn't made by the maker it names, so it can't be trusted.",
            "Not installed. This licence has been changed, or wasn't made by the maker it names, so it can't be trusted. Download it again from the licence page on ThruLine's website, with your licence key.",
        ),
        (
            "wrong product",
            Some(issuer.licence("otherthing", "pro", "2099-01-01T00:00:00Z")),
            "This licence is for a different product, not ThruLine.",
            "Not installed. This licence is for a different product, not ThruLine.",
        ),
        (
            "wrong key",
            Some(stranger.valid()),
            "This licence names a maker this version of ThruLine doesn't know, so it can't check it.",
            "Not installed. This licence names a maker this version of ThruLine doesn't know, so it can't check it. Update ThruLine, then install the licence again; if it's still refused, download it again from the licence page on ThruLine's website, with your licence key.",
        ),
    ];
    for (name, licence, status_sentence, install_sentence) in cases {
        // `licence status`, with the file placed where an install would put it.
        let home = Home::new();
        if let Some(text) = &licence {
            std::fs::write(home.installed(), text).expect("place");
        }
        let o = under_denial(bin(), &["licence", "status"], &home, Some(&key));
        assert!(
            !killed_by_the_denial(&o),
            "{name}: `licence status` attempted a network operation"
        );
        assert!(o.status.success(), "{name}: status exited {:?}", o.status);
        if name == "absent" {
            assert!(
                stdout(&o).trim().starts_with(status_sentence),
                "{name}: status: {}",
                stdout(&o)
            );
            assert!(!stdout(&o).to_ascii_lowercase().contains("trial"));
        } else {
            assert_eq!(stdout(&o).trim(), status_sentence, "{name}: status");
        }

        // `licence install <file>`, into an empty home.
        let home = Home::new();
        let path = match &licence {
            Some(text) => home.file("licence-from-email.txt", text),
            None => home.dir.path().join("no-such-file.txt"),
        };
        let path = path.to_string_lossy().into_owned();
        let o = under_denial(bin(), &["licence", "install", &path], &home, Some(&key));
        assert!(
            !killed_by_the_denial(&o),
            "{name}: `licence install` attempted a network operation"
        );
        assert!(
            stdout(&o).trim().starts_with(install_sentence),
            "{name}: install said {:?}",
            stdout(&o)
        );
        assert_eq!(
            home.installed().exists(),
            name == "valid",
            "{name}: only a licence that works is ever installed"
        );
    }

    // A leftover v0.1.0 file is inert even under the no-network profile.
    let home = Home::new();
    let old = br#"{"format":1,"started_on":"2099-01-01","created_at":"2026-10-07T00:00:00Z"}"#;
    std::fs::write(home.trial(), old).expect("place old trial");
    for args in [
        &["licence", "status"][..],
        &["licence", "status", "--json"][..],
    ] {
        let o = under_denial(bin(), args, &home, Some(&key));
        assert!(
            !killed_by_the_denial(&o),
            "`{}` attempted a network operation",
            args.join(" ")
        );
        assert!(o.status.success(), "{}: {:?}", args.join(" "), o.status);
        assert!(
            !stdout(&o).to_ascii_lowercase().contains("trial"),
            "{}",
            stdout(&o)
        );
    }
    assert_eq!(std::fs::read(home.trial()).expect("old trial"), old);
}

#[cfg(not(target_os = "macos"))]
#[test]
fn b_zero_network_is_proven_on_macos_only_in_this_build() {
    // Said, not silently passed: the Linux arm (`unshare -rn` or seccomp) is
    // not written. This test prints so a log reader cannot mistake it for B.
    eprintln!(
        "NOT RUN: T-DIST-4 acceptance B (process-level zero network) runs under \
         macOS sandbox-exec only; this host is not macOS"
    );
}

/* ── install never replaces a working licence with one that doesn't ───── */

#[test]
fn install_refuses_a_bad_licence_and_keeps_the_good_one() {
    let issuer = Issuer::new();
    let key = issuer.public_hex();
    let home = Home::new();
    let good = home.file("good.txt", &issuer.valid());
    let o = home.cli(Some(&key), &["licence", "install", &good.to_string_lossy()]);
    assert!(o.status.success(), "{}", stdout(&o));
    let before = std::fs::read(home.installed()).expect("installed");

    let bad = home.file("bad.txt", &tampered(&issuer.valid()));
    let o = home.cli(Some(&key), &["license", "install", &bad.to_string_lossy()]);
    assert!(!o.status.success(), "a tampered licence was installed");
    assert_eq!(
        std::fs::read(home.installed()).expect("still installed"),
        before,
        "a refused install touched the licence that works"
    );
    let o = home.cli(Some(&key), &["licence", "status"]);
    assert!(stdout(&o).starts_with("This machine has a ThruLine Pro licence"));
}

/* ── the bytes checked are the bytes installed (ADV-dist-install-1) ────── */

/// A source whose bytes change between two reads: a FIFO that gives a licence
/// that works on its first open and a tampered one on its second — the
/// adversary's reproduction (`d_install_toctou.sh`). An install that reads the
/// file twice verifies the first and installs the second, replacing a licence
/// that works with one that does not, while saying "Installed.".
///
/// The founder's three assertions, each by name: ONE READ (the install opened
/// its source once — the FIFO's second text was still waiting for a reader
/// when the install exited), ONE INSTALL (one line, "Installed.", exit 0, the
/// licence in place changed from the one there before, and no staging file
/// left beside it), and THE SAME BYTES (what is in place is byte for byte what
/// the one read gave). A working Team licence is in place first, so an install
/// that wrote nothing cannot pass for one that wrote the right bytes.
#[cfg(unix)]
#[test]
fn install_writes_exactly_the_bytes_it_verified() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    let issuer = Issuer::new();
    let key = issuer.public_hex();
    let home = Home::new();
    let before = issuer.licence("thruline", "team", "2099-01-01T00:00:00Z");
    let placed = home.file("team.txt", &before);
    let o = home.cli(
        Some(&key),
        &["licence", "install", &placed.to_string_lossy()],
    );
    assert!(o.status.success(), "{}", stdout(&o));
    assert_eq!(
        std::fs::read_to_string(home.installed()).expect("installed"),
        before
    );

    let good = issuer.valid();
    let fifo = home.dir.path().join("download.txt");
    let made = Command::new("mkfifo").arg(&fifo).status().expect("mkfifo");
    assert!(made.success(), "mkfifo failed");
    let feed = [good.clone(), tampered(&good)];
    let fed = Arc::new(AtomicUsize::new(0));
    let writer = {
        let fifo = fifo.clone();
        let fed = Arc::clone(&fed);
        std::thread::spawn(move || {
            for text in feed {
                // Opening a FIFO to write waits for a reader: one text per open.
                std::fs::write(&fifo, text).expect("feed the fifo");
                fed.fetch_add(1, Ordering::SeqCst);
                // Let the reader see the end of the file and close it first.
                std::thread::sleep(Duration::from_millis(500));
            }
        })
    };
    let o = home.cli(Some(&key), &["licence", "install", &fifo.to_string_lossy()]);
    // Give the writer time to settle: finished (both texts read) or blocked on
    // the second open, waiting for a reader.
    let waited = Instant::now();
    while !writer.is_finished() && waited.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(20));
    }
    let opened_by_install = fed.load(Ordering::SeqCst);
    // An install that opens its source once leaves the second text waiting for
    // a reader that never comes: be that reader, so the writer can finish.
    if opened_by_install < 2 {
        let mut rest = Vec::new();
        std::fs::File::open(&fifo)
            .and_then(|mut f| f.read_to_end(&mut rest))
            .expect("drain the fifo");
    }
    writer.join().expect("writer");

    // ONE READ.
    assert_eq!(
        opened_by_install, 1,
        "one read: the install opened its source {opened_by_install} times"
    );
    // ONE INSTALL.
    assert!(o.status.success(), "one install: {}", stdout(&o));
    assert_eq!(
        stdout(&o).lines().collect::<Vec<_>>(),
        ["Installed. This machine has a ThruLine Pro licence, valid until 1 Jan 2099."],
        "one install: one line, and it says what was installed"
    );
    let in_place = std::fs::read_to_string(home.installed()).expect("installed");
    assert_ne!(
        in_place, before,
        "one install: the licence in place is still the one from before"
    );
    let left: Vec<String> = std::fs::read_dir(home.thruline_home())
        .expect("the ThruLine folder")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("licence.txt."))
        .collect();
    assert!(left.is_empty(), "one install: left behind: {left:?}");
    // THE SAME BYTES.
    assert_eq!(
        in_place, good,
        "the same bytes: the installed bytes are not the bytes the one read gave"
    );
    let o = home.cli(Some(&key), &["licence", "status"]);
    assert_eq!(
        stdout(&o).trim(),
        "This machine has a ThruLine Pro licence, valid until 1 Jan 2099."
    );
}

/* ── two installs at once (ADV-dist-install-2) ─────────────────────────── */

/// What an install says when the licence it installed was replaced or removed
/// by something else at the same moment, so it is not the one in place.
const REPLACED: &str = "This licence was installed, and at the same moment something else replaced or removed it. Run `thruline licence status` to see which licence this machine has now, and install this one again if it is the one you want.";

/// Two `licence install` runs at the same moment, both of licences that work —
/// the adversary's race (`e_concurrent_install.sh`: in 150 races, 7 left a
/// mixture of the two as the installed file while one command said
/// "Installed.", and 38 ended in a raw "No such file or directory"). The Pro
/// licence carries a 4000-character comment line, as the adversary's did, so
/// the two writes overlap. Twenty rounds.
#[test]
fn two_installs_at_once_leave_one_whole_licence_and_say_what_happened() {
    let issuer = Issuer::new();
    let key = issuer.public_hex();
    let home = Home::new();
    let pro = issuer
        .valid()
        .replacen('\n', &format!("\n# {}\n", "x".repeat(4000)), 1);
    let team = issuer.licence("thruline", "team", "2099-01-01T00:00:00Z");
    let sources = [
        (
            home.file("pro.txt", &pro),
            pro.clone(),
            "ThruLine Pro licence",
        ),
        // Ruling 2 (D-298): a dormant Team licence is named by no plan.
        (
            home.file("team.txt", &team),
            team.clone(),
            "ThruLine licence",
        ),
    ];
    let start = |file: &Path| {
        let mut cmd = Command::new(bin());
        cmd.args(["licence", "install", &file.to_string_lossy()]);
        home.env(&mut cmd, Some(&key));
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start an install")
    };
    for round in 1..=20 {
        let _ = std::fs::remove_file(home.installed());
        let running: Vec<Child> = sources.iter().map(|(file, _, _)| start(file)).collect();
        let outputs: Vec<Output> = running
            .into_iter()
            .map(|c| c.wait_with_output().expect("wait for an install"))
            .collect();
        let installed = std::fs::read_to_string(home.installed())
            .unwrap_or_else(|e| panic!("round {round}: no licence is installed: {e}"));
        assert!(
            sources.iter().any(|(_, text, _)| *text == installed),
            "round {round}: the installed file is neither licence ({} bytes)",
            installed.len()
        );
        let mut in_place_said_installed = false;
        for ((_, text, plan), o) in sources.iter().zip(&outputs) {
            assert!(
                o.stderr.is_empty(),
                "round {round}: {plan}: {}",
                String::from_utf8_lossy(&o.stderr)
            );
            let said = stdout(o);
            let installed_line =
                format!("Installed. This machine has a {plan}, valid until 1 Jan 2099.");
            if said.trim() == installed_line {
                assert!(o.status.success(), "round {round}: {plan}: {said}");
                in_place_said_installed |= *text == installed;
            } else {
                assert_eq!(said.trim(), REPLACED, "round {round}: {plan}");
                assert!(!o.status.success(), "round {round}: {plan}: {said}");
            }
        }
        assert!(
            in_place_said_installed,
            "round {round}: the licence in place is not one a command said it installed"
        );
        let left: Vec<String> = std::fs::read_dir(home.thruline_home())
            .expect("the ThruLine folder")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("licence.txt."))
            .collect();
        assert!(left.is_empty(), "round {round}: left behind: {left:?}");
    }
}

/// The check after the write, held to a test: "Installed." is said only of
/// the licence that is in place. The race above cannot reach it — the moment
/// between an install's rename and its read-back is too short to aim at from
/// outside, and with the comparison removed that test still passed 3 of 3
/// runs. So the install is stopped at exactly that moment, through the
/// debug-only seam [`AFTER_RENAME`] pointed at a FIFO: the install opens it
/// only after its rename, and reads it to the end before its read-back. While
/// it waits, a second command runs to completion — another install, then, in
/// a second round, `licence remove` — and only then is the FIFO closed. No step
/// depends on timing: each one waits for the one before it.
#[cfg(unix)]
#[test]
fn an_install_whose_licence_is_replaced_before_its_read_back_says_so_and_fails() {
    use std::sync::mpsc;
    use std::time::Duration;
    let issuer = Issuer::new();
    let key = issuer.public_hex();
    let home = Home::new();
    let pro = issuer.valid();
    let team = issuer.licence("thruline", "team", "2099-01-01T00:00:00Z");
    let pro_file = home.file("pro.txt", &pro);
    let team_file = home.file("team.txt", &team);
    let seam = home.dir.path().join("after-rename");
    let made = Command::new("mkfifo").arg(&seam).status().expect("mkfifo");
    assert!(made.success(), "mkfifo failed");

    // Install Pro, held between its rename and its read-back while `other`
    // runs to completion; then let it finish. (the held install, the other)
    let held_while = |other: &[&str]| -> (Output, Output) {
        let mut cmd = Command::new(bin());
        cmd.args(["licence", "install", &pro_file.to_string_lossy()]);
        home.env(&mut cmd, Some(&key));
        cmd.env(AFTER_RENAME, &seam);
        let mut held = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start the install");
        // Opening a FIFO to write waits for its reader, and the install
        // becomes that reader only after its rename.
        let (tx, rx) = mpsc::channel();
        let opener = {
            let seam = seam.clone();
            std::thread::spawn(move || {
                let _ = tx.send(std::fs::OpenOptions::new().write(true).open(&seam));
            })
        };
        let door = loop {
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(door) => break door.expect("open the seam to write"),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if held.try_wait().expect("poll the install").is_some() {
                        // It ended without reaching the seam. Be the reader the
                        // opener waits for, then say what the install said.
                        let _ = std::fs::File::open(&seam);
                        let _ = opener.join();
                        let o = held.wait_with_output().expect("the install's output");
                        panic!(
                            "the install ended without stopping between its rename and its read-back: {}{}",
                            stdout(&o),
                            String::from_utf8_lossy(&o.stderr)
                        );
                    }
                }
                Err(e) => panic!("the seam's opener: {e}"),
            }
        };
        opener.join().expect("the seam's opener");
        assert_eq!(
            std::fs::read_to_string(home.installed()).expect("a licence in place while held"),
            pro,
            "the install is held after its rename, so its licence is the one in place"
        );
        let other = home.cli(Some(&key), other);
        // Closing the only writer ends the install's read of the seam; it then
        // reads its licence back.
        drop(door);
        (
            held.wait_with_output().expect("the install's output"),
            other,
        )
    };
    let staging_left = || -> Vec<String> {
        std::fs::read_dir(home.thruline_home())
            .expect("the ThruLine folder")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("licence.txt."))
            .collect()
    };

    // REPLACED: another install lands between the rename and the read-back.
    let (held, other) = held_while(&["licence", "install", &team_file.to_string_lossy()]);
    assert_eq!(
        stdout(&other).lines().collect::<Vec<_>>(),
        ["Installed. This machine has a ThruLine licence, valid until 1 Jan 2099."],
        "the other install"
    );
    assert!(
        other.status.success(),
        "the other install: {}",
        stdout(&other)
    );
    assert_eq!(
        stdout(&held).lines().collect::<Vec<_>>(),
        [REPLACED],
        "replaced: the held install must say its licence was replaced, never \"Installed.\""
    );
    assert_eq!(held.status.code(), Some(1), "replaced: {}", stdout(&held));
    assert!(
        held.stderr.is_empty(),
        "replaced: {}",
        String::from_utf8_lossy(&held.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(home.installed()).expect("installed"),
        team,
        "replaced: the licence in place is the other install's"
    );
    let o = home.cli(Some(&key), &["licence", "status"]);
    assert_eq!(
        stdout(&o).trim(),
        "This machine has a ThruLine licence, valid until 1 Jan 2099."
    );
    let left = staging_left();
    assert!(left.is_empty(), "replaced: left behind: {left:?}");

    // REMOVED: `licence remove` lands between the rename and the read-back.
    let (held, other) = held_while(&["licence", "remove"]);
    assert_eq!(
        stdout(&other).trim(),
        "Removed the licence. Every record ThruLine keeps is untouched.",
        "the remove"
    );
    assert!(other.status.success(), "the remove: {}", stdout(&other));
    assert_eq!(
        stdout(&held).lines().collect::<Vec<_>>(),
        [REPLACED],
        "removed: the held install must say its licence was removed, never \"Installed.\""
    );
    assert_eq!(held.status.code(), Some(1), "removed: {}", stdout(&held));
    assert!(
        held.stderr.is_empty(),
        "removed: {}",
        String::from_utf8_lossy(&held.stderr)
    );
    assert!(
        !home.installed().exists(),
        "removed: a licence is in place after the remove"
    );
    let left = staging_left();
    assert!(left.is_empty(), "removed: left behind: {left:?}");
}

/* ── the licence and its folder are owner-only (ADV-dist-install-5) ────── */

/// On a fresh home the licence may be the first thing ThruLine writes. A
/// licence tied to one machine carries that machine's fingerprint, which the
/// product shows on this machine's own terminal only — so the folder made for
/// it is 0700 and the file 0600, whatever the umask. `umask 022`, the one most
/// machines run with, is set for the child alone, so the result does not
/// depend on the test runner's.
#[cfg(unix)]
#[test]
fn install_makes_the_folder_and_the_licence_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let issuer = Issuer::new();
    let key = issuer.public_hex();
    let home = Home::new();
    std::fs::remove_dir(home.thruline_home()).expect("a fresh home has no ThruLine folder");
    let placed = home.file("good.txt", &issuer.valid());
    let install = || {
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", "umask 022 && exec \"$0\" \"$@\"", bin()])
            .args(["licence", "install", &placed.to_string_lossy()]);
        home.env(&mut cmd, Some(&key));
        let o = cmd.output().expect("run thruline");
        assert!(o.status.success(), "{}", stdout(&o));
    };
    // In octal, as `ls -l` and `chmod` speak it, so a failure reads as a mode.
    let mode = |p: &Path| {
        let bits = std::fs::metadata(p).expect("metadata").permissions().mode() & 0o777;
        format!("{bits:o}")
    };
    install();
    assert_eq!(
        (mode(&home.thruline_home()), mode(&home.installed())),
        ("700".to_string(), "600".to_string()),
        "(the ThruLine folder, the licence)"
    );

    // A licence an earlier build left readable by others is replaced by one
    // that is not.
    std::fs::set_permissions(home.installed(), std::fs::Permissions::from_mode(0o644))
        .expect("loosen");
    install();
    assert_eq!(
        mode(&home.installed()),
        "600",
        "the licence, installed again"
    );
}

#[test]
fn no_key_means_the_binary_says_so_and_treats_the_licence_as_absent() {
    let issuer = Issuer::new();
    // The gate is on by default, and no key is built in: it must stay off.
    let home = Home::keyless();
    std::fs::write(home.installed(), issuer.valid()).expect("place");
    // No seam key, and the built-in one removed by seam: this debug binary
    // trusts nothing, as a scratch build made without the production key does
    // (the shipped build carries it since P11).
    let o = home.cli(None, &["licence", "status"]);
    assert_eq!(
        stdout(&o).trim(),
        "This copy of ThruLine doesn't check licences, and nothing in it needs one."
    );
    let o = home.cli(None, &["licence", "status", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).expect("json");
    // R-2026-09-24-25 / report §08: a build without a licence key could never
    // install one, so its gate is off whatever the flag asks — approving is
    // open, and no licence state is counted.
    assert_eq!(v["paid_gate"], false);
    assert_eq!(v["entitlements"]["approve_version"]["allowed"], true);
    assert!(v.get("trial").is_none(), "{v}");
    assert_eq!(v["technical"]["keys_built_in"], 0);
    assert!(
        !String::from_utf8_lossy(&o.stdout).contains(&home.home().to_string_lossy().to_string()),
        "the status document carries a path"
    );
}

/// ADV-dist-copy-3: a build with no licence key built in — so the paid gate is
/// forced OFF — says what is true and nothing alarming. Before P11 that was the
/// build that shipped; now it is a scratch build only. It never tells anyone to
/// use a licence, because nothing in it needs one, so there is nothing wrong
/// and nothing to do.
#[test]
fn a_keyless_build_says_plainly_that_nothing_in_it_needs_a_licence() {
    const SENTENCE: &str =
        "This copy of ThruLine doesn't check licences, and nothing in it needs one.";
    const NOTHING_TO_DO: &str = "You don't need to do anything.";
    let alarming = |text: &str| {
        ["official release", "built without", "needs a look"]
            .iter()
            .find(|word| text.contains(**word))
            .map(|word| word.to_string())
    };
    // The gate off and no key at all, both asked for by seam: the build every
    // licence surface used to be written for.
    let home = Home::build(false, false);

    let o = home.cli(None, &["licence", "status"]);
    assert!(o.status.success(), "status exited {:?}", o.status);
    assert_eq!(stdout(&o).trim(), SENTENCE);
    assert_eq!(alarming(&stdout(&o)), None, "{}", stdout(&o));

    let o = home.cli(None, &["licence", "status", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).expect("json");
    assert_eq!(v["status"], "no_key");
    assert_eq!(v["sentence"], SENTENCE);
    assert_eq!(v["enable"], NOTHING_TO_DO);
    assert_eq!(v["paid_gate"], false);
    assert_eq!(v["entitlements"]["approve_version"]["allowed"], true);
    assert_eq!(alarming(&stdout(&o)), None, "{}", stdout(&o));

    // A licence that would work in a build with a key: this one cannot check
    // it, says so in the same words, and keeps nothing.
    let file = home.file("licence.txt", &Issuer::new().valid());
    let o = home.cli(None, &["licence", "install", &file.to_string_lossy()]);
    assert_eq!(
        stdout(&o).trim(),
        format!("Not installed. {SENTENCE} {NOTHING_TO_DO}")
    );
    assert_eq!(alarming(&stdout(&o)), None, "{}", stdout(&o));
    assert!(
        !home.installed().exists(),
        "a licence it cannot check was kept"
    );
    assert!(
        !home.trial().exists(),
        "a trial was counted in a build that gates nothing"
    );
}

/* ── D: removal keeps every record ─────────────────────────────────────── */

fn core_fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../core/tests/fixtures")
        .join(name)
}

fn private(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
    }
}

fn row_counts(db: &Path) -> Vec<(String, i64)> {
    let conn =
        rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("open read-only");
    let mut names: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .expect("prepare")
        .query_map([], |r| r.get(0))
        .expect("tables")
        .map(|r| r.expect("name"))
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|t| {
            let n: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM \"{t}\""), [], |r| r.get(0))
                .expect("count");
            (t, n)
        })
        .collect()
}

fn sha256_of(path: &Path) -> String {
    let bytes = std::fs::read(path).expect("read ledger");
    thruline_licence::to_hex(&Sha256::digest(&bytes))
}

#[test]
fn d_removing_the_licence_keeps_every_record() {
    let issuer = Issuer::new();
    let key = issuer.public_hex();
    let home = Home::new();
    let db = home.thruline_home().join("ledger.db");
    let db_arg = db.to_string_lossy().into_owned();

    // Real records: the core's synthetic Claude Code fixture, ingested.
    let root = home.dir.path().join("projects");
    let project = root.join("-Users-dev-demo");
    std::fs::create_dir_all(&project).expect("mkdir");
    std::fs::copy(
        core_fixture("claude_code_sessions.jsonl"),
        project.join("cccccccc-0000-4000-8000-000000000001.jsonl"),
    )
    .expect("copy fixture");
    let mirrors = home.thruline_home().join("mirrors");
    std::fs::create_dir_all(&mirrors).expect("mirrors");
    private(&mirrors);
    private(&home.thruline_home());
    let o = home.cli(
        None,
        &[
            "--db",
            &db_arg,
            "ingest",
            "--apply",
            "--source",
            "claude-code",
            "--mirror-root",
            &mirrors.to_string_lossy(),
            &root.to_string_lossy(),
        ],
    );
    assert!(
        o.status.success(),
        "ingest: {}",
        String::from_utf8_lossy(&o.stderr)
    );

    let licence = home.file("licence.txt", &issuer.valid());
    let o = home.cli(
        Some(&key),
        &["licence", "install", &licence.to_string_lossy()],
    );
    assert!(o.status.success(), "install: {}", stdout(&o));

    let verify = |label: &str| {
        let o = home.cli(Some(&key), &["--db", &db_arg, "verify"]);
        assert!(o.status.success(), "{label}: verify failed: {}", stdout(&o));
        // Everything `verify` says, except how long it took saying it.
        stdout(&o)
            .lines()
            .filter(|l| !l.starts_with("elapsed"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let verify_before = verify("before");
    let rows_before = row_counts(&db);
    let hash_before = sha256_of(&db);
    assert!(
        rows_before.iter().any(|(t, n)| t == "events" && *n > 0),
        "the fixture put no events in the ledger, so this test would prove nothing: {rows_before:?}"
    );

    let o = home.cli(Some(&key), &["licence", "remove"]);
    assert!(o.status.success());
    assert!(
        !home.installed().exists(),
        "the licence file is still there"
    );

    assert_eq!(
        row_counts(&db),
        rows_before,
        "removing the licence changed a row count"
    );
    assert_eq!(
        sha256_of(&db),
        hash_before,
        "removing the licence changed the ledger's bytes"
    );
    assert_eq!(
        verify("after"),
        verify_before,
        "`verify` says something different"
    );
    assert!(db.exists());
}

/* ── E: the never-gated census ─────────────────────────────────────────── */

fn cli_src() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn rust_sources(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
    for entry in std::fs::read_dir(dir).expect("read_dir").flatten() {
        let p = entry.path();
        if p.is_dir() {
            rust_sources(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            let text = std::fs::read_to_string(&p).expect("read");
            out.push((p, text));
        }
    }
}

/// Every concrete address the UI server answers, derived from its source:
/// exact `"/api/…"` literals, and every `p.starts_with(A) [&& p.ends_with(B)]
/// [&& p.contains(C)]` arm rendered as a real path.
fn server_routes() -> Vec<String> {
    let mut sources = Vec::new();
    rust_sources(&cli_src().join("uiserve"), &mut sources);
    sources.push((
        cli_src().join("uiserve.rs"),
        std::fs::read_to_string(cli_src().join("uiserve.rs")).expect("uiserve.rs"),
    ));
    let lit = |s: &str| -> Option<String> {
        let s = s.strip_prefix('"')?;
        Some(s[..s.find('"')?].to_string())
    };
    let mut routes = std::collections::BTreeSet::new();
    for (_, text) in &sources {
        // Guarded arms.
        let mut rest = text.as_str();
        while let Some(at) = rest.find("p.starts_with(") {
            let after = &rest[at + "p.starts_with(".len()..];
            let arm_end = after.find("=>").unwrap_or(after.len());
            let arm = &after[..arm_end];
            if let Some(prefix) = lit(arm) {
                if prefix.starts_with("/api/") {
                    let find =
                        |call: &str| arm.find(call).and_then(|i| lit(&arm[i + call.len()..]));
                    let mut path = format!("{prefix}census");
                    if let Some(mid) = find("p.contains(") {
                        path.push_str(&format!("{mid}run-census"));
                    }
                    if let Some(suffix) = find("p.ends_with(") {
                        path.push_str(&suffix);
                    }
                    routes.insert(path);
                }
            }
            rest = &after[arm_end.min(after.len())..];
        }
        // Exact literals.
        for piece in text.split('"').skip(1).step_by(2) {
            if piece.starts_with("/api/") && !piece.ends_with('/') && !piece.contains(' ') {
                routes.insert(piece.to_string());
            }
        }
    }
    routes.into_iter().collect()
}

struct Server {
    child: Child,
    port: u16,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Server {
    fn start(home: &Home, key: Option<&str>) -> Server {
        Server::start_on(home, key, None)
    }

    /// `thruline [--db <db>] ui`: the ledger the server reads is `db` when one
    /// is given, so a test can put it outside the ThruLine home and then make
    /// that home unwritable without taking the record with it.
    fn start_on(home: &Home, key: Option<&str>, db: Option<&Path>) -> Server {
        let mut cmd = Command::new(bin());
        if let Some(db) = db {
            cmd.arg("--db").arg(db);
        }
        cmd.args(["ui", "--no-open", "--port", "0", "--dir"])
            .arg(home.dir.path().join("export"));
        home.env(&mut cmd, key);
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("thruline ui");
        let out = child.stdout.take().expect("stdout");
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines() {
                let Ok(line) = line else { return };
                if tx.send(line).is_err() {
                    return;
                }
            }
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            match rx.recv_timeout(left) {
                Ok(line) => {
                    if let Some(rest) = line.trim().strip_prefix("ThruLine is at http://127.0.0.1:")
                    {
                        let port = rest
                            .split('/')
                            .next()
                            .and_then(|p| p.trim().parse().ok())
                            .expect("port");
                        return Server { child, port };
                    }
                }
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("`thruline ui` never printed its address");
                }
            }
        }
    }

    fn send(&self, method: &str, path: &str) -> (u16, String) {
        self.send_body(method, path, r#"{"actor":"census","seen_version":""}"#)
    }

    fn send_body(&self, method: &str, path: &str, body: &str) -> (u16, String) {
        let mut socket = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(120)))
            .expect("timeout");
        let request = if method == "POST" {
            format!(
                "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{p}\r\nOrigin: http://127.0.0.1:{p}\r\nContent-Type: application/json\r\nContent-Length: {n}\r\nConnection: close\r\n\r\n{body}",
                p = self.port,
                n = body.len()
            )
        } else {
            format!(
                "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
                self.port
            )
        };
        socket.write_all(request.as_bytes()).expect("request");
        let mut raw = Vec::new();
        let _ = socket.read_to_end(&mut raw);
        let text = String::from_utf8_lossy(&raw).into_owned();
        let status = text
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        (status, text)
    }
}

const APPROVE: &str = "/api/changes/census/decisions";

/// THE TABLE — every surface that refuses for want of a licence when the paid
/// gate is ON, as `(method, address, PAID slug)`. It is the report's block
/// list for what is built today (§08: "Free refuses exactly one act: recording
/// an approval of a version"), and nothing else. A gated write is added here by
/// name, in the same commit as its CLAUDE.md write-address row and its
/// `thruline_licence::PAID` row; the row count below is pinned so that a
/// second row cannot arrive unannounced. No CLI verb is in it: no terminal door
/// approves today.
const PAID_SURFACES: &[(&str, &str, &str)] = &[("POST", APPROVE, "approve_version")];

/// Walk every route the UI server answers, GET and POST, and list the ones
/// refused for want of a licence, as `METHOD address (status)` — and, beside
/// them, every answer's status as `(METHOD address, status)`, in walk order.
fn licence_refusals(server: &Server, routes: &[String]) -> (Vec<String>, Vec<(String, u16)>) {
    let mut refused = Vec::new();
    let mut statuses = Vec::new();
    for route in routes {
        for method in ["GET", "POST"] {
            let (status, text) = server.send(method, route);
            if text.contains("licence_required") {
                refused.push(format!("{method} {route} ({status})"));
            }
            statuses.push((format!("{method} {route}"), status));
        }
    }
    (refused, statuses)
}

fn licence_doc(server: &Server) -> serde_json::Value {
    let (status, text) = server.send("GET", "/api/licence");
    assert_eq!(status, 200, "{text}");
    let body = &text[text.find("\r\n\r\n").expect("body") + 4..];
    serde_json::from_str(body).expect("json")
}

#[test]
fn e_every_server_route_answers_unlicensed_and_only_approving_is_refused() {
    // The table is the report's list, one row, and it is the licence crate's
    // own table: the two cannot drift apart silently.
    assert_eq!(
        PAID_SURFACES.len(),
        1,
        "the paid-surface table's row count changed. Report §08 names exactly \
         one act today (recording an approval of a version); a new row needs \
         the founder's ruling on the paid set (D-3) and its own write-address \
         row, and this number moves in the same commit."
    );
    assert_eq!(
        PAID_SURFACES
            .iter()
            .map(|(_, _, slug)| *slug)
            .collect::<Vec<_>>(),
        thruline_licence::PAID
            .iter()
            .map(|(s, _)| s.slug())
            .collect::<Vec<_>>(),
        "this table and thruline_licence::PAID name different surfaces"
    );
    // The flag ships ON (P11: FI-34, D-238 — "don't ship v0.1.0 with the paid
    // gate disabled"; it was OFF under R-2026-09-24-25 until then). Read at run
    // time, so a build that ships it off fails THIS test by name instead of
    // failing to compile this whole file.
    assert!(
        std::hint::black_box(thruline_licence::PAID_GATE),
        "the paid gate ships OFF, and P11 (FI-34, D-238) says v0.1.0 does not"
    );

    let routes = server_routes();
    // The walk is live: it found the routes this card names, by their shapes.
    for must in [
        "/api/licence",
        "/api/storage",
        "/api/explain",
        "/api/claims",
        "/api/changes",
        APPROVE,
        "/api/changes/census/decisions/withdraw",
        "/api/changes/census/runs/run-census/cancel",
        "/api/changes/census/links",
        "/api/changes/census/associations",
        "/api/changes/census/coverage",
    ] {
        assert!(
            routes.iter().any(|r| r == must),
            "the route walk missed {must}: {routes:?}"
        );
    }
    assert!(
        routes.len() >= 25,
        "the route walk found only {}: {routes:?}",
        routes.len()
    );
    for (method, address, _) in PAID_SURFACES {
        assert!(
            *method == "POST" && routes.iter().any(|r| r == address),
            "a table row names an address the server does not answer: {method} {address}"
        );
    }

    // Three builds, each with no licence installed.
    //   gate OFF, key built in   → nothing refuses (the reference build)
    //   no key built in          → nothing refuses (a keyless build is off)
    //   gate ON, key built in    → exactly the table refuses (as it ships)
    let table: Vec<String> = PAID_SURFACES
        .iter()
        .map(|(m, a, _)| format!("{m} {a} (403)"))
        .collect();
    let mut gate_off_statuses: Option<Vec<(String, u16)>> = None;
    for (name, home, expected, gate) in [
        ("flag OFF", Home::gate_off(), Vec::new(), false),
        ("flag ON, no key", Home::keyless(), Vec::new(), false),
        ("flag ON", Home::new(), table.clone(), true),
    ] {
        let server = Server::start(&home, None);
        let (refused, statuses) = licence_refusals(&server, &routes);
        assert_eq!(
            refused, expected,
            "{name}: the addresses refused for want of a licence are not the \
             table's. With the gate off nothing may refuse; with it on, exactly \
             the table — anything else is a surface the table does not name \
             being gated anyway."
        );
        // A refusal in any other words is still a refusal: with the gate on,
        // every address outside the table answers with exactly the status it
        // answers with the gate off, and so does every address in a keyless
        // build (which is always off).
        match &gate_off_statuses {
            None => gate_off_statuses = Some(statuses),
            Some(off) => {
                assert_eq!(statuses.len(), off.len(), "{name}: the walk changed length");
                let moved: Vec<String> = statuses
                    .iter()
                    .zip(off)
                    .filter(|((address, status), (_, off_status))| {
                        let in_table = PAID_SURFACES
                            .iter()
                            .any(|(m, a, _)| *address == format!("{m} {a}"));
                        status != off_status && !(gate && in_table)
                    })
                    .map(|((address, status), (_, off_status))| {
                        format!(
                            "{address}: {status}, where the gate-OFF build answered {off_status}"
                        )
                    })
                    .collect();
                assert!(
                    moved.is_empty(),
                    "{name}: addresses the table does not name answer differently \
                     from the gate-OFF build:\n{}",
                    moved.join("\n")
                );
            }
        }
        // The status route says where the gate stands, and names the table.
        let v = licence_doc(&server);
        assert_eq!(v["shape"], "thruline.licence.status");
        assert_eq!(v["paid"][0]["surface"], "approve_version");
        assert_eq!(v["paid_gate"], gate, "{name}: {v}");
        assert_eq!(
            v["entitlements"]["approve_version"]["allowed"], !gate,
            "{name}: {v}"
        );
    }

    // With the gate off, a fresh home stays without a legacy trial file
    // after `thruline ui` and a status read.
    let home = Home::gate_off();
    let server = Server::start(&home, None);
    let v = licence_doc(&server);
    assert!(v.get("trial").is_none(), "{v}");
    let (_, text) = server.send("POST", APPROVE);
    assert!(!text.contains("licence_required"), "{text}");
    drop(server);
    assert!(
        !home.trial().exists(),
        "a build with the gate off created a legacy trial file"
    );
}

#[test]
fn e_with_a_licence_that_works_approving_is_no_longer_refused_for_one() {
    let issuer = Issuer::new();
    let key = issuer.public_hex();
    let home = Home::new();
    std::fs::write(home.installed(), issuer.valid()).expect("place");
    let server = Server::start(&home, Some(&key));
    let (status, text) = server.send("POST", APPROVE);
    assert!(
        !text.contains("licence_required"),
        "a working licence did not open approving ({status}): {text}"
    );
    // It reached the record, which refuses for its own reason: no such Change.
    assert!(text.contains("\"error\":\"refused\""), "{text}");
}

#[test]
fn e_the_gate_is_one_function_and_only_the_approve_route_reaches_it() {
    let mut sources = Vec::new();
    rust_sources(&cli_src(), &mut sources);
    let named = |needle: &str| -> Vec<String> {
        sources
            .iter()
            .filter(|(_, t)| t.contains(needle))
            .map(|(p, _)| {
                p.strip_prefix(cli_src())
                    .unwrap_or(p)
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    };
    assert_eq!(
        named("change_decision::approve("),
        vec!["licence_cmd.rs".to_string()],
        "the core's approve is reachable from outside the gated door"
    );
    let callers: Vec<String> = named("approve_version(")
        .into_iter()
        .filter(|f| f != "licence_cmd.rs")
        .collect();
    assert_eq!(callers, vec!["uiserve.rs".to_string()]);
    let uiserve = std::fs::read_to_string(cli_src().join("uiserve.rs")).expect("uiserve");
    assert_eq!(uiserve.matches("approve_version(").count(), 1);
    // `entitled(` is asked in the licence module only: no other surface asks,
    // however the call is spelled.
    assert_eq!(
        named("entitled("),
        vec!["licence_cmd.rs".to_string()],
        "a second surface asks for an entitlement"
    );

    // …and inside the licence module, function by function. The file-level
    // census above cannot see a helper that lives in licence_cmd.rs, asks
    // `entitled`, and is then called from a verb — the verifier's mutation M4
    // on 2026-09-26 did exactly that through `thruline model status` and every
    // assertion above stayed green.
    let module = std::fs::read_to_string(cli_src().join("licence_cmd.rs")).expect("licence_cmd");
    assert_eq!(
        fns_calling(&module, "entitled("),
        vec!["approve_version".to_string()],
        "a function in licence_cmd.rs other than the one gated door asks for an entitlement"
    );
    // `decide` is what `entitled` answers with; the status document asks it
    // only to REPORT whether approving would be open. Nothing else may.
    assert_eq!(
        fns_calling(&module, "decide("),
        vec!["entitled".to_string(), "status_body".to_string()],
        "a function in licence_cmd.rs decides an entitlement outside the door and the report"
    );
    // Every door into the licence module from the rest of the CLI, by name. A
    // new one is a new way for a surface to reach the gate, and is added here
    // in the same commit as the table row (or the reason it is not one).
    let mut doors = std::collections::BTreeSet::new();
    for (path, text) in &sources {
        if path.file_name().is_some_and(|f| f == "licence_cmd.rs") {
            continue;
        }
        for piece in text.split("licence_cmd::").skip(1) {
            let ident: String = piece
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            doors.insert(ident);
        }
    }
    assert_eq!(
        doors.into_iter().collect::<Vec<_>>(),
        ["Approval", "Args", "approve_version", "run", "status_body"]
            .map(String::from)
            .to_vec(),
        "the doors into licence_cmd.rs from the rest of the CLI changed"
    );
    // And no other file reads a licence or the gate for itself.
    assert_eq!(
        named("thruline_licence"),
        vec!["licence_cmd.rs".to_string()],
        "a second file reaches the licence crate directly"
    );
}

/// The names of the functions in `module` whose bodies contain `needle`,
/// sorted and without repeats. A body runs from its `fn` line to the next
/// one; comment lines are skipped, so a doc comment naming a call is not one.
fn fns_calling(module: &str, needle: &str) -> Vec<String> {
    let mut current: Option<String> = None;
    let mut found = std::collections::BTreeSet::new();
    for line in module.lines() {
        let t = line.trim_start();
        if t.starts_with("//") {
            continue;
        }
        let head = t
            .strip_prefix("pub(crate) ")
            .or_else(|| t.strip_prefix("pub "))
            .unwrap_or(t);
        if let Some(rest) = head.strip_prefix("fn ") {
            current = Some(
                rest.chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect(),
            );
            continue;
        }
        if line.contains(needle) {
            if let Some(name) = &current {
                found.insert(name.clone());
            }
        }
    }
    found.into_iter().collect()
}

/// One node of the CLI's verb tree: its words, the `Usage:` line its own
/// `--help` prints, and whether it is a group (it has subcommands).
struct VerbNode {
    path: Vec<String>,
    usage: String,
    group: bool,
}

/// The CLI's verbs, from the shipping binary's own `--help`, walked
/// recursively: every group (the bare command included) and every leaf.
fn verbs(path: &[String], out: &mut Vec<VerbNode>) {
    let o = Command::new(bin())
        .args(path)
        .arg("--help")
        .env_remove(SEAM)
        .output()
        .expect("help");
    let help = String::from_utf8_lossy(&o.stdout).into_owned();
    let usage = help
        .lines()
        .find(|l| l.starts_with("Usage:"))
        .unwrap_or_default()
        .to_string();
    let Some(start) = help.find("\nCommands:\n") else {
        out.push(VerbNode {
            path: path.to_vec(),
            usage,
            group: false,
        });
        return;
    };
    out.push(VerbNode {
        path: path.to_vec(),
        usage,
        group: true,
    });
    let body = &help[start + "\nCommands:\n".len()..];
    let end = body.find("\n\n").unwrap_or(body.len());
    for line in body[..end].lines() {
        // A verb's own line is indented exactly two spaces; a wrapped
        // description is indented further.
        if !line.starts_with("  ") || line.starts_with("   ") {
            continue;
        }
        let Some(name) = line.split_whitespace().next() else {
            continue;
        };
        if name == "help" {
            continue;
        }
        let mut next = path.to_vec();
        next.push(name.to_string());
        verbs(&next, out);
    }
}

#[test]
fn e_no_cli_verb_can_be_refused_for_want_of_a_licence() {
    let mut all = Vec::new();
    verbs(&[], &mut all);
    let names: Vec<String> = all
        .iter()
        .filter(|v| !v.group)
        .map(|v| v.path.join(" "))
        .collect();
    // Live: the walk reached the verbs the card says are never gated.
    for must in [
        "ingest",
        "verify",
        "explain",
        "claims",
        "fold",
        "hook",
        "export",
        "licence status",
        "licence install",
        "licence remove",
        "licence fingerprint",
        "change cancel-run",
        "ui",
    ] {
        assert!(
            names
                .iter()
                .any(|n| n == must || n.starts_with(&format!("{must} "))),
            "the verb walk missed `{must}`: {names:?}"
        );
    }
    // The gate is reachable only through `approve_version`, which only the UI
    // server's approve route calls (the test above). So no verb's own dispatch
    // can refuse for a licence; the one surface a verb opens that CAN refuse is
    // `ui`'s approve route, which is the paid surface. Every other verb, run
    // unlicensed, is never refused — held here for the ones that need nothing
    // but a home, which include every licence surface, and for EVERY verb, run,
    // by `e_every_cli_verb_is_run_in_every_build_and_only_the_table_refuses`.
    //
    // Walked in all three builds (R-2026-09-24-25): the gate off, the gate on
    // with no key, and the gate on — the table (`PAID_SURFACES`) has no CLI
    // row, so no verb refuses in any of them.
    assert!(
        PAID_SURFACES
            .iter()
            .all(|(m, a, _)| *m == "POST" && a.starts_with("/api/")),
        "a table row is not a server address; this walk must then expect it"
    );
    for (name, home) in [
        ("flag OFF", Home::gate_off()),
        ("flag ON, no key", Home::keyless()),
        ("flag ON", Home::new()),
    ] {
        for args in [
            vec!["licence", "status"],
            vec!["licence", "status", "--json"],
            vec!["licence", "remove"],
            vec!["licence", "fingerprint"],
            vec!["json-shapes"],
        ] {
            // Reporting the status (which names the paid surface and why it is
            // off) is not a refusal; a refusal is a failed exit or the marker.
            let o = home.cli(None, &args);
            let all = format!("{}{}", stdout(&o), String::from_utf8_lossy(&o.stderr));
            assert!(
                o.status.success() && !all.contains("licence_required"),
                "{name}: `thruline {}` was refused while unlicensed ({:?}): {all}",
                args.join(" "),
                o.status
            );
        }
    }
    assert!(names.len() > 40, "the verb walk found only {}", names.len());
}

/* ── E, run: every verb, executed, in all three builds ─────────────────── */

/// The three builds every walk compares. `Off` is the reference; the other two
/// must answer exactly as it does wherever the table has no row. Since P11 the
/// shipped build is `On`, and the two references are asked for by seam.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Build {
    /// The gate OFF (`THRULINE_PAID_GATE_DEV=off`), a key built in.
    Off,
    /// No key (`THRULINE_LICENCE_DEV_NO_BUILT_IN_KEYS=1`): a keyless build is
    /// always off, whatever the flag says.
    OnNoKey,
    /// The gate ON, the production key built in: as it ships.
    On,
}

/// The project the template ingests (the core fixture's), and the value a
/// verb's required `PROJECT` gets.
const WALK_PROJECT: &str = "-Users-dev-demo";

/// A command for the walk. On macOS it runs under the zero-network denial, so
/// no verb the walk reaches can open a socket; a verb that tries is killed,
/// the same way in every build.
#[cfg(target_os = "macos")]
fn walk_command() -> Command {
    let mut cmd = Command::new("/usr/bin/sandbox-exec");
    cmd.arg("-p").arg(NO_NETWORK).arg(bin());
    cmd
}

#[cfg(not(target_os = "macos"))]
fn walk_command() -> Command {
    Command::new(bin())
}

/// The walk's environment: nothing inherited, everything the binary could
/// write inside `root`, and the build chosen by the two test seams only.
fn walk_env(cmd: &mut Command, root: &Path, build: Build, key: &str) {
    cmd.env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", root.join("h"))
        .env("THRULINE_HOME", root.join("h/.thruline"))
        .env("TMPDIR", root.join("tmp"))
        .current_dir(root.join("repo"))
        .stdin(Stdio::null());
    match build {
        Build::Off => {
            cmd.env(GATE, "off");
        }
        Build::OnNoKey => {
            cmd.env(NO_BUILT_IN_KEYS, "1");
        }
        Build::On => {}
    }
    if build != Build::OnNoKey {
        cmd.env(SEAM, key);
    }
}

/// Remove `dir`, first making everything under it writable again: the
/// ledger's private mirrors are written read-only on purpose.
fn scrub(dir: &Path) {
    fn writable(p: &Path) {
        let Ok(meta) = std::fs::symlink_metadata(p) else {
            return;
        };
        if meta.file_type().is_symlink() {
            return;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = if meta.is_dir() { 0o700 } else { 0o600 };
            let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode));
        }
        if meta.is_dir() {
            if let Ok(entries) = std::fs::read_dir(p) {
                for entry in entries.flatten() {
                    writable(&entry.path());
                }
            }
        }
    }
    writable(dir);
    let _ = std::fs::remove_dir_all(dir);
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("mkdir");
    for entry in std::fs::read_dir(from).expect("read_dir").flatten() {
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        let kind = entry.file_type().expect("file type");
        if kind.is_dir() {
            copy_tree(&src, &dst);
        } else if kind.is_symlink() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(std::fs::read_link(&src).expect("link"), &dst)
                .expect("symlink");
        } else {
            std::fs::copy(&src, &dst).expect("copy");
        }
    }
}

/// The home every run starts from: the core's synthetic Claude Code fixture
/// ingested and one Change opened on it. With the gate ON, this unlicensed
/// machine is refused approving; with the gate OFF, it is allowed. Returns the
/// Change's id.
fn walk_template(template: &Path, key: &str) -> String {
    let thruline_home = template.join("h/.thruline");
    let mirrors = template.join("mirrors");
    let project = template.join("projects").join(WALK_PROJECT);
    for dir in [&thruline_home, &mirrors, &project, &template.join("repo")] {
        std::fs::create_dir_all(dir).expect("mkdir");
    }
    std::fs::create_dir_all(template.join("tmp")).expect("tmp");
    private(&thruline_home);
    private(&mirrors);
    std::fs::copy(
        core_fixture("claude_code_sessions.jsonl"),
        project.join("cccccccc-0000-4000-8000-000000000001.jsonl"),
    )
    .expect("copy fixture");
    let run = |args: &[&str]| -> Output {
        let mut cmd = Command::new(bin());
        cmd.args(args);
        walk_env(&mut cmd, template, Build::Off, key);
        cmd.output().expect("run thruline")
    };
    let o = run(&[
        "ingest",
        "--apply",
        "--source",
        "claude-code",
        "--mirror-root",
        &mirrors.to_string_lossy(),
        &template.join("projects").to_string_lossy(),
    ]);
    assert!(
        o.status.success(),
        "ingest: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    let o = run(&[
        "change",
        "new",
        "--title",
        "census",
        "--requester",
        "census",
        "--json",
    ]);
    assert!(
        o.status.success(),
        "change new: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).expect("json");
    v["change"].as_str().expect("change id").to_string()
}

/// What one run of one verb came to.
#[derive(Debug, PartialEq)]
struct Ran {
    exit: String,
    stdout: String,
    stderr: String,
}

impl Ran {
    fn all(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}

/// Run `args` in a fresh copy of the template at `dir`, in `build`, and
/// remove the copy afterwards. The copy is always at `dir`, so the runs of one
/// verb see the same paths and can be compared line for line.
fn run_in_copy(dir: &Path, template: &Path, key: &str, build: Build, args: &[String]) -> Ran {
    scrub(dir);
    copy_tree(&template.join("h"), &dir.join("h"));
    std::fs::create_dir_all(dir.join("repo")).expect("repo");
    std::fs::create_dir_all(dir.join("tmp")).expect("tmp");
    let (out, err) = (dir.join("stdout"), dir.join("stderr"));
    let mut cmd = walk_command();
    cmd.args(args);
    walk_env(&mut cmd, dir, build, key);
    let mut child = cmd
        .stdout(std::fs::File::create(&out).expect("stdout"))
        .stderr(std::fs::File::create(&err).expect("stderr"))
        .spawn()
        .expect("spawn thruline");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    let exit = loop {
        if let Some(status) = child.try_wait().expect("wait") {
            break status.to_string();
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            break "did not finish in 120 s".to_string();
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    let ran = Ran {
        exit,
        stdout: String::from_utf8_lossy(&std::fs::read(&out).expect("read stdout")).into_owned(),
        stderr: String::from_utf8_lossy(&std::fs::read(&err).expect("read stderr")).into_owned(),
    };
    scrub(dir);
    ran
}

/// Output with what honestly differs between two runs of the same verb on the
/// same record taken out: ULIDs, hex digests and ids, and every number (times,
/// durations, sizes). What is left is what the verb SAID.
fn normalised(text: &str) -> String {
    fn mask(word: &str, out: &mut String) {
        let crockford =
            |b: u8| b.is_ascii_digit() || (b.is_ascii_uppercase() && !b"ILOU".contains(&b));
        if !word.bytes().any(|b| b.is_ascii_digit()) {
            out.push_str(word);
        } else if word.len() == 26 && word.bytes().all(crockford) {
            out.push_str("<ulid>");
        } else if word.len() >= 8 && word.bytes().all(|b| b.is_ascii_hexdigit()) {
            out.push_str("<hex>");
        } else {
            let mut digits = false;
            for c in word.chars() {
                if c.is_ascii_digit() {
                    if !digits {
                        out.push('#');
                    }
                    digits = true;
                } else {
                    out.push(c);
                    digits = false;
                }
            }
        }
    }
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            word.push(c);
        } else {
            mask(&word, &mut out);
            word.clear();
            out.push(c);
        }
    }
    mask(&word, &mut out);
    out
}

/// The value a required argument gets: the template's own Change and project
/// where a verb names one, so the verb gets past "no such Change" into its own
/// work; a plain word everywhere else.
fn filler(spec: &str, change: &str) -> String {
    match spec.trim_matches(|c| c == '<' || c == '>' || c == '.') {
        "CHANGE" => change.to_string(),
        "PROJECT" => WALK_PROJECT.to_string(),
        _ => "census".to_string(),
    }
}

/// A verb's arguments as `(name, value)`: `--flag` with its value, `<NAME>`
/// for a positional, and `--` for the separator.
type ArgList = Vec<(String, String)>;

/// Every argument a verb's `Usage:` line says is required, filled.
fn required_args(node: &VerbNode, change: &str) -> ArgList {
    let words: Vec<&str> = node
        .usage
        .split_whitespace()
        .skip(2 + node.path.len())
        .collect();
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut i = 0;
    while i < words.len() {
        let w = words[i];
        if depth > 0 || w.starts_with('[') {
            depth = (depth + w.matches('[').count()).saturating_sub(w.matches(']').count());
        } else if w == "--" {
            out.push(("--".to_string(), String::new()));
        } else if w.starts_with('-') {
            let spec = words.get(i + 1).copied().unwrap_or_default();
            out.push((w.to_string(), filler(spec, change)));
            i += 1;
        } else if w.starts_with('<') {
            out.push((w.trim_end_matches('.').to_string(), filler(w, change)));
        }
        i += 1;
    }
    out
}

fn argv(node: &VerbNode, args: &ArgList) -> Vec<String> {
    let mut out = node.path.clone();
    for (name, value) in args {
        if name == "--" {
            out.push(name.clone());
        } else if name.starts_with('<') {
            out.push(value.clone());
        } else if value.starts_with('-') {
            // The fixture's project id begins with '-'; joined, it is a value.
            out.push(format!("{name}={value}"));
        } else {
            out.push(name.clone());
            out.push(value.clone());
        }
    }
    out
}

/// The argument parser refused the list before the verb's own code ran.
fn stopped_by_the_parser(ran: &Ran) -> bool {
    ran.exit == "exit status: 2" && ran.stderr.contains("For more information, try '--help'.")
}

/// Take what the parser asked for: a value it accepts in place of one it
/// refused, or the arguments it said were missing. `false` when there is
/// nothing to take, so the caller stops asking.
fn take_the_parsers_answer(args: &mut ArgList, stderr: &str, change: &str) -> bool {
    if let Some(at) = stderr.find("invalid value '") {
        let rest = &stderr[at + "invalid value '".len()..];
        let Some((bad, rest)) = rest.split_once("' for '") else {
            return false;
        };
        let Some((spec, _)) = rest.split_once('\'') else {
            return false;
        };
        let name = spec.split_whitespace().next().unwrap_or_default();
        let good = stderr
            .split_once("[possible values: ")
            .and_then(|(_, r)| r.split([',', ']']).next())
            .map_or("1", str::trim)
            .to_string();
        return match args.iter_mut().find(|(n, v)| n == name && v == bad) {
            Some(slot) if slot.1 != good => {
                slot.1 = good;
                true
            }
            _ => false,
        };
    }
    let Some((_, missing)) =
        stderr.split_once("the following required arguments were not provided:\n")
    else {
        return false;
    };
    let mut took = false;
    for line in missing.lines().take_while(|l| l.starts_with("  ")) {
        let words: Vec<&str> = line.split_whitespace().collect();
        let (Some(first), Some(last)) = (words.first(), words.last()) else {
            continue;
        };
        // Before a `--`, so a flag is never read as part of a command.
        let at = args
            .iter()
            .position(|(n, _)| n == "--")
            .unwrap_or(args.len());
        args.insert(
            at,
            (
                first.trim_end_matches('.').to_string(),
                filler(last, change),
            ),
        );
        took = true;
    }
    took
}

/// One verb, walked: the arguments it ran with, whether its own code ran,
/// and what it did in each build (the gate-OFF build twice).
struct Walked {
    args: Vec<String>,
    reached_its_own_code: bool,
    /// How many times the verb was run, parser retries included.
    runs: usize,
    off: Ran,
    off_again: Ran,
    no_key: Ran,
    on: Ran,
}

fn walk_verb(node: &VerbNode, dir: &Path, template: &Path, key: &str, change: &str) -> Walked {
    let mut args = if node.group {
        Vec::new()
    } else {
        required_args(node, change)
    };
    // Ask the parser until it takes the list: a refused list never reaches the
    // verb's own code, so it would prove nothing about the gate.
    let mut off = run_in_copy(dir, template, key, Build::Off, &argv(node, &args));
    let mut runs = 1;
    for _ in 0..8 {
        if !stopped_by_the_parser(&off) || !take_the_parsers_answer(&mut args, &off.stderr, change)
        {
            break;
        }
        off = run_in_copy(dir, template, key, Build::Off, &argv(node, &args));
        runs += 1;
    }
    let args = argv(node, &args);
    Walked {
        reached_its_own_code: node.group || !stopped_by_the_parser(&off),
        runs: runs + 3,
        off_again: run_in_copy(dir, template, key, Build::Off, &args),
        no_key: run_in_copy(dir, template, key, Build::OnNoKey, &args),
        on: run_in_copy(dir, template, key, Build::On, &args),
        off,
        args,
    }
}

#[test]
fn e_every_cli_verb_is_run_in_every_build_and_only_the_table_refuses() {
    // The table has no CLI row: no terminal door approves today. A CLI row
    // would be a verb this walk must then EXPECT to refuse with the gate on.
    assert!(
        PAID_SURFACES
            .iter()
            .all(|(m, a, _)| *m == "POST" && a.starts_with("/api/")),
        "a table row is not a server address; this walk must then expect it"
    );
    let mut nodes = Vec::new();
    verbs(&[], &mut nodes);
    // `thruline ui` serves until it is stopped. It is the verb the route walk
    // (`e_every_server_route_answers_unlicensed_and_only_approving_is_refused`)
    // starts in each of the same three builds, and `Server::start` fails that
    // test if it does not come up, so it is walked there and not here.
    assert!(
        nodes.iter().any(|n| n.path == ["ui"]),
        "the verb walk lost `ui`"
    );
    nodes.retain(|n| n.path != ["ui"]);
    let leaves = nodes.iter().filter(|n| !n.group).count();
    assert!(leaves > 40, "the verb walk found only {leaves} verbs");

    // Removed on the way out, pass or fail: the mirrors are read-only.
    struct Scrubbed(tempfile::TempDir);
    impl Drop for Scrubbed {
        fn drop(&mut self) {
            scrub(self.0.path());
        }
    }
    let guard = Scrubbed(tempfile::tempdir().expect("tempdir"));
    let root = guard.0.path();
    let key = Issuer::new().public_hex();
    let template = root.join("template");
    let change = walk_template(&template, &key);

    // The walk is live: the ON build really is gated — approving is refused on
    // this record — and the other two are not. Otherwise "nothing refused"
    // would prove nothing.
    for (build, gated) in [
        (Build::Off, false),
        (Build::OnNoKey, false),
        (Build::On, true),
    ] {
        let ran = run_in_copy(
            &root.join("liveness"),
            &template,
            &key,
            build,
            &["licence", "status", "--json"].map(String::from),
        );
        let v: serde_json::Value = serde_json::from_str(&ran.stdout).expect("status json");
        assert_eq!(v["paid_gate"], gated, "{build:?}: {v}");
        assert_eq!(
            v["entitlements"]["approve_version"]["allowed"], !gated,
            "{build:?}: {v}"
        );
    }

    // Every verb and every group, in parallel, each in its own directory.
    let next = std::sync::atomic::AtomicUsize::new(0);
    let walked = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..6 {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let Some(node) = nodes.get(i) else { break };
                let dir = root.join(format!("verb-{i:03}"));
                let w = walk_verb(node, &dir, &template, &key, &change);
                walked.lock().expect("lock").push((i, w));
            });
        }
    });
    let mut walked = walked.into_inner().expect("walked");
    walked.sort_by_key(|(i, _)| *i);
    assert_eq!(walked.len(), nodes.len(), "a verb was not walked");

    let mut parser_stopped = Vec::new();
    let mut unsteady = Vec::new();
    let mut refused = Vec::new();
    for (i, w) in &walked {
        let node = &nodes[*i];
        let name = format!("`thruline {}`", w.args.join(" "));
        if !w.reached_its_own_code {
            parser_stopped.push(format!("{name}: {}", w.off.stderr.trim()));
        }
        // The same verb, the same build, twice: if these differ, the
        // comparison below would be comparing noise.
        if w.off_again.exit != w.off.exit
            || normalised(&w.off_again.stdout) != normalised(&w.off.stdout)
            || normalised(&w.off_again.stderr) != normalised(&w.off.stderr)
        {
            unsteady.push(format!(
                "{name}:\n--- once\n{}\n--- again\n{}",
                w.off.all(),
                w.off_again.all()
            ));
        }
        // `licence status` exists to say where the gate stands, so its words
        // differ between the builds by design; its exit status may not.
        let reports_the_gate = node.path == ["licence", "status"];
        for (build, ran) in [
            (Build::Off, &w.off),
            (Build::OnNoKey, &w.no_key),
            (Build::On, &w.on),
        ] {
            if ran.all().contains("licence_required") {
                refused.push(format!("{name} in {build:?}: says licence_required"));
            }
            if build == Build::Off {
                continue;
            }
            if ran.exit != w.off.exit {
                refused.push(format!(
                    "{name} in {build:?}: {} where the gate-OFF build gave {}",
                    ran.exit, w.off.exit
                ));
            } else if !reports_the_gate
                && (normalised(&ran.stdout) != normalised(&w.off.stdout)
                    || normalised(&ran.stderr) != normalised(&w.off.stderr))
            {
                refused.push(format!(
                    "{name} in {build:?} says something the gate-OFF build does not:\n--- gate OFF\n{}\n--- {build:?}\n{}",
                    w.off.all(),
                    ran.all()
                ));
            }
        }
        if reports_the_gate {
            assert!(
                w.on.exit == "exit status: 0" && w.off.exit == "exit status: 0",
                "{name}: reporting the status is not a refusal: {:?} / {:?}",
                w.off,
                w.on
            );
        }
    }
    assert!(
        parser_stopped.is_empty(),
        "the walk could not get {} verb(s) past the argument parser, so their own \
         code never ran in any build; teach `filler` a value they accept:\n{}",
        parser_stopped.len(),
        parser_stopped.join("\n")
    );
    assert!(
        unsteady.is_empty(),
        "{} verb(s) said different things on two gate-OFF runs of the same record; \
         `normalised` must learn what varies before this walk can compare builds:\n{}",
        unsteady.len(),
        unsteady.join("\n\n")
    );
    assert!(
        refused.is_empty(),
        "with the gate on, {} verb run(s) answered differently from the gate-OFF \
         build. The table (`PAID_SURFACES`) has no CLI row, so every verb must \
         answer exactly as it does with the gate off:\n{}",
        refused.len(),
        refused.join("\n\n")
    );
    eprintln!(
        "walked {} verb(s) and {} group(s), {} runs, in 3 builds: none refused",
        leaves,
        walked.len() - leaves,
        walked.iter().map(|(_, w)| w.runs).sum::<usize>()
    );
}

/* ── the screen and the terminal say the same sentences ────────────────── */

#[test]
fn the_screen_copy_and_the_binary_agree_on_every_sentence() {
    let golden_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../ui/src/workspace/hq-copy.golden.json");
    let golden: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(golden_path).expect("golden")).expect("json");
    use thruline_licence::Problem;
    let all = [
        Problem::Absent,
        Problem::Unreadable,
        Problem::NotALicence,
        Problem::NewerFormat,
        Problem::NoTrustedKey,
        Problem::UnknownKey,
        Problem::Tampered,
        Problem::WrongProduct,
        Problem::UnknownPlan,
        // The golden fills `{date}` with the copy test's sample day.
        Problem::Expired {
            on: "17 Sep 2026".into(),
        },
        Problem::WrongMachine,
        Problem::MachineUnknown,
    ];
    assert_eq!(all.len(), Problem::SLUGS.len());
    for p in &all {
        let slug = p.slug();
        // C20: an absent file is the normal Free state on the screen, while
        // the core problem still describes the missing file for refusals.
        if !matches!(p, Problem::Absent) {
            assert_eq!(
                golden[format!("licence.status.{slug}.problem")],
                serde_json::Value::from(p.problem()),
                "licence.status.{slug}.problem"
            );
        }
        assert_eq!(
            golden[format!("licence.status.{slug}.fix")],
            serde_json::Value::from(p.fix()),
            "licence.status.{slug}.fix"
        );
    }

    // The Free status sentence the terminal prints is the screen's sentence.
    let free = golden["licence.status.absent.problem"]
        .as_str()
        .expect("free sentence in the screen copy");
    assert_eq!(status(&Home::new()), free);
}

/* ── C20: Free status and legacy trial files against the binary ─────────── */

fn status(home: &Home) -> String {
    let o = home.cli(None, &["licence", "status"]);
    assert!(o.status.success(), "status exited {:?}", o.status);
    stdout(&o).trim().to_string()
}

fn status_json(home: &Home) -> serde_json::Value {
    let o = home.cli(None, &["licence", "status", "--json"]);
    assert!(o.status.success(), "status --json exited {:?}", o.status);
    serde_json::from_slice(&o.stdout).expect("json")
}

/* ── one Change a person can approve, in a ledger of its own ───────────── */

const CHANGE_ID: &str = "cccccccc-0000-4000-8000-00000000c001";
const PROJECT_ID: &str = "cccccccc-0000-4000-8000-00000000p001";
const VERSION: &str = "snap_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// A ledger OUTSIDE the ThruLine home holding one Change that is accepted,
/// bound to a version, and not yet approved, with its project folder beside
/// it. Made straight in the tables, as `change_export_route.rs` does, so the
/// approval a test then asks for is a real one: it appends a row and writes
/// `.thruline/decisions/<version>.json`. The ledger is outside the home so a
/// test can make the home unwritable without taking the record with it.
struct Approvable {
    db: PathBuf,
    root: PathBuf,
}

impl Approvable {
    fn new(home: &Home) -> Approvable {
        use rusqlite::params;
        let db = home.dir.path().join("approvable.db");
        let root = home.dir.path().join("worktree");
        std::fs::create_dir_all(&root).expect("worktree");
        let ledger = thruline_core::ledger::Ledger::open(&db).expect("open the ledger");
        let conn = ledger.conn();
        conn.execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES(?1, ?2, 'claude_code', 'Synthetic', '2026-01-01T00:00:00Z')",
            params![PROJECT_ID, root.to_string_lossy()],
        )
        .expect("project");
        conn.execute(
            "INSERT INTO changes(id, project_id, opened_at, title, purpose, request_event_id,
                                 requester, request_source)
             VALUES(?1, ?2, '2026-09-22T08:00:00Z', 'Totals add up', 'They do not.', NULL,
                    'sam', 'person')",
            params![CHANGE_ID, PROJECT_ID],
        )
        .expect("change");
        conn.execute(
            "INSERT INTO change_scope_revisions(change_id, scope_version, proposed_at,
                 criteria_json, exclusions_json, allowed_scope_json, constraints_json,
                 check_profile_json, content_sha256)
             VALUES(?1, 1, '2026-09-22T08:30:00Z', '[\"The totals add up\"]', '[]',
                    '[\"checkout/\"]', '[]', '[]', 'deadbeef')",
            params![CHANGE_ID],
        )
        .expect("scope");
        conn.execute(
            "INSERT INTO change_scope_acceptances(change_id, scope_version, accepted_by,
                 accepted_at, rationale)
             VALUES(?1, 1, 'sam', '2026-09-22T09:00:00Z', '')",
            params![CHANGE_ID],
        )
        .expect("acceptance");
        let doc = serde_json::json!({
            "id": VERSION, "taken_at": "2026-09-22T10:00:00Z", "root": root.to_string_lossy(),
            "git": null,
            "tree": { "staged": [], "unstaged": [], "untracked": [], "deleted": [],
                      "dirty_diff_sha256": null },
            "environment": { "os": "test", "arch": "test", "git_version": null,
                             "env_names": [], "lockfiles": {} },
            "processes": { "ps_sha256": null, "ps_lines": 0, "ports": null, "task_ids": null,
                           "log_cursors": null, "gaps": "none" },
            "complete": true, "note": null,
        });
        let text = serde_json::to_string(&doc).expect("doc");
        conn.execute(
            "INSERT INTO workspace_snapshots(id, taken_at, root, head, branch, complete, doc,
                                             doc_sha256)
             VALUES(?1, '2026-09-22T10:00:00Z', ?2, NULL, NULL, 1, ?3, ?4)",
            params![
                VERSION,
                root.to_string_lossy(),
                text,
                thruline_core::chain::sha256_hex(text.as_bytes())
            ],
        )
        .expect("snapshot");
        conn.execute(
            "INSERT INTO change_implementations(change_id, binding_version, bound_at,
                 repository_identity, base_revision, snapshot_id, workspace_identity)
             VALUES(?1, 1, '2026-09-22T10:00:00Z', 'shop', 'base0000', ?2, NULL)",
            params![CHANGE_ID, VERSION],
        )
        .expect("implementation");
        Approvable { db, root }
    }

    /// The file an approval writes under the project folder.
    fn decision_file(&self) -> PathBuf {
        thruline_core::change_decision::record_path_for(&self.root, VERSION)
    }

    /// How many approvals and withdrawals the ledger holds for the Change.
    fn decision_rows(&self) -> i64 {
        row_counts(&self.db)
            .into_iter()
            .find(|(t, _)| t == "change_decisions")
            .map_or(0, |(_, n)| n)
    }

    /// The request the Change screen's Approve sends, for the version the
    /// screen shows.
    fn approve(&self, server: &Server) -> (u16, String) {
        server.send_body(
            "POST",
            &format!("/api/changes/{CHANGE_ID}/decisions"),
            &format!(r#"{{"actor":"sam","seen_version":"{VERSION}"}}"#),
        )
    }
}

/// The names in the ThruLine folder, sorted.
fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read_dir")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn assert_no_new_trial_or_licence_files(before: &[String], after: Vec<String>) {
    let licence_files: Vec<String> = after
        .into_iter()
        .filter(|n| !before.contains(n) && (n.starts_with("trial") || n.starts_with("licence")))
        .collect();
    assert!(
        licence_files.is_empty(),
        "reading the licence wrote into the ThruLine folder: {licence_files:?}"
    );
}

#[test]
fn b_licence_status_ui_and_api_report_free_without_writing_licence_files() {
    let home = Home::new();
    let before = names_in(&home.thruline_home());

    let plain = status(&home);
    assert!(plain.starts_with("Free"), "{plain}");
    assert!(!plain.to_ascii_lowercase().contains("trial"), "{plain}");
    let doc = status_json(&home);
    assert_eq!(doc["schema_version"], 4, "{doc}");
    assert_eq!(doc["paid_gate"], true);
    assert_eq!(doc["headline"], "free");
    assert!(doc.get("trial").is_none(), "{doc}");
    assert_eq!(doc["entitlements"]["approve_version"]["allowed"], false);
    assert_no_new_trial_or_licence_files(&before, names_in(&home.thruline_home()));

    let server = Server::start(&home, None);
    let served = licence_doc(&server);
    assert_eq!(served, doc, "the screen and terminal disagree");
    drop(server);
    assert_no_new_trial_or_licence_files(&before, names_in(&home.thruline_home()));
}

#[test]
fn b_ui_launch_creates_no_new_trial_or_licence_files_before_status_reads() {
    let home = Home::new();
    let before = names_in(&home.thruline_home());
    eprintln!("ui startup names before: {before:?}");
    assert!(
        before
            .iter()
            .all(|n| !n.starts_with("trial") && !n.starts_with("licence")),
        "fresh home already has trial or licence files: {before:?}"
    );

    let server = Server::start(&home, None);
    let after = names_in(&home.thruline_home());
    eprintln!("ui startup names after: {after:?}");
    assert_no_new_trial_or_licence_files(&before, after);
    drop(server);
}

#[test]
fn b_unlicensed_approval_is_refused_without_writing_a_record_or_trial_file() {
    let home = Home::new();
    let store = Approvable::new(&home);
    let server = Server::start_on(&home, None, Some(&store.db));
    let rows_before = row_counts(&store.db);
    let hash_before = sha256_of(&store.db);

    let (code, text) = store.approve(&server);
    assert_eq!(code, 403, "{text}");
    assert!(text.contains("\"error\":\"licence_required\""), "{text}");
    assert!(text.contains("Pro licence"), "{text}");
    assert!(!text.to_ascii_lowercase().contains("trial"), "{text}");
    assert_eq!(store.decision_rows(), 0);
    assert!(!store.decision_file().exists());
    assert_eq!(row_counts(&store.db), rows_before);
    assert_eq!(sha256_of(&store.db), hash_before);
    assert!(!home.trial().exists());
}

#[test]
fn c_no_verb_or_server_route_creates_a_legacy_trial_file() {
    let home = Home::new();
    let store = Approvable::new(&home);
    for args in [
        &["licence", "status"][..],
        &["licence", "status", "--json"][..],
        &["licence", "fingerprint"][..],
        &["licence", "remove"][..],
        &["licence", "install", "no-such-file.txt"][..],
        &["json-shapes"][..],
    ] {
        let _ = home.cli(None, args);
        assert!(
            !home.trial().exists(),
            "`thruline {}` wrote trial.json",
            args.join(" ")
        );
    }
    let server = Server::start_on(&home, None, Some(&store.db));
    for route in server_routes() {
        for method in ["GET", "POST"] {
            let (code, text) = server.send(method, &route);
            if method == "POST" && route == APPROVE {
                assert_eq!(code, 403, "{text}");
                assert!(text.contains("licence_required"), "{text}");
            }
            assert!(!home.trial().exists(), "{method} {route} wrote trial.json");
        }
    }
    assert_eq!(store.decision_rows(), 0);
    assert!(!store.decision_file().exists());
}

#[test]
fn an_expired_licence_is_told_to_renew_on_the_licence_page_and_install_the_new_licence() {
    let issuer = Issuer::new();
    let key = issuer.public_hex();
    let home = Home::new();
    std::fs::write(
        home.installed(),
        issuer.licence("thruline", "pro", "2020-01-01T00:00:00Z"),
    )
    .expect("place");
    let o = home.cli(Some(&key), &["licence", "status", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).expect("json");
    assert_eq!(v["status"], "expired", "{v}");
    assert_eq!(v["enable"], EXPIRED_FIX, "{v}");
    assert_eq!(v["entitlements"]["approve_version"]["allowed"], false);
    assert_eq!(v["entitlements"]["approve_version"]["enable"], EXPIRED_FIX);
    let enable = v["entitlements"]["approve_version"]["enable"]
        .as_str()
        .unwrap_or_default();
    for part in ["licence page", "renew", "install"] {
        assert!(enable.contains(part), "no {part:?} in the renew path: {v}");
    }
    let server = Server::start(&home, Some(&key));
    let (code, text) = server.send("POST", APPROVE);
    assert_eq!(code, 403, "{text}");
    assert!(text.contains(EXPIRED_FIX), "{text}");
}

#[test]
fn b_a_legacy_trial_file_does_not_change_free_status_or_approval_refusal() {
    let home = Home::new();
    let old = br#"{"format":1,"started_on":"2099-01-01","created_at":"2026-10-07T00:00:00Z"}"#;
    std::fs::write(home.trial(), old).expect("place old file");
    let store = Approvable::new(&home);
    let server = Server::start_on(&home, None, Some(&store.db));
    let rows_before = row_counts(&store.db);
    let hash_before = sha256_of(&store.db);

    let doc = status_json(&home);
    assert_eq!(doc["headline"], "free", "{doc}");
    assert!(doc.get("trial").is_none(), "{doc}");
    assert_eq!(doc["entitlements"]["approve_version"]["allowed"], false);
    let (code, text) = store.approve(&server);
    assert_eq!(code, 403, "{text}");
    assert!(text.contains("licence_required"), "{text}");
    assert!(!text.to_ascii_lowercase().contains("trial"), "{text}");
    assert_eq!(store.decision_rows(), 0);
    assert!(!store.decision_file().exists());
    assert_eq!(std::fs::read(home.trial()).expect("old file"), old);
    assert_eq!(row_counts(&store.db), rows_before);
    assert_eq!(sha256_of(&store.db), hash_before);
}

/// Every file under `dir`, with its bytes' SHA-256.
fn snapshot(dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).expect("read_dir").flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push((p.to_string_lossy().into_owned(), sha256_of(&p)));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn b_a_corrupt_legacy_trial_file_is_ignored_and_left_untouched() {
    for old in [
        &b"{ not a trial"[..],
        b"",
        b"{\"format\":1,\"started_on\":\"soon\",\"created_at\":\"x\"}",
    ] {
        let home = Home::new();
        std::fs::write(home.trial(), old).expect("place old file");
        let before = snapshot(&home.home());
        for args in [
            &["licence", "status"][..],
            &["licence", "status", "--json"][..],
        ] {
            let o = home.cli(None, args);
            assert!(o.status.success(), "{:?}: {}", o.status, stdout(&o));
            assert!(
                !stdout(&o).to_ascii_lowercase().contains("trial"),
                "{}",
                stdout(&o)
            );
            if args.len() == 2 {
                assert!(stdout(&o).trim().starts_with("Free"), "{}", stdout(&o));
            } else {
                let v: serde_json::Value = serde_json::from_slice(&o.stdout).expect("json");
                assert_eq!(v["headline"], "free", "{v}");
                assert!(v.get("trial").is_none(), "{v}");
                assert_eq!(v["entitlements"]["approve_version"]["allowed"], false);
            }
        }
        assert_eq!(
            snapshot(&home.home()),
            before,
            "old file or another file changed"
        );
    }
}

/* ── C20: hook paths never create a legacy trial file ───────────────────── */

#[test]
fn c_the_sensor_and_speaker_hooks_never_create_a_trial_file() {
    let home = Home::new();
    let payload = br#"{"session_id":"s1","transcript_path":"/t/s1.jsonl","cwd":"/w","hook_event_name":"Stop"}"#;
    for args in [
        &["sensor", "append", "SessionStart"][..],
        &["sensor", "append", "Stop"][..],
        &["brief", "--hook", "session-start"][..],
        &["audit", "--hook", "stop"][..],
        &["digest", "precompute", "--hook", "session-end"][..],
    ] {
        let mut cmd = Command::new(bin());
        cmd.args(args);
        home.env(&mut cmd, None);
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn hook");
        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(payload)
            .expect("payload");
        let o = child.wait_with_output().expect("wait");
        assert!(o.status.success(), "{}: {:?}", args.join(" "), o.status);
        assert!(
            !home.trial().exists(),
            "`thruline {}` created a legacy trial file — a hook path gained work",
            args.join(" ")
        );
    }
    // The sensor did record, so the path above really ran. A licence status
    // read does not create the obsolete file either.
    let db = home.thruline_home().join("ledger.db");
    assert!(
        row_counts(&db)
            .iter()
            .any(|(t, n)| t.contains("sensor") && *n > 0),
        "the sensor recorded nothing, so this test would prove nothing"
    );
    let _ = status(&home);
    assert!(
        !home.trial().exists(),
        "`licence status` created trial.json"
    );
}

/* ── T-DIST-6 acceptance A: an issued licence, accepted by THIS binary ─── */

/// Run one fixture per provider through `thruline_licence_issue`, signed with
/// a webhook secret and an Ed25519 key both generated here, and return
/// (provider, licence text, the plan name a person reads) plus the public key
/// as the seam wants it. The fixtures are SYNTHESISED FROM DOCUMENTATION, NOT
/// RECORDED — unverified against a live account.
fn issued_from_webhooks(dir: &Path) -> (String, Vec<(&'static str, String, &'static str)>) {
    use rand_core::RngCore;
    use thruline_licence_issue::{fixtures, lemonsqueezy, paddle, Config, Store, WebhookSecret};
    let signing = SigningKey::generate(&mut OsRng);
    let public = thruline_licence::to_hex(&signing.verifying_key().to_bytes());
    let mut raw = [0u8; 32];
    OsRng.fill_bytes(&mut raw);
    let secret = thruline_licence::to_hex(&raw).into_bytes();
    let now = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_secs(),
    )
    .expect("secs");
    let mut issuer = thruline_licence_issue::Issuer::new(
        Config::from_toml(fixtures::FIXTURE_MAPPING).expect("mapping"),
        Store::open(&dir.join("issued.sqlite")).expect("store"),
        signing,
    );
    let live = WebhookSecret::live(secret.clone());
    let ls = fixtures::LEMONSQUEEZY_ORDER_CREATED.as_bytes();
    let pd = fixtures::PADDLE_TRANSACTION_COMPLETED.as_bytes();
    let a = issuer
        .handle(
            "lemonsqueezy",
            &live,
            &lemonsqueezy::sign(&secret, ls),
            ls,
            now,
        )
        .expect("lemonsqueezy fixture issued");
    let b = issuer
        .handle("paddle", &live, &paddle::sign(&secret, pd, now), pd, now)
        .expect("paddle fixture issued");
    let text = |v: Vec<u8>| String::from_utf8(v).expect("utf8");
    (
        public,
        vec![
            ("lemonsqueezy", text(a.licence), "ThruLine Pro licence"),
            // Ruling 2 (D-298): the fixture's Team licence is dormant, and is
            // named by no plan.
            ("paddle", text(b.licence), "ThruLine licence"),
        ],
    )
}

#[test]
fn t_dist_6_a_a_licence_issued_from_each_providers_webhook_installs_and_names_its_plan() {
    let home = Home::new();
    let (key, issued) = issued_from_webhooks(home.dir.path());
    for (provider, licence, plan) in issued {
        let file = home.file(&format!("{provider}.txt"), &licence);
        let o = home.cli(Some(&key), &["licence", "install", &file.to_string_lossy()]);
        assert!(
            o.status.success(),
            "{provider}: the binary refused an issued licence: {}",
            stdout(&o)
        );
        assert_eq!(
            std::fs::read_to_string(home.installed()).expect("installed"),
            licence,
            "{provider}: install changed the bytes"
        );
        let o = home.cli(Some(&key), &["licence", "status"]);
        let line = stdout(&o);
        assert!(
            line.starts_with(&format!("This machine has a {plan}, valid until ")),
            "{provider}: {line}"
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn t_dist_6_a_the_same_install_and_status_open_no_socket() {
    let home = Home::new();
    let (key, issued) = issued_from_webhooks(home.dir.path());
    for (provider, licence, plan) in issued {
        let file = home.file(&format!("{provider}.txt"), &licence);
        let path = file.to_string_lossy().to_string();
        let o = under_denial(bin(), &["licence", "install", &path], &home, Some(&key));
        assert!(
            !killed_by_the_denial(&o) && o.status.success(),
            "{provider}: install under the network denial: {:?} {}",
            o.status,
            stdout(&o)
        );
        let o = under_denial(bin(), &["licence", "status"], &home, Some(&key));
        assert!(
            !killed_by_the_denial(&o),
            "{provider}: status opened a socket"
        );
        assert!(
            stdout(&o).starts_with(&format!("This machine has a {plan}")),
            "{provider}: {}",
            stdout(&o)
        );
    }
}

/* ── ruling 2: Team is dormant, and never named ────────────────────────── */

/// Founder ruling 2 (D-298): "Pro is the only purchasable plan. Team licence
/// code may stay dormant, but Team must not be buyable or shown anywhere a
/// user sees." A Team licence signed with the test key installs and keeps
/// approving open (dormant), and every line the terminal prints of it — the
/// install, the status, an expired one's refusal — names no plan: never
/// "Team", and never "Pro", which it is not. The JSON lists Pro as the only
/// plan that unlocks approving; the installed licence's own `plan` field may
/// still read "team", because that is what the file says.
#[test]
fn ruling_2_a_dormant_team_licence_works_and_the_terminal_never_names_team() {
    let issuer = Issuer::new();
    let key = issuer.public_hex();
    let home = Home::new();
    let team = issuer.licence("thruline", "team", "2099-01-01T00:00:00Z");
    let file = home.file("team.txt", &team);

    let installed = home.cli(Some(&key), &["licence", "install", &file.to_string_lossy()]);
    assert!(installed.status.success(), "{}", stdout(&installed));
    let status = home.cli(Some(&key), &["licence", "status"]);
    assert!(status.status.success());
    for (what, said) in [("install", stdout(&installed)), ("status", stdout(&status))] {
        assert!(!said.contains("Team"), "{what} named Team: {said}");
        assert!(
            !said.contains("Pro"),
            "{what} called a Team licence Pro: {said}"
        );
    }
    assert_eq!(
        stdout(&status).trim(),
        "This machine has a ThruLine licence, valid until 1 Jan 2099."
    );

    // The screen says the same sentence, from the copy file's golden.
    let golden: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../ui/src/workspace/hq-copy.golden.json"),
        )
        .expect("golden"),
    )
    .expect("json");
    assert_eq!(
        golden["licence.status.valid.sentence_unnamed"]
            .as_str()
            .unwrap_or_default()
            .replace("17 Sep 2026", "1 Jan 2099"),
        stdout(&status).trim(),
        "the screen and the terminal word a dormant Team licence differently"
    );

    let o = home.cli(Some(&key), &["licence", "status", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).expect("status json");
    assert_eq!(v["status"], "valid");
    assert_eq!(
        v["plan"], "team",
        "the licence's own field says what the file says"
    );
    assert!(!v["sentence"].as_str().unwrap_or_default().contains("Team"));
    assert_eq!(
        v["paid"],
        serde_json::json!([{"surface": "approve_version", "plans": ["pro"]}]),
        "the plans the status document lists are the plans anyone can buy"
    );
    assert_eq!(
        v["entitlements"]["approve_version"]["allowed"], true,
        "a dormant Team licence keeps approving open: {v}"
    );

    // Expired, the refusal names no plan either.
    let expired = issuer.licence("thruline", "team", "2020-01-01T00:00:00Z");
    std::fs::write(home.installed(), &expired).expect("place an expired Team licence");
    let o = home.cli(Some(&key), &["licence", "status"]);
    let said = stdout(&o);
    assert!(
        said.starts_with("This licence expired on 1 Jan 2020."),
        "{said}"
    );
    assert!(!said.contains("Team") && !said.contains("Pro"), "{said}");
}

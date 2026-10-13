//! A199 — `thruline init`, the one-command door into the Live Truth Loop's
//! substrate: the sensor hook pack plus the MCP context server entry.
//!
//! The contract under test: init composes machinery that already exists (the
//! `sensor install` merge, one `mcpServers` key), reports every file it
//! touched and every entry it wrote, is idempotent, and `--undo` reverses
//! exactly what init wrote while sparing every foreign byte in both files.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use tempfile::tempdir;

fn run(db: &Path, args: &[&str]) -> (String, String, bool) {
    let out = Command::new(env!("CARGO_BIN_EXE_thruline"))
        .args(["--db", db.to_str().unwrap()])
        .args(args)
        .output()
        .expect("run thruline");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).expect("read json")).expect("parse json")
}

#[test]
fn init_registers_hooks_and_the_context_server_and_spares_foreign_bytes() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let settings = temp.path().join("settings.json");
    let claude_json = temp.path().join("claude.json");
    // A foreign hook and foreign config the user already has: both must
    // survive untouched, byte-for-byte in structure.
    std::fs::write(
        &settings,
        r#"{"model":"opus","hooks":{"Stop":[{"hooks":[{"type":"command","command":"/usr/local/bin/somebody-else"}]}]}}"#,
    )
    .expect("seed settings");
    std::fs::write(
        &claude_json,
        r#"{"numStartups":7,"unknownFuture":{"nested":[1,2,3]},"mcpServers":{"somebody":{"type":"stdio","command":"/usr/bin/other","args":[]}}}"#,
    )
    .expect("seed claude.json");

    let s = settings.to_str().unwrap();
    let c = claude_json.to_str().unwrap();
    // `--yes`: stdin here is not a terminal, so without the flag nobody is
    // there to consent and the speakers are not installed (INIT-CONSENT-NO-TTY,
    // pinned below). This test is about what a consented init writes.
    let (stdout, stderr, ok) = run(&db, &["init", "--yes", "--settings", s, "--claude-json", c]);
    assert!(ok, "init failed: {stdout} {stderr}");

    // The report names both files and what was written into each.
    assert!(
        stdout.contains(s),
        "the report names the settings file: {stdout}"
    );
    assert!(
        stdout.contains(c),
        "the report names the config file: {stdout}"
    );
    assert!(
        stdout.contains("SessionStart") && stdout.contains("mcpServers.thruline"),
        "the report names the entries it wrote: {stdout}"
    );

    // Hooks: the full sensor set is registered; the foreign hook survives.
    let installed = read_json(&settings);
    assert_eq!(installed["model"], "opus", "foreign top-level keys survive");
    for event in thruline_core::sensor::install_set() {
        assert!(
            installed["hooks"][event].is_array(),
            "{event} was not registered"
        );
    }
    // A200: Stop carries the foreign hook, the sensor appender, and the
    // speaker audit — consented to by `--yes`.
    let stop = installed["hooks"]["Stop"].as_array().expect("Stop entries");
    assert_eq!(
        stop.len(),
        3,
        "sensor and speaker append beside the foreign entry"
    );
    for (event, verb) in [
        ("SessionStart", "brief --hook session-start"),
        ("Stop", "audit --hook stop"),
        ("SessionEnd", "digest precompute --hook session-end"),
    ] {
        let entries = installed["hooks"][event].as_array().expect("entries");
        assert!(
            entries.iter().any(|e| e["hooks"][0]["command"]
                .as_str()
                .is_some_and(|c| c.ends_with(verb))),
            "the {event} speaker entry was not registered"
        );
    }

    // MCP: one key of ours, beside the foreign server, unknown fields intact.
    let config = read_json(&claude_json);
    assert_eq!(config["numStartups"], 7, "unknown scalar keys survive");
    assert_eq!(
        config["unknownFuture"]["nested"],
        serde_json::json!([1, 2, 3]),
        "unknown structured keys survive"
    );
    assert!(
        config["mcpServers"]["somebody"].is_object(),
        "the foreign MCP server survives"
    );
    let ours = &config["mcpServers"]["thruline"];
    assert_eq!(ours["type"], "stdio");
    assert!(
        ours["command"]
            .as_str()
            .is_some_and(|cmd| Path::new(cmd).file_name().is_some_and(|n| n == "thruline")),
        "the entry invokes the thruline binary: {ours}"
    );
    assert_eq!(
        ours["args"],
        serde_json::json!(["mcp", "--agent", "claude", "--db", db.to_str().unwrap()]),
        "the entry serves this ledger over MCP"
    );

    // Every first-level line speaks the product's voice.
    //
    // **Filesystem paths are removed first, and that is a fix rather than a
    // convenience.** This assertion is about the words ThruLine writes, and
    // `init`'s report prints the paths it touched. Observed failing on a
    // workspace gate: `mktemp` produced `.tmpNLIrdo`, the census matched `nli`
    // inside the temporary directory's own random name, and the test failed
    // over a string neither this product nor this repository authored. It would
    // have done so roughly once in a few hundred runs, for as long as the test
    // existed — which is a guard that fails at random, and a guard that fails
    // at random is one people learn to re-run.
    //
    // Every word the product DID author is still checked: the paths are the
    // only thing removed, and `hooks`, `settings.json` and the rest of each
    // line stay in the search space.
    let tmp_root = temp.path().to_string_lossy().to_string();
    for line in stdout.lines() {
        let spoken = line.replace(&tmp_root, "<tmp>");
        assert_eq!(
            thruline_core::voice::banned_in_output(&spoken),
            None,
            "engine lexicon reached init's report: {spoken:?}"
        );
    }
}

#[test]
fn init_is_idempotent_and_creates_a_missing_config() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let settings = temp.path().join("settings.json");
    let claude_json = temp.path().join("claude.json");
    let s = settings.to_str().unwrap();
    let c = claude_json.to_str().unwrap();

    // Neither file exists: init creates both and says what it wrote.
    let (first, _, ok) = run(&db, &["init", "--settings", s, "--claude-json", c]);
    assert!(ok, "{first}");
    assert!(settings.exists() && claude_json.exists());
    let config = read_json(&claude_json);
    assert!(config["mcpServers"]["thruline"].is_object());

    // A second run changes nothing, and says so.
    let settings_before = std::fs::read_to_string(&settings).expect("read");
    let config_before = std::fs::read_to_string(&claude_json).expect("read");
    let (second, _, ok) = run(&db, &["init", "--settings", s, "--claude-json", c]);
    assert!(ok, "{second}");
    assert!(
        second.contains("nothing to do"),
        "a no-op run says so: {second}"
    );
    assert_eq!(
        settings_before,
        std::fs::read_to_string(&settings).expect("read"),
        "a second init must not rewrite the settings"
    );
    assert_eq!(
        config_before,
        std::fs::read_to_string(&claude_json).expect("read"),
        "a second init must not rewrite the config"
    );
}

#[test]
fn init_undo_reverses_exactly_what_init_wrote() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let settings = temp.path().join("settings.json");
    let claude_json = temp.path().join("claude.json");
    std::fs::write(
        &settings,
        r#"{"model":"opus","hooks":{"Stop":[{"hooks":[{"type":"command","command":"/usr/local/bin/somebody-else"}]}]}}"#,
    )
    .expect("seed settings");
    std::fs::write(
        &claude_json,
        r#"{"numStartups":7,"mcpServers":{"somebody":{"type":"stdio","command":"/usr/bin/other","args":[]}}}"#,
    )
    .expect("seed claude.json");
    let s = settings.to_str().unwrap();
    let c = claude_json.to_str().unwrap();

    let (_, _, ok) = run(&db, &["init", "--settings", s, "--claude-json", c]);
    assert!(ok);
    let (undo, _, ok) = run(
        &db,
        &["init", "--undo", "--settings", s, "--claude-json", c],
    );
    assert!(ok, "{undo}");

    // Hooks: exactly ours removed, the foreign one intact.
    let after = read_json(&settings);
    assert_eq!(after["model"], "opus");
    let stop = after["hooks"]["Stop"].as_array().expect("Stop entries");
    assert_eq!(stop.len(), 1, "exactly ours was removed");
    assert!(
        stop[0]["hooks"][0]["command"]
            .as_str()
            .is_some_and(|cmd| cmd.contains("somebody-else")),
        "the foreign hook survives"
    );

    // MCP: our key gone, the foreign server and unknown keys intact.
    let config = read_json(&claude_json);
    assert!(
        config["mcpServers"].get("thruline").is_none(),
        "our entry is gone"
    );
    assert!(config["mcpServers"]["somebody"].is_object());
    assert_eq!(config["numStartups"], 7);

    // A second undo removes nothing, and says so.
    let config_before = std::fs::read_to_string(&claude_json).expect("read");
    let (second, _, ok) = run(
        &db,
        &["init", "--undo", "--settings", s, "--claude-json", c],
    );
    assert!(ok, "{second}");
    assert!(second.contains("nothing"), "a no-op undo says so: {second}");
    assert_eq!(
        config_before,
        std::fs::read_to_string(&claude_json).expect("read"),
        "a second undo must not rewrite the config"
    );
}

#[test]
fn init_never_overwrites_or_removes_a_foreign_thruline_entry() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let settings = temp.path().join("settings.json");
    let claude_json = temp.path().join("claude.json");
    // A `thruline` key someone else wrote: not our command, not our shape.
    let foreign = r#"{"mcpServers":{"thruline":{"type":"stdio","command":"/usr/bin/not-ours","args":["serve"]}}}"#;
    std::fs::write(&claude_json, foreign).expect("seed claude.json");
    let s = settings.to_str().unwrap();
    let c = claude_json.to_str().unwrap();

    let (stdout, _, ok) = run(&db, &["init", "--settings", s, "--claude-json", c]);
    assert!(ok, "{stdout}");
    assert!(
        stdout.contains("left alone"),
        "the report states the entry was not touched: {stdout}"
    );
    let config = read_json(&claude_json);
    assert_eq!(
        config["mcpServers"]["thruline"]["command"], "/usr/bin/not-ours",
        "a foreign entry under our key is never overwritten"
    );

    let (undo, _, ok) = run(
        &db,
        &["init", "--undo", "--settings", s, "--claude-json", c],
    );
    assert!(ok, "{undo}");
    assert!(
        undo.contains("left alone"),
        "undo states the entry was not touched: {undo}"
    );
    let config = read_json(&claude_json);
    assert_eq!(
        config["mcpServers"]["thruline"]["command"], "/usr/bin/not-ours",
        "a foreign entry under our key is never removed"
    );
}

/// The three speaker verbs, one per event, as `init_speaker_install_into`
/// writes them.
const SPEAKERS: [(&str, &str); 3] = [
    ("SessionStart", "brief --hook session-start"),
    ("Stop", "audit --hook stop"),
    ("SessionEnd", "digest precompute --hook session-end"),
];

/// How many of the three speaker entries the settings file carries.
fn speakers_installed(settings: &Path) -> usize {
    let installed = read_json(settings);
    SPEAKERS
        .iter()
        .filter(|(event, verb)| {
            installed["hooks"][*event]
                .as_array()
                .is_some_and(|entries| {
                    entries.iter().any(|e| {
                        e["hooks"][0]["command"]
                            .as_str()
                            .is_some_and(|c| c.ends_with(verb))
                    })
                })
        })
        .count()
}

/// INIT-CONSENT-NO-TTY (D-344, known issue 2 of v0.1.0). A200 puts the speaker
/// hooks behind an explicit consent. When `init` runs without a terminal — an
/// agent running it, or a script — nobody is there to give it, so the answer
/// is no, whatever arrives on stdin, and one line names the flag that opts in.
/// It used to take the `[Y/n]` default and install the speakers with no
/// person's yes.
#[test]
fn init_without_a_terminal_installs_no_speaker_hooks_and_names_the_flag() {
    // stdin is not a terminal in both runs: empty (EOF), then an agent's "y".
    for piped in [None, Some("y\n")] {
        let temp = tempdir().expect("tempdir");
        let db = temp.path().join("ledger.db");
        let settings = temp.path().join("settings.json");
        let claude_json = temp.path().join("claude.json");
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_thruline"));
        cmd.args(["--db", db.to_str().unwrap()])
            .args(["init", "--settings", settings.to_str().unwrap()])
            .args(["--claude-json", claude_json.to_str().unwrap()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().expect("run thruline");
        {
            let mut stdin = child.stdin.take().expect("stdin");
            if let Some(answer) = piped {
                stdin.write_all(answer.as_bytes()).expect("write stdin");
            }
        }
        let out = child.wait_with_output().expect("wait");
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        assert!(
            out.status.success(),
            "init failed ({piped:?}): {stdout} {}",
            String::from_utf8_lossy(&out.stderr)
        );

        assert_eq!(
            speakers_installed(&settings),
            0,
            "no terminal, no consent: no speaker entry may be written ({piped:?}): {stdout}"
        );
        // The rest of init still happens: the sensor pack and the context server.
        let installed = read_json(&settings);
        for event in thruline_core::sensor::install_set() {
            assert!(
                installed["hooks"][event].is_array(),
                "{event} was not registered"
            );
        }
        assert!(read_json(&claude_json)["mcpServers"]["thruline"].is_object());

        let naming: Vec<&str> = stdout
            .lines()
            .filter(|l| l.contains("thruline init --yes"))
            .collect();
        assert_eq!(
            naming.len(),
            1,
            "exactly one line names the flag that opts in ({piped:?}): {stdout}"
        );
        assert!(
            !stdout.contains("by your answer"),
            "nobody answered, so the report must not say somebody did ({piped:?}): {stdout}"
        );
        let tmp_root = temp.path().to_string_lossy().to_string();
        for line in stdout.lines() {
            let spoken = line.replace(&tmp_root, "<tmp>");
            assert_eq!(
                thruline_core::voice::banned_in_output(&spoken),
                None,
                "engine lexicon reached init's report: {spoken:?}"
            );
        }
    }
}

/// `init` on a real terminal, answering `answer` to its one question.
#[cfg(unix)]
fn init_on_a_terminal(answer: &str) -> (String, usize) {
    use std::io::Read;
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let settings = temp.path().join("settings.json");
    let claude_json = temp.path().join("claude.json");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_thruline"));
    cmd.args(["--db", db.to_str().unwrap()])
        .args(["init", "--settings", settings.to_str().unwrap()])
        .args(["--claude-json", claude_json.to_str().unwrap()]);
    let mut pty = thruline_capture::pty::spawn_on_pty(cmd).expect("pseudoterminal");
    // Typed ahead: the terminal holds the line until init reads it.
    pty.controller
        .write_all(answer.as_bytes())
        .expect("type the answer");
    let mut out = Vec::new();
    // EOF on a controller is `Ok(0)` on some platforms and `EIO` on others.
    let _ = pty.controller.read_to_end(&mut out);
    let status = pty.child.wait().expect("wait");
    let text = String::from_utf8_lossy(&out).replace('\r', "");
    assert!(status.success(), "init failed on a terminal: {text}");
    (text, speakers_installed(&settings))
}

/// The terminal path is unchanged: one question, a plain Enter is yes.
#[cfg(unix)]
#[test]
fn init_on_a_terminal_asks_and_a_plain_enter_installs_the_speakers() {
    let (text, installed) = init_on_a_terminal("\n");
    assert!(
        text.contains("[Y/n]"),
        "a person at a terminal is asked: {text}"
    );
    assert_eq!(installed, 3, "Enter is yes on a terminal: {text}");
}

/// ... and an `n` is respected without argument.
#[cfg(unix)]
#[test]
fn init_on_a_terminal_respects_an_n() {
    let (text, installed) = init_on_a_terminal("n\n");
    assert!(
        text.contains("[Y/n]"),
        "a person at a terminal is asked: {text}"
    );
    assert_eq!(installed, 0, "an n writes no speaker entry: {text}");
    assert!(
        text.contains("by your answer"),
        "the report says why: {text}"
    );
}

// ───────────────────────── `init --db`: the ledger travels with the hooks ─────────────────────────
//
// Found by the live test of the public build: `thruline init --db X` wrote the
// context server's `--db X` and nothing else, so every hook it installed fell
// back to `$HOME/.thruline/ledger.db` when the harness fired it. Nothing
// failed — a hook exits 0 in silence — the record simply went to a ledger the
// person never named.

const BIN: &str = env!("CARGO_BIN_EXE_thruline");

/// Every ThruLine hook command in the settings file as `(event, command)`.
/// ThruLine's own commands are the ones that begin with this binary; the
/// foreign entry seeded by the tests that need one does not.
fn thruline_hook_commands(settings: &Path) -> Vec<(String, String)> {
    let installed = read_json(settings);
    let mut out = Vec::new();
    for (event, entries) in installed["hooks"].as_object().into_iter().flatten() {
        for entry in entries.as_array().into_iter().flatten() {
            for hook in entry["hooks"].as_array().into_iter().flatten() {
                if let Some(command) = hook["command"].as_str() {
                    if command.starts_with(BIN) {
                        out.push((event.clone(), command.to_string()));
                    }
                }
            }
        }
    }
    out.sort();
    out
}

/// What the harness would run for each hook `init --yes --gate` installs, when
/// the ledger was named as `db_word` (already shell-quoted, or empty).
fn expected_hook_commands(db_word: &str) -> Vec<(String, String)> {
    let global = if db_word.is_empty() {
        String::new()
    } else {
        format!(" --db {db_word}")
    };
    let mut out: Vec<(String, String)> = thruline_core::sensor::install_set()
        .into_iter()
        .map(|event| {
            (
                event.to_string(),
                format!("{BIN}{global} sensor append {event}"),
            )
        })
        .collect();
    for (event, verb) in SPEAKERS {
        out.push((event.to_string(), format!("{BIN}{global} {verb}")));
    }
    out.push((
        "Stop".to_string(),
        format!("{BIN}{global} gate --hook stop"),
    ));
    out.sort();
    out
}

#[test]
fn init_with_a_db_pins_it_into_every_hook_and_the_context_server_and_undo_removes_them() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let settings = temp.path().join("settings.json");
    let claude_json = temp.path().join("claude.json");
    std::fs::write(
        &settings,
        r#"{"model":"opus","hooks":{"Stop":[{"hooks":[{"type":"command","command":"/usr/local/bin/somebody-else"}]}]}}"#,
    )
    .expect("seed settings");
    std::fs::write(
        &claude_json,
        r#"{"numStartups":7,"mcpServers":{"somebody":{"type":"stdio","command":"/usr/bin/other","args":[]}}}"#,
    )
    .expect("seed claude.json");
    let s = settings.to_str().unwrap();
    let c = claude_json.to_str().unwrap();

    let (stdout, stderr, ok) = run(
        &db,
        &[
            "init",
            "--yes",
            "--gate",
            "--settings",
            s,
            "--claude-json",
            c,
        ],
    );
    assert!(ok, "init failed: {stdout} {stderr}");

    // Every hook — the sensor pack, the three speakers and the gate — names the
    // ledger, in the form every recogniser reads: the global flag before the
    // verb, so the verb is still the end of the command.
    let quoted = format!("'{}'", db.display());
    let commands = thruline_hook_commands(&settings);
    assert!(
        commands.len() > 4,
        "a walk that found almost nothing would pass: {commands:?}"
    );
    assert_eq!(
        commands,
        expected_hook_commands(&quoted),
        "a hook init installed does not carry the ledger it was told to use"
    );

    // The context server already named it; it still does.
    let config = read_json(&claude_json);
    assert_eq!(
        config["mcpServers"]["thruline"]["args"],
        serde_json::json!(["mcp", "--agent", "claude", "--db", db.to_str().unwrap()]),
    );

    // Undo, given the same `--db`, finds and removes exactly those entries.
    let (undo, _, ok) = run(
        &db,
        &["init", "--undo", "--settings", s, "--claude-json", c],
    );
    assert!(ok, "{undo}");
    assert_eq!(
        thruline_hook_commands(&settings),
        Vec::<(String, String)>::new(),
        "undo left a ThruLine hook behind: {undo}"
    );
    let after = read_json(&settings);
    let stop = after["hooks"]["Stop"].as_array().expect("Stop entries");
    assert_eq!(stop.len(), 1, "exactly ours was removed");
    assert!(stop[0]["hooks"][0]["command"]
        .as_str()
        .is_some_and(|cmd| cmd.contains("somebody-else")));
    let config = read_json(&claude_json);
    assert!(config["mcpServers"].get("thruline").is_none());
    assert!(config["mcpServers"]["somebody"].is_object());

    // And nothing in a run that already had it all installed is rewritten.
    let (_, _, ok) = run(
        &db,
        &[
            "init",
            "--yes",
            "--gate",
            "--settings",
            s,
            "--claude-json",
            c,
        ],
    );
    assert!(ok);
    let before = std::fs::read_to_string(&settings).expect("read");
    let (again, _, ok) = run(
        &db,
        &[
            "init",
            "--yes",
            "--gate",
            "--settings",
            s,
            "--claude-json",
            c,
        ],
    );
    assert!(ok, "{again}");
    assert!(again.contains("nothing to do"), "{again}");
    assert_eq!(before, std::fs::read_to_string(&settings).expect("read"));
}

#[test]
fn init_without_a_db_installs_hooks_that_name_none_so_each_machine_resolves_its_own() {
    let temp = tempdir().expect("tempdir");
    let home = temp.path().join("home");
    std::fs::create_dir_all(&home).expect("home");
    let settings = temp.path().join("settings.json");
    let claude_json = temp.path().join("claude.json");

    // No `--db`: the ledger is the default one under this HOME (a private one).
    let out = Command::new(BIN)
        .args(["init", "--yes", "--gate", "--settings"])
        .arg(&settings)
        .arg("--claude-json")
        .arg(&claude_json)
        .env("HOME", &home)
        .env_remove("THRULINE_HOME")
        .output()
        .expect("run thruline init");
    assert!(
        out.status.success(),
        "init failed: {} {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    assert_eq!(
        thruline_hook_commands(&settings),
        expected_hook_commands(""),
        "with no --db given, a hook must not bake the default ledger path in"
    );
    // The context server is unchanged: it has always named the ledger it serves.
    let config = read_json(&claude_json);
    assert_eq!(
        config["mcpServers"]["thruline"]["args"],
        serde_json::json!([
            "mcp",
            "--agent",
            "claude",
            "--db",
            home.join(".thruline/ledger.db").to_str().unwrap()
        ]),
    );

    // Undo, also without `--db`, removes all of it.
    let out = Command::new(BIN)
        .args(["init", "--undo", "--settings"])
        .arg(&settings)
        .arg("--claude-json")
        .arg(&claude_json)
        .env("HOME", &home)
        .env_remove("THRULINE_HOME")
        .output()
        .expect("run thruline init --undo");
    assert!(out.status.success());
    assert!(thruline_hook_commands(&settings).is_empty());
}

#[test]
fn a_hook_init_wrote_with_a_db_records_into_that_ledger_and_not_the_default_one() {
    // The bug itself, end to end: fire the installed command the way the
    // harness does — through a shell, with a HOME that has no ledger — and look
    // at where the row went. The directory and the file name both need quoting
    // (a space, and a single quote), because a mangled `--db` is silent too.
    let temp = tempdir().expect("tempdir");
    let home = temp.path().join("home");
    std::fs::create_dir_all(&home).expect("home");
    let dir = temp.path().join("my ledgers");
    std::fs::create_dir_all(&dir).expect("ledger dir");
    let db = dir.join("it's.db");
    let settings = temp.path().join("settings.json");
    let claude_json = temp.path().join("claude.json");

    let (stdout, stderr, ok) = run(
        &db,
        &[
            "init",
            "--yes",
            "--settings",
            settings.to_str().unwrap(),
            "--claude-json",
            claude_json.to_str().unwrap(),
        ],
    );
    assert!(ok, "init failed: {stdout} {stderr}");

    let (_, command) = thruline_hook_commands(&settings)
        .into_iter()
        .find(|(event, c)| event == "Stop" && c.ends_with(" sensor append Stop"))
        .expect("the Stop sensor entry");
    let mut child = Command::new("sh")
        .args(["-c", &command])
        .env("HOME", &home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run the hook as the harness would");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(br#"{"session_id":"s1","transcript_path":"/t/s1.jsonl","cwd":"/w","hook_event_name":"Stop"}"#)
        .expect("payload");
    let out = child.wait_with_output().expect("wait");
    assert!(out.status.success());
    assert!(
        out.stdout.is_empty(),
        "a sensor never speaks into the session"
    );

    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open the named ledger");
    assert_eq!(
        thruline_core::sensor::summary(&ledger).expect("summary"),
        vec![("Stop".to_string(), 1)],
        "the delivery did not reach the ledger init was told to use: {command}"
    );
    assert!(
        !home.join(".thruline").exists(),
        "the hook fell back to the default ledger under HOME: {command}"
    );
}

#[test]
fn a_relative_db_is_made_absolute_because_a_hook_runs_somewhere_else() {
    let temp = tempdir().expect("tempdir");
    let cwd = temp.path().canonicalize().expect("canonical tempdir");
    let settings = cwd.join("settings.json");
    let claude_json = cwd.join("claude.json");

    let out = Command::new(BIN)
        .args(["--db", "ledger.db", "init", "--yes", "--settings"])
        .arg(&settings)
        .arg("--claude-json")
        .arg(&claude_json)
        .current_dir(&cwd)
        .output()
        .expect("run thruline init");
    assert!(
        out.status.success(),
        "init failed: {} {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let absolute = cwd.join("ledger.db");
    let quoted = format!("'{}'", absolute.display());
    let sensor: Vec<String> = thruline_hook_commands(&settings)
        .into_iter()
        .map(|(_, c)| c)
        .filter(|c| c.contains(" sensor append "))
        .collect();
    assert!(!sensor.is_empty());
    for command in &sensor {
        assert!(
            command.contains(&format!(" --db {quoted} sensor append ")),
            "a relative --db would mean a different file for every session cwd: {command}"
        );
    }
    let config = read_json(&claude_json);
    assert_eq!(
        config["mcpServers"]["thruline"]["args"][4],
        absolute.to_str().unwrap(),
        "the context server's --db is absolute too"
    );
}

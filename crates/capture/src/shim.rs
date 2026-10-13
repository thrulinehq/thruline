//! BUILD-SPEC §4.4 — the PATH shim directory.
//!
//! > *"fallback to shell-wrapper injection (`PATH` shim directory that wraps
//! > common runners: `pytest`, `cargo`, `go`, `npm`, `pnpm`, `yarn`, `npx`,
//! > `make`, `mvn`, `gradle`, `dotnet`). Ship the shim path first; it is
//! > portable and sufficient for 90% of value."*
//!
//! A wrapper is a five-line POSIX `sh` script that `exec`s
//! `thruline shim exec`, which records the invocation and then runs the real
//! program. Everything about the design follows from two properties it must
//! not lose.
//!
//! # 1. It must fail open
//!
//! If the ThruLine binary is deleted, moved, or was built for another
//! architecture, a wrapper that simply `exec`s it would make **every**
//! `cargo`, `npm` and `make` on the user's machine fail. So each wrapper tests
//! the recorder first and, if it is not executable, strips the shim directory
//! out of `PATH` and `exec`s the real program — in pure shell, with no external
//! commands, because `grep` and `paste` are not guaranteed to be on a `PATH`
//! this script has just rewritten.
//!
//! This is the same contract the hook path already keeps (`claude_code::run`
//! always exits 0): capture must never be the reason a developer's command
//! breaks.
//!
//! # 2. It must be checkable
//!
//! §13 lists *"hooks removed or PATH shims bypassed"* as a threat, and the
//! answer is that the install is **hashed**. [`ShimManifest`] records the
//! SHA-256 of every wrapper it wrote, and [`verify`] reports a wrapper that was
//! edited, deleted, or added by someone else. A shim that quietly stopped
//! recording, or started recording something else, is exactly the failure that
//! makes a receipt lie, so `doctor` has to be able to see it.
//!
//! Wrapper scripts are byte-deterministic given `(thruline_bin, shim_dir,
//! program)`, so a hash mismatch means an edit and nothing else.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The runners §4.4 names, in the spec's order, plus the launchers a measured
/// corpus showed the spec's list does not reach.
///
/// # HISTORY — the measurement that produced this list
///
/// **Everything under this heading is the state BEFORE `uv`, `uvx`, `python`
/// and `python3` were added, and it is the argument for adding them. It is not
/// a description of what this build captures.** It is marked because two
/// tracks read it two ways in the same week: Track R quoted the 3.7% as a
/// current figure, and Track H declined it as stale. Both were reading the same
/// paragraph, and the paragraph did not say which it was.
///
/// ## The spec's list captured 3.7% of Terminal-Bench, not 90%
///
/// Measured over all 241 `run-tests.sh` in terminal-bench @ `d28711d0`, by
/// simulating PATH resolution against the pilot's container topology
/// (`scripts/pilot-run.sh`) and classifying the invocation each script's test
/// line would actually reach:
///
/// ```text
/// uv run pytest              219 tasks   `uv` was not wrapped
/// bare pytest, no venv         9 tasks   captured
/// python[3] -m pytest          7 tasks   `python` was not wrapped
/// bare pytest, venv active     6 tasks   venv bin precedes the shim — see below
/// ------------------------------------------------------------------
/// captured                     9 / 241 = 3.7%
/// ```
///
/// `uv` alone accounted for 219 of the 232 misses, so it was not a long-tail
/// addition: it is the single program that corpus runs its tests through.
/// `python`/`python3` close the `-m pytest` shape, which `Runner::from_argv`
/// has always parsed and the capture layer could never deliver — a parser arm
/// written for input no wrapper could produce.
///
/// `uvx` is here because it is `uv tool run` under another name and the parser
/// treats them identically; wrapping one and not the other would recreate that
/// same asymmetry on purpose.
///
/// # CURRENT — what this list cannot fix, and does not pretend to
///
/// **6 of the 241 stay uncaptured and no wrapper list can change that.** They
/// `source <venv>/bin/activate` and then run a bare `pytest` (or
/// `python -m pytest`) that the venv's own `bin/` provides: activation
/// prepends that directory, so PATH resolution finds the venv's real program
/// before it ever reaches the shim. A wrapper is only ever consulted when it
/// is *first* on PATH, and a venv is entitled to be first in its own shell.
/// That is a design limit of PATH interposition, not a gap in this list, and
/// the honest consequence is that those six tasks yield `unverifiable`.
///
/// A second shadowing case *is* fixable and was fixed where it belongs: 217 of
/// the 219 `uv` tasks `source $HOME/.local/bin/env`, which **prepends**
/// `$HOME/.local/bin` — where uv's own installer puts `uv`. Wrapping `uv` here
/// changes nothing on its own for them; the pilot's PATH has to already carry
/// `$HOME/.local/bin` (behind the shim) so that env script finds it present
/// and prepends nothing. See `scripts/pilot-run.sh`.
///
/// # CURRENT — not added, deliberately
///
/// `pip`/`pip3` and `poetry`: no `from_argv` arm maps any of them to a runner,
/// so wrapping them would add exec rows and **zero** test evidence — the exact
/// half-a-mechanism this doc comment exists to record. Add the parser arm and
/// the wrapper together, or neither.
///
/// # CURRENT — this is a candidate list, not the written set
///
/// Since [`WrapPolicy::Resolvable`] became the default, a name here gets a
/// wrapper only if a real program of that name resolves. The historical
/// numbers above are unchanged by that — `uv` still carried 219 of the 241
/// **misses** in that corpus, which is a fact about the pre-widening state and
/// not about what is captured now — **and only on the condition that `uv` is
/// present when the shim is installed.** It is not, in 237 of the
/// 241 task images by default, which is why the pilot image pre-provisions it
/// at build time rather than letting the shim wrap a name that is not there
/// yet. Reading this list as "what is being captured" is wrong; read
/// [`ShimManifest::wrappers`].
pub const WRAPPED: &[&str] = &[
    "pytest", "cargo", "go", "npm", "pnpm", "yarn", "npx", "make", "mvn", "gradle", "dotnet", "uv",
    "uvx", "python", "python3",
];

/// Which of the [`WRAPPED`] candidates actually get a wrapper written.
///
/// # Why the default is not "all of them"
///
/// A wrapper is a real, executable file with a program's name, first on
/// `PATH`. Writing one for a program the machine does not have does not
/// capture anything — there is nothing to capture — but it **does** answer
/// `command -v <name>` in the affirmative, and scripts branch on that answer.
///
/// Measured, on the Terminal-Bench corpus, with the oracle (the *known-good*
/// solution) driving:
///
/// * `lean4-proof`: `command -v pytest || uv pip install pytest`. The shim's
///   `pytest` satisfied the test, the install was skipped, and the grader died
///   at exit 127 — **after the oracle's own work had been verified complete**.
///   A correct solution was scored as a failure by the instrument measuring it.
/// * The same shape suppressed uv's `$HOME/.local/bin/env` script in 219 of
///   241 tasks' `run-tests.sh`.
///
/// The three tasks broken this way produced **empty** spools, so they entered
/// the coverage average as zeros: the instrument's damage was reported as the
/// subject's under-performance. That is the failure mode this enum exists to
/// remove, and it is why the default is [`WrapPolicy::Resolvable`] rather than
/// the literal list in §4.4.
///
/// [`WrapPolicy::All`] is kept because it is what §4.4 says, and because the
/// wrapper-script tests are about bytes rather than about the machine they run
/// on. It is never what an install chooses.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WrapPolicy {
    /// Wrap a name only if a real program of that name resolves. The default.
    #[default]
    Resolvable,
    /// Wrap every candidate, resolvable or not (§4.4 literally).
    All,
    /// Wrap only these names, of the candidates.
    Only(Vec<String>),
}

/// **The predicate.** Does a real program of this name resolve, ignoring our
/// own wrappers?
///
/// This is [`crate::exec::resolve_program`] and nothing else — the very call
/// the recorder makes when it runs the child. Asking the question a second way
/// would let install-time and run-time disagree, and the disagreement would be
/// invisible: a wrapper written for a name the recorder cannot resolve exits
/// 127 for every invocation.
pub fn resolves(program: &str, shim_dir: &Path) -> bool {
    crate::exec::resolve_program(program, Some(shim_dir)).is_some()
}

/// [`resolves`] against a named `PATH` rather than the process's.
///
/// Install uses this so the PATH the predicate was evaluated against is a
/// recorded input ([`ShimManifest::install_path`]) rather than ambient state.
pub fn resolves_on(program: &str, shim_dir: &Path, path_var: Option<&OsStr>) -> bool {
    crate::exec::resolve_program_on(program, Some(shim_dir), path_var).is_some()
}

/// The manifest file inside the shim directory.
pub const MANIFEST: &str = "thruline-shim.json";

/// Env var naming the programs a recorder is currently recording, colon
/// delimited (`:pytest:cargo:`).
///
/// **The re-entry guard, and the reason it is per PROGRAM rather than a plain
/// depth counter.** A wrapper whose program is already in this set fails open
/// immediately, which breaks a cycle through a third-party shim that
/// re-dispatches the same name. Keying it on the program preserves the case
/// that matters: `make` -> `pytest` is a different program at depth 1 and is
/// still recorded, where a blanket "fail open above depth 1" would lose every
/// nested runner -- which is most of the evidence §6.2 exists to read.
pub const ACTIVE_VAR: &str = "THRULINE_SHIM_ACTIVE";

/// Env var carrying wrapper nesting depth. Shared with `capture::exec`, which
/// records it on the event as `depth`.
pub const DEPTH_VAR_NAME: &str = "THRULINE_EXEC_DEPTH";

/// Depth at which the WRAPPER fails open, in shell, before the recorder runs.
///
/// The backstop for a cycle that passes through several different program names
/// (`a` -> `b` -> `c` -> `a`), which the per-program set alone would not close
/// until it came back round. Legitimate nesting is shallow -- `make` -> `npm`
/// -> `npx` -> `pytest` is 4 -- so 8 is far above any real chain and far below
/// a hang.
///
/// This has to be enforced in the WRAPPER, not in the recorder: the recorder's
/// own escape hatch used to be `passthrough`, which runs the resolved program
/// -- and in a pyenv cycle the resolved program IS the third-party shim, so it
/// re-entered at the same depth forever. Failing open in shell is the only exit
/// that leaves the cycle.
pub const WRAPPER_MAX_DEPTH: u32 = 8;

/// Version of the wrapper script format. A change here changes every hash, so
/// `doctor` reports "wrapper differs" until the user reinstalls — which is the
/// intended behaviour, not a bug.
pub const WRAPPER_VERSION: u32 = 1;

/// Something went wrong installing or reading a shim directory.
#[derive(Debug, thiserror::Error)]
pub enum ShimError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{0} could not be made absolute, and a shim script cannot carry a relative path")]
    NotAbsolute(PathBuf),
    /// A path a POSIX `sh` script cannot carry safely.
    #[error(
        "{0:?} contains a newline; a shim script cannot quote it safely, so nothing was written"
    )]
    Unquotable(String),
    #[error("{path} is not a ThruLine shim directory (no {MANIFEST})")]
    NotAShimDir { path: PathBuf },
    #[error("{path}: {source}")]
    Manifest {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    /// The directory holds files ThruLine did not write. Refusing beats
    /// clobbering: this could be `~/bin`.
    #[error("{path} already holds {count} file(s) ThruLine did not write ({names}); refusing to install into it")]
    Occupied {
        path: PathBuf,
        count: usize,
        names: String,
    },
}

/// What a shim directory contains, and what this build wrote there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShimManifest {
    pub wrapper_version: u32,
    /// The recorder every wrapper `exec`s.
    pub thruline_bin: PathBuf,
    pub shim_dir: PathBuf,
    /// Where recorded events are spooled. The fold reads this file.
    pub spool: PathBuf,
    /// Where full redacted stdout/stderr is kept, content-addressed.
    pub output_store: PathBuf,
    /// The session recorded execs belong to, when it is known at install time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Fallback repository for snapshots when the working directory is not
    /// inside one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_root: Option<PathBuf>,
    pub created_at: String,
    /// program name → SHA-256 of the wrapper script written for it.
    ///
    /// **The written set, not the candidate set.** Under
    /// [`WrapPolicy::Resolvable`] these differ, and everything that reports
    /// what is being captured must read this one — a UI or a `shim install`
    /// banner that printed [`WRAPPED`] would name wrappers that do not exist.
    pub wrappers: BTreeMap<String, String>,
    /// The policy the write loop applied.
    #[serde(default)]
    pub wrap_policy: WrapPolicy,
    /// Every name considered, written or not. `verify` needs it: a file in the
    /// shim directory named after a candidate we *declined* to write is a
    /// forgery, and without the candidate list that is indistinguishable from
    /// a stray file.
    #[serde(default)]
    pub candidates: Vec<String>,
    /// name → why no wrapper was written for it.
    #[serde(default)]
    pub skipped: BTreeMap<String, String>,
    /// The `PATH` the predicate was evaluated against.
    ///
    /// **Recorded because `verify` cannot recover it.** Both callers of
    /// [`verify`] (`shim status`, `doctor`) pass `path_var: None`, so anything
    /// re-derived there would be derived from whoever happened to run `doctor`
    /// — a different shell, a different `PATH`, a different answer, reported as
    /// though it were a property of the install. The skips are only readable
    /// against the PATH that produced them, so that PATH is written down.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_path: Option<String>,
}

impl ShimManifest {
    /// Read the manifest from a shim directory.
    pub fn load(dir: &Path) -> Result<ShimManifest, ShimError> {
        let path = dir.join(MANIFEST);
        let text = std::fs::read_to_string(&path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                ShimError::NotAShimDir {
                    path: dir.to_path_buf(),
                }
            } else {
                ShimError::Io {
                    path: path.clone(),
                    source,
                }
            }
        })?;
        serde_json::from_str(&text).map_err(|source| ShimError::Manifest { path, source })
    }
}

/// What to install.
#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub dir: PathBuf,
    pub thruline_bin: PathBuf,
    pub spool: PathBuf,
    pub output_store: PathBuf,
    pub session_id: Option<String>,
    pub repo_root: Option<PathBuf>,
    /// Programs to wrap. Defaults to [`WRAPPED`].
    pub programs: Vec<String>,
    /// Which of `programs` actually get a wrapper. See [`WrapPolicy`].
    pub policy: WrapPolicy,
    /// The `PATH` the predicate is evaluated against. `None` means the
    /// process's, which is what every real install wants; tests name one so
    /// they need not mutate an environment shared with every other test in the
    /// binary.
    pub install_path: Option<OsString>,
    pub now: String,
}

impl InstallOptions {
    /// Defaults rooted at `home` (`$THRULINE_HOME`, or `~/.thruline`).
    pub fn under(home: &Path, thruline_bin: PathBuf, now: String) -> InstallOptions {
        InstallOptions {
            dir: home.join("shim"),
            thruline_bin,
            spool: home.join("spool").join("claude-code.jsonl"),
            output_store: home.join("exec-output"),
            session_id: None,
            repo_root: None,
            programs: WRAPPED.iter().map(|s| (*s).to_string()).collect(),
            // Every install goes through here — `shim install`, `thruline run`,
            // the pilot scripts. The amended predicate has to be the default
            // and not an opt-in, or the caller that forgets it is the one that
            // makes `command -v pytest` lie.
            policy: WrapPolicy::Resolvable,
            install_path: None,
            now,
        }
    }
}

/// Write the shim directory. Idempotent: reinstalling over a ThruLine shim
/// directory replaces exactly the files this build writes.
pub fn install(opts: &InstallOptions) -> Result<ShimManifest, ShimError> {
    let dir = absolute(&opts.dir)?;
    let bin = absolute(&opts.thruline_bin)?;
    quotable(&dir)?;
    quotable(&bin)?;

    std::fs::create_dir_all(&dir).map_err(|source| ShimError::Io {
        path: dir.clone(),
        source,
    })?;

    // Never clobber a directory that is not ours. `~/bin` is a plausible
    // `--dir` and it is full of scripts the user wrote.
    let ours: BTreeSet<String> = opts
        .programs
        .iter()
        .cloned()
        .chain(std::iter::once(MANIFEST.to_string()))
        .collect();
    let previous = ShimManifest::load(&dir).ok();
    let mut foreign: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let known = ours.contains(&name)
                || previous
                    .as_ref()
                    .is_some_and(|m| m.wrappers.contains_key(&name));
            if !known {
                foreign.push(name);
            }
        }
    }
    if !foreign.is_empty() {
        foreign.sort();
        return Err(ShimError::Occupied {
            count: foreign.len(),
            names: foreign.join(", "),
            path: dir,
        });
    }

    // The PATH the predicate is answered against, fixed once so every skip in
    // this manifest was decided against the same basis.
    let install_path: Option<OsString> = opts
        .install_path
        .clone()
        .or_else(|| std::env::var_os("PATH"));

    let mut wrappers = BTreeMap::new();
    let mut skipped: BTreeMap<String, String> = BTreeMap::new();
    for program in &opts.programs {
        if let Some(reason) = skip_reason(opts, program, &dir, install_path.as_deref()) {
            // A wrapper we decline to write must not survive from an earlier
            // install with a different answer. It would not be in
            // `manifest.wrappers`, so `verify` would file it under
            // `unexpected` — and `ShimIntegrity::ok()` does not read
            // `unexpected`. The exact file this skip exists to prevent would
            // sit on PATH, answer `command -v`, and pass `doctor` green.
            //
            // So the error is propagated. `let _ =` here would turn "I could
            // not remove the thing that makes the shim observable" into
            // silence, which is the whole failure this predicate is for.
            remove_stale(&dir.join(program))?;
            skipped.insert(program.clone(), reason);
            continue;
        }
        let script = wrapper_script(&bin, &dir, program);
        let path = dir.join(program);
        write_executable(&path, &script)?;
        wrappers.insert(
            program.clone(),
            thruline_core::chain::sha256_hex(script.as_bytes()),
        );
    }

    let manifest = ShimManifest {
        wrapper_version: WRAPPER_VERSION,
        thruline_bin: bin,
        shim_dir: dir.clone(),
        spool: opts.spool.clone(),
        output_store: opts.output_store.clone(),
        session_id: opts.session_id.clone(),
        repo_root: opts.repo_root.clone(),
        created_at: opts.now.clone(),
        wrappers,
        wrap_policy: opts.policy.clone(),
        candidates: opts.programs.clone(),
        skipped,
        install_path: install_path.map(|p| p.to_string_lossy().to_string()),
    };
    let path = dir.join(MANIFEST);
    let body = serde_json::to_string_pretty(&manifest).map_err(|source| ShimError::Manifest {
        path: path.clone(),
        source,
    })?;
    std::fs::write(&path, format!("{body}\n")).map_err(|source| ShimError::Io { path, source })?;
    Ok(manifest)
}

/// What an uninstall did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Removed {
    pub wrappers: Vec<String>,
    /// Wrappers whose bytes no longer matched the manifest. Removed — leaving a
    /// tampered wrapper on `PATH` would be worse — but **named**, because
    /// deleting the evidence without saying so is the failure this product is
    /// about.
    pub modified: Vec<String>,
    /// Files in the directory ThruLine did not write. Left alone.
    pub left_alone: Vec<String>,
    pub dir_removed: bool,
}

/// Remove exactly the wrappers named in the manifest, then the manifest.
pub fn uninstall(dir: &Path) -> Result<Removed, ShimError> {
    let manifest = ShimManifest::load(dir)?;
    let mut removed = Removed::default();
    for (program, expected) in &manifest.wrappers {
        let path = dir.join(program);
        match std::fs::read(&path) {
            Ok(bytes) => {
                if thruline_core::chain::sha256_hex(&bytes) != *expected {
                    removed.modified.push(program.clone());
                }
                std::fs::remove_file(&path).map_err(|source| ShimError::Io {
                    path: path.clone(),
                    source,
                })?;
                removed.wrappers.push(program.clone());
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => return Err(ShimError::Io { path, source }),
        }
    }
    let manifest_path = dir.join(MANIFEST);
    std::fs::remove_file(&manifest_path).map_err(|source| ShimError::Io {
        path: manifest_path,
        source,
    })?;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            removed
                .left_alone
                .push(entry.file_name().to_string_lossy().to_string());
        }
    }
    removed.left_alone.sort();
    if removed.left_alone.is_empty() {
        removed.dir_removed = std::fs::remove_dir(dir).is_ok();
    }
    Ok(removed)
}

/// The integrity of an installed shim directory (§13, `doctor`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShimIntegrity {
    pub installed: bool,
    /// Wrappers present with the exact bytes the manifest recorded.
    pub intact: Vec<String>,
    /// Present, but not the bytes ThruLine wrote. A wrapper that records
    /// something else, or nothing.
    pub modified: Vec<String>,
    /// In the manifest and gone from disk. Every command it wrapped is now
    /// invisible to the recorder.
    pub missing: Vec<String>,
    /// In the directory and not in the manifest.
    pub unexpected: Vec<String>,
    /// **In the directory, not in the manifest, and shaped like a wrapper or
    /// named after a candidate this install declined to write.**
    ///
    /// Computed over the *directory listing*, because that is the only place
    /// the thing can be seen: a check that walks `manifest.wrappers` can never
    /// notice a file the manifest does not mention, which is precisely what a
    /// forged wrapper is. `unexpected` already listed such files and
    /// [`ShimIntegrity::ok`] does not read `unexpected` — so a planted
    /// `pytest`, answering `command -v` for a pytest that does not exist,
    /// passed `doctor` green. This field is what makes it fail.
    #[serde(default)]
    pub forged: Vec<String>,
    /// Candidates that were skipped at install and whose real program resolves
    /// **now**. Their invocations are invisible to the recorder.
    ///
    /// Reported, and deliberately does **not** fail [`ShimIntegrity::ok`]: a
    /// tool installed after the shim was is a reinstall, not a compromise, and
    /// this is measured against the PATH of whoever ran `doctor` rather than
    /// against [`ShimManifest::install_path`].
    #[serde(default)]
    pub uncovered: Vec<String>,
    /// Is the shim directory actually on `PATH`? An intact shim that nothing
    /// invokes records nothing.
    pub on_path: bool,
    /// Is it *ahead* of the real programs it wraps? A shim behind
    /// `/usr/local/bin` on `PATH` is never reached.
    pub shadowed_by: Vec<String>,
    /// **Installed, and wrapping nothing at all.** A capture layer that
    /// captures nothing, reporting healthy.
    ///
    /// # It became reachable-as-green the day the predicate was amended
    ///
    /// Under [`WrapPolicy::All`] an install always wrote 15 wrappers, so
    /// `intact == 0` implied `missing == 15` and [`ok`](ShimIntegrity::ok)
    /// already failed. Under [`WrapPolicy::Resolvable`] an install where no
    /// candidate resolves writes **zero** wrappers and records **zero** in the
    /// manifest, so `missing` is empty too and nothing was left to object.
    /// Measured, with no forgery and no tampering: `shim status` printed
    /// `intact 0 / on PATH true` and exited 0, and `doctor` reported
    /// `{"key":"shim","ok":true,"detail":"0 wrappers intact and first on
    /// PATH"}`.
    ///
    /// That is not an exotic state. It is what a container whose `uv`
    /// provisioning silently failed looks like, and what any minimal image
    /// looks like — the exact case the pilot's whole build-time provisioning
    /// step exists to prevent, wearing a green check.
    ///
    /// So it fails `ok`. The remedy is not "reinstall and hope": read
    /// [`ShimManifest::skipped`], which names every candidate and why, against
    /// [`ShimManifest::install_path`], which is the PATH the predicate saw.
    #[serde(default)]
    pub captures_nothing: bool,
    pub problem: Option<String>,
}

impl ShimIntegrity {
    pub fn ok(&self) -> bool {
        self.problem.is_none()
            && self.modified.is_empty()
            && self.missing.is_empty()
            && self.forged.is_empty()
            && !self.captures_nothing
            && (!self.installed || self.on_path)
            && self.shadowed_by.is_empty()
    }
}

/// Check an installed shim directory against its manifest and against `PATH`.
pub fn verify(dir: &Path, path_var: Option<&str>) -> ShimIntegrity {
    let mut out = ShimIntegrity::default();
    let manifest = match ShimManifest::load(dir) {
        Ok(m) => m,
        Err(ShimError::NotAShimDir { .. }) => return out,
        Err(e) => {
            out.problem = Some(e.to_string());
            return out;
        }
    };
    out.installed = true;

    for (program, expected) in &manifest.wrappers {
        match std::fs::read(dir.join(program)) {
            Ok(bytes) => {
                if thruline_core::chain::sha256_hex(&bytes) == *expected {
                    out.intact.push(program.clone());
                } else {
                    out.modified.push(program.clone());
                }
            }
            Err(_) => out.missing.push(program.clone()),
        }
    }
    // Over `intact`, not over `manifest.wrappers`: a directory whose 15
    // wrappers were all deleted is `missing`, which is already a fault and a
    // different one. This is the state where nothing is wrong with any
    // individual file because there are no files — and the capture is
    // nevertheless total zero.
    out.captures_nothing = out.intact.is_empty();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name == MANIFEST || manifest.wrappers.contains_key(&name) {
                continue;
            }
            // A candidate we declined to write, or anything carrying our own
            // wrapper marker, is a forgery: ThruLine's directory, ThruLine's
            // name, bytes ThruLine did not write. Everything else in here is
            // merely unexpected — someone's stray script — which is reported
            // and is not a fault.
            if manifest.candidates.contains(&name) || crate::exec::is_our_wrapper(&dir.join(&name))
            {
                out.forged.push(name.clone());
            }
            out.unexpected.push(name);
        }
    }
    out.unexpected.sort();
    out.forged.sort();

    let path_var = path_var
        .map(str::to_string)
        .or_else(|| std::env::var_os("PATH").map(|p| p.to_string_lossy().to_string()));

    // What the shim is not covering. Uses the same single walk the recorder
    // uses, against the PATH this check was given.
    let basis: Option<OsString> = path_var.as_ref().map(OsString::from);
    for name in manifest.skipped.keys() {
        if resolves_on(name, dir, basis.as_deref()) {
            out.uncovered.push(name.clone());
        }
    }

    if let Some(path) = path_var {
        let entries: Vec<&str> = path.split(':').collect();
        let dir_str = dir.to_string_lossy().to_string();
        let position = entries.iter().position(|e| *e == dir_str);
        out.on_path = position.is_some();
        // Anything earlier on PATH that also provides a wrapped program wins,
        // and the shim never runs for it.
        if let Some(at) = position {
            for earlier in &entries[..at] {
                for program in manifest.wrappers.keys() {
                    if Path::new(earlier).join(program).is_file() {
                        out.shadowed_by.push(format!("{earlier}/{program}"));
                    }
                }
            }
            out.shadowed_by.sort();
            out.shadowed_by.dedup();
        }
    }
    out
}

/// The exact bytes of one wrapper. Deterministic in its three inputs, so a hash
/// mismatch means an edit.
pub fn wrapper_script(thruline_bin: &Path, shim_dir: &Path, program: &str) -> String {
    let bin = sh_quote(&thruline_bin.to_string_lossy());
    let dir = sh_quote(&shim_dir.to_string_lossy());
    let prog = sh_quote(program);
    format!(
        "#!/bin/sh\n\
         # ThruLine exec shim v{WRAPPER_VERSION} (BUILD-SPEC §4.4). Generated by\n\
         # `thruline shim install`. DO NOT EDIT: `thruline doctor` hashes this file\n\
         # and reports it as modified if it changes.\n\
         THRULINE_SHIM_DIR={dir}\n\
         export THRULINE_SHIM_DIR\n\
         # --- Re-entry guard. Read the note in `capture::shim` before touching. ---\n\
         # A third-party shim (pyenv, asdf, rbenv, ccache) carries neither our\n\
         # marker nor our path, so the recorder resolves TO it, and it re-dispatches\n\
         # the bare program name through PATH -- straight back into this wrapper,\n\
         # forever. Measured: a 25 s hang with the program never running.\n\
         _tl_open=0\n\
         [ -x {bin} ] || _tl_open=1\n\
         case \":${{{ACTIVE_VAR}-}}:\" in\n\
         \t*\":\"{prog}\":\"*) _tl_open=1 ;;\n\
         esac\n\
         case \"${{{DEPTH_VAR_NAME}-0}}\" in\n\
         \t''|*[!0-9]*) {DEPTH_VAR_NAME}=0 ;;\n\
         esac\n\
         [ \"${{{DEPTH_VAR_NAME}-0}}\" -lt {WRAPPER_MAX_DEPTH} ] || _tl_open=1\n\
         if [ \"$_tl_open\" = 0 ]; then\n\
         \texec {bin} shim exec --program {prog} -- \"$@\"\n\
         fi\n\
         # Fail open: run the real program rather than breaking the user's shell.\n\
         # Dropping OUR directory from PATH is what breaks the cycle -- it is the\n\
         # only edge in it we own.\n\
         _tl_new=\n\
         _tl_ifs=$IFS\n\
         IFS=:\n\
         for _tl_d in $PATH; do\n\
         \tif [ \"$_tl_d\" != \"$THRULINE_SHIM_DIR\" ]; then\n\
         \t\tif [ -z \"$_tl_new\" ]; then _tl_new=$_tl_d; else _tl_new=$_tl_new:$_tl_d; fi\n\
         \tfi\n\
         done\n\
         IFS=$_tl_ifs\n\
         PATH=$_tl_new\n\
         export PATH\n\
         exec {prog} \"$@\"\n"
    )
}

/// Single-quote for POSIX `sh`.
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn quotable(path: &Path) -> Result<(), ShimError> {
    let s = path.to_string_lossy().to_string();
    if s.contains('\n') || s.contains('\0') {
        return Err(ShimError::Unquotable(s));
    }
    Ok(())
}

fn absolute(path: &Path) -> Result<PathBuf, ShimError> {
    if path.is_absolute() {
        return Ok(normalize(path));
    }
    let cwd = std::env::current_dir().map_err(|source| ShimError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(normalize(&cwd.join(path)))
}

/// Remove `.` and `..` without touching the filesystem, so a path that does not
/// exist yet can still be made absolute.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Why this candidate gets no wrapper, or `None` to write one.
fn skip_reason(
    opts: &InstallOptions,
    program: &str,
    dir: &Path,
    path_var: Option<&OsStr>,
) -> Option<String> {
    match &opts.policy {
        WrapPolicy::All => None,
        WrapPolicy::Only(names) => {
            if names.iter().any(|n| n == program) {
                None
            } else {
                Some("not in the requested set".to_string())
            }
        }
        WrapPolicy::Resolvable => {
            if resolves_on(program, dir, path_var) {
                None
            } else {
                Some("no real program of that name resolves on PATH".to_string())
            }
        }
    }
}

/// Remove a wrapper we are declining to write. Absent is the expected case and
/// is not an error; anything else is, and is not swallowed.
fn remove_stale(path: &Path) -> Result<(), ShimError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(ShimError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn write_executable(path: &Path, contents: &str) -> Result<(), ShimError> {
    std::fs::write(path, contents).map_err(|source| ShimError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).map_err(
            |source| ShimError::Io {
                path: path.to_path_buf(),
                source,
            },
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;

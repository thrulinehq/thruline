//! Product-owned dependency fetching. Project output never grants environmental status.
//!
//! Cargo may invoke rustc even while fetching. This plan runs native tools outside
//! the project, from a neutral directory, with declarative source configuration only.
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::error::{CoreError, Result};

fn invalid(message: impl Into<String>) -> CoreError {
    CoreError::Config(message.into())
}

/// Retains the isolated configuration until the fetch child has been reaped.
pub struct Plan {
    temporary: tempfile::TempDir,
    pub argv: Vec<String>,
    pub configuration: Vec<Value>,
    rustc: PathBuf,
    path: std::ffi::OsString,
    registry_indexes: BTreeMap<String, String>,
}

impl Plan {
    pub fn cwd(&self) -> &Path {
        self.temporary.path()
    }

    pub fn command(&self) -> Command {
        let mut command = native_command(Path::new(&self.argv[0]), &self.path, self.cwd());
        command
            .envs(&self.registry_indexes)
            .args(&self.argv[1..])
            .env("CARGO_HOME", self.cwd().join("cargo-home"))
            .env("CARGO_NET_OFFLINE", "true")
            .env("CARGO_NET_GIT_FETCH_WITH_CLI", "false")
            .env("RUSTC", &self.rustc);
        command
    }

    pub fn metadata(&self, requested: &[String]) -> serde_json::Map<String, Value> {
        serde_json::Map::from_iter([
            ("phase".into(), json!("dependency_fetch")),
            ("requested_argv".into(), json!(requested)),
            ("project_configuration".into(), json!(self.configuration)),
            ("registry_index_environment".into(), json!(self.registry_indexes.iter()
                .map(|(name, value)| json!({"name":name, "sha256":crate::chain::sha256_hex(value.as_bytes())}))
                .collect::<Vec<_>>())),
        ])
    }
}

fn native_command(program: &Path, path: &OsStr, cwd: &Path) -> Command {
    let mut command = Command::new(program);
    command
        .env_clear()
        .env("PATH", path)
        .env("RUSTUP_AUTO_INSTALL", "0")
        .current_dir(cwd)
        .stdin(Stdio::null());
    // No Cargo, Rust, loader, git, credential-provider or project environment is
    // inherited. These only locate the user's toolchain and writable temp area.
    for name in [
        "HOME",
        "RUSTUP_HOME",
        "TMPDIR",
        "TEMP",
        "TMP",
        "SystemRoot",
        "CARGO_TARGET_DIR",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
}

// Only a Child spawned by the metadata query reaches this cleanup path. Its
// signal goes through bounded_child's guarded signaller (OFFLINE-KILL-GUARD,
// R-2026-09-28-7), which refuses pgid <= 1 and HQ's own group.
fn kill_owned_probe(child: &mut Child) -> std::io::Result<()> {
    crate::bounded_child::kill_owned_group(child)
}

/// How long the compiler metadata probe may run, and how long its killed group
/// then has to be reaped before a reaper thread takes it over. Production is
/// always [`ProbeBound::PRODUCTION`]; only a test passes another bound
/// (OFFLINE-KILL-GUARD-R5), so its stand-in compiler is not racing the
/// production deadline to prove it was started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProbeBound {
    run: Duration,
    reap: Duration,
}

impl ProbeBound {
    const PRODUCTION: ProbeBound = ProbeBound {
        run: Duration::from_secs(2),
        reap: Duration::from_millis(500),
    };
}

/// Query only the pinned external compiler, never with a project target. File
/// output avoids waiting for inherited pipes. The foreground wait is bounded;
/// on an OS cleanup delay a reaper retains ownership, and no plan is returned.
fn compiler_metadata(
    rustc: &Path,
    path: &OsStr,
    neutral: &Path,
    args: &[&str],
    bound: ProbeBound,
) -> Result<String> {
    const LIMIT: u64 = 256 * 1024;
    let mut output = tempfile::tempfile_in(neutral).map_err(|e| CoreError::io(neutral, e))?;
    let stdout = output.try_clone().map_err(|e| CoreError::io(neutral, e))?;
    let mut probe = native_command(rustc, path, neutral);
    probe.args(args).stdout(stdout).stderr(Stdio::null());
    // A group of its own, so cleanup can signal exactly this probe's group.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        probe.process_group(0);
    }
    let mut child = probe.spawn().map_err(|e| CoreError::io(rustc, e))?;
    let mut deadline = Instant::now() + bound.run;
    let mut refusal = None;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if let Some(reason) = refusal {
                    return Err(invalid(reason));
                }
                if !status.success() {
                    return Err(invalid(
                        "native rustc could not report built-in target metadata",
                    ));
                }
                break;
            }
            Err(error) => {
                let _ = kill_owned_probe(&mut child);
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
                return Err(CoreError::io(rustc, error));
            }
            Ok(None) => {}
        }
        let now = Instant::now();
        if refusal.is_none()
            && (now >= deadline || output.metadata().map_or(true, |m| m.len() > LIMIT))
        {
            refusal = Some(if now >= deadline {
                "native rustc target metadata probe timed out"
            } else {
                "native rustc target metadata exceeded its output bound"
            });
            let _ = kill_owned_probe(&mut child);
            deadline = now + bound.reap;
        } else if refusal.is_some() && now >= deadline {
            // The killed leader is still owned until the OS permits its reap.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            return Err(invalid(
                "native rustc target metadata probe timed out; reaping remains pending",
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    output.rewind().map_err(|e| CoreError::io(neutral, e))?;
    let mut bytes = Vec::new();
    output
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| CoreError::io(neutral, e))?;
    if bytes.len() as u64 > LIMIT {
        return Err(invalid(
            "native rustc target metadata exceeded its output bound",
        ));
    }
    String::from_utf8(bytes).map_err(|_| invalid("native rustc target metadata was not UTF-8"))
}

/// Both sides are compared canonicalized: a candidate is resolved through every
/// symlink, and so is the root, so a project reached through a linked path
/// (macOS `/var` → `/private/var`, or any user link) is still the project.
/// A root that cannot be resolved refuses rather than compares unresolved.
fn outside_project(path: &Path, root: &Path) -> Result<PathBuf> {
    let resolved = std::fs::canonicalize(path).map_err(|e| CoreError::io(path, e))?;
    let root = std::fs::canonicalize(root).map_err(|e| CoreError::io(root, e))?;
    if resolved.starts_with(&root) || !resolved.is_file() {
        return Err(invalid(
            "dependency fetching requires a native tool outside the project",
        ));
    }
    Ok(resolved)
}

/// Where the owned fetch may take cargo and rustc from (OFFLINE-KILL-GUARD-R3,
/// founder ruling R-2026-09-28-26): the Rust toolchain directories
/// `$RUSTUP_HOME/toolchains` (default `~/.rustup`) and `$CARGO_HOME/bin`
/// (default `~/.cargo`), and the standard system directories — never an
/// arbitrary PATH entry. A path check alone cannot see a hard link or a plain
/// copy of a project's rustc placed outside the project; a location allow-list
/// can. Both sides are canonical: the candidate is resolved through every
/// symlink, and so is each root, so a link in an allowed directory pointing
/// elsewhere is judged by where it leads. A root that does not exist or is not
/// absolute is left out, never widened.
///
/// OFFLINE-KILL-GUARD-R4 (P-2026-09-28-4 (2)): one link is judged by where it
/// was found as well as where it leads — a cargo or rustc found in a Homebrew
/// installation's own `bin` (`/opt/homebrew/bin`, `/usr/local/bin`) whose
/// canonical path lies under that same installation's `Cellar`, which is how
/// `brew` installs every program. Nothing else is widened: a link from an
/// allowed directory to any other place, or into a Cellar from any other
/// folder, is still not taken.
///
/// A tool this allow-list does not admit is not a refusal of the check
/// (P-2026-09-28-4 (1)): the owned fetch is skipped and the check runs as the
/// person's own command ([`Prepared::Skipped`]). A tool inside the project is
/// still refused, by any path.
///
/// The roots come from ThruLine's own environment, never from a command's
/// literal `CARGO_HOME=...` assignment, which the project controls.
///
/// KNOWN LIMIT: a compromised toolchain directory, or a tool planted inside
/// one, is outside this guard's reach. Anything that can write into
/// `$RUSTUP_HOME/toolchains`, `$CARGO_HOME/bin` or a system directory is
/// already trusted by every cargo the user runs.
pub(crate) struct ToolRoots {
    roots: Vec<PathBuf>,
    /// Canonical `(<prefix>/bin, <prefix>/Cellar)` of each Homebrew prefix.
    cellars: Vec<(PathBuf, PathBuf)>,
}

const SYSTEM_TOOL_DIRECTORIES: [&str; 3] = ["/usr/bin", "/usr/local/bin", "/opt/homebrew/bin"];

/// The Homebrew prefixes whose own `bin` may link into their own `Cellar`.
const HOMEBREW_PREFIXES: [&str; 2] = ["/opt/homebrew", "/usr/local"];

/// What would allow a tool the allow-list did not admit, in the words every
/// skip reason ends with.
fn what_would_allow(program: &str) -> String {
    format!(
        "ThruLine's own fetch step takes cargo and rustc only from a Rust toolchain directory \
         ($RUSTUP_HOME/toolchains, $CARGO_HOME/bin), from {}, or from Homebrew's own Cellar \
         reached through /opt/homebrew/bin or /usr/local/bin; installing the toolchain with \
         rustup, or putting {program} in one of those directories, would allow it.",
        SYSTEM_TOOL_DIRECTORIES.join(", "),
    )
}

impl ToolRoots {
    pub(crate) fn from_environment(variable: impl Fn(&str) -> Option<std::ffi::OsString>) -> Self {
        let system: Vec<PathBuf> = SYSTEM_TOOL_DIRECTORIES.iter().map(PathBuf::from).collect();
        let homebrew: Vec<PathBuf> = HOMEBREW_PREFIXES.iter().map(PathBuf::from).collect();
        Self::under(variable, &system, &homebrew)
    }

    /// [`ToolRoots::from_environment`] with the system directories and the
    /// Homebrew prefixes given, so a test can point them into a temp dir.
    pub(crate) fn under(
        variable: impl Fn(&str) -> Option<std::ffi::OsString>,
        system: &[PathBuf],
        homebrew: &[PathBuf],
    ) -> Self {
        let set = |name: &str| variable(name).filter(|v| !v.is_empty()).map(PathBuf::from);
        let home = set("HOME");
        let rustup_home = set("RUSTUP_HOME").or_else(|| home.as_ref().map(|h| h.join(".rustup")));
        let cargo_home = set("CARGO_HOME").or_else(|| home.as_ref().map(|h| h.join(".cargo")));
        let canonical = |candidate: &Path| {
            candidate
                .is_absolute()
                .then(|| std::fs::canonicalize(candidate).ok())
                .flatten()
        };
        let mut roots = Vec::new();
        for candidate in [
            rustup_home.map(|p| p.join("toolchains")),
            cargo_home.map(|p| p.join("bin")),
        ]
        .into_iter()
        .flatten()
        .chain(system.iter().cloned())
        {
            if let Some(root) = canonical(&candidate) {
                if !roots.contains(&root) {
                    roots.push(root);
                }
            }
        }
        let cellars = homebrew
            .iter()
            .filter_map(|prefix| {
                Some((
                    canonical(&prefix.join("bin"))?,
                    canonical(&prefix.join("Cellar"))?,
                ))
            })
            .collect();
        ToolRoots { roots, cellars }
    }

    /// `resolved` is the canonical path `outside_project` returned for `found`.
    /// Admitted when it lies under a canonical allowed root, or when `found`
    /// sits in a Homebrew prefix's own `bin` and `resolved` under that same
    /// prefix's `Cellar`. Otherwise not admitted, naming the program, where it
    /// was found and what would allow it.
    fn admit(
        &self,
        program: &str,
        found: &Path,
        resolved: PathBuf,
    ) -> std::result::Result<PathBuf, Refusal> {
        if self.roots.iter().any(|root| resolved.starts_with(root)) {
            return Ok(resolved);
        }
        let found_in = found
            .parent()
            .and_then(|directory| std::fs::canonicalize(directory).ok());
        if self.cellars.iter().any(|(bin, cellar)| {
            found_in.as_deref() == Some(bin.as_path()) && resolved.starts_with(cellar)
        }) {
            return Ok(resolved);
        }
        Err(Refusal::NotAdmitted(format!(
            "{program} was found at {} (it resolves to {}). {}",
            found.display(),
            resolved.display(),
            what_would_allow(program),
        )))
    }
}

/// Why no owned fetch can be prepared from the tools found. `InProject` (and
/// every other `CoreError`) refuses the check; `NotAdmitted` skips the owned
/// fetch and lets the check run as the person's own command.
#[derive(Debug)]
enum Refusal {
    Refused(CoreError),
    NotAdmitted(String),
}

impl From<CoreError> for Refusal {
    fn from(error: CoreError) -> Self {
        Refusal::Refused(error)
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::Refused(error) => error.fmt(f),
            Refusal::NotAdmitted(why) => f.write_str(why),
        }
    }
}

fn find_program(program: &str, path: &OsStr, cwd: &Path) -> Option<PathBuf> {
    let p = Path::new(program);
    if p.components().count() > 1 {
        return Some(if p.is_absolute() {
            p.into()
        } else {
            cwd.join(p)
        });
    }
    std::env::split_paths(path)
        .map(|p| if p.is_absolute() { p } else { cwd.join(p) })
        .map(|p| p.join(program))
        .find(|p| p.is_file())
}

#[cfg(test)]
fn native_tools(
    program: &str,
    path: &OsStr,
    cwd: &Path,
    root: &Path,
    neutral: &Path,
    roots: &ToolRoots,
) -> std::result::Result<(PathBuf, PathBuf), Refusal> {
    native_tools_for(program, path, cwd, root, neutral, roots, None)
}

/// `toolchain` is the toolchain the check itself is pinned to
/// (`RUSTUP_TOOLCHAIN`, as the run route's guard checked it). A rustup proxy
/// is asked for that toolchain's cargo and rustc, so the owned fetch never
/// uses a different toolchain from the check it prepares; a toolchain rustup
/// cannot name here, or one outside the allow-list, skips the owned fetch.
fn native_tools_for(
    program: &str,
    path: &OsStr,
    cwd: &Path,
    root: &Path,
    neutral: &Path,
    roots: &ToolRoots,
    toolchain: Option<&OsStr>,
) -> std::result::Result<(PathBuf, PathBuf), Refusal> {
    let selected = find_program(program, path, cwd)
        .ok_or_else(|| invalid("Cargo is not available for dependency fetching"))?;
    // A project tool refuses the check by any path (R1, R2), before the
    // allow-list is consulted — a skip never walks past one.
    let cargo = outside_project(&selected, root)?;
    let is_rustup = cargo.file_stem() == Some(OsStr::new("rustup"));
    if !is_rustup && cargo.file_stem() != Some(OsStr::new("cargo")) {
        return Err(invalid("selected Cargo is not a native cargo executable").into());
    }
    // The rustc a native cargo would use: its sibling, or the search path's.
    let fallback_rustc = if is_rustup {
        None
    } else {
        let sibling = cargo.with_file_name("rustc");
        let candidate = if sibling.is_file() {
            Some(sibling)
        } else {
            find_program("rustc", path, cwd)
        };
        match candidate {
            Some(candidate) => {
                let resolved = outside_project(&candidate, root)?;
                Some((candidate, resolved))
            }
            None => None,
        }
    };
    let cargo = roots.admit("cargo", &selected, cargo)?;
    // Resolve a rustup shim only from the neutral directory. A project's
    // rust-toolchain file never selects a local override; the toolchain the
    // check is pinned to, when there is one, is named explicitly.
    let which = |rustup: &Path, name: &str| -> std::result::Result<PathBuf, Refusal> {
        let mut ask = native_command(rustup, path, neutral);
        if let Some(toolchain) = toolchain {
            ask.env("RUSTUP_TOOLCHAIN", toolchain);
        }
        let output = ask
            .args(["which", name])
            .output()
            .map_err(|e| CoreError::io(rustup, e))?;
        if !output.status.success() {
            return Err(Refusal::NotAdmitted(format!(
                "rustup at {} could not name the {name} of the toolchain this check runs \
                 without installing one. {}",
                rustup.display(),
                what_would_allow(name),
            )));
        }
        let found = String::from_utf8(output.stdout)
            .map_err(|_| invalid("toolchain path was not UTF-8"))?;
        let found = Path::new(found.trim());
        roots.admit(name, found, outside_project(found, root)?)
    };
    if is_rustup {
        return Ok((which(&cargo, "cargo")?, which(&cargo, "rustc")?));
    }
    let Some((selected_rustc, resolved_rustc)) = fallback_rustc else {
        return Err(Refusal::NotAdmitted(format!(
            "No rustc was found beside cargo at {} or on the search path. {}",
            cargo.display(),
            what_would_allow("rustc"),
        )));
    };
    // The rustc beside an admitted cargo, in the same canonical directory, is
    // that installation's own (a Homebrew cargo's sibling is in its Cellar).
    let rustc = if resolved_rustc.parent().is_some() && resolved_rustc.parent() == cargo.parent() {
        resolved_rustc
    } else {
        roots.admit("rustc", &selected_rustc, resolved_rustc)?
    };
    let rustc = if rustc.file_stem() == Some(OsStr::new("rustup")) {
        which(&rustc, "rustc")?
    } else {
        rustc
    };
    Ok((cargo, rustc))
}

/// Parse only a command consisting of literal environment assignments followed by
/// `exec cargo ...`. No shell is evaluated while deciding the fetch environment.
fn shell_cargo(script: &str, cwd: &Path) -> Option<(Vec<String>, BTreeMap<String, String>)> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut chars = script.chars().peekable();
    while let Some(ch) = chars.next() {
        if matches!(
            ch,
            '`' | '\n' | ';' | '|' | '&' | '<' | '>' | '*' | '?' | '[' | ']' | '(' | ')'
        ) {
            return None;
        }
        if ch == '\\' {
            return None;
        }
        if ch == '\'' || ch == '"' {
            if quote == Some(ch) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(ch);
            } else {
                word.push(ch);
            }
        } else if ch == '$' && quote != Some('\'') {
            let mut name = String::new();
            while chars
                .peek()
                .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
            {
                name.push(chars.next()?);
            }
            let value = match name.as_str() {
                "PWD" => cwd.display().to_string(),
                "CARGO_HOME" | "CARGO_TARGET_DIR" | "CARGO_BUILD_TARGET" => {
                    std::env::var(&name).unwrap_or_default()
                }
                _ => return None,
            };
            word.push_str(&value);
        } else if ch.is_whitespace() && quote.is_none() {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
        } else {
            word.push(ch);
        }
    }
    if quote.is_some() {
        return None;
    }
    if !word.is_empty() {
        words.push(word);
    }
    let exec = words.iter().position(|s| s == "exec")?;
    let mut env = BTreeMap::new();
    for assignment in &words[..exec] {
        let (name, value) = assignment.split_once('=')?;
        if !matches!(
            name,
            "CARGO_HOME" | "CARGO_TARGET_DIR" | "CARGO_BUILD_TARGET"
        ) {
            return None;
        }
        env.insert(name.to_string(), value.to_string());
    }
    Some((words[exec + 1..].to_vec(), env))
}

fn cargo_request(argv: &[String], cwd: &Path) -> Option<(Vec<String>, BTreeMap<String, String>)> {
    if Path::new(argv.first()?).file_name() == Some(OsStr::new("cargo")) {
        return Some((argv.to_vec(), BTreeMap::new()));
    }
    let program = Path::new(argv.first()?).file_name()?.to_str()?;
    if matches!(program, "sh" | "bash") {
        let script = match argv.get(1).map(String::as_str) {
            Some("-c") if argv.len() == 3 => &argv[2],
            Some("-o")
                if argv.get(2).is_some_and(|s| s == "pipefail")
                    && argv.get(3).is_some_and(|s| s == "-c")
                    && argv.len() == 5 =>
            {
                &argv[4]
            }
            _ => return None,
        };
        let request = shell_cargo(script, cwd)?;
        if Path::new(request.0.first()?).file_name() == Some(OsStr::new("cargo")) {
            return Some(request);
        }
    }
    None
}

fn config_path(directory: &Path) -> Option<PathBuf> {
    // Cargo gives the extensionless legacy file precedence when both exist.
    ["config", "config.toml"]
        .into_iter()
        .map(|name| directory.join(name))
        .find(|p| p.is_file())
}

fn merge(target: &mut toml::Value, source: toml::Value) {
    if let (Some(target), Some(source)) = (target.as_table_mut(), source.as_table()) {
        for (key, value) in source {
            if let Some(prior) = target.get_mut(key) {
                merge(prior, value.clone());
            } else {
                target.insert(key.clone(), value.clone());
            }
        }
    } else {
        *target = source;
    }
}

fn declarative_config(paths: &[PathBuf]) -> Result<(toml::Value, Vec<Value>, Option<Vec<String>>)> {
    let mut merged = toml::Value::Table(Default::default());
    let mut records = Vec::new();
    let mut targets: Option<Vec<String>> = None;
    let mut targets_are_array = false;
    for path in paths {
        let bytes = std::fs::read(path).map_err(|e| CoreError::io(path, e))?;
        let text =
            std::str::from_utf8(&bytes).map_err(|_| invalid("Cargo config was not UTF-8"))?;
        let value: toml::Value =
            toml::from_str(text).map_err(|e| invalid(format!("{}: {e}", path.display())))?;
        let mut selected = toml::map::Map::new();
        let mut applied = Vec::<String>::new();
        let mut excluded = Vec::<String>::new();
        for (top, value) in value
            .as_table()
            .ok_or_else(|| invalid("Cargo config must be a table"))?
        {
            if top == "source" || top == "registries" {
                let mut entries = toml::map::Map::new();
                for (name, settings) in value
                    .as_table()
                    .ok_or_else(|| invalid("Cargo source configuration must be a table"))?
                {
                    let mut entry = toml::map::Map::new();
                    for (key, value) in settings
                        .as_table()
                        .ok_or_else(|| invalid("Cargo source entry must be a table"))?
                    {
                        let field = format!("{top}.{name}.{key}");
                        let allowed = if top == "source" {
                            matches!(
                                key.as_str(),
                                "registry"
                                    | "replace-with"
                                    | "directory"
                                    | "local-registry"
                                    | "git"
                                    | "branch"
                                    | "tag"
                                    | "rev"
                            )
                        } else {
                            key == "index"
                        };
                        if allowed {
                            let mut value = value.clone();
                            if matches!(key.as_str(), "directory" | "local-registry") {
                                let relative = Path::new(value.as_str().ok_or_else(|| {
                                    invalid("Cargo source path must be a string")
                                })?);
                                if relative.is_relative() {
                                    let base =
                                        path.parent().and_then(Path::parent).ok_or_else(|| {
                                            invalid("Cargo config has no base directory")
                                        })?;
                                    value = toml::Value::String(
                                        base.join(relative).display().to_string(),
                                    );
                                }
                            }
                            entry.insert(key.clone(), value);
                            applied.push(field);
                        } else {
                            excluded.push(field);
                        }
                    }
                    if !entry.is_empty() {
                        entries.insert(name.clone(), toml::Value::Table(entry));
                    }
                }
                selected.insert(top.clone(), toml::Value::Table(entries));
            } else if top == "build" {
                for (key, value) in value
                    .as_table()
                    .ok_or_else(|| invalid("Cargo build configuration must be a table"))?
                {
                    if key == "target" {
                        let incoming_array = value.is_array();
                        let mut incoming = match value {
                            toml::Value::String(s) => vec![s.clone()],
                            toml::Value::Array(a) => a
                                .iter()
                                .map(|v| {
                                    v.as_str()
                                        .map(str::to_owned)
                                        .ok_or_else(|| invalid("Cargo target must be a string"))
                                })
                                .collect::<Result<Vec<_>>>()?,
                            _ => return Err(invalid("Cargo target must be a string or list")),
                        };
                        let base = path
                            .parent()
                            .and_then(Path::parent)
                            .ok_or_else(|| invalid("Cargo config has no base directory"))?;
                        for target in &mut incoming {
                            if target.ends_with(".json") && Path::new(target).is_relative() {
                                *target = base.join(&*target).display().to_string();
                            }
                        }
                        if targets.is_some() && targets_are_array != incoming_array {
                            return Err(invalid(
                                "Cargo target configuration mixes string and array types",
                            ));
                        }
                        if incoming_array {
                            targets.get_or_insert_with(Vec::new).extend(incoming);
                        } else {
                            targets = Some(incoming);
                        }
                        targets_are_array = incoming_array;
                        applied.push("build.target".into());
                    } else {
                        excluded.push(format!("build.{key}"));
                    }
                }
            } else {
                excluded.push(top.clone());
            }
        }
        merge(&mut merged, toml::Value::Table(selected));
        records.push(json!({"path":path, "sha256":crate::chain::sha256_hex(&bytes), "applied":applied,"excluded":excluded}));
    }
    Ok((merged, records, targets))
}

/// What ThruLine's own dependency-fetch step will do before a check.
pub enum Prepared {
    /// Not a supported Cargo command: it runs normally, and its output can
    /// never authorize an environmental verdict.
    Unsupported,
    /// The owned fetch runs first, with this plan.
    Plan(Plan),
    /// OFFLINE-KILL-GUARD-R4 (P-2026-09-28-4 (1)): no allow-listed toolchain
    /// is available, so the owned fetch does not run and the check runs as the
    /// person's own command. Its failure reads failed, never couldn't-check
    /// (R-2026-09-27-14). The reason, in words, names the program, where it
    /// was found and what would allow it; the record and the run sheet carry it.
    Skipped(String),
}

impl Prepared {
    /// The plan, when the owned fetch runs.
    pub fn into_plan(self) -> Option<Plan> {
        match self {
            Prepared::Plan(plan) => Some(plan),
            Prepared::Unsupported | Prepared::Skipped(_) => None,
        }
    }
}

/// [`prepare_owned`] with no pinned toolchain, answering only whether an owned
/// fetch runs: `None` when the command is not a supported Cargo command or
/// when the owned fetch is skipped. Either way the command can run normally,
/// and its output can never authorize an environmental verdict.
pub fn prepare(
    argv: &[String],
    cwd: &Path,
    root: &Path,
    path_override: Option<&OsStr>,
) -> Result<Option<Plan>> {
    prepare_owned(argv, cwd, root, path_override, None).map(Prepared::into_plan)
}

/// Refusals (an `Err`, which the caller records as "dependency fetch could not
/// be prepared"): a cargo or rustc inside the project, by any path; an
/// unsupported target; project or ancestor Cargo configuration reaching the
/// neutral directory. A cargo or rustc outside the toolchain and system
/// directories [`ToolRoots`] names is not a refusal: the owned fetch is
/// [`Prepared::Skipped`], with a reason naming the program, where it was found
/// and what would allow it. `toolchain` is the toolchain the check is pinned
/// to (`RUSTUP_TOOLCHAIN`), which a rustup proxy is asked for, so the fetch and
/// the check use one toolchain. Known limit: a compromised toolchain
/// directory, or a tool planted inside one, is outside this guard's reach.
pub fn prepare_owned(
    argv: &[String],
    cwd: &Path,
    root: &Path,
    path_override: Option<&OsStr>,
    toolchain: Option<&OsStr>,
) -> Result<Prepared> {
    let roots = ToolRoots::from_environment(|name| std::env::var_os(name));
    prepare_in(argv, cwd, root, path_override, toolchain, &roots)
}

#[cfg(test)]
fn prepare_with(
    argv: &[String],
    cwd: &Path,
    root: &Path,
    path_override: Option<&OsStr>,
    roots: &ToolRoots,
) -> Result<Option<Plan>> {
    prepare_in(argv, cwd, root, path_override, None, roots).map(Prepared::into_plan)
}

fn prepare_in(
    argv: &[String],
    cwd: &Path,
    root: &Path,
    path_override: Option<&OsStr>,
    toolchain: Option<&OsStr>,
    roots: &ToolRoots,
) -> Result<Prepared> {
    prepare_bounded(
        argv,
        cwd,
        root,
        path_override,
        toolchain,
        roots,
        ProbeBound::PRODUCTION,
    )
}

fn prepare_bounded(
    argv: &[String],
    cwd: &Path,
    root: &Path,
    path_override: Option<&OsStr>,
    toolchain: Option<&OsStr>,
    roots: &ToolRoots,
    bound: ProbeBound,
) -> Result<Prepared> {
    let Some((request, environment)) = cargo_request(argv, cwd) else {
        return Ok(Prepared::Unsupported);
    };
    let mut verb = 1;
    while request.get(verb).is_some_and(|s| {
        matches!(
            s.as_str(),
            "-q" | "--quiet" | "-v" | "--verbose" | "--offline" | "--locked" | "--frozen"
        )
    }) {
        verb += 1;
    }
    if !request
        .get(verb)
        .is_some_and(|s| matches!(s.as_str(), "test" | "build" | "check"))
    {
        return Ok(Prepared::Unsupported);
    }
    let temporary = tempfile::tempdir().map_err(|e| CoreError::io(std::env::temp_dir(), e))?;
    let path = path_override
        .map(OsStr::to_os_string)
        .or_else(|| std::env::var_os("PATH"))
        .unwrap_or_default();
    let root = std::fs::canonicalize(root).map_err(|e| CoreError::io(root, e))?;
    // CARGO_HOME does not disable Cargo's ancestor-config discovery. A temp
    // directory is neutral only when its ancestors contain no Cargo config.
    let neutral =
        std::fs::canonicalize(temporary.path()).map_err(|e| CoreError::io(temporary.path(), e))?;
    if neutral.starts_with(&root)
        || neutral
            .ancestors()
            .any(|ancestor| config_path(&ancestor.join(".cargo")).is_some())
    {
        return Err(invalid("the dependency-fetch temporary directory inherits project or ancestor Cargo configuration"));
    }
    let (cargo, rustc) =
        match native_tools_for(&request[0], &path, cwd, &root, &neutral, roots, toolchain) {
            Ok(tools) => tools,
            Err(Refusal::Refused(error)) => return Err(error),
            Err(Refusal::NotAdmitted(why)) => return Ok(Prepared::Skipped(why)),
        };
    let effective = |name: &str| {
        environment
            .get(name)
            .cloned()
            .or_else(|| std::env::var(name).ok())
    };
    let cargo_home = effective("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".cargo")))
        .ok_or_else(|| invalid("Cargo home is not available"))?;
    let cargo_home = if cargo_home.is_absolute() {
        cargo_home
    } else {
        cwd.join(cargo_home)
    };
    let mut paths = config_path(&cargo_home).into_iter().collect::<Vec<_>>();
    for ancestor in cwd.ancestors().collect::<Vec<_>>().into_iter().rev() {
        if let Some(p) = config_path(&ancestor.join(".cargo")) {
            if !paths.contains(&p) {
                paths.push(p);
            }
        }
    }
    let (mut configuration, mut records, configured_targets) = declarative_config(&paths)?;
    for ancestor in cwd.ancestors() {
        let toolchain = ["rust-toolchain.toml", "rust-toolchain"]
            .into_iter()
            .map(|name| ancestor.join(name))
            .find(|p| p.is_file());
        if let Some(path) = toolchain {
            let bytes = std::fs::read(&path).map_err(|e| CoreError::io(&path, e))?;
            records.push(json!({"path":path,"sha256":crate::chain::sha256_hex(&bytes),"applied":[],"excluded":["project_toolchain_override"]}));
            break;
        }
    }
    let mut manifest = None;
    let mut targets = Vec::new();
    let mut switches = Vec::new();
    let mut position = 1;
    while position < request.len() {
        let arg = &request[position];
        if arg == "--" {
            break;
        }
        if matches!(arg.as_str(), "--manifest-path" | "--target") {
            position += 1;
            let value = request
                .get(position)
                .ok_or_else(|| invalid(format!("{arg} has no value")))?
                .clone();
            if arg == "--manifest-path" {
                manifest = Some(value);
            } else {
                targets.push(value);
            }
        } else if let Some(value) = arg.strip_prefix("--manifest-path=") {
            manifest = Some(value.into());
        } else if let Some(value) = arg.strip_prefix("--target=") {
            targets.push(value.into());
        } else if arg == "--locked" || arg == "--frozen" {
            switches.push(arg.clone());
        } else if arg == "--config" || arg.starts_with("--config=") || arg.starts_with("-Z") {
            return Err(invalid(
                "dependency fetching cannot isolate this Cargo configuration override",
            ));
        }
        position += 1;
    }
    let manifest = match manifest {
        Some(p) => {
            let p = PathBuf::from(p);
            if p.is_absolute() {
                p
            } else {
                cwd.join(p)
            }
        }
        None => cwd
            .ancestors()
            .map(|p| p.join("Cargo.toml"))
            .find(|p| p.is_file())
            // Let the owned fetch record Cargo's missing-manifest failure too.
            .unwrap_or_else(|| cwd.join("Cargo.toml")),
    };
    if targets.is_empty() {
        targets = effective("CARGO_BUILD_TARGET")
            .map(|s| vec![s])
            .or(configured_targets)
            .unwrap_or_default();
    }
    // Custom specifications can select compiler behavior beyond a built-in
    // target. They are unsupported during a prerequisite that runs no project
    // code. Reject paths before querying the trusted compiler's own catalog.
    let unsupported = |target: &str| {
        invalid(format!(
        "dependency fetching supports only built-in compiler targets; target {target:?} is unsupported"
    ))
    };
    for target in &targets {
        if target.contains('/')
            || target.contains('\\')
            || target.to_ascii_lowercase().ends_with(".json")
        {
            return Err(unsupported(target));
        }
    }
    if targets.is_empty() || targets.iter().any(|target| target == "host-tuple") {
        let metadata = compiler_metadata(&rustc, &path, temporary.path(), &["-vV"], bound)?;
        let host = metadata
            .lines()
            .find_map(|line| line.strip_prefix("host: "))
            .ok_or_else(|| invalid("native rustc did not identify its host"))?;
        if targets.is_empty() {
            targets.push(host.into());
        } else {
            for target in &mut targets {
                if target == "host-tuple" {
                    *target = host.into();
                }
            }
        }
    }
    let catalog = compiler_metadata(
        &rustc,
        &path,
        temporary.path(),
        &["--print", "target-list"],
        bound,
    )?;
    for target in &targets {
        if !catalog.lines().any(|builtin| builtin == target) {
            return Err(unsupported(target));
        }
    }
    let home = temporary.path().join("cargo-home");
    std::fs::create_dir(&home).map_err(|e| CoreError::io(&home, e))?;
    // Cache aliases share Cargo's own lock files with the original home.
    // No credential/configuration file is copied or linked.
    for name in ["registry", "git", ".package-cache", ".package-cache-mutate"] {
        let source = cargo_home.join(name);
        if source.exists() || name.starts_with('.') {
            #[cfg(unix)]
            std::os::unix::fs::symlink(&source, home.join(name))
                .map_err(|e| CoreError::io(&source, e))?;
        }
    }
    configuration
        .as_table_mut()
        .ok_or_else(|| invalid("isolated Cargo configuration is not a table"))?
        .insert(
            "net".into(),
            toml::Value::Table(toml::map::Map::from_iter([
                ("offline".into(), toml::Value::Boolean(true)),
                ("git-fetch-with-cli".into(), toml::Value::Boolean(false)),
            ])),
        );
    let config = home.join("config.toml");
    std::fs::write(
        &config,
        toml::to_string(&configuration).map_err(|e| invalid(e.to_string()))?,
    )
    .map_err(|e| CoreError::io(&config, e))?;
    let mut argv = vec![
        cargo.display().to_string(),
        "fetch".into(),
        "--offline".into(),
        "--manifest-path".into(),
        manifest.display().to_string(),
    ];
    argv.extend(switches);
    for target in targets {
        argv.extend(["--target".into(), target]);
    }
    // Registry index variables are declarative input. Keep Cargo's own name
    // normalization (including dashes/underscores), while withholding credential
    // providers and every other inherited Cargo/Rust override.
    let registry_indexes = std::env::vars_os()
        .filter_map(|(name, value)| {
            let name = name.to_str()?;
            ((name.starts_with("CARGO_REGISTRIES_") && name.ends_with("_INDEX"))
                || name == "CARGO_REGISTRY_INDEX")
                .then(|| (name.to_owned(), value))
        })
        .map(|(name, value)| {
            value
                .into_string()
                .map(|value| (name, value))
                .map_err(|_| invalid("Cargo registry index override is not UTF-8"))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    Ok(Prepared::Plan(Plan {
        temporary,
        argv,
        configuration: records,
        rustc,
        path,
        registry_indexes,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(root: &Path, contents: &str) -> PathBuf {
        let directory = root.join(".cargo");
        std::fs::create_dir_all(&directory).expect("config directory");
        let path = directory.join("config.toml");
        std::fs::write(&path, contents).expect("config");
        path
    }

    /// A fake toolchain layout under `base`: `RUSTUP_HOME`, `CARGO_HOME` and
    /// `HOME` all point into the temp dir, never at the real ~/.rustup.
    /// Returns the roots, `$CARGO_HOME/bin` and one `$RUSTUP_HOME/toolchains/*/bin`.
    fn fake_toolchain(base: &Path) -> (ToolRoots, PathBuf, PathBuf) {
        let rustup_home = base.join("rustup-home");
        let cargo_home = base.join("cargo-home");
        let cargo_bin = cargo_home.join("bin");
        let toolchain_bin = rustup_home.join("toolchains/stable-test/bin");
        for path in [&cargo_bin, &toolchain_bin] {
            std::fs::create_dir_all(path).expect("toolchain directory");
        }
        let home = base.join("home");
        let roots = ToolRoots::from_environment(|name| match name {
            "RUSTUP_HOME" => Some(rustup_home.clone().into()),
            "CARGO_HOME" => Some(cargo_home.clone().into()),
            "HOME" => Some(home.clone().into()),
            _ => None,
        });
        (roots, cargo_bin, toolchain_bin)
    }

    #[test]
    fn cargo_and_rustc_may_live_in_separate_external_directories() {
        let directory = tempfile::tempdir().expect("temp");
        let root = directory.path().join("project");
        let (roots, cargo_dir, rustc_dir) = fake_toolchain(directory.path());
        std::fs::create_dir_all(&root).expect("directory");
        std::fs::write(cargo_dir.join("cargo"), "external cargo").expect("cargo");
        std::fs::write(rustc_dir.join("rustc"), "external rustc").expect("rustc");
        let path = std::env::join_paths([&cargo_dir, &rustc_dir]).expect("path");
        let (cargo, rustc) = native_tools("cargo", &path, &root, &root, directory.path(), &roots)
            .expect("separate external installations");
        assert_eq!(
            cargo,
            std::fs::canonicalize(cargo_dir.join("cargo")).expect("cargo path")
        );
        assert_eq!(
            rustc,
            std::fs::canonicalize(rustc_dir.join("rustc")).expect("rustc path")
        );
    }

    #[test]
    fn a_project_rustc_is_never_the_external_cargo_fallback() {
        let directory = tempfile::tempdir().expect("temp");
        let root = directory.path().join("project");
        let (roots, cargo_dir, _) = fake_toolchain(directory.path());
        std::fs::create_dir_all(&root).expect("project");
        std::fs::write(cargo_dir.join("cargo"), "external cargo").expect("cargo");
        std::fs::write(root.join("rustc"), "project rustc must not run").expect("rustc");
        let path = std::env::join_paths([&cargo_dir, &root]).expect("path");
        assert!(native_tools("cargo", &path, &root, &root, directory.path(), &roots).is_err());
    }

    /// OFFLINE-KILL-GUARD-R2: a project reached through a symlinked path is
    /// still the project. Its own rustc is refused as the fallback compiler
    /// whether the root handed in is the link or the real directory.
    #[cfg(unix)]
    #[test]
    fn a_project_rustc_behind_a_symlinked_root_is_never_the_fallback() {
        let directory = tempfile::tempdir().expect("temp");
        let real = directory.path().join("real");
        let project = real.join("project");
        let (roots, cargo_dir, _) = fake_toolchain(directory.path());
        std::fs::create_dir_all(&project).expect("project");
        std::fs::write(cargo_dir.join("cargo"), "external cargo").expect("cargo");
        std::fs::write(project.join("rustc"), "project rustc must not run").expect("rustc");
        let link = directory.path().join("link");
        std::os::unix::fs::symlink(&real, &link).expect("symlink");
        let root = link.join("project");
        for (search, given) in [(&root, &root), (&project, &root), (&root, &project)] {
            let path = std::env::join_paths([&cargo_dir, search]).expect("path");
            let refused = native_tools("cargo", &path, given, given, directory.path(), &roots)
                .expect_err("a project rustc behind a symlinked root was accepted");
            assert!(
                refused
                    .to_string()
                    .contains("requires a native tool outside the project"),
                "unexpected refusal: {refused}"
            );
        }
    }

    /// OFFLINE-KILL-GUARD-R3 (R-2026-09-28-26): a project's rustc carried out
    /// of the project — by a hard link or by a plain copy — onto an arbitrary
    /// PATH entry is not a toolchain rustc. No path comparison can see a hard
    /// link or a copy for what it is, so the location allow-list refuses it.
    fn assert_carried_out_rustc_refused(carry: fn(&Path, &Path) -> std::io::Result<()>) {
        let directory = tempfile::tempdir().expect("temp");
        let root = directory.path().join("project");
        let (roots, cargo_dir, _) = fake_toolchain(directory.path());
        let outside = directory.path().join("outside-tools");
        std::fs::create_dir_all(&root).expect("project");
        std::fs::create_dir_all(&outside).expect("outside directory");
        std::fs::write(cargo_dir.join("cargo"), "toolchain cargo").expect("cargo");
        std::fs::write(root.join("rustc"), "project rustc must not run").expect("rustc");
        carry(&root.join("rustc"), &outside.join("rustc")).expect("carry the project rustc out");
        let path = std::env::join_paths([&cargo_dir, &outside]).expect("path");
        let refused = native_tools("cargo", &path, &root, &root, directory.path(), &roots)
            .expect_err("a project rustc carried outside the project was accepted");
        let message = refused.to_string();
        assert!(
            message.contains("rustc"),
            "refusal names no program: {message}"
        );
        assert!(
            message.contains(&outside.join("rustc").display().to_string()),
            "refusal does not say where it was found: {message}"
        );
        assert!(
            message.contains("$RUSTUP_HOME/toolchains") && message.contains("$CARGO_HOME/bin"),
            "refusal does not say what would allow it: {message}"
        );
    }

    #[test]
    fn a_project_rustc_hard_linked_onto_an_arbitrary_path_entry_is_refused() {
        assert_carried_out_rustc_refused(|from, to| std::fs::hard_link(from, to));
    }

    #[test]
    fn a_project_rustc_copied_onto_an_arbitrary_path_entry_is_refused() {
        assert_carried_out_rustc_refused(|from, to| std::fs::copy(from, to).map(|_| ()));
    }

    #[test]
    fn a_cargo_on_an_arbitrary_path_entry_is_refused() {
        let directory = tempfile::tempdir().expect("temp");
        let root = directory.path().join("project");
        let (roots, _, toolchain_bin) = fake_toolchain(directory.path());
        let arbitrary = directory.path().join("arbitrary-bin");
        std::fs::create_dir_all(&root).expect("project");
        std::fs::create_dir_all(&arbitrary).expect("arbitrary directory");
        std::fs::write(arbitrary.join("cargo"), "arbitrary cargo").expect("cargo");
        std::fs::write(toolchain_bin.join("rustc"), "toolchain rustc").expect("rustc");
        let path = std::env::join_paths([&arbitrary, &toolchain_bin]).expect("path");
        let refused = native_tools("cargo", &path, &root, &root, directory.path(), &roots)
            .expect_err("a cargo from an arbitrary PATH entry was accepted");
        assert!(refused.to_string().contains("cargo"), "{refused}");
    }

    #[test]
    fn a_genuine_toolchain_cargo_and_rustc_are_accepted() {
        let directory = tempfile::tempdir().expect("temp");
        let root = directory.path().join("project");
        let (roots, _, toolchain_bin) = fake_toolchain(directory.path());
        std::fs::create_dir_all(&root).expect("project");
        std::fs::write(toolchain_bin.join("cargo"), "toolchain cargo").expect("cargo");
        std::fs::write(toolchain_bin.join("rustc"), "toolchain rustc").expect("rustc");
        let path = std::env::join_paths([&toolchain_bin]).expect("path");
        let (cargo, rustc) = native_tools("cargo", &path, &root, &root, directory.path(), &roots)
            .expect("a toolchain under $RUSTUP_HOME/toolchains is accepted");
        assert_eq!(
            cargo,
            std::fs::canonicalize(toolchain_bin.join("cargo")).expect("cargo")
        );
        assert_eq!(
            rustc,
            std::fs::canonicalize(toolchain_bin.join("rustc")).expect("rustc")
        );
    }

    #[test]
    fn toolchain_roots_default_from_home_when_unset() {
        let directory = tempfile::tempdir().expect("temp");
        let home = directory.path().join("home");
        let root = directory.path().join("project");
        let bin = home.join(".rustup/toolchains/stable-test/bin");
        let cargo_bin = home.join(".cargo/bin");
        for path in [&root, &bin, &cargo_bin] {
            std::fs::create_dir_all(path).expect("directory");
        }
        std::fs::write(cargo_bin.join("cargo"), "toolchain cargo").expect("cargo");
        std::fs::write(bin.join("rustc"), "toolchain rustc").expect("rustc");
        let roots = ToolRoots::from_environment(|name| {
            (name == "HOME").then(|| home.clone().into_os_string())
        });
        let path = std::env::join_paths([&cargo_bin, &bin]).expect("path");
        native_tools("cargo", &path, &root, &root, directory.path(), &roots)
            .expect("~/.cargo/bin and ~/.rustup/toolchains are the defaults");
    }

    /* ── OFFLINE-KILL-GUARD-R4 (P-2026-09-28-4) ───────────────────────── */

    /// What `prepare_in` answered, said in one word, so a test failure names it.
    fn skipped_reason(prepared: Result<Prepared>) -> String {
        match prepared {
            Ok(Prepared::Skipped(why)) => why,
            Ok(Prepared::Plan(plan)) => panic!("the owned fetch was planned: {:?}", plan.argv),
            Ok(Prepared::Unsupported) => panic!("the command was not read as a Cargo check"),
            Err(error) => panic!("the check was refused instead of the fetch skipped: {error}"),
        }
    }

    fn cargo_test() -> Vec<String> {
        vec!["cargo".into(), "test".into()]
    }

    /// P-2026-09-28-4 (1): a cargo and rustc outside the allow-list do not
    /// refuse the check. ThruLine's own fetch step is skipped, and what the
    /// record keeps names the program, where it was found and what would
    /// allow it.
    #[test]
    fn a_toolchain_outside_the_allow_list_skips_the_owned_fetch_and_says_why() {
        let directory = tempfile::tempdir().expect("temp");
        let root = directory.path().join("project");
        let (roots, _, _) = fake_toolchain(directory.path());
        let arbitrary = directory.path().join("arbitrary-bin");
        std::fs::create_dir_all(&root).expect("project");
        std::fs::create_dir_all(&arbitrary).expect("arbitrary directory");
        std::fs::write(arbitrary.join("cargo"), "arbitrary cargo").expect("cargo");
        std::fs::write(arbitrary.join("rustc"), "arbitrary rustc").expect("rustc");
        let path = std::env::join_paths([&arbitrary]).expect("path");
        let why = skipped_reason(prepare_in(
            &cargo_test(),
            &root,
            &root,
            Some(&path),
            None,
            &roots,
        ));
        assert!(why.contains("cargo"), "names no program: {why}");
        assert!(
            why.contains(&arbitrary.join("cargo").display().to_string()),
            "does not say where it was found: {why}"
        );
        assert!(
            why.contains("$RUSTUP_HOME/toolchains") && why.contains("$CARGO_HOME/bin"),
            "does not say what would allow it: {why}"
        );
        // The legacy door reads a skipped fetch as "no owned fetch", never a plan.
        assert!(
            prepare_with(&cargo_test(), &root, &root, Some(&path), &roots)
                .expect("not a refusal")
                .is_none()
        );
    }

    /// R1/R2 still hold when the fetch would be skipped: a project rustc next
    /// to an outside cargo refuses, by any path.
    #[test]
    fn a_project_rustc_is_still_refused_when_the_cargo_is_outside_the_allow_list() {
        let directory = tempfile::tempdir().expect("temp");
        let root = directory.path().join("project");
        let (roots, _, _) = fake_toolchain(directory.path());
        let arbitrary = directory.path().join("arbitrary-bin");
        std::fs::create_dir_all(&root).expect("project");
        std::fs::create_dir_all(&arbitrary).expect("arbitrary directory");
        std::fs::write(arbitrary.join("cargo"), "arbitrary cargo").expect("cargo");
        std::fs::write(root.join("rustc"), "project rustc must not run").expect("rustc");
        let path = std::env::join_paths([&arbitrary, &root]).expect("path");
        match prepare_in(&cargo_test(), &root, &root, Some(&path), None, &roots) {
            Err(error) => assert!(
                error
                    .to_string()
                    .contains("requires a native tool outside the project"),
                "{error}"
            ),
            Ok(Prepared::Skipped(why)) => panic!("a project rustc was skipped past: {why}"),
            Ok(_) => panic!("a project rustc was accepted"),
        }
    }

    /// A Homebrew-shaped prefix under `base`: `bin/cargo` and `bin/rustc` are
    /// relative links into `Cellar/rust/1.0.0/bin`, as `brew` lays them out.
    /// The roots are this prefix's `bin` and Homebrew prefix only, never the
    /// machine's own `/opt/homebrew` or `/usr/local`.
    #[cfg(unix)]
    fn homebrew(base: &Path) -> (ToolRoots, PathBuf, PathBuf) {
        let prefix = base.join("homebrew");
        let bin = prefix.join("bin");
        let cellar_bin = prefix.join("Cellar/rust/1.0.0/bin");
        for path in [&bin, &cellar_bin] {
            std::fs::create_dir_all(path).expect("homebrew directory");
        }
        for tool in ["cargo", "rustc"] {
            std::fs::write(cellar_bin.join(tool), format!("homebrew {tool}")).expect("tool");
            std::os::unix::fs::symlink(
                Path::new("../Cellar/rust/1.0.0/bin").join(tool),
                bin.join(tool),
            )
            .expect("brew link");
        }
        let roots = ToolRoots::under(|_| None, std::slice::from_ref(&bin), &[prefix]);
        (roots, bin, cellar_bin)
    }

    /// P-2026-09-28-4 (2): Homebrew's own cargo, a link from its `bin` into
    /// its own Cellar, is accepted for the owned fetch, with the rustc beside it.
    #[cfg(unix)]
    #[test]
    fn homebrews_own_cargo_linked_into_its_cellar_is_accepted() {
        let directory = tempfile::tempdir().expect("temp");
        let root = directory.path().join("project");
        std::fs::create_dir_all(&root).expect("project");
        let (roots, bin, cellar_bin) = homebrew(directory.path());
        let path = std::env::join_paths([&bin]).expect("path");
        let (cargo, rustc) = native_tools("cargo", &path, &root, &root, directory.path(), &roots)
            .expect("Homebrew's own cargo is accepted");
        assert_eq!(
            cargo,
            std::fs::canonicalize(cellar_bin.join("cargo")).expect("cargo")
        );
        assert_eq!(
            rustc,
            std::fs::canonicalize(cellar_bin.join("rustc")).expect("rustc")
        );
    }

    /// Nothing else is widened: a link from an allowed `bin` into any other
    /// place is still not taken for the owned fetch, and the fetch is skipped.
    #[cfg(unix)]
    #[test]
    fn a_link_from_an_allowed_bin_to_anywhere_else_skips_the_owned_fetch() {
        let directory = tempfile::tempdir().expect("temp");
        let root = directory.path().join("project");
        std::fs::create_dir_all(&root).expect("project");
        let (roots, bin, _) = homebrew(directory.path());
        let elsewhere = directory.path().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).expect("elsewhere");
        for tool in ["cargo", "rustc"] {
            std::fs::write(elsewhere.join(tool), format!("elsewhere {tool}")).expect("tool");
            std::fs::remove_file(bin.join(tool)).expect("unlink the brew link");
            std::os::unix::fs::symlink(elsewhere.join(tool), bin.join(tool)).expect("link");
        }
        let path = std::env::join_paths([&bin]).expect("path");
        let refused = native_tools("cargo", &path, &root, &root, directory.path(), &roots)
            .expect_err("a link from an allowed bin to elsewhere was accepted");
        assert!(
            refused
                .to_string()
                .contains(&bin.join("cargo").display().to_string()),
            "{refused}"
        );
        let why = skipped_reason(prepare_in(
            &cargo_test(),
            &root,
            &root,
            Some(&path),
            None,
            &roots,
        ));
        assert!(why.contains("cargo"), "{why}");
    }

    /// The Cellar is accepted only when reached through that installation's
    /// own `bin`: the same Cellar cargo linked from another folder is not.
    #[cfg(unix)]
    #[test]
    fn a_cellar_reached_from_outside_homebrews_bin_skips_the_owned_fetch() {
        let directory = tempfile::tempdir().expect("temp");
        let root = directory.path().join("project");
        std::fs::create_dir_all(&root).expect("project");
        let (roots, _, cellar_bin) = homebrew(directory.path());
        let other = directory.path().join("other-bin");
        std::fs::create_dir_all(&other).expect("other");
        for tool in ["cargo", "rustc"] {
            std::os::unix::fs::symlink(cellar_bin.join(tool), other.join(tool)).expect("link");
        }
        let path = std::env::join_paths([&other]).expect("path");
        native_tools("cargo", &path, &root, &root, directory.path(), &roots)
            .expect_err("a Cellar reached from outside Homebrew's bin was accepted");
        skipped_reason(prepare_in(
            &cargo_test(),
            &root,
            &root,
            Some(&path),
            None,
            &roots,
        ));
    }

    #[test]
    fn configured_target_paths_belong_to_the_defining_config_root() {
        let directory = tempfile::tempdir().expect("temp");
        let path = config(
            directory.path(),
            "[build]\ntarget = 'targets/custom.json'\n",
        );
        let (_, _, targets) = declarative_config(&[path]).expect("config");
        assert_eq!(
            targets,
            Some(vec![directory
                .path()
                .join("targets/custom.json")
                .display()
                .to_string()])
        );
    }

    #[test]
    fn configured_target_arrays_keep_parent_and_child_targets() {
        let directory = tempfile::tempdir().expect("temp");
        let parent = config(
            directory.path(),
            "[build]\ntarget = ['aarch64-apple-darwin']\n",
        );
        let child = config(
            &directory.path().join("member"),
            "[build]\ntarget = ['x86_64-unknown-linux-gnu']\n",
        );
        let (_, _, targets) = declarative_config(&[parent, child]).expect("configs");
        assert_eq!(
            targets,
            Some(vec![
                "aarch64-apple-darwin".into(),
                "x86_64-unknown-linux-gnu".into()
            ])
        );
    }

    fn target_fixture() -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().expect("target fixture");
        let root = directory.path().join("project");
        std::fs::create_dir_all(root.join("targets")).expect("project");
        // No backend, executable, compiler hook, linker, or code is provided.
        std::fs::write(root.join("targets/custom.json"), "{}\n").expect("harmless JSON");
        (directory, root)
    }

    fn assert_target_refused(root: &Path, argv: &[String]) {
        match prepare(argv, root, root, None) {
            Err(error) => {
                let message = error.to_string();
                assert!(message.contains("target"), "{message}");
                assert!(
                    message.contains("built-in"),
                    "refusal must name supported targets: {message}"
                );
            }
            Ok(_) => panic!("unsupported target produced a fetch plan: {argv:?}"),
        }
    }

    #[test]
    fn custom_target_paths_never_produce_a_fetch_plan() {
        let (_directory, root) = target_fixture();
        let absolute = root.join("targets/custom.json");
        for target in [
            "targets/custom.json".to_string(),
            absolute.display().to_string(),
        ] {
            assert_target_refused(
                &root,
                &[
                    "cargo".into(),
                    "test".into(),
                    "--target".into(),
                    target.clone(),
                ],
            );
            assert_target_refused(
                &root,
                &["cargo".into(), "test".into(), format!("--target={target}")],
            );
        }
        #[cfg(unix)]
        {
            let alias = root.join("target-alias");
            std::os::unix::fs::symlink(&absolute, &alias).expect("target symlink");
            assert_target_refused(
                &root,
                &[
                    "cargo".into(),
                    "test".into(),
                    "--target".into(),
                    alias.display().to_string(),
                ],
            );
        }
    }

    #[test]
    fn custom_target_from_a_literal_shell_environment_is_refused() {
        let (_directory, root) = target_fixture();
        assert_target_refused(
            &root,
            &[
                "sh".into(),
                "-c".into(),
                "CARGO_BUILD_TARGET='targets/custom.json' exec cargo test".into(),
            ],
        );
    }

    #[test]
    fn configured_custom_target_is_refused_after_parsing_its_original_path() {
        for setting in [
            "'targets/custom.json'",
            "['aarch64-apple-darwin', 'targets/custom.json']",
        ] {
            let (_directory, root) = target_fixture();
            config(&root, &format!("[build]\ntarget = {setting}\n"));
            assert_target_refused(&root, &["cargo".into(), "test".into()]);
        }
    }

    #[test]
    fn a_triple_shaped_name_not_in_the_compiler_catalog_is_refused() {
        let (_directory, root) = target_fixture();
        assert_target_refused(
            &root,
            &[
                "cargo".into(),
                "test".into(),
                "--target".into(),
                "thruline-unknown-not-a-real-target".into(),
            ],
        );
    }

    #[test]
    fn builtin_catalog_targets_and_default_host_keep_the_fetch_plan() {
        let (_directory, root) = target_fixture();
        for target in [
            None,
            Some("aarch64-apple-darwin"),
            Some("x86_64-unknown-linux-gnu"),
            Some("host-tuple"),
        ] {
            let mut argv = vec!["cargo".into(), "test".into()];
            if let Some(target) = target {
                argv.extend(["--target".into(), target.into()]);
            }
            let plan = prepare(&argv, &root, &root, None)
                .expect("builtin target is supported")
                .expect("Cargo fetch plan");
            assert_eq!(plan.argv[1], "fetch");
            assert!(plan.argv.iter().any(|arg| arg == "--offline"));
            let target_at = plan
                .argv
                .iter()
                .position(|arg| arg == "--target")
                .expect("effective target");
            assert!(!plan.argv[target_at + 1].ends_with(".json"));
            if let Some(target) = target.filter(|target| *target != "host-tuple") {
                assert_eq!(plan.argv[target_at + 1], target, "target selection changed");
            }
        }
    }

    // OFFLINE-KILL-GUARD-R5: the production probe answers within 2 s, then
    // allows 500 ms for the killed group to be reaped. Pinned here, because the
    // stalled-compiler test below runs the same probe at a longer bound.
    #[test]
    fn the_compiler_probe_keeps_its_production_bound() {
        use std::time::Duration;
        assert_eq!(
            ProbeBound::PRODUCTION,
            ProbeBound {
                run: Duration::from_secs(2),
                reap: Duration::from_millis(500),
            }
        );
    }

    // OFFLINE-KILL-GUARD-R5: the stand-in is a shell script, and under a busy
    // machine a shell can take longer than the production 2 s to run its first
    // line (the gate of e3fb20dc saw that 3/3 at load ~20; a direct spawn took
    // up to 7.8 s at load ~28, most of it the first exec of a new file). The
    // stand-in is warmed once before the clock starts, and the probe here runs
    // at five times the production bound, so the evidence that the compiler
    // was invoked does not race the production deadline; the elapsed bound is
    // scaled by the same factor and is exactly 5 s at the production bound.
    #[cfg(unix)]
    #[test]
    fn a_stalled_trusted_compiler_catalog_is_refused_and_reaped() {
        use std::os::unix::fs::PermissionsExt;
        use std::time::Instant;
        let bound = ProbeBound {
            run: ProbeBound::PRODUCTION.run * 5,
            reap: ProbeBound::PRODUCTION.reap * 5,
        };
        let (_directory, root) = target_fixture();
        let (roots, tools, _) = fake_toolchain(root.parent().expect("parent"));
        std::fs::write(tools.join("cargo"), "external cargo placeholder").expect("cargo");
        let marker = tools.join("compiler.pid");
        let rustc = tools.join("rustc");
        std::fs::write(
            &rustc,
            format!(
                "#!/bin/sh\n[ \"$1\" = warm ] && exit 0\nprintf '%s' \"$$\" > '{}'\nexec /bin/sleep 30\n",
                marker.display()
            ),
        )
        .expect("harmless stalled host tool");
        std::fs::set_permissions(&rustc, std::fs::Permissions::from_mode(0o755))
            .expect("executable");
        // The first exec of a freshly written executable is slow on macOS
        // (measured 160-700 ms at load ~35 against 5-13 ms for a repeat exec
        // of the same file); that one-time cost is paid here, unbounded, so
        // the probe's own spawn is not charged for it.
        // The warm run exits before writing the marker: only the probe's own
        // compiler can write it.
        let warm = Command::new(&rustc)
            .arg("warm")
            .status()
            .expect("stand-in runs");
        assert!(warm.success(), "stand-in warm-up failed");
        assert!(!marker.exists(), "the warm-up wrote the invocation marker");
        let path = std::env::join_paths([&tools]).expect("path");
        let started = Instant::now();
        let result = prepare_bounded(
            &[
                "cargo".into(),
                "test".into(),
                "--target".into(),
                "aarch64-apple-darwin".into(),
            ],
            &root,
            &root,
            Some(&path),
            None,
            &roots,
            bound,
        )
        .map(Prepared::into_plan);
        let elapsed = started.elapsed();
        assert!(result.is_err(), "a stalled compiler produced a fetch plan");
        assert!(
            elapsed < (bound.run + bound.reap) * 2,
            "compiler probe did not respect its bound"
        );
        assert!(
            elapsed >= bound.run,
            "the probe refused before its own deadline"
        );
        let message = result.err().expect("error").to_string();
        assert!(message.contains("timed out"), "{message}");
        let pid = std::fs::read_to_string(&marker).expect("compiler was invoked");
        let alive = Command::new("/bin/kill")
            .args(["-0", pid.trim()])
            .status()
            .expect("own process check");
        assert!(
            !alive.success(),
            "the owned compiler leader survived the refusal"
        );
    }
}

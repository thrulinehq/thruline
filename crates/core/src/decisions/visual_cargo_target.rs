//! Route and monitor historical Cargo output separately from scratch records.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
    time::{Duration, Instant},
};
const GIB: u64 = 1 << 30;
static START: Mutex<()> = Mutex::new(());

pub(super) struct DirectoryPin {
    path: PathBuf,
    file: File,
    #[cfg(unix)]
    dev: u64,
    #[cfg(unix)]
    ino: u64,
}
impl DirectoryPin {
    pub(super) fn open(path: &Path) -> Result<Self, &'static str> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let before =
                std::fs::symlink_metadata(path).map_err(|_| "Cargo target root is unavailable")?;
            if !before.is_dir() || before.file_type().is_symlink() {
                return Err("Cargo target root must be an existing nonsymlink directory");
            }
            let fd = rustix::fs::open(
                path,
                rustix::fs::OFlags::RDONLY
                    | rustix::fs::OFlags::DIRECTORY
                    | rustix::fs::OFlags::NOFOLLOW
                    | rustix::fs::OFlags::CLOEXEC,
                rustix::fs::Mode::empty(),
            )
            .map_err(|_| "Cargo target root cannot be pinned")?;
            let file = File::from(fd);
            let after = file
                .metadata()
                .map_err(|_| "Cargo target root cannot be pinned")?;
            if before.dev() != after.dev() || before.ino() != after.ino() {
                return Err("Cargo target root changed while being pinned");
            }
            Ok(Self {
                path: path.to_path_buf(),
                file,
                dev: after.dev(),
                ino: after.ino(),
            })
        }
        #[cfg(not(unix))]
        {
            let _ = path;
            Err("Cargo target volume verification is unavailable on this platform")
        }
    }
    pub(super) fn verify(&self) -> Result<(), &'static str> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let meta = std::fs::symlink_metadata(&self.path)
                .map_err(|_| "Cargo target root disappeared")?;
            if !meta.is_dir() || meta.dev() != self.dev || meta.ino() != self.ino {
                return Err("Cargo target root changed or its volume disconnected");
            }
            Ok(())
        }
        #[cfg(not(unix))]
        {
            Err("Cargo target volume verification is unavailable on this platform")
        }
    }
    pub(super) fn path(&self) -> &Path {
        &self.path
    }
    pub(super) fn child(&self, name: &std::ffi::OsStr) -> Result<Self, &'static str> {
        if Path::new(name).components().count() != 1
            || !matches!(
                Path::new(name).components().next(),
                Some(std::path::Component::Normal(_))
            )
        {
            return Err("generated directory name is invalid");
        }
        self.verify()?;
        let path = self.path.join(name);
        if std::fs::symlink_metadata(&path).is_err() {
            create_under(self, name)?;
        }
        let child = Self::open(&path)?;
        #[cfg(unix)]
        if child.dev != self.dev {
            return Err("generated directory changed target volume");
        }
        self.verify()?;
        Ok(child)
    }
    pub(super) fn create_file(&self, name: &std::ffi::OsStr) -> Result<File, &'static str> {
        if Path::new(name).components().count() != 1
            || !matches!(
                Path::new(name).components().next(),
                Some(std::path::Component::Normal(_))
            )
        {
            return Err("generated file name is invalid");
        }
        self.verify()?;
        #[cfg(unix)]
        {
            let fd = rustix::fs::openat(
                &self.file,
                name,
                rustix::fs::OFlags::WRONLY
                    | rustix::fs::OFlags::CREATE
                    | rustix::fs::OFlags::EXCL
                    | rustix::fs::OFlags::NOFOLLOW
                    | rustix::fs::OFlags::CLOEXEC,
                rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
            )
            .map_err(|_| "generated file cannot be created on its pinned volume")?;
            Ok(File::from(fd))
        }
        #[cfg(not(unix))]
        {
            Err("generated file creation is unavailable on this platform")
        }
    }
    pub(super) fn free(&self) -> Result<u64, &'static str> {
        self.verify()?;
        #[cfg(unix)]
        {
            let stat = rustix::fs::fstatvfs(&self.file)
                .map_err(|_| "Cargo target capacity is unavailable")?;
            stat.f_bavail
                .checked_mul(stat.f_frsize)
                .ok_or("Cargo target capacity is invalid")
        }
        #[cfg(not(unix))]
        {
            Err("Cargo target capacity is unavailable on this platform")
        }
    }
}

pub(super) struct CargoTarget {
    root: PathBuf,
    pub target: PathBuf,
    pin: DirectoryPin,
    root_exists: bool,
    namespace_pin: Option<DirectoryPin>,
}
impl CargoTarget {
    pub fn from_env(commit: &str) -> Result<Self, &'static str> {
        let supplied = std::env::var_os("THRULINE_VISUAL_CARGO_TARGET_ROOT").map(PathBuf::from);
        Self::resolve(commit, supplied.as_deref(), &std::env::temp_dir())
    }
    fn resolve(commit: &str, supplied: Option<&Path>, temp: &Path) -> Result<Self, &'static str> {
        if !matches!(commit.len(), 40 | 64)
            || !commit
                .bytes()
                .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
        {
            return Err("Cargo target namespace requires a full immutable Git commit");
        }
        let root =
            supplied.map_or_else(|| temp.join("thruline-c1-cargo-target"), Path::to_path_buf);
        if !root.is_absolute() {
            return Err("Cargo target root must be absolute");
        }
        let root_exists = std::fs::symlink_metadata(&root).is_ok();
        if supplied.is_some() && !root_exists {
            return Err("explicit Cargo target root is unavailable; it will not be recreated");
        }
        let pin = DirectoryPin::open(if root_exists { &root } else { temp })?;
        let route = Self {
            target: root.join(commit),
            root,
            pin,
            root_exists,
            namespace_pin: None,
        };
        route.verify()?;
        Ok(route)
    }
    fn verify(&self) -> Result<(), &'static str> {
        self.pin.verify()?;
        if self.root_exists {
            // Existing roots are pinned directly. A target namespace cannot
            // be redirected to another volume or an alias directory.
            verify_namespace(&self.target, &self.pin)?;
        }
        if let Some(pin) = &self.namespace_pin {
            pin.verify()?;
        }
        Ok(())
    }
    pub fn check_start(&self) -> Result<(), &'static str> {
        self.verify()?;
        check_space(self.pin.free()?, active_compilers()?, true)
    }
    pub(super) fn check_running(&self) -> Result<(), &'static str> {
        self.verify()?;
        check_space(self.pin.free()?, 0, false)
    }
    pub fn prepare(&mut self) -> Result<(), &'static str> {
        self.check_start()?;
        if !self.root_exists {
            // Only the legacy local root may be created. Parent identity and
            // its 25 GiB gate were verified before this single mkdir.
            create_under(&self.pin, std::ffi::OsStr::new("thruline-c1-cargo-target"))?;
            self.pin = DirectoryPin::open(&self.root)?;
            self.root_exists = true;
        }
        self.verify()?;
        if !self.target.exists() {
            create_under(
                &self.pin,
                self.target
                    .file_name()
                    .ok_or("Cargo commit namespace is invalid")?,
            )?;
        }
        self.namespace_pin = Some(DirectoryPin::open(&self.target)?);
        self.verify()
    }
    pub fn configure(&self, command: &mut Command) {
        command
            .env("CARGO_TARGET_DIR", &self.target)
            .env("CARGO_BUILD_JOBS", "2")
            .env("CARGO_PROFILE_DEV_DEBUG", "0")
            .env("CARGO_PROFILE_TEST_DEBUG", "0")
            .env("CARGO_PROFILE_RELEASE_DEBUG", "0")
            .env("CARGO_INCREMENTAL", "0");
    }
    pub fn run(&self, command: &mut Command) -> Result<Output, &'static str> {
        self.run_with_checks(command, || Ok(()))
    }
    pub(super) fn run_with_checks(
        &self,
        command: &mut Command,
        mut additional: impl FnMut() -> Result<(), &'static str>,
    ) -> Result<Output, &'static str> {
        self.configure(command);
        let scratch = DirectoryPin::open(
            command
                .get_current_dir()
                .ok_or("visual scratch source directory is unavailable")?,
        )?;
        let mut lock = Some(start_lock(
            std::env::var_os("THRULINE_VISUAL_BUILD_LOCK_FILE")
                .map(PathBuf::from)
                .as_deref(),
        )?);
        let mut started = false;
        monitored_output(command, || {
            additional()?;
            if scratch.free()? < 15 * GIB {
                return Err("visual scratch volume fell below 15 GiB");
            }
            if !started {
                started = true;
                self.check_start()
            } else {
                // The leader is now visible to the shared process census.
                lock.take();
                self.check_running()
            }
        })
    }
}
fn create_under(pin: &DirectoryPin, name: &std::ffi::OsStr) -> Result<(), &'static str> {
    #[cfg(unix)]
    {
        rustix::fs::mkdirat(&pin.file, name, rustix::fs::Mode::RWXU)
            .map_err(|_| "Cargo directory cannot be created on its pinned volume")
    }
    #[cfg(not(unix))]
    {
        let _ = (pin, name);
        Err("Cargo directory creation is unavailable on this platform")
    }
}
fn verify_namespace(path: &Path, pin: &DirectoryPin) -> Result<(), &'static str> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if !meta.is_dir() || meta.dev() != pin.dev {
                    return Err("Cargo commit namespace is redirected or unavailable");
                }
            }
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("Cargo commit namespace is unavailable"),
    }
}
fn check_space(free: u64, active: usize, start: bool) -> Result<(), &'static str> {
    let floor = if start {
        (active as u64)
            .checked_mul(10)
            .and_then(|n| n.checked_add(25))
            .and_then(|n| n.checked_mul(GIB))
            .ok_or("compiler capacity reserve overflowed")?
    } else {
        15 * GIB
    };
    if free < floor {
        Err(if start {
            "Cargo target volume lacks 25 GiB plus 10 GiB per active compiler"
        } else {
            "Cargo target volume fell below 15 GiB"
        })
    } else {
        Ok(())
    }
}
fn active_compilers() -> Result<usize, &'static str> {
    // macOS ps is setuid and cannot be spawned inside a capture sandbox.
    // Read only native PID/parent/name metadata; never process argv or env.
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("python3");
        command.args(["-I", "-c", include_str!("visual_process_census.py")]);
        command
    };
    #[cfg(not(target_os = "macos"))]
    let mut command = {
        let mut command = Command::new("/bin/ps");
        command.args(["-axo", "pid=,ppid=,comm="]);
        command
    };
    let output = match crate::bounded_child::output_until(
        &mut command,
        Instant::now() + Duration::from_secs(5),
    )
    .map_err(|_| "active compiler census is unavailable")?
    {
        crate::bounded_child::Outcome::Finished(output) => output,
        crate::bounded_child::Outcome::TimedOut => return Err("active compiler census timed out"),
    };
    if !output.status.success() {
        return Err("active compiler census failed");
    }
    compiler_roots(
        &String::from_utf8(output.stdout).map_err(|_| "active compiler census is invalid")?,
    )
}
fn compiler_roots(text: &str) -> Result<usize, &'static str> {
    let mut rows = BTreeMap::new();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let pid = fields
            .next()
            .and_then(|p| p.parse::<u32>().ok())
            .ok_or("active compiler census is invalid")?;
        let parent = fields
            .next()
            .and_then(|p| p.parse::<u32>().ok())
            .ok_or("active compiler census is invalid")?;
        let command = fields.collect::<Vec<_>>().join(" ");
        if command.is_empty() || rows.insert(pid, (parent, command)).is_some() {
            return Err("active compiler census is invalid");
        }
    }
    if rows.is_empty() {
        return Err("active compiler census is empty");
    }
    let builders: BTreeSet<_> = rows
        .iter()
        .filter_map(|(&pid, (_, cmd))| {
            matches!(
                Path::new(cmd).file_name().and_then(|p| p.to_str()),
                Some("cargo" | "cargo-clippy" | "rustc")
            )
            .then_some(pid)
        })
        .collect();
    let mut roots = BTreeSet::new();
    for &pid in &builders {
        let mut parent = rows[&pid].0;
        let mut seen = BTreeSet::new();
        while rows.contains_key(&parent) && !builders.contains(&parent) {
            if !seen.insert(parent) {
                return Err("active compiler ancestry is cyclic");
            }
            parent = rows[&parent].0;
        }
        if !builders.contains(&parent) {
            roots.insert(pid);
        }
    }
    Ok(roots.len())
}
struct StartLock {
    _local: MutexGuard<'static, ()>,
    file: Option<File>,
}
impl Drop for StartLock {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(file) = &self.file {
            let _ = rustix::fs::flock(file, rustix::fs::FlockOperation::Unlock);
        }
    }
}
fn start_lock(path: Option<&Path>) -> Result<StartLock, &'static str> {
    let local = START
        .lock()
        .map_err(|_| "compiler start lock is unavailable")?;
    let Some(path) = path else {
        return Ok(StartLock {
            _local: local,
            file: None,
        });
    };
    if !path.is_absolute() {
        return Err("compiler start lock must be an absolute existing file");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let fd = rustix::fs::open(
            path,
            rustix::fs::OFlags::RDWR | rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )
        .map_err(|_| "compiler start lock is unavailable")?;
        let file = File::from(fd);
        let meta = file
            .metadata()
            .map_err(|_| "compiler start lock is unavailable")?;
        if !meta.is_file() || meta.uid() != rustix::process::geteuid().as_raw() {
            return Err("compiler start lock must be a regular file owned by this account");
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive) {
                Ok(()) => break,
                Err(rustix::io::Errno::AGAIN) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(50))
                }
                _ => return Err("compiler start lock is busy or unavailable"),
            }
        }
        let now = std::fs::symlink_metadata(path).map_err(|_| "compiler start lock changed")?;
        if !now.is_file() || now.dev() != meta.dev() || now.ino() != meta.ino() {
            let _ = rustix::fs::flock(&file, rustix::fs::FlockOperation::Unlock);
            return Err("compiler start lock changed");
        }
        Ok(StartLock {
            _local: local,
            file: Some(file),
        })
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err("shared compiler locking is unavailable on this platform")
    }
}
struct ChildGuard {
    child: Child,
    reaped: bool,
}
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
impl ChildGuard {
    fn stop(&mut self) -> Result<std::process::ExitStatus, &'static str> {
        if !self.reaped {
            // The leader is not reaped until this group signal, reserving its
            // pid against reuse by an unrelated process.
            let _ = crate::bounded_child::kill_owned_group(&mut self.child);
            let _ = self.child.kill();
        }
        let status = self
            .child
            .wait()
            .map_err(|_| "Cargo child could not be reaped")?;
        self.reaped = true;
        Ok(status)
    }
}
fn child_exited(child: &Child) -> Result<bool, &'static str> {
    #[cfg(unix)]
    {
        let pid = i32::try_from(child.id())
            .ok()
            .and_then(rustix::process::Pid::from_raw)
            .ok_or("Cargo child pid is invalid")?;
        loop {
            match rustix::process::waitid(
                rustix::process::WaitId::Pid(pid),
                rustix::process::WaitIdOptions::EXITED
                    | rustix::process::WaitIdOptions::NOWAIT
                    | rustix::process::WaitIdOptions::NOHANG,
            ) {
                Ok(status) => return Ok(status.is_some()),
                Err(rustix::io::Errno::INTR) => continue,
                Err(_) => return Err("Cargo child status is unavailable"),
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child;
        Err("Cargo child group monitoring is unavailable on this platform")
    }
}
fn monitored_output(
    command: &mut Command,
    mut guard: impl FnMut() -> Result<(), &'static str>,
) -> Result<Output, &'static str> {
    guard()?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = ChildGuard {
        child: command
            .spawn()
            .map_err(|_| "exact server builder is unavailable")?,
        reaped: false,
    };
    let bad = Arc::new(AtomicBool::new(false));
    let stdout = child
        .child
        .stdout
        .take()
        .ok_or("Cargo output is unavailable")?;
    let stderr = child
        .child
        .stderr
        .take()
        .ok_or("Cargo diagnostics are unavailable")?;
    let out_bad = bad.clone();
    let err_bad = bad.clone();
    let out = std::thread::spawn(move || read_output(stdout, &out_bad));
    let err = std::thread::spawn(move || read_output(stderr, &err_bad));
    let status = loop {
        let failure = guard().err().or_else(|| {
            bad.load(Ordering::Relaxed)
                .then_some("Cargo output exceeded its bound")
        });
        if let Some(reason) = failure {
            let _ = child.stop();
            let _ = out.join();
            let _ = err.join();
            return Err(reason);
        }
        match child_exited(&child.child) {
            Ok(true) => break child.stop()?,
            Ok(false) => std::thread::sleep(Duration::from_millis(100)),
            Err(_) => {
                let _ = child.stop();
                let _ = out.join();
                let _ = err.join();
                return Err("Cargo child could not be reaped");
            }
        }
    };
    // Cargo can leave descendants holding output pipes after its own exit.
    let _ = child.stop();
    let stdout = out.join().map_err(|_| "Cargo output reader failed")?;
    let stderr = err.join().map_err(|_| "Cargo diagnostic reader failed")?;
    if bad.load(Ordering::Relaxed) {
        return Err("Cargo output exceeded its bound");
    }
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}
fn read_output(mut reader: impl Read, bad: &AtomicBool) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) if bytes.len() + n <= 1024 * 1024 => bytes.extend_from_slice(&buf[..n]),
            _ => {
                bad.store(true, Ordering::Relaxed);
                break;
            }
        }
    }
    bytes
}
#[cfg(all(test, target_os = "macos"))]
#[path = "visual_cargo_census_tests.rs"]
mod census_tests;
#[cfg(test)]
#[path = "visual_cargo_target_tests.rs"]
mod tests;

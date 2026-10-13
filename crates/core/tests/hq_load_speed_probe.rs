//! C27 load-speed probe. Ignored by default: it reads real recordings IN PLACE
//! and writes only into a scratch store named by `THRULINE_PROBE_STORE`
//! (`<store>/ledger.db`, `<store>/mirrors`). It measures; it asserts nothing.
//!
//! ```text
//! THRULINE_PROBE_STORE=<scratch dir> THRULINE_PROBE_ROOT=/Users/me/repo \
//!   [THRULINE_PROBE_CLAUDE=~/.claude/projects] [THRULINE_PROBE_CODEX=a:b] \
//!   [THRULINE_PROBE_TSV=<out.tsv>] [THRULINE_PROBE_LABEL=before-reload] \
//!   cargo test --release -p thruline-core --test hq_load_speed_probe -- --ignored --nocapture
//! ```
//!
//! It runs the project's load plan exactly as `POST /api/hq/projects/<id>/load`
//! does ([`thruline_core::hq::run_load`]), file by file, and prints the total,
//! per-phase totals (from `IngestReport::phase_ns`), files/s, and the ten
//! slowest files with their size and phase split. Run it twice on the same
//! store: the first run is a first load, the second a no-change reload.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use thruline_core::{hq, Ledger};

#[path = "support/live_store.rs"]
mod live_store;

/// A path from the environment, refused (C29) when it lies in the live store:
/// every store, mirror and output path this probe opens comes through here.
fn env_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os(name).map(PathBuf::from)?;
    live_store::refuse_live_store(&path);
    Some(path)
}

#[test]
#[ignore = "measurement against real recordings; needs THRULINE_PROBE_STORE and THRULINE_PROBE_ROOT"]
fn load_the_plan_file_by_file() {
    let (Some(store), Some(root)) = (
        env_path("THRULINE_PROBE_STORE"),
        std::env::var("THRULINE_PROBE_ROOT").ok(),
    ) else {
        eprintln!("THRULINE_PROBE_STORE / THRULINE_PROBE_ROOT not set; nothing measured");
        return;
    };
    let home = env_path("HOME").expect("HOME");
    let claude = env_path("THRULINE_PROBE_CLAUDE").unwrap_or_else(|| home.join(".claude/projects"));
    let codex: Vec<PathBuf> = match std::env::var("THRULINE_PROBE_CODEX") {
        Ok(v) => v.split(':').map(PathBuf::from).collect(),
        Err(_) => thruline_core::ingest::default_codex_roots(),
    };
    let label = std::env::var("THRULINE_PROBE_LABEL").unwrap_or_else(|_| "run".into());
    std::fs::create_dir_all(&store).expect("store dir");
    let db = store.join("ledger.db");
    let mirror = store.join("mirrors");

    let t = Instant::now();
    let plan = hq::load_plan(&root, &claude, &codex);
    let plan_time = t.elapsed();
    eprintln!(
        "[{label}] load_plan {:?}: {} claude + {} codex files, {} bytes",
        plan_time,
        plan.claude_code.len(),
        plan.codex.len(),
        plan.bytes
    );

    if std::env::var_os("THRULINE_PROBE_PLAN_ONLY").is_some() {
        return;
    }
    let workers = std::env::var("THRULINE_PROBE_WORKERS")
        .ok()
        .and_then(|w| w.parse().ok())
        .unwrap_or_else(hq::default_workers);
    let mut ledger = Ledger::open(&db).expect("open scratch ledger");
    let started = Instant::now();
    let mut rows: Vec<hq::LoadFileTiming> = Vec::new();
    // Time to the first thread HQ can show for this root: after each file
    // that added rows, until one is seen, a second connection asks HQ what
    // the Inbox would — the project list, then the project's threads.
    //
    // That check runs inside the load's per-file callback, so in a parallel
    // load its time is charged to the next file's `wall_ns` and to the
    // total. On a large store it is a full index build (tens of seconds), so
    // `THRULINE_PROBE_NO_FIRST_THREAD=1` turns it off to time the load alone,
    // as the server runs it (its callback only updates a status).
    let check_first_thread = std::env::var_os("THRULINE_PROBE_NO_FIRST_THREAD").is_none();
    let mut first_thread: Option<std::time::Duration> = None;
    let outcome = hq::run_load_with(&mut ledger, &mirror, &plan, workers, &mut |timing| {
        rows.push(timing.clone());
        if check_first_thread && first_thread.is_none() && timing.events_added > 0 {
            if let Ok(reader) = Ledger::open(&db) {
                let now = hq::parse_ms("2026-10-07T12:00:00Z").unwrap_or(0);
                let shown = hq::hq_projects(&reader)
                    .ok()
                    .and_then(|ps| ps.into_iter().find(|p| p.root == root))
                    .and_then(|p| hq::project(&reader, &p.id, now).ok().flatten())
                    .is_some_and(|view| !view.threads.is_empty());
                if shown {
                    first_thread = Some(started.elapsed());
                }
            }
        }
        true
    });
    let total = started.elapsed();
    eprintln!(
        "[{label}] workers {workers}; first thread visible after {:?}; mirror open + orphan scan {:.3} s",
        first_thread,
        outcome.setup_ns as f64 / 1e9
    );

    let mut phases: BTreeMap<String, u64> = BTreeMap::new();
    for r in &rows {
        for (k, v) in &r.phase_ns {
            *phases.entry(k.clone()).or_insert(0) += v;
        }
    }
    let files = rows.len();
    eprintln!(
        "[{label}] total {:.2} s for {} files ({:.1} files/s), events added {}, opened {}, skipped unopened {}, errors {}",
        total.as_secs_f64(),
        files,
        files as f64 / total.as_secs_f64().max(1e-9),
        outcome.events_added,
        rows.iter().filter(|r| !r.skipped_unopened).count(),
        rows.iter().filter(|r| r.skipped_unopened).count(),
        outcome.errors.len(),
    );
    for (k, v) in &phases {
        eprintln!("[{label}]   phase {k:<14} {:>9.3} s", *v as f64 / 1e9);
    }
    let accounted: u64 = phases.values().sum();
    eprintln!(
        "[{label}]   phase {:<14} {:>9.3} s  (wall minus every named phase)",
        "unattributed",
        (total.as_nanos() as f64 - accounted as f64) / 1e9
    );
    let mut slow: Vec<&hq::LoadFileTiming> = rows.iter().collect();
    slow.sort_by_key(|r| std::cmp::Reverse(r.wall_ns));
    eprintln!("[{label}] ten slowest files:");
    for r in slow.iter().take(10) {
        let split: Vec<String> = r
            .phase_ns
            .iter()
            .map(|(k, v)| format!("{k}={:.3}", *v as f64 / 1e9))
            .collect();
        eprintln!(
            "[{label}]   {:>8.3} s {:>11} B {} {} [{}]",
            r.wall_ns as f64 / 1e9,
            r.bytes,
            if r.codex { "codex " } else { "claude" },
            r.file.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
            split.join(" ")
        );
    }
    if let Some(out) = env_path("THRULINE_PROBE_TSV") {
        let mut tsv =
            String::from("file\tsource\tbytes\twall_ns\tskipped_unopened\tevents\tphases\n");
        for r in &rows {
            let split: Vec<String> = r.phase_ns.iter().map(|(k, v)| format!("{k}={v}")).collect();
            tsv.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                r.file.display(),
                if r.codex { "codex" } else { "claude_code" },
                r.bytes,
                r.wall_ns,
                r.skipped_unopened,
                r.events_added,
                split.join(",")
            ));
        }
        std::fs::write(&out, tsv).expect("write tsv");
    }
}

/// The once-per-load orphan walk, measured READ-ONLY against a real mirror:
/// every generation directory is listed and `lstat`ed exactly as
/// `recover_unaccepted_generations` does for an accepted generation, and the
/// ledger's accepted generation ids are read through an immutable, read-only
/// SQLite URI. Nothing is written anywhere.
///
/// `THRULINE_PROBE_RO_DB=<ledger.db> THRULINE_PROBE_RO_MIRROR=<mirrors/ledgers/<authority>>`
#[test]
#[ignore = "read-only measurement against a real store"]
fn time_the_orphan_walk_read_only() {
    let (Some(db), Some(mirror)) = (
        env_path("THRULINE_PROBE_RO_DB"),
        env_path("THRULINE_PROBE_RO_MIRROR"),
    ) else {
        eprintln!("THRULINE_PROBE_RO_DB / THRULINE_PROBE_RO_MIRROR not set; nothing measured");
        return;
    };
    let uri = format!("file:{}?mode=ro&immutable=1", db.display());
    let conn = rusqlite::Connection::open_with_flags(
        uri,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .expect("open read-only");
    let t = Instant::now();
    let mut accepted = std::collections::HashSet::new();
    let mut by_source: BTreeMap<String, u64> = BTreeMap::new();
    let mut stmt = conn
        .prepare(
            "SELECT g.id, s.source FROM session_generations g
             JOIN source_sessions s ON s.session_id = g.session_id",
        )
        .expect("query");
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .expect("rows");
    for row in rows {
        let (id, source) = row.expect("row");
        *by_source.entry(source).or_insert(0) += 1;
        accepted.insert(id.trim_start_matches("generation_").to_string());
    }
    eprintln!(
        "accepted generations {:?} read in {:?}",
        by_source,
        t.elapsed()
    );
    let t = Instant::now();
    let (mut dirs, mut unaccepted) = (0u64, 0u64);
    let generations = mirror.join("generations");
    for project in std::fs::read_dir(&generations)
        .expect("generations")
        .flatten()
    {
        let _ = std::fs::symlink_metadata(project.path());
        for session in std::fs::read_dir(project.path())
            .into_iter()
            .flatten()
            .flatten()
        {
            let _ = std::fs::symlink_metadata(session.path());
            for generation in std::fs::read_dir(session.path())
                .into_iter()
                .flatten()
                .flatten()
            {
                let _ = std::fs::symlink_metadata(generation.path());
                dirs += 1;
                let key = generation.file_name().to_string_lossy().into_owned();
                if !accepted.contains(&key) {
                    unaccepted += 1;
                }
            }
        }
    }
    eprintln!(
        "walked {dirs} generation directories ({unaccepted} not accepted by this ledger) in {:?}",
        t.elapsed()
    );
}

/// C27 budget (c): where one thread read's time goes. For the project whose
/// root is `THRULINE_PROBE_ROOT` in the store `THRULINE_PROBE_DB`, the
/// `THRULINE_PROBE_THREADS` (default 8) threads with the most edits: project
/// lookup, index (cold build, then cached-equivalent reuse), view build and
/// serialization, each timed, with the payload's size. Read only.
#[test]
#[ignore = "measurement against a store named by THRULINE_PROBE_DB"]
fn time_one_thread_read() {
    let (Some(db), Some(root)) = (
        env_path("THRULINE_PROBE_DB"),
        std::env::var("THRULINE_PROBE_ROOT").ok(),
    ) else {
        eprintln!("THRULINE_PROBE_DB / THRULINE_PROBE_ROOT not set; nothing measured");
        return;
    };
    let n: usize = std::env::var("THRULINE_PROBE_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8);
    let ledger = Ledger::open(&db).expect("open");
    let now = hq::parse_ms("2026-10-07T12:00:00Z").unwrap_or(0);
    let t = Instant::now();
    let project = hq::hq_projects(&ledger)
        .expect("projects")
        .into_iter()
        .find(|p| p.root == root)
        .expect("project for root");
    let found = t.elapsed();
    let t = Instant::now();
    let _ = hq::find_project(&ledger, &project.id).expect("find");
    let find = t.elapsed();
    let t = Instant::now();
    let fp = hq::fingerprint(&ledger, &project).expect("fingerprint");
    let fingerprint = t.elapsed();
    let t = Instant::now();
    let index = hq::index(&ledger, &project).expect("index");
    let built = t.elapsed();
    eprintln!(
        "project list {found:?}; find_project {find:?}; fingerprint {fingerprint:?} ({} chars); index build (cold) {built:?}",
        fp.len()
    );
    let mut threads = index.view(now).threads;
    threads.sort_by_key(|th| std::cmp::Reverse(th.edits));
    for th in threads.iter().take(n) {
        let t = Instant::now();
        let view = index
            .thread_view(&ledger, &th.id, now)
            .expect("view")
            .expect("present");
        let build = t.elapsed();
        let t = Instant::now();
        let bytes = serde_json::to_vec(&view).expect("json");
        let ser = t.elapsed();
        let hunks: usize = serde_json::to_value(&view)
            .ok()
            .and_then(|v| v.get("edits").cloned())
            .map(|e| serde_json::to_vec(&e).map(|b| b.len()).unwrap_or(0))
            .unwrap_or(0);
        eprintln!(
            "thread {} edits {} — view {build:?}, serialize {ser:?}, {} bytes ({} in edits)",
            th.id,
            th.edits,
            bytes.len(),
            hunks
        );
        if let Some(out) = env_path("THRULINE_PROBE_DUMP") {
            if !out.join(format!("{}.json", th.id)).exists() {
                let _ = std::fs::create_dir_all(&out);
                let _ = std::fs::write(out.join(format!("{}.json", th.id)), &bytes);
            }
        }
    }
}

/// C27 budget (b): why a no-change reload of one project still reads files.
///
/// For every file of the project's load plan, the branch of
/// [`thruline_core::ingest::LoadMirror::unchanged`] that decides it, judged
/// against a REAL store opened READ-ONLY: the ledger through the SQLite URI
/// `mode=ro` (hooks write a live store, so `immutable=1` is not used), and
/// the mirror through [`thruline_core::mirror::MirrorStore::open_for_verification`],
/// which creates nothing. The recordings themselves are only `stat`ed and,
/// for a file the check would not skip, read once to time a pass over it.
/// Nothing is written anywhere except the optional TSV.
///
/// **Point `THRULINE_PROBE_RO_DB` at a copy-on-write clone of the ledger, not
/// at the live file.** SQLite documents that a `mode=ro` connection to a WAL
/// database creates the `-wal` and `-shm` files when they are absent and the
/// directory is writable, so opening the live ledger can create files beside
/// it. `cp -c` (APFS clone) taken while no `-wal` exists gives a checkpointed
/// point-in-time copy that costs no space up front; the mirror is read in
/// place (`THRULINE_PROBE_RO_MIRRORS`).
///
/// ```text
/// THRULINE_PROBE_RO_DB=<clone>/ledger.db THRULINE_PROBE_ROOT=/Users/me/repo \
///   THRULINE_PROBE_RO_MIRRORS=~/.thruline/mirrors [THRULINE_PROBE_TSV=<out.tsv>] \
///   cargo test --release -p thruline-core --test hq_load_speed_probe \
///     why_a_reload_reads_read_only -- --ignored --nocapture
/// ```
#[test]
#[ignore = "read-only diagnosis against a real store"]
fn why_a_reload_reads_read_only() {
    use std::os::unix::fs::MetadataExt;
    use std::path::Path;
    use thruline_core::adapter::{stable_source_session_id, TranscriptAdapter};
    use thruline_core::codex_ingest::CodexAdapter;
    use thruline_core::mirror::{MirrorSeal, MirrorStore, SealedMirror};
    use thruline_core::model::Source;

    let (Some(db), Some(root)) = (
        env_path("THRULINE_PROBE_RO_DB"),
        std::env::var("THRULINE_PROBE_ROOT").ok(),
    ) else {
        eprintln!("THRULINE_PROBE_RO_DB / THRULINE_PROBE_ROOT not set; nothing measured");
        return;
    };
    let home = env_path("HOME").expect("HOME");
    let mirrors = env_path("THRULINE_PROBE_RO_MIRRORS").unwrap_or_else(|| {
        db.parent()
            .map(|d| d.join("mirrors"))
            .unwrap_or_else(|| home.join(".thruline/mirrors"))
    });
    live_store::refuse_live_store(&mirrors);
    let uri = format!("file:{}?mode=ro", db.display());
    let conn = rusqlite::Connection::open_with_flags(
        uri,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .expect("open read-only");
    conn.busy_timeout(std::time::Duration::from_secs(30))
        .expect("busy timeout");
    let authority: String = conn
        .query_row(
            "SELECT authority_key FROM ledger_authority WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .expect("authority");
    let mirror = MirrorStore::open_for_verification(mirrors.join("ledgers").join(&authority))
        .expect("mirror (read only)");
    let claude = home.join(".claude/projects");
    let codex_roots = thruline_core::ingest::default_codex_roots();
    let plan = hq::load_plan(&root, &claude, &codex_roots);
    let order = hq::load_order(&plan);
    eprintln!(
        "plan: {} claude + {} codex files",
        plan.claude_code.len(),
        plan.codex.len()
    );

    struct Gen {
        id: String,
        sealed: SealedMirror,
        total_envelopes: i64,
    }
    let accepted = |session: &str| -> Option<Gen> {
        conn.query_row(
            "SELECT g.id, g.mirror_payload_path, g.mirror_seal_path, g.payload_sha256,
                    g.payload_bytes, g.seal_sha256, g.total_envelopes
             FROM session_generation_heads h
             JOIN session_generations g ON g.id = h.generation_id
             WHERE h.session_id = ?1",
            [session],
            |r| {
                let id: String = r.get(0)?;
                Ok(Gen {
                    sealed: SealedMirror {
                        generation_key: id.trim_start_matches("generation_").to_string(),
                        relative_payload: r.get(1)?,
                        relative_seal: r.get(2)?,
                        payload_sha256: r.get(3)?,
                        payload_bytes: r.get::<_, i64>(4)? as u64,
                        seal_sha256: r.get(5)?,
                    },
                    id,
                    total_envelopes: r.get(6)?,
                })
            },
        )
        .ok()
    };
    // Which seal fields differ from the file now ("" when none).
    let stat_diff = |file: &Path, seal: &MirrorSeal| -> String {
        let Ok(m) = std::fs::metadata(file) else {
            return "unstatable".into();
        };
        let mtime = m
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos() as u64);
        let mut d = Vec::new();
        if m.len() != seal.payload_bytes {
            d.push(format!("size({}->{})", seal.payload_bytes, m.len()));
        }
        if mtime != seal.source_modified_unix_nanos {
            d.push(format!(
                "mtime({:?}->{:?})",
                seal.source_modified_unix_nanos, mtime
            ));
        }
        if Some(m.dev()) != seal.source_device {
            d.push("dev".into());
        }
        if Some(m.ino()) != seal.source_inode {
            d.push(format!("ino({:?}->{})", seal.source_inode, m.ino()));
        }
        d.join(" ")
    };
    // Every sealed generation in the directory of one session (accepted or
    // not): was one of them sealed from exactly this file as it is now?
    let sibling_matching_stat = |gen: &Gen, file: &Path| -> Option<(String, String)> {
        let seal_path = mirror.resolve_seal(&gen.sealed).ok()?;
        let session_dir = seal_path.parent()?.parent()?;
        for entry in std::fs::read_dir(session_dir).ok()?.flatten() {
            let Ok(bytes) = std::fs::read(entry.path().join("seal.part")) else {
                continue;
            };
            let Ok(seal) = serde_json::from_slice::<MirrorSeal>(&bytes) else {
                continue;
            };
            if stat_diff(file, &seal).is_empty() {
                return Some((
                    entry.file_name().to_string_lossy().into_owned(),
                    seal.payload_sha256,
                ));
            }
        }
        None
    };
    // C27 (b): would one read prove the bytes are the accepted generation's
    // (`LoadMirror::unchanged_how` → `ByContent`)?
    let content_verdict = |file: &Path, gen: &Gen, row_digest: Option<&str>| -> String {
        let t = Instant::now();
        let Ok(read) = thruline_core::ingest::file_digests(file) else {
            return "content=unreadable".into();
        };
        let same_bytes = read.bytes == gen.sealed.payload_bytes
            && read.payload_sha256 == gen.sealed.payload_sha256
            && read.raw_lines as i64 == gen.total_envelopes;
        let row = row_digest.map(|d| d == read.raw_digest);
        format!(
            "content_identical={same_bytes} row_digest_is_these_bytes={row:?} ({:.3} s)",
            t.elapsed().as_secs_f64()
        )
    };
    let seal_dev = |gen: &Gen| -> String {
        mirror
            .read_seal(&gen.sealed)
            .map(|s| format!("seal_dev={:?}", s.source_device))
            .unwrap_or_default()
    };
    let recently_modified = |file: &Path| -> bool {
        std::fs::metadata(file)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age.as_secs() < 600)
    };

    // Codex locators, exactly as LoadMirror::open reads them.
    let mut locators: std::collections::HashMap<String, Option<String>> = Default::default();
    {
        let mut stmt = conn
            .prepare(
                "SELECT source_locator, session_id FROM source_sessions
                 WHERE source = 'codex' AND source_locator IS NOT NULL",
            )
            .expect("locators");
        for row in stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .expect("rows")
        {
            let (locator, session) = row.expect("row");
            locators
                .entry(locator)
                .and_modify(|seen| {
                    if seen.as_deref() != Some(session.as_str()) {
                        *seen = None;
                    }
                })
                .or_insert(Some(session));
        }
    }
    // Codex (device, inode) -> session, as LoadMirror builds it lazily.
    let mut inodes: std::collections::HashMap<(u64, u64), Option<String>> = Default::default();
    {
        let mut stmt = conn
            .prepare(
                "SELECT h.session_id FROM session_generation_heads h
                 JOIN source_sessions s ON s.session_id = h.session_id
                 WHERE s.source = 'codex'",
            )
            .expect("codex heads");
        let sessions: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .expect("rows")
            .flatten()
            .collect();
        for session in sessions {
            let Some(gen) = accepted(&session) else {
                continue;
            };
            let Some(seal) = mirror.read_seal(&gen.sealed) else {
                continue;
            };
            if let (Some(dev), Some(ino)) = (seal.source_device, seal.source_inode) {
                inodes
                    .entry((dev, ino))
                    .and_modify(|seen| {
                        if seen.as_deref() != Some(session.as_str()) {
                            *seen = None;
                        }
                    })
                    .or_insert(Some(session));
            }
        }
    }
    let adapter = CodexAdapter::new(None);
    let mut stems: BTreeMap<String, u64> = BTreeMap::new();
    for f in &plan.claude_code {
        if let Some(s) = f.file_stem() {
            *stems.entry(s.to_string_lossy().into_owned()).or_insert(0) += 1;
        }
    }

    let mut table: BTreeMap<String, (u64, u64, f64)> = BTreeMap::new();
    let mut tsv = String::from("file\tsource\tbytes\tclass\tdetail\tone_pass_s\n");
    for (file, codex) in &order {
        let shown = file.display().to_string();
        let bytes = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
        let live = if recently_modified(file) {
            " [modified <10 min ago]"
        } else {
            ""
        };
        let (class, detail): (String, String) = if !file.exists() {
            ("gone".into(), String::new())
        } else if *codex {
            // The session the full path would route this file to.
            let routed = adapter
                .discover(std::slice::from_ref(file))
                .ok()
                .and_then(|p| p.into_iter().next())
                .and_then(|p| adapter.inspect_capture_route(&p).ok())
                .map(|r| {
                    stable_source_session_id(&r.owner_key, Source::Codex, &r.source_session_key)
                });
            let (found, how) = match locators.get(&shown) {
                Some(Some(s)) => (Some(s.clone()), "locator"),
                Some(None) => (None, "locator_ambiguous"),
                None => {
                    let m = std::fs::metadata(file).expect("stat");
                    match inodes.get(&(m.dev(), m.ino())) {
                        Some(Some(s)) => (Some(s.clone()), "inode"),
                        Some(None) => (None, "inode_ambiguous"),
                        None => (None, "not_found"),
                    }
                }
            };
            match found {
                None => {
                    let routed_gen = routed.as_deref().and_then(accepted);
                    let sib = routed_gen
                        .as_ref()
                        .and_then(|g| sibling_matching_stat(g, file));
                    let head_stat = routed_gen
                        .as_ref()
                        .and_then(|g| mirror.read_seal(&g.sealed))
                        .map(|s| stat_diff(file, &s));
                    (
                        format!(
                            "codex_{how}{}",
                            if routed_gen.is_some() {
                                "_routed_session_has_generation"
                            } else {
                                "_no_generation"
                            }
                        ),
                        format!(
                            "routed={routed:?} head_stat_diff={head_stat:?} sealed_sibling_with_this_stat={sib:?}{live}"
                        ),
                    )
                }
                Some(session) => match accepted(&session) {
                    None => (format!("codex_{how}_no_generation"), live.into()),
                    Some(gen) => match mirror.read_seal(&gen.sealed) {
                        None => (format!("codex_{how}_seal_unreadable"), live.into()),
                        Some(seal) => {
                            let diff = stat_diff(file, &seal);
                            if diff.is_empty() {
                                (format!("codex_{how}_skip"), String::new())
                            } else {
                                let sib = sibling_matching_stat(&gen, file);
                                let refusals: Vec<String> = conn
                                    .prepare(
                                        "SELECT status || ':' || reason FROM candidate_assessments
                                         WHERE source='codex' AND predecessor_generation_id = ?1",
                                    )
                                    .and_then(|mut s| {
                                        s.query_map([&gen.id], |r| r.get::<_, String>(0))?
                                            .collect::<std::result::Result<Vec<_>, _>>()
                                    })
                                    .unwrap_or_default();
                                let routed_same = routed.as_deref() == Some(session.as_str());
                                (
                                    format!(
                                        "codex_{how}_stat_differs{}",
                                        if sib.is_some() {
                                            "_matches_other_sealed_generation"
                                        } else {
                                            ""
                                        }
                                    ),
                                    format!(
                                        "{diff}; {}; {}; routed_same_session={routed_same}; sibling={sib:?}; refusals_after_head={}{live}",
                                        seal_dev(&gen),
                                        content_verdict(file, &gen, None),
                                        refusals.join("|")
                                    ),
                                )
                            }
                        }
                    },
                },
            }
        } else {
            let session_id = file
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let project = file
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let dup = stems.get(&session_id).copied().unwrap_or(0);
            let row: Option<(String, String, i64, Option<String>)> = conn
                .query_row(
                    "SELECT project_id, file_path, raw_line_count, raw_digest FROM sessions WHERE id=?1",
                    [&session_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .ok();
            match row {
                None => ("claude_no_session_row".into(), live.into()),
                Some((row_project, row_file, lines, digest)) => match accepted(&session_id) {
                    None => ("claude_no_generation".into(), live.into()),
                    Some(gen) => {
                        let digest_is_payload =
                            digest.as_deref() == Some(gen.sealed.payload_sha256.as_str());
                        let seal = mirror.read_seal(&gen.sealed);
                        let diff = seal
                            .as_ref()
                            .map(|s| stat_diff(file, s))
                            .unwrap_or_else(|| "seal_unreadable".into());
                        let mut why: Vec<(&str, String)> = Vec::new();
                        if row_project != project {
                            why.push(("project", format!("project_differs(row={row_project})")));
                        }
                        if row_file != shown {
                            why.push(("file", format!("file_differs(row={row_file})")));
                        }
                        if lines.max(0) != gen.total_envelopes {
                            why.push((
                                "lines",
                                format!(
                                    "lines_differ(row={lines},generation={})",
                                    gen.total_envelopes
                                ),
                            ));
                        }
                        if digest.is_none() {
                            why.push(("digest_null", "digest_null".into()));
                        } else if digest_is_payload {
                            why.push(("digest_is_payload", "digest_is_payload".into()));
                        }
                        if !diff.is_empty() {
                            let sib = sibling_matching_stat(&gen, file);
                            why.push((
                                if sib.is_some() {
                                    "stat_matches_other_sealed_generation"
                                } else {
                                    "stat"
                                },
                                format!(
                                    "stat_differs {diff} {} sibling={sib:?} {}",
                                    seal_dev(&gen),
                                    content_verdict(file, &gen, digest.as_deref())
                                ),
                            ));
                        }
                        let class = if why.is_empty() {
                            "claude_skip".to_string()
                        } else {
                            let keys: Vec<&str> = why.iter().map(|w| w.0).collect();
                            format!("claude_{}", keys.join("+"))
                        };
                        let text: Vec<String> = why.into_iter().map(|w| w.1).collect();
                        (
                            class,
                            format!("{}; same_stem_in_plan={dup}{live}", text.join("; ")),
                        )
                    }
                },
            }
        };
        let skip = class.ends_with("_skip") || class == "gone";
        let secs = if skip {
            0.0
        } else {
            let t = Instant::now();
            let _ = thruline_core::ingest::whole_file_digest(file);
            t.elapsed().as_secs_f64()
        };
        let e = table.entry(class.clone()).or_insert((0, 0, 0.0));
        e.0 += 1;
        e.1 += bytes;
        e.2 += secs;
        if !skip {
            eprintln!(
                "{:<70} {:>11} B  {class}  {detail}",
                file.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
                bytes
            );
        }
        tsv.push_str(&format!(
            "{shown}\t{}\t{bytes}\t{class}\t{detail}\t{secs:.4}\n",
            if *codex { "codex" } else { "claude_code" }
        ));
    }
    eprintln!("\nclass -> files, bytes, one sha256 pass (s)");
    for (class, (n, b, s)) in &table {
        eprintln!("  {class:<70} {n:>4} {b:>12} {s:>8.3}");
    }
    if let Some(out) = env_path("THRULINE_PROBE_TSV") {
        std::fs::write(&out, tsv).expect("write tsv");
    }
}

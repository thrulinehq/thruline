//! HQ timing probe against a real store. Ignored by default: it needs a store
//! named by `HQ_PERF_DB` and measures, it does not assert.
//!
//! `HQ_PERF_DB=<ledger.db> cargo test --release -p thruline-core --test hq_perf_probe -- --ignored --nocapture`

use std::time::Instant;

use thruline_core::{hq, Ledger};

#[path = "support/live_store.rs"]
mod live_store;

#[test]
#[ignore = "measurement against a store named by HQ_PERF_DB"]
fn time_every_hq_read() {
    let Some(db) = std::env::var_os("HQ_PERF_DB") else {
        eprintln!("HQ_PERF_DB not set; nothing measured");
        return;
    };
    live_store::refuse_live_store(std::path::Path::new(&db));
    let ledger = Ledger::open(&db).expect("open the store");
    let t = Instant::now();
    let projects = hq::hq_projects(&ledger).expect("projects");
    eprintln!(
        "hq_projects {:?} ({} projects)",
        t.elapsed(),
        projects.len()
    );
    let now = hq::parse_ms("2026-10-07T12:00:00Z").unwrap_or(0);
    for p in &projects {
        let t = Instant::now();
        let index = hq::index(&ledger, p).expect("index");
        let built = t.elapsed();
        let t = Instant::now();
        let view = index.view(now);
        eprintln!(
            "{} index {:?} view {:?} threads {}",
            p.name,
            built,
            t.elapsed(),
            view.threads.len()
        );
        for th in view.threads.iter().take(5) {
            let t = Instant::now();
            let tv = index
                .thread_view(&ledger, &th.id, now)
                .expect("thread")
                .expect("present");
            let bytes = serde_json::to_vec(&tv).map(|v| v.len()).unwrap_or(0);
            eprintln!("  thread {} {:?} {} bytes", th.id, t.elapsed(), bytes);
        }
    }
}

#[test]
#[ignore = "measurement"]
fn raw_query_cost() {
    let Some(db) = std::env::var_os("HQ_PERF_DB") else {
        return;
    };
    let Some(sql) = std::env::var_os("HQ_PERF_SQL") else {
        return;
    };
    live_store::refuse_live_store(std::path::Path::new(&db));
    let sql = std::fs::read_to_string(sql).expect("sql");
    let ledger = Ledger::open(&db).expect("ledger");
    let conn = ledger.conn();
    let v: String = conn
        .query_row("select sqlite_version()", [], |r| r.get(0))
        .expect("v");
    eprintln!("raw sqlite {v}");
    let mut plan = conn
        .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
        .expect("plan");
    let lines: Vec<String> = plan
        .query_map([], |r| r.get::<_, String>(3))
        .expect("plan rows")
        .filter_map(|r| r.ok())
        .collect();
    eprintln!("raw plan {lines:?}");
    for _ in 0..1 {
        let t = Instant::now();
        let mut stmt = conn.prepare(&sql).expect("prep");
        let n = stmt.column_count();
        let mut rows = stmt.query([]).expect("q");
        let mut count = 0usize;
        while let Some(r) = rows.next().expect("row") {
            for i in 0..n {
                let _ = r.get_ref(i);
            }
            count += 1;
        }
        eprintln!("raw {count} rows {:?}", t.elapsed());
    }
}

/// For every folder under `HQ_CLAUDE_PROJECTS` (read only): the root decoded
/// from its name next to the `cwd` its recordings state.
#[test]
#[ignore = "measurement against a folder named by HQ_CLAUDE_PROJECTS"]
fn decode_every_folder_name() {
    let Some(dir) = std::env::var_os("HQ_CLAUDE_PROJECTS") else {
        return;
    };
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .expect("dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    entries.sort();
    let (mut same, mut differ, mut no_cwd) = (0, 0, 0);
    let cwds: Vec<(String, Option<String>)> = entries
        .iter()
        .map(|d| {
            let name = d
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let cwd = std::fs::read_dir(d)
                .ok()
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
                .find_map(|f| hq::recorded_cwd(&f));
            (name, cwd)
        })
        .collect();
    for (i, (name, cwd)) in cwds.iter().enumerate() {
        // Decode as if this folder's own cwd were unknown.
        let known: Vec<String> = cwds
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .filter_map(|(_, (_, c))| c.clone())
            .collect();
        let (decoded, basis) = hq::decode_folder_name(name, &known);
        match cwd {
            Some(c) if *c == decoded => same += 1,
            Some(c) => {
                differ += 1;
                eprintln!("DIFF {basis} decoded={decoded} cwd={c}");
            }
            None => {
                no_cwd += 1;
                eprintln!("NOCWD {basis} decoded={decoded}");
            }
        }
    }
    eprintln!("same {same} differ {differ} no_cwd {no_cwd}");
}

/// Titles under the current rule for threads saved as JSON by
/// `HQ_TITLES_JSON` (`[{id, before, prompts: [{text, work}]}]`, read only).
#[test]
#[ignore = "measurement against a file named by HQ_TITLES_JSON"]
fn retitle_saved_threads() {
    let Some(path) = std::env::var_os("HQ_TITLES_JSON") else {
        return;
    };
    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("read")).expect("json");
    for t in saved.as_array().into_iter().flatten() {
        let prompts: Vec<(&str, bool)> = t["prompts"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|p| (p["text"].as_str().unwrap_or(""), p["work"] == true))
            .collect();
        let first = prompts.iter().position(|p| p.1).unwrap_or(0);
        let mut order: Vec<&str> = prompts.get(first).map(|p| vec![p.0]).unwrap_or_default();
        order.extend(
            prompts
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != first)
                .map(|(_, p)| p.0),
        );
        let (title, basis, _) = hq::prompts::thread_title(&order);
        let after = title
            .map(|s| hq::prompts::cut_title(&s).0)
            .unwrap_or_default();
        eprintln!(
            "RETITLE {} | {} | {:?} | {}",
            t["id"].as_str().unwrap_or(""),
            t["before"].as_str().unwrap_or(""),
            basis,
            after
        );
    }
}

/// C27 (a): the index of one project built each way [`hq::RowRead`] offers,
/// on a store opened READ-ONLY (`mode=ro`, never `immutable`), with the
/// project view's bytes compared across ways. Measures; asserts only that the
/// ways agree.
///
/// `HQ_PERF_DB=<ledger.db> HQ_PERF_MEMBERS=<ledger project id>,… HQ_PERF_ROOT=<root>
///  HQ_PERF_WAYS=one,par8r,par8s,… cargo test --release -p thruline-core
///  --test hq_perf_probe index_read_ways -- --ignored --nocapture`
///
/// A way is `one` (one statement) or `par<N><r|s>`: N readers, rows read in
/// rowid (`r`) or session (`s`) order.
#[test]
#[ignore = "measurement against a store named by HQ_PERF_DB"]
fn index_read_ways() {
    use rusqlite::{Connection, OpenFlags};
    let (Some(db), Some(members), Some(root)) = (
        std::env::var_os("HQ_PERF_DB"),
        std::env::var("HQ_PERF_MEMBERS").ok(),
        std::env::var("HQ_PERF_ROOT").ok(),
    ) else {
        eprintln!("HQ_PERF_DB / HQ_PERF_MEMBERS / HQ_PERF_ROOT not set; nothing measured");
        return;
    };
    live_store::refuse_live_store(std::path::Path::new(&db));
    let ways = std::env::var("HQ_PERF_WAYS").unwrap_or_else(|_| "one,par8r".to_string());
    let uri = format!("file:{}?mode=ro", db.to_string_lossy());
    let conn = Connection::open_with_flags(
        &uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .expect("open read-only");
    let project = hq::HqProject {
        id: hq::project_id_for_root(&root),
        name: root.rsplit('/').next().unwrap_or("").to_string(),
        root: root.clone(),
        members: members
            .split(',')
            .map(|id| {
                let source: String = conn
                    .query_row("SELECT source FROM projects WHERE id = ?1", [id], |r| {
                        r.get(0)
                    })
                    .expect("member");
                hq::Member {
                    ledger_project_id: id.to_string(),
                    source,
                    root: root.clone(),
                    root_basis: "recorded_cwd",
                }
            })
            .collect(),
    };
    let now = hq::parse_ms("2026-10-07T12:00:00Z").unwrap_or(0);
    let load = || {
        std::process::Command::new("sysctl")
            .args(["-n", "vm.loadavg"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    };
    let mut first: Option<(String, Vec<u8>)> = None;
    for way in ways.split(',') {
        let read = if way == "one" {
            hq::RowRead::OneStatement
        } else {
            let body = way.trim_start_matches("par");
            let (n, order) = body.split_at(body.len() - 1);
            hq::RowRead::Parallel {
                workers: n.parse().expect("readers"),
                rowid_order: order == "r",
                runs_per_reader: hq::RUNS_PER_READER,
            }
        };
        let before = load();
        let t = Instant::now();
        let index = hq::index_with(&conn, &project, read).expect("index");
        let built = t.elapsed();
        let bytes = serde_json::to_vec(&index.view(now)).expect("json");
        eprintln!(
            "WAY {way:>7} index {:>8.3}s view {} bytes threads {} load {before}",
            built.as_secs_f64(),
            bytes.len(),
            index.view(now).threads.len()
        );
        match &first {
            None => first = Some((way.to_string(), bytes)),
            Some((w, b)) => assert!(*b == bytes, "{way} differs from {w}"),
        }
    }
}

//! The world's digest: everything the C34 card promises not to move, reduced to
//! hashes a committed fixture can hold.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::World;

/// THE NORMALISATION LIST. A column named here holds a clock reading or a random
/// id and so differs between two runs of the very same sequence; it is replaced
/// by `"<volatile>"` before anything is hashed. Nothing else is touched but the
/// temp directory's own path (`<TMP>`), which is where the world lives and is
/// different on every run. The list is written into the digest itself, so the
/// committed fixture says what was left out of the comparison.
///
/// Every entry was FOUND, not assumed: the sequence was run twice on the base
/// code and every column that differed was added here, and
/// `the_digest_is_stable_across_two_runs_of_the_same_sequence` fails if a
/// further one ever appears.
pub const NORMALISED: &[(&str, &str)] = &[
    // random ULIDs minted at ingest / delivery
    ("sensor_events", "id"),
    ("events", "id"),
    ("event_generation_provenance", "event_id"),
    ("nodes", "anchor_event_id"),
    // clock readings
    ("sensor_events", "received_at"),
    ("node_projection_heads", "built_at"),
    ("project_heads", "computed_at"),
    ("projects", "created_at"),
    ("session_generations", "accepted_at"),
    ("schema_migrations", "applied_at"),
    ("speaker_findings", "recorded_at"),
    // a hash over a seal file whose body holds a clock reading, a device and an inode
    ("session_generations", "seal_sha256"),
];

/// Keys inside a mirror `seal.part` (JSON) that hold a clock reading or this
/// machine's device and inode numbers. Every other key of the seal, the owner,
/// the keys and the payload's own sha256 and length, is compared.
pub const NORMALISED_SEAL_KEYS: &[&str] = &[
    "captured_at_unix_millis",
    "source_modified_unix_nanos",
    "source_device",
    "source_inode",
];

/// The mirror's coordination database is a SQLite file whose bytes hold the
/// ledger's random id and clock readings. It is listed by name; its content is
/// not compared.
pub const MIRROR_NAME_ONLY: &str = "coordination.sqlite";

/// The temp dir's path in the two spellings macOS gives it.
fn tmp_spellings(w: &World) -> Vec<String> {
    let raw = w.root().to_string_lossy().into_owned();
    let mut out = vec![raw.clone()];
    if let Ok(real) = std::fs::canonicalize(w.root()) {
        let real = real.to_string_lossy().into_owned();
        if real != raw {
            out.push(real);
        }
    }
    // Longest first, so `/private/var/…` is not half-replaced by `/var/…`.
    out.sort_by_key(|s| std::cmp::Reverse(s.len()));
    out
}

fn norm_text(s: &str, tmp: &[String]) -> String {
    let mut out = s.to_string();
    for t in tmp {
        out = out.replace(t.as_str(), "<TMP>");
    }
    ledger_id_hidden(&out)
}

/// `ledger_` followed by a 26-character ULID is the mirror's per-ledger random
/// directory name; it is replaced by `ledger_<LEDGER>`.
fn ledger_id_hidden(s: &str) -> String {
    const PREFIX: &str = "ledger_";
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find(PREFIX) {
        let (head, tail) = rest.split_at(at + PREFIX.len());
        out.push_str(head);
        let id: String = tail
            .chars()
            .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            .collect();
        if id.len() == 26 {
            out.push_str("<LEDGER>");
            rest = &tail[id.len()..];
        } else {
            rest = tail;
        }
    }
    out.push_str(rest);
    out
}

fn value_json(v: rusqlite::types::ValueRef<'_>, tmp: &[String]) -> Value {
    use rusqlite::types::ValueRef;
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => json!(i),
        ValueRef::Real(f) => json!(f),
        ValueRef::Text(t) => json!(norm_text(&String::from_utf8_lossy(t), tmp)),
        ValueRef::Blob(b) => json!(format!("blob:{}", thruline_core::chain::sha256_hex(b))),
    }
}

type TableRows = (Value, Vec<Vec<Value>>, Vec<String>);

fn table_digest(conn: &rusqlite::Connection, table: &str, tmp: &[String]) -> TableRows {
    let mut cols_stmt = conn
        .prepare(&format!("PRAGMA table_info(\"{table}\")"))
        .expect("table_info");
    let cols: Vec<String> = cols_stmt
        .query_map([], |r| r.get::<_, String>(1))
        .expect("cols")
        .filter_map(|c| c.ok())
        .collect();
    let mut stmt = conn
        .prepare(&format!("SELECT * FROM \"{table}\" ORDER BY rowid"))
        .or_else(|_| conn.prepare(&format!("SELECT * FROM \"{table}\"")))
        .expect("select");
    let mut rows_out: Vec<Vec<Value>> = Vec::new();
    let mut rows = stmt.query([]).expect("query");
    while let Some(row) = rows.next().expect("row") {
        let mut one = Vec::new();
        for (i, name) in cols.iter().enumerate() {
            if NORMALISED.contains(&(table, name.as_str())) {
                one.push(json!("<volatile>"));
            } else {
                one.push(value_json(row.get_ref(i).expect("cell"), tmp));
            }
        }
        rows_out.push(one);
    }
    let joined = rows_out
        .iter()
        .map(|r| serde_json::to_string(r).expect("json"))
        .collect::<Vec<_>>()
        .join("\n");
    let summary = json!({
        "rows": rows_out.len(),
        "sha256": thruline_core::chain::sha256_hex(joined.as_bytes()),
    });
    (summary, rows_out, cols)
}

/// Where two digests differ, as `path: left / right`, at most ten places.
pub fn diff(a: &Value, b: &Value) -> Vec<String> {
    fn go(a: &Value, b: &Value, path: &str, out: &mut Vec<String>) {
        if out.len() >= 10 || a == b {
            return;
        }
        match (a, b) {
            (Value::Object(x), Value::Object(y)) => {
                let keys: std::collections::BTreeSet<&String> = x.keys().chain(y.keys()).collect();
                for k in keys {
                    go(
                        x.get(k).unwrap_or(&Value::Null),
                        y.get(k).unwrap_or(&Value::Null),
                        &format!("{path}/{k}"),
                        out,
                    );
                }
            }
            (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
                for (i, (p, q)) in x.iter().zip(y.iter()).enumerate() {
                    go(p, q, &format!("{path}[{i}]"), out);
                }
            }
            (Value::Array(x), Value::Array(y)) => {
                // Lists of `[name, …]` rows of different lengths: say which
                // names are on one side only.
                let names = |v: &Vec<Value>| -> std::collections::BTreeSet<String> {
                    v.iter()
                        .map(|r| {
                            r.get(0)
                                .and_then(Value::as_str)
                                .map(str::to_string)
                                .unwrap_or_else(|| r.to_string())
                        })
                        .collect()
                };
                let (nx, ny) = (names(x), names(y));
                for n in nx.difference(&ny).take(4) {
                    out.push(format!("{path}: only in the first: {n}"));
                }
                for n in ny.difference(&nx).take(4) {
                    out.push(format!("{path}: only in the second: {n}"));
                }
                out.push(format!("{path}: {} entries / {}", x.len(), y.len()));
            }
            _ => {
                let short = |v: &Value| v.to_string().chars().take(160).collect::<String>();
                out.push(format!("{path}: {} / {}", short(a), short(b)));
            }
        }
    }
    let mut out = Vec::new();
    go(a, b, "", &mut out);
    out
}

/// A world read out in full, normalised, so two of them can be compared column
/// by column after the worlds themselves are gone.
pub struct Captured {
    pub summary: Value,
    tables: std::collections::BTreeMap<String, (Vec<String>, Vec<Vec<Value>>)>,
}

/// Every `table.column` whose values differ between two worlds that ran the
/// same sequence (after the normalisation above): the columns a person would
/// have to add to [`NORMALISED`], found rather than guessed.
pub fn differing_columns(a: &Captured, b: &Captured) -> Vec<String> {
    let mut out = Vec::new();
    for (table, (cols, ra)) in &a.tables {
        let Some((_, rb)) = b.tables.get(table) else {
            out.push(format!("{table}: absent from the second world"));
            continue;
        };
        if ra.len() != rb.len() {
            out.push(format!("{table}: {} rows against {}", ra.len(), rb.len()));
            continue;
        }
        for (i, name) in cols.iter().enumerate() {
            if let Some((x, y)) = ra.iter().zip(rb.iter()).find(|(x, y)| x[i] != y[i]) {
                let short = |v: &Value| v.to_string().chars().take(120).collect::<String>();
                out.push(format!(
                    "{table}.{name} (e.g. {} / {})",
                    short(&x[i]),
                    short(&y[i])
                ));
            }
        }
    }
    out
}

fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, Value, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for p in paths {
        if p.is_dir() {
            walk(&p, base, out);
            continue;
        }
        let Ok(bytes) = std::fs::read(&p) else {
            continue;
        };
        let rel = p
            .strip_prefix(base)
            .unwrap_or(&p)
            .to_string_lossy()
            .into_owned();
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name == MIRROR_NAME_ONLY {
            out.push((rel, json!("<volatile>"), "<volatile>".to_string()));
        } else if name == "seal.part" {
            let mut seal: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
            if let Some(map) = seal.as_object_mut() {
                for key in NORMALISED_SEAL_KEYS {
                    if map.contains_key(*key) {
                        map.insert((*key).to_string(), json!("<volatile>"));
                    }
                }
            }
            let canon = serde_json::to_string(&seal).unwrap_or_default();
            out.push((
                rel,
                json!(canon.len()),
                thruline_core::chain::sha256_hex(canon.as_bytes()),
            ));
        } else {
            out.push((
                rel,
                json!(bytes.len()),
                thruline_core::chain::sha256_hex(&bytes),
            ));
        }
    }
}

/// Everything the card promises not to move, digested: every table row
/// (normalised as above), the event chain's head per session, the sensor
/// chain, every mirror file's name, size and sha256, and the names of the files
/// that sit beside the ledger.
pub fn digest(w: &World) -> Captured {
    let tmp = tmp_spellings(w);
    let conn =
        rusqlite::Connection::open_with_flags(w.db(), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("read-only open");
    let mut names_stmt = conn
        .prepare(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .expect("tables");
    let tables: Vec<String> = names_stmt
        .query_map([], |r| r.get::<_, String>(0))
        .expect("names")
        .filter_map(|n| n.ok())
        .collect();

    let mut per_table = serde_json::Map::new();
    let mut all_rows = std::collections::BTreeMap::new();
    let mut sensor_chain: Vec<Value> = Vec::new();
    let mut session_heads: Vec<Value> = Vec::new();
    for table in &tables {
        let (summary, rows, cols) = table_digest(&conn, table, &tmp);
        let at = |name: &str| cols.iter().position(|c| c == name);
        if table == "sensor_events" {
            if let (Some(seq), Some(hash)) = (at("seq"), at("chain_hash")) {
                sensor_chain = rows.iter().map(|r| json!([r[seq], r[hash]])).collect();
            }
        }
        if table == "events" {
            if let (Some(sid), Some(seq), Some(hash)) =
                (at("session_id"), at("seq"), at("chain_hash"))
            {
                let mut heads: std::collections::BTreeMap<String, (i64, String)> =
                    std::collections::BTreeMap::new();
                for r in &rows {
                    let s = r[sid].as_str().unwrap_or_default().to_string();
                    let n = r[seq].as_i64().unwrap_or(-1);
                    let h = r[hash].as_str().unwrap_or_default().to_string();
                    let entry = heads.entry(s).or_insert((-1, String::new()));
                    if n >= entry.0 {
                        *entry = (n, h);
                    }
                }
                session_heads = heads
                    .into_iter()
                    .map(|(s, (n, h))| json!({"session": s, "last_seq": n, "chain_hash": h}))
                    .collect();
            }
        }
        per_table.insert(table.clone(), summary);
        all_rows.insert(table.clone(), (cols, rows));
    }

    let mut mirror_files = Vec::new();
    walk(&w.mirror(), &w.mirror(), &mut mirror_files);
    let mirror_listing: Vec<Value> = mirror_files
        .iter()
        .map(|(rel, len, sha)| json!([norm_text(rel, &tmp), len, sha]))
        .collect();
    let mirror_sha = thruline_core::chain::sha256_hex(
        serde_json::to_string(&mirror_listing)
            .expect("json")
            .as_bytes(),
    );

    let mut beside: Vec<String> = std::fs::read_dir(w.home().join(".thruline"))
        .map(|d| {
            d.filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
                .filter(|n| !n.starts_with("ledger.db"))
                .collect()
        })
        .unwrap_or_default();
    beside.sort();

    let summary = json!({
        "normalised": NORMALISED.iter().map(|(t, c)| format!("{t}.{c}")).collect::<Vec<_>>(),
        "normalised_also": ["the temp dir's own path, in both spellings, is replaced by <TMP> in every text value and mirror path"],
        "tables": Value::Object(per_table),
        "event_chain_heads": session_heads,
        "sensor_chain": sensor_chain,
        "mirror": {"files": mirror_files.len(), "sha256": mirror_sha, "listing": mirror_listing},
        "beside_the_ledger": beside,
    });
    Captured {
        summary,
        tables: all_rows,
    }
}

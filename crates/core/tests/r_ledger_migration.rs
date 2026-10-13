//! C30 R-LEDGER (C34 rulings Q1, Q8, Q9, Q11-Q14, Q16, Q17) — what a person
//! did, as record: six append-only tables arrive as a migration and cost
//! nothing until `thruline upgrade` is run deliberately (A168/A193).
//!
//! **This task changes DDL; `thruline upgrade` is required; every other verb
//! exits 3 until it runs.** Asked against the schema at `74b06079` (the head of
//! `hq-next/ui` after the merge of `decisions/engine`), whose DDL is
//! `tests/fixtures/schema-c30-r-ledger-base.sql`, byte for byte.
//!
//! Beside the compatibility questions sit the ones the design (§4.3) asks of
//! the tables themselves: no row in any of them moves a verdict, a coverage
//! count, readiness or which approval stands; an objection has no column a
//! result could be written into; and the derivation, the rebuild, the speaker
//! and the MCP tools name none of the six.

use rusqlite::{params, Connection};
use thruline_core::capture_hold::{self, Act as HoldAct, Hold, Recorded, Refusal};
use thruline_core::change_decision::{self, Decide};
use thruline_core::change_decision_second::{self, Seconding};
use thruline_core::error::CoreError;
use thruline_core::hq;
use thruline_core::ledger::Ledger;
use thruline_core::upgrade::UpgradeState;

/// The DDL digest at `74b06079`. Pinned so the bump is a fact, not a belief.
const SCHEMA_BEFORE: &str =
    "schema_sha256_c4a93038ac02f0152719db97c72c73d394677c7fe6596d3e3739fe54c0b2ab22";

/// The commit the fixture is the DDL of, in full.
const BUILT_ON: &str = "74b060794d5949994e6d1587590553408523f6e3";

const THIS_TASK: &str = "C30-R-LEDGER";

/// The six tables, in the order the DDL creates them.
const TABLES: [&str; 6] = [
    "change_notes",
    "statement_objections",
    "change_decision_seconds",
    "change_agent_messages",
    "capture_holds",
    "inbox_hides",
];

const INDEXES: [&str; 6] = [
    "idx_change_notes_change",
    "idx_statement_objections_event",
    "idx_change_decision_seconds_decision",
    "idx_change_agent_messages_thread",
    "idx_capture_holds_folder",
    "idx_inbox_hides_thread",
];

mod migration_registry;

fn fixture_schema() -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/schema-c30-r-ledger-base.sql"),
    )
    .expect("the pre-R-LEDGER schema fixture must be readable")
}

/// The 30 objects: six tables, six indexes, eighteen triggers.
fn the_thirty() -> Vec<String> {
    let mut out: Vec<String> = TABLES.iter().map(|t| t.to_string()).collect();
    out.extend(INDEXES.iter().map(|i| i.to_string()));
    for table in TABLES {
        for suffix in ["no_replace", "no_update", "no_delete"] {
            out.push(format!("{table}_{suffix}"));
        }
    }
    out.sort();
    out
}

fn this_entry() -> migration_registry::Entry {
    migration_registry::entries()
        .into_iter()
        .find(|e| e.task == THIS_TASK)
        .expect("the registry must name this task")
}

/* ── 1. the fixture is the base commit's DDL ──────────────────────────── */

#[test]
fn the_fixture_is_really_the_schema_at_the_base_commit() {
    let digest = format!(
        "schema_sha256_{}",
        thruline_core::chain::sha256_hex(fixture_schema().as_bytes())
    );
    assert_eq!(digest, SCHEMA_BEFORE);
    let me = this_entry();
    assert_eq!(
        me.pinned_schema_digest, SCHEMA_BEFORE,
        "the registry and this test must pin the same pre-task schema"
    );
    assert_eq!(
        me.built_on_commit, BUILT_ON,
        "the registry must name the commit after the Decisions merge (Q8)"
    );
    assert_eq!(
        me.order, 160,
        "order 160, after the Decisions lane's 150/151 (Q8)"
    );
    assert_eq!(me.migration_test, "crates/core/tests/r_ledger_migration.rs");
}

/// The strongest form of "no ALTER, no rename, no backfill": this tree's DDL is
/// the base commit's DDL byte for byte, followed by its own appended block
/// that creates the thirty objects and nothing else. Later tasks may append
/// after that block, but cannot change it.
#[test]
fn the_added_ddl_is_a_pure_append_that_creates_exactly_the_thirty() {
    let base = fixture_schema();
    let head = Ledger::schema_ddl();
    assert!(
        head.starts_with(&base),
        "this tree's DDL must begin with the base commit's DDL, unaltered"
    );
    let next_task = head
        .find("-- C47 / Remove records per project (migration registry order 170).")
        .expect("C47 must append after the C30 block");
    let appended = &head[base.len()..next_task];
    let code: Vec<&str> = appended
        .lines()
        .map(str::trim_start)
        .filter(|l| !l.is_empty() && !l.starts_with("--"))
        .collect();
    let mut created: Vec<String> = Vec::new();
    for line in &code {
        for prefix in [
            "CREATE TABLE IF NOT EXISTS ",
            "CREATE INDEX IF NOT EXISTS ",
            "CREATE TRIGGER IF NOT EXISTS ",
        ] {
            if let Some(rest) = line.strip_prefix(prefix) {
                created.push(
                    rest.split(['(', ' '])
                        .next()
                        .expect("a name follows the prefix")
                        .to_string(),
                );
            }
        }
    }
    created.sort();
    assert_eq!(created, the_thirty());
    // No ALTER, no DROP, no backfill: the appended block holds no statement
    // that writes, renames or removes anything that was already there.
    for word in [
        "ALTER ",
        "DROP ",
        "INSERT INTO",
        "DELETE FROM",
        " SET ",
        "RENAME",
    ] {
        assert!(
            !code.iter().any(|l| l.contains(word)),
            "the appended DDL contains {word:?}"
        );
    }
}

/* ── 2. the fingerprint moved ─────────────────────────────────────────── */

#[test]
fn the_schema_version_moved_when_the_r_ledger_tables_landed() {
    assert_ne!(Ledger::schema_fingerprint(), SCHEMA_BEFORE);
}

/* ── 3. an old store: refused, then upgraded with every row kept ──────── */

const CARRIED: [&str; 5] = [
    "projects",
    "changes",
    "change_decisions",
    "run_permissions",
    "events",
];

const OLD_PROJECT: &str = "p-old";
const OLD_CHANGE: &str = "chg-old";
const OLD_APPROVAL: &str = "cd_old";
const OLD_VERSION: &str = "snap_old";

/// A store exactly as the base commit would leave it: its whole DDL, its
/// fingerprint, one project, one session, one event (for the foreign keys),
/// one Change with a standing approval, and one run-permission grant on its
/// chain.
fn base_commit_store(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("ledger.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(&fixture_schema()).unwrap();
    conn.execute_batch(
        "INSERT INTO projects(id,root_path,source,display_name,created_at)
           VALUES('p-old','/old/project','claude_code','Old','2026-01-01T00:00:00Z');
         INSERT INTO sessions(id,project_id,file_path,started_at,last_seq,raw_line_count,event_count)
           VALUES('s-old','p-old','/old/project/s-old.jsonl','2026-01-01T00:00:00Z',0,1,1);
         INSERT INTO events(id,project_id,session_id,seq,ts,kind,role,text,raw_file,raw_line,
                            raw_sha256,prev_hash,chain_hash)
           VALUES('e-old','p-old','s-old',0,'2026-01-01T00:00:00Z','user_msg','user',
                  'Make the totals add up.','/old/project/s-old.jsonl',1,
                  'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                  'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
                  'cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc');
         -- The source-identity mapping every earlier upgrade backfilled, so the
         -- store is what the base commit leaves rather than one the schema
         -- path would still have to fill in.
         INSERT INTO source_project_identities(project_id,owner_key,source,source_project_key,
                                               normalized_root_hint)
           VALUES('p-old','legacy-local','claude_code','p-old','/old/project');
         INSERT INTO source_sessions(session_id,project_id,owner_key,source,source_session_key,
                                     source_locator)
           VALUES('s-old','p-old','legacy-local','claude_code','s-old','/old/project/s-old.jsonl');
         INSERT INTO changes(id,project_id,opened_at,title,purpose,requester,request_source)
           VALUES('chg-old','p-old','2026-01-01T00:00:00Z','Old','Kept','sam','person');
         INSERT INTO change_decisions(id,change_id,act,target_revision,scope_version,actor,
                                      decided_at,withdraws,record_path)
           VALUES('cd_old','chg-old','approved','snap_old',1,'sam',
                  '2026-01-01T00:05:00Z',NULL,NULL);",
    )
    .unwrap();
    // The grant is a real link on the `run_permission` chain, made by the same
    // construction the base commit's code used, so `verify_chain` can say the
    // upgrade left the chain adding up.
    let consent = thruline_core::run_permission::Consent {
        project_id: OLD_PROJECT,
        project_folder: "/old/project",
        actor: "sam",
        recorded_at: "2026-01-01T00:06:00Z",
    };
    let id = "rpm_old";
    let content = thruline_core::run_permission::content(
        id,
        thruline_core::run_permission::Act::Granted,
        &consent,
        None,
    )
    .unwrap();
    let sha = thruline_core::chain::sha256_hex(content.as_bytes());
    let prev = thruline_core::chain::genesis_prev_hash(thruline_core::run_permission::CHAIN_SCOPE);
    let link = thruline_core::chain::chain_hash(
        &prev,
        &sha,
        thruline_core::run_permission::CHAIN_SCOPE,
        0,
    );
    conn.execute(
        "INSERT INTO run_permissions(id,seq,act,project_id,project_folder,actor,recorded_at,
                                     revokes,content_sha256,prev_hash,chain_hash)
         VALUES(?1,0,'granted',?2,'/old/project','sam','2026-01-01T00:06:00Z',NULL,?3,?4,?5)",
        params![id, OLD_PROJECT, sha, prev, link],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO schema_migrations(version) VALUES(?1)",
        params![SCHEMA_BEFORE],
    )
    .unwrap();
    path
}

fn dump(conn: &Connection) -> Vec<Vec<String>> {
    CARRIED
        .iter()
        .map(|t| {
            let mut s = conn
                .prepare(&format!("SELECT * FROM {t} ORDER BY rowid"))
                .unwrap();
            let n = s.column_count();
            s.query_map([], |row| {
                let mut out = Vec::new();
                for i in 0..n {
                    let v: rusqlite::types::Value = row.get(i)?;
                    out.push(format!("{v:?}"));
                }
                Ok(out.join("|"))
            })
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
        })
        .collect()
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn decide(actor: &str, seen: &str) -> Decide {
    Decide {
        actor: actor.to_string(),
        at: "2026-01-02T00:00:00Z".to_string(),
        seen_revision: seen.to_string(),
        note: None,
    }
}

#[test]
fn an_old_store_is_refused_before_the_upgrade_and_keeps_every_row_after_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = base_commit_store(dir.path());
    let before = {
        let conn = Connection::open(&path).unwrap();
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM change_decisions"), 1);
        dump(&conn)
    };
    // The census `thruline upgrade` takes (Q14) sees the same before and after.
    let census_before = thruline_core::upgrade::row_census(&path).unwrap();

    // A193: refused, and nothing paid.
    assert!(Ledger::upgrade_state(&path).unwrap().is_pending());
    match Ledger::open(&path) {
        Err(CoreError::UpgradePending { estimate, .. }) => assert!(!estimate.is_empty()),
        Err(other) => panic!("expected UpgradePending, got {other:?}"),
        Ok(_) => panic!("open must refuse a pending migration"),
    }
    let conn = Connection::open(&path).unwrap();
    for table in TABLES {
        assert_eq!(
            count(
                &conn,
                &format!("SELECT COUNT(*) FROM sqlite_master WHERE name='{table}'")
            ),
            0,
            "the refusal paid nothing: {table} exists before the upgrade"
        );
    }
    drop(conn);

    drop(Ledger::open_upgrading(&path, &mut |_, _, _| {}).unwrap());
    assert_eq!(Ledger::upgrade_state(&path).unwrap(), UpgradeState::Current);

    let conn = Connection::open(&path).unwrap();
    for table in TABLES {
        assert_eq!(
            count(&conn, &format!("SELECT COUNT(*) FROM {table}")),
            0,
            "a migration must invent no note, second, message, objection, hide or hold: {table}"
        );
    }
    assert_eq!(dump(&conn), before, "every earlier row is preserved");
    drop(conn);

    // Q14: rows before = rows after for every table the store had, and the
    // six new ones are empty. (The refreshes also create a few derived tables
    // an old fixture store never had; those are empty too, and named.)
    let census = thruline_core::upgrade::compare_census(
        &census_before,
        &thruline_core::upgrade::row_census(&path).unwrap(),
    );
    assert!(census.unchanged(), "{census:?}");
    for table in TABLES {
        assert!(
            census
                .new_tables
                .iter()
                .any(|(name, rows)| name == table && *rows == 0),
            "{table} is a new, empty table: {census:?}"
        );
    }
    assert!(
        census.new_tables_holding_rows().is_empty(),
        "a migration must invent no row anywhere: {census:?}"
    );

    // The old approval serves exactly what it served before: the same fields,
    // and no `note` or `seconds_standing` key (Q11: neither is in the
    // decision's own record in this commit).
    let reopened = Ledger::open(&path).unwrap();
    let view = change_decision::read(&reopened, OLD_CHANGE).unwrap();
    let served = serde_json::to_vec(&view).unwrap();
    let as_value: serde_json::Value = serde_json::from_slice(&served).unwrap();
    let old_row = serde_json::json!({
        "id": OLD_APPROVAL,
        "act": "approved",
        "target_revision": OLD_VERSION,
        "scope_version": 1,
        "actor": "sam",
        "decided_at": "2026-01-01T00:05:00Z",
        "withdraws": null,
        "record_path": null,
        "exceptions": [],
    });
    assert_eq!(as_value["standing"], old_row);
    assert_eq!(as_value["history"], serde_json::json!([old_row]));
    let text = String::from_utf8(served.clone()).unwrap();
    for key in [
        "\"note\"",
        "\"notes\"",
        "\"seconds_standing\"",
        "\"seconds\"",
    ] {
        assert!(
            !text.contains(key),
            "{key} appeared on the old approval: {text}"
        );
    }

    // After the upgrade: a second by another person is accepted; the
    // approval's own actor, however typed, is the same person.
    let second =
        change_decision_second::second(&reopened, OLD_CHANGE, &decide("alex", OLD_VERSION))
            .unwrap();
    let Seconding::Recorded { second } = second else {
        panic!("a second by another person must be recorded, got {second:?}");
    };
    assert_eq!(second.decision_id, OLD_APPROVAL);
    assert_eq!(
        second.target_revision, OLD_VERSION,
        "a second copies the version (B6.13)"
    );
    assert_eq!(second.scope_version, 1);
    for same in ["sam", "  SAM ", "Sam"] {
        assert_eq!(
            change_decision_second::second(&reopened, OLD_CHANGE, &decide(same, OLD_VERSION))
                .unwrap(),
            Seconding::SamePerson {
                with: "sam".to_string()
            },
            "{same:?} is the approval's own actor, compared trimmed and case-folded (Q3)"
        );
    }
    // A second joined it, and the approval serves exactly what it served
    // before PLUS the one derived count on the standing approval: the history
    // entry, and every other key, are the bytes they were.
    let after: serde_json::Value =
        serde_json::to_value(change_decision::read(&reopened, OLD_CHANGE).unwrap()).unwrap();
    assert_eq!(
        after["history"], as_value["history"],
        "history is untouched"
    );
    let mut standing_after = after["standing"].clone();
    assert_eq!(
        standing_after["seconds_standing"], 1,
        "the standing approval says one second stands"
    );
    standing_after
        .as_object_mut()
        .expect("an object")
        .remove("seconds_standing");
    assert_eq!(
        standing_after, as_value["standing"],
        "a second adds the count and changes nothing else an approval serves"
    );

    // The capture-hold chain starts at its own genesis.
    let hold = Hold {
        project_id: OLD_PROJECT,
        project_folder: "/old/project",
        actor: "sam",
        recorded_at: "2026-01-02T00:01:00Z",
    };
    let Recorded::Appended(first) =
        capture_hold::record(&reopened, HoldAct::Paused, &hold, None).unwrap()
    else {
        panic!("a pause over nothing is appended");
    };
    assert_eq!(first.seq, 0);
    assert_eq!(
        first.prev_hash,
        thruline_core::chain::sha256_hex(b"capture_hold"),
        "the chain's genesis is sha256(\"capture_hold\") (Q9)"
    );
    assert_eq!(capture_hold::verify_chain(&reopened).unwrap(), 1);
    // The run-permission chain the store already had still adds up.
    assert_eq!(
        thruline_core::run_permission::verify_chain(&reopened).unwrap(),
        1
    );
    drop(reopened);

    let again = Ledger::open_upgrading(&path, &mut |_, _, _| {}).expect("a second upgrade");
    assert!(again.took_schema_fast_path(), "a second upgrade is a no-op");
}

/* ── 4. six tables added, nothing altered ─────────────────────────────── */

/// The migration adds six tables, six indexes and eighteen triggers, and
/// alters none, proven against the base commit's schema rather than a memory
/// of it.
#[test]
fn the_migration_adds_six_tables_and_alters_no_existing_one() {
    let old = Connection::open_in_memory().unwrap();
    old.execute_batch(&fixture_schema()).unwrap();
    let new = Connection::open_in_memory().unwrap();
    new.execute_batch(Ledger::schema_ddl()).unwrap();
    let shape = |conn: &Connection| -> Vec<(String, String)> {
        let mut s = conn
            .prepare(
                "SELECT name, COALESCE(sql,'') FROM sqlite_master
                  WHERE type IN ('table','trigger','index')
                    AND name NOT LIKE 'sqlite_autoindex_%' ORDER BY name",
            )
            .unwrap();
        s.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
    };
    let (old_shape, new_shape) = (shape(&old), shape(&new));
    assert!(!old_shape.is_empty(), "the schema fixture is empty");
    for (name, sql) in &old_shape {
        let found = new_shape.iter().find(|(n, _)| n == name);
        assert!(found.is_some(), "{name} disappeared");
        assert_eq!(&found.unwrap().1, sql, "{name} was altered");
    }
    let mut added: Vec<String> = new_shape
        .iter()
        .map(|(n, _)| n.clone())
        .filter(|n| !old_shape.iter().any(|(o, _)| o == n))
        .collect();
    added.sort();
    let mut expected = the_thirty();
    assert_eq!(expected.len(), 30);
    expected.extend(migration_registry::objects_added_after(THIS_TASK));
    expected.sort();
    assert_eq!(added, expected);

    // And the registry says the same thing this tree creates, each object
    // claimed once.
    let me = this_entry();
    let mut claimed: Vec<String> = me
        .tables
        .into_iter()
        .chain(me.indexes)
        .chain(me.triggers)
        .collect();
    claimed.sort();
    let mut deduped = claimed.clone();
    deduped.dedup();
    assert_eq!(claimed, deduped, "an object is claimed twice");
    assert_eq!(claimed, the_thirty());
}

/* ── 5. the cost is quoted from the function ─────────────────────────── */

/// A193: the cost is stated, not guessed. The registry note quotes
/// `upgrade::estimate_phrase` for the 2.01 M-event corpus; this pins that the
/// quote is what the function says.
#[test]
fn the_registry_states_the_upgrade_cost_estimate_phrase_gives() {
    let me = this_entry();
    let phrase = thruline_core::upgrade::estimate_phrase(2_010_000);
    let note =
        std::fs::read_to_string(migration_registry::registry_dir().join("160-c30-r-ledger.toml"))
            .expect("the registry file is readable");
    assert!(
        note.contains(&format!("`upgrade::estimate_phrase` gives \"{phrase}\"")),
        "the note must quote {phrase:?}; task {}",
        me.task
    );
}

/* ── the DDL's own constraints ────────────────────────────────────────── */

const SHA64: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// One valid row per table (foreign keys are switched off for these, which test
/// the CHECKs and not the parents), and the ways each one can be wrong.
/// What is wrong with a row, and the statement that writes it.
type WrongRow = (&'static str, String);

/// A table, its control row, and the ways that row can be wrong.
type ConstraintCase = (&'static str, String, Vec<WrongRow>);

fn constraint_cases() -> Vec<ConstraintCase> {
    let long = |n: usize| format!("hex(zeroblob({}))", n.div_ceil(2));
    vec![
        (
            "change_notes",
            "INSERT INTO change_notes(id,change_id,decision_id,actor,body,recorded_at)
             VALUES('n1','c1',NULL,'sam','kept','t')"
                .to_string(),
            vec![
                (
                    "a blank body",
                    "INSERT INTO change_notes VALUES('n2','c1',NULL,'sam','   ','t')".to_string(),
                ),
                (
                    "a body over 2,000 characters",
                    format!(
                        "INSERT INTO change_notes VALUES('n2','c1',NULL,'sam',{},'t')",
                        long(2001)
                    ),
                ),
                (
                    "a nameless actor",
                    "INSERT INTO change_notes VALUES('n2','c1',NULL,' ','kept','t')".to_string(),
                ),
            ],
        ),
        (
            "statement_objections",
            format!(
                "INSERT INTO statement_objections(id,act,statement_id,event_id,claim_ordinal,
                     statement_sha256,actor,reason,recorded_at,withdraws)
                 VALUES('o1','objected','e1#0','e1',0,'{SHA64}','sam','why','t',NULL)"
            ),
            vec![
                (
                    "a statement id that is not <event>#<ordinal>",
                    format!(
                        "INSERT INTO statement_objections VALUES('o2','objected','e1#1','e1',0,
                         '{SHA64}','sam','why','t',NULL)"
                    ),
                ),
                (
                    "an objection with no reason",
                    format!(
                        "INSERT INTO statement_objections VALUES('o2','objected','e1#0','e1',0,
                         '{SHA64}','sam','  ','t',NULL)"
                    ),
                ),
                (
                    "a withdrawal that withdraws nothing",
                    format!(
                        "INSERT INTO statement_objections VALUES('o2','withdrawn','e1#0','e1',0,
                         '{SHA64}','sam','','t',NULL)"
                    ),
                ),
                (
                    "an objection that claims to withdraw",
                    format!(
                        "INSERT INTO statement_objections VALUES('o2','objected','e1#0','e1',0,
                         '{SHA64}','sam','why','t','o1')"
                    ),
                ),
                (
                    "a digest that is not 64 characters",
                    "INSERT INTO statement_objections VALUES('o2','objected','e1#0','e1',0,
                     'abc','sam','why','t',NULL)"
                        .to_string(),
                ),
                (
                    "a negative ordinal",
                    format!(
                        "INSERT INTO statement_objections VALUES('o2','objected','e1#-1','e1',-1,
                         '{SHA64}','sam','why','t',NULL)"
                    ),
                ),
                (
                    "an act that is neither word",
                    format!(
                        "INSERT INTO statement_objections VALUES('o2','agreed','e1#0','e1',0,
                         '{SHA64}','sam','why','t',NULL)"
                    ),
                ),
                (
                    "a reason over 2,000 characters",
                    format!(
                        "INSERT INTO statement_objections VALUES('o2','objected','e1#0','e1',0,
                         '{SHA64}','sam',{},'t',NULL)",
                        long(2001)
                    ),
                ),
            ],
        ),
        (
            "change_decision_seconds",
            "INSERT INTO change_decision_seconds(id,change_id,act,decision_id,target_revision,
                 scope_version,actor,recorded_at,withdraws)
             VALUES('s1','c1','approved','cd1','rev',1,'alex','t',NULL)"
                .to_string(),
            vec![
                (
                    "a scope version of 0",
                    "INSERT INTO change_decision_seconds VALUES('s2','c1','approved','cd1','rev',0,
                     'alex','t',NULL)"
                        .to_string(),
                ),
                (
                    "a withdrawal that withdraws nothing",
                    "INSERT INTO change_decision_seconds VALUES('s2','c1','withdrawn','cd1','rev',1,
                     'alex','t',NULL)"
                        .to_string(),
                ),
                (
                    "a third act",
                    "INSERT INTO change_decision_seconds VALUES('s2','c1','amended','cd1','rev',1,
                     'alex','t',NULL)"
                        .to_string(),
                ),
                (
                    "an approval that claims to withdraw",
                    "INSERT INTO change_decision_seconds VALUES('s2','c1','approved','cd1','rev',1,
                     'alex','t','s1')"
                        .to_string(),
                ),
            ],
        ),
        (
            "change_agent_messages",
            format!(
                "INSERT INTO change_agent_messages(id,act,thread_id,thread_event_id,change_id,
                     version_id,message,message_sha256,actor,recorded_at,refers_to)
                 VALUES('m1','copied','th1','e1','c1','v1','hello','{SHA64}','sam','t',NULL)"
            ),
            vec![
                (
                    "a copy with no text",
                    "INSERT INTO change_agent_messages VALUES('m2','copied','th1','e1','c1','v1',
                     NULL,NULL,'sam','t',NULL)"
                        .to_string(),
                ),
                (
                    "a message over 8,000 characters",
                    format!(
                        "INSERT INTO change_agent_messages VALUES('m2','copied','th1','e1','c1','v1',
                         {},'{SHA64}','sam','t',NULL)",
                        long(8001)
                    ),
                ),
                (
                    "a pasted mark that carries text",
                    format!(
                        "INSERT INTO change_agent_messages VALUES('m2','marked_pasted','th1','e1','c1',
                         'v1','hello','{SHA64}','sam','t','m1')"
                    ),
                ),
                (
                    "a copy that refers to a copy",
                    format!(
                        "INSERT INTO change_agent_messages VALUES('m2','copied','th1','e1','c1','v1',
                         'hello','{SHA64}','sam','t','m1')"
                    ),
                ),
                (
                    "a pasted mark that points at nothing",
                    "INSERT INTO change_agent_messages VALUES('m2','marked_pasted','th1','e1','c1',
                     'v1',NULL,NULL,'sam','t',NULL)"
                        .to_string(),
                ),
                (
                    "a digest that is not 64 characters",
                    "INSERT INTO change_agent_messages VALUES('m2','copied','th1','e1','c1','v1',
                     'hello','abc','sam','t',NULL)"
                        .to_string(),
                ),
                (
                    "an act with a sending word",
                    "INSERT INTO change_agent_messages VALUES('m2','sent','th1','e1','c1','v1',
                     NULL,NULL,'sam','t','m1')"
                        .to_string(),
                ),
            ],
        ),
        (
            "capture_holds",
            "INSERT INTO capture_holds(id,seq,act,project_id,project_folder,pattern,actor,
                 recorded_at,ends,content_sha256,prev_hash,chain_hash)
             VALUES('h1',0,'paused','p','/f',NULL,'sam','t',NULL,'a','b','c')"
                .to_string(),
            vec![
                (
                    "a pause that names a pattern",
                    "INSERT INTO capture_holds VALUES('h2',1,'paused','p','/f','env','sam','t',NULL,
                     'a','b','c')"
                        .to_string(),
                ),
                (
                    "an exclusion that names no pattern",
                    "INSERT INTO capture_holds VALUES('h2',1,'excluded','p','/f',NULL,'sam','t',NULL,
                     'a','b','c')"
                        .to_string(),
                ),
                (
                    "a resumption that ends nothing",
                    "INSERT INTO capture_holds VALUES('h2',1,'resumed','p','/f',NULL,'sam','t',NULL,
                     'a','b','c')"
                        .to_string(),
                ),
                (
                    "a pause that claims to end something",
                    "INSERT INTO capture_holds VALUES('h2',1,'paused','p','/f',NULL,'sam','t','h1',
                     'a','b','c')"
                        .to_string(),
                ),
                (
                    "a negative place on the chain",
                    "INSERT INTO capture_holds VALUES('h2',-1,'paused','p','/f',NULL,'sam','t',NULL,
                     'a','b','c')"
                        .to_string(),
                ),
                (
                    "a pattern over 256 characters",
                    format!(
                        "INSERT INTO capture_holds VALUES('h2',1,'excluded','p','/f',{},'sam','t',NULL,
                         'a','b','c')",
                        long(257)
                    ),
                ),
                (
                    "the same place on the chain twice",
                    "INSERT INTO capture_holds VALUES('h2',0,'paused','p','/f',NULL,'sam','t',NULL,
                     'a','b','c')"
                        .to_string(),
                ),
            ],
        ),
        (
            "inbox_hides",
            format!(
                "INSERT INTO inbox_hides(id,act,row_kind,thread_id,thread_event_id,row_sha256,
                     actor,recorded_at,shows)
                 VALUES('i1','hidden','thread','th1','e1','{SHA64}','sam','t',NULL)"
            ),
            vec![
                (
                    "a show that shows nothing",
                    format!(
                        "INSERT INTO inbox_hides VALUES('i2','shown','thread','th1','e1','{SHA64}',
                         'sam','t',NULL)"
                    ),
                ),
                (
                    "a hide that claims to show",
                    format!(
                        "INSERT INTO inbox_hides VALUES('i2','hidden','thread','th1','e1','{SHA64}',
                         'sam','t','i1')"
                    ),
                ),
                (
                    "a row kind that is a Change",
                    format!(
                        "INSERT INTO inbox_hides VALUES('i2','hidden','change','th1','e1','{SHA64}',
                         'sam','t',NULL)"
                    ),
                ),
                (
                    "a digest that is not 64 characters",
                    "INSERT INTO inbox_hides VALUES('i2','hidden','thread','th1','e1','abc','sam',
                     't',NULL)"
                        .to_string(),
                ),
            ],
        ),
    ]
}

/// Each table's CHECKs say what the design says they say: the control row goes
/// in, and every wrong variant of it is refused by a CHECK (not by anything
/// else), so a constraint that is loosened turns this red by name.
#[test]
fn every_check_constraint_in_the_six_tables_holds() {
    let ledger = Ledger::open_in_memory().unwrap();
    let conn = ledger.conn();
    conn.execute_batch("PRAGMA foreign_keys=OFF").unwrap();
    let cases = constraint_cases();
    assert_eq!(
        cases.iter().map(|(t, _, _)| *t).collect::<Vec<_>>(),
        TABLES,
        "the cases cover the six tables, in the DDL's order"
    );
    for (table, valid, wrong) in cases {
        conn.execute(&valid, [])
            .unwrap_or_else(|e| panic!("{table}: the control row was refused: {e}"));
        for (what, sql) in wrong {
            let err = conn
                .execute(&sql, [])
                .err()
                .unwrap_or_else(|| panic!("{table}: {what} was accepted"));
            assert!(
                format!("{err}").contains("CHECK constraint failed")
                    || format!("{err}").contains("UNIQUE constraint failed"),
                "{table}: {what} was refused by something other than a constraint: {err}"
            );
        }
    }
}

/* ── beside it: nothing here is a result ─────────────────────────────── */

const PERSON_CHANGE: &str = "cccccccc-0000-4000-8000-00000000c160";
const PERSON_VERSION: &str = "snap_r_ledger_aaaaaaaaaaaaaaaaaaaaaaaa";

fn hq_fixtures() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hq")
}

/// A store with real recorded work (the HQ fixtures, through the real ingest)
/// and one Change on it with a standing approval.
fn a_store_with_work(dir: &std::path::Path) -> Ledger {
    use thruline_core::ingest::{ingest, ingest_codex, CodexIngestOptions};
    let db = dir.join("ledger.db");
    let mut ledger = Ledger::open(&db).expect("open");
    ingest(
        &mut ledger,
        &[hq_fixtures().join("claude/-work-app")],
        &dir.join("mirror"),
    )
    .expect("claude ingest");
    let codex: Vec<std::path::PathBuf> = std::fs::read_dir(hq_fixtures().join("codex"))
        .expect("codex fixtures")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    ingest_codex(
        &mut ledger,
        &codex,
        &CodexIngestOptions {
            mirror_root: dir.join("mirror"),
            owner_key: Some("r-ledger-test-owner".to_string()),
        },
    )
    .expect("codex ingest");

    let project = hq::hq_projects(&ledger).expect("projects").remove(0);
    let ledger_project = project.members[0].ledger_project_id.clone();
    let root = dir.join("worktree");
    std::fs::create_dir_all(&root).expect("worktree");
    let conn = ledger.conn();
    conn.execute(
        "INSERT INTO changes(id, project_id, opened_at, title, purpose, request_event_id,
                             requester, request_source)
         VALUES(?1, ?2, '2026-09-22T08:00:00Z', 'Totals add up', 'They do not.', NULL,
                'sam', 'person')",
        params![PERSON_CHANGE, ledger_project],
    )
    .expect("insert change");
    conn.execute(
        "INSERT INTO change_scope_revisions(change_id, scope_version, proposed_at, criteria_json,
                                            exclusions_json, allowed_scope_json, constraints_json,
                                            check_profile_json, content_sha256)
         VALUES(?1, 1, '2026-09-22T08:30:00Z', '[\"The totals add up\"]', '[]', '[\"app/\"]',
                '[]', '[]', 'deadbeef')",
        params![PERSON_CHANGE],
    )
    .expect("insert scope revision");
    conn.execute(
        "INSERT INTO change_scope_acceptances(change_id, scope_version, accepted_by, accepted_at,
                                              rationale)
         VALUES(?1, 1, 'sam', '2026-09-22T09:00:00Z', '')",
        params![PERSON_CHANGE],
    )
    .expect("insert scope acceptance");
    let doc = serde_json::json!({
        "id": PERSON_VERSION,
        "taken_at": "2026-09-22T10:00:00Z",
        "root": root.to_string_lossy(),
        "git": null,
        "tree": { "staged": [], "unstaged": [], "untracked": [], "deleted": [],
                  "dirty_diff_sha256": null },
        "environment": { "os": "test", "arch": "test", "git_version": null,
                         "env_names": [], "lockfiles": {} },
        "processes": { "ps_sha256": null, "ps_lines": 0, "ports": null, "task_ids": null,
                       "log_cursors": null, "gaps": "none" },
        "complete": true,
        "note": null,
    });
    let text = serde_json::to_string(&doc).expect("snapshot doc");
    conn.execute(
        "INSERT INTO workspace_snapshots(id, taken_at, root, head, branch, complete, doc,
                                         doc_sha256)
         VALUES(?1, '2026-09-22T10:00:00Z', ?2, NULL, NULL, 1, ?3, ?4)",
        params![
            PERSON_VERSION,
            root.to_string_lossy(),
            text,
            thruline_core::chain::sha256_hex(text.as_bytes())
        ],
    )
    .expect("insert snapshot");
    conn.execute(
        "INSERT INTO change_implementations(change_id, binding_version, bound_at,
                                            repository_identity, base_revision, snapshot_id,
                                            workspace_identity)
         VALUES(?1, 1, '2026-09-22T10:00:00Z', 'app', 'base0000', ?2, NULL)",
        params![PERSON_CHANGE, PERSON_VERSION],
    )
    .expect("insert implementation");
    let outcome = change_decision::approve(
        &ledger,
        PERSON_CHANGE,
        &Decide {
            actor: "sam".to_string(),
            at: "2026-09-22T11:00:00Z".to_string(),
            seen_revision: PERSON_VERSION.to_string(),
            note: None,
        },
    )
    .expect("approve");
    assert!(
        matches!(outcome, change_decision::Outcome::Recorded { .. }),
        "the fixture's approval must stand: {outcome:?}"
    );
    ledger
}

fn now_ms() -> i64 {
    hq::parse_ms("2026-10-07T00:00:00Z").expect("instant")
}

/// Everything a verdict, a count, readiness or an approval's standing is read
/// from, as the bytes each serves.
fn results(ledger: &Ledger) -> Vec<(String, Vec<u8>)> {
    let project = hq::hq_projects(ledger).expect("projects").remove(0);
    let statements = hq::index(ledger, &project).expect("index").statements();
    let view = hq::project(ledger, &project.id, now_ms())
        .expect("project view")
        .expect("present");
    let environment = thruline_core::change_propose::machine_environment(ledger, PERSON_CHANGE)
        .expect("environment");
    let coverage = thruline_core::change_evidence::coverage(ledger, PERSON_CHANGE, &environment)
        .expect("coverage");
    let readiness = thruline_core::change_readiness::readiness(ledger, PERSON_CHANGE, &coverage)
        .expect("readiness");
    let decisions = change_decision::read(ledger, PERSON_CHANGE).expect("decisions");
    vec![
        (
            "statements".into(),
            serde_json::to_vec(&statements).unwrap(),
        ),
        ("project view".into(), serde_json::to_vec(&view).unwrap()),
        ("coverage".into(), serde_json::to_vec(&coverage).unwrap()),
        ("readiness".into(), serde_json::to_vec(&readiness).unwrap()),
        (
            "standing".into(),
            serde_json::to_vec(&decisions.standing).unwrap(),
        ),
        ("decisions".into(), serde_json::to_vec(&decisions).unwrap()),
    ]
}

/// One row, or more, in every one of the six tables: through the core function
/// where there is one, and as the column values the design names where there is
/// not yet (the routes that write them are later commits).
fn put_a_row_in_every_table(ledger: &Ledger) {
    let project = hq::hq_projects(ledger).expect("projects").remove(0);
    let index = hq::index(ledger, &project).expect("index");
    let statements = index.statements();
    let claim = statements
        .claims
        .first()
        .expect("the HQ fixtures record at least one statement");
    let (event_id, ordinal) = claim
        .id
        .rsplit_once('#')
        .expect("a statement id is <event>#<n>");
    let ordinal: i64 = ordinal.parse().expect("the ordinal is a number");
    let view = hq::project(ledger, &project.id, now_ms())
        .expect("project view")
        .expect("present");
    let thread = view.threads.first().expect("the fixtures make a thread");
    let identity = index
        .thread_identity(&thread.id)
        .expect("the thread has an identity");
    let at = "2026-10-07T12:00:00Z";
    let conn = ledger.conn();

    conn.execute(
        "INSERT INTO change_notes(id, change_id, decision_id, actor, body, recorded_at)
         VALUES('cn_1', ?1, NULL, 'sam', 'Read it twice; fine by me.', ?2)",
        params![PERSON_CHANGE, at],
    )
    .expect("a note");
    conn.execute(
        "INSERT INTO statement_objections(id, act, statement_id, event_id, claim_ordinal,
             statement_sha256, actor, reason, recorded_at, withdraws)
         VALUES('so_1', 'objected', ?1, ?2, ?3, ?4, 'sam', 'The run it points at failed.', ?5, NULL)",
        params![
            claim.id,
            event_id,
            ordinal,
            thruline_core::chain::sha256_hex(b"All tests pass."),
            at
        ],
    )
    .expect("an objection");
    let Seconding::Recorded { .. } =
        change_decision_second::second(ledger, PERSON_CHANGE, &decide("alex", PERSON_VERSION))
            .expect("a second")
    else {
        panic!("a second by another person must be recorded");
    };
    let message = "Please re-run the totals test and show me the output.";
    conn.execute(
        "INSERT INTO change_agent_messages(id, act, thread_id, thread_event_id, change_id,
             version_id, message, message_sha256, actor, recorded_at, refers_to)
         VALUES('cam_1', 'copied', ?1, ?2, ?3, ?4, ?5, ?6, 'sam', ?7, NULL)",
        params![
            thread.id,
            identity.first_prompt_event_id,
            PERSON_CHANGE,
            PERSON_VERSION,
            message,
            thruline_core::chain::sha256_hex(message.as_bytes()),
            at
        ],
    )
    .expect("a copy for the agent");
    conn.execute(
        "INSERT INTO change_agent_messages(id, act, thread_id, thread_event_id, change_id,
             version_id, message, message_sha256, actor, recorded_at, refers_to)
         VALUES('cam_2', 'marked_pasted', ?1, ?2, ?3, ?4, NULL, NULL, 'sam', ?5, 'cam_1')",
        params![
            thread.id,
            identity.first_prompt_event_id,
            PERSON_CHANGE,
            PERSON_VERSION,
            at
        ],
    )
    .expect("a pasted mark");
    let hold = Hold {
        project_id: &project.id,
        project_folder: &project.root,
        actor: "sam",
        recorded_at: at,
    };
    assert!(matches!(
        capture_hold::record(ledger, HoldAct::Paused, &hold, None).expect("pause"),
        Recorded::Appended(_)
    ));
    assert!(matches!(
        capture_hold::record(ledger, HoldAct::Excluded, &hold, Some("env")).expect("exclude"),
        Recorded::Appended(_)
    ));
    conn.execute(
        "INSERT INTO inbox_hides(id, act, row_kind, thread_id, thread_event_id, row_sha256,
             actor, recorded_at, shows)
         VALUES('ih_1', 'hidden', 'thread', ?1, ?2, ?3, 'sam', ?4, NULL)",
        params![
            thread.id,
            identity.first_prompt_event_id,
            thruline_core::chain::sha256_hex(b"the row as the screen showed it"),
            at
        ],
    )
    .expect("a hide");
}

/// With rows in all six tables, everything a verdict, a coverage count,
/// readiness or an approval's standing is read from serves the same bytes as
/// the same store without them.
#[test]
fn person_acts_move_no_result() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = a_store_with_work(dir.path());
    let before = results(&ledger);
    assert!(
        before.iter().all(|(_, bytes)| !bytes.is_empty()),
        "every reading must serve something"
    );

    put_a_row_in_every_table(&ledger);
    for table in TABLES {
        let rows: i64 = ledger
            .conn()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert!(
            rows > 0,
            "{table} must hold a row for this test to mean anything"
        );
    }

    let after = results(&ledger);
    for ((name, was), (_, now)) in before.iter().zip(after.iter()) {
        // The one thing a second adds: the derived count on the standing
        // approval. It is read out of the approval's reading, never into a
        // result, so the two approval readings are compared without it.
        let comparable = |bytes: &[u8]| -> serde_json::Value {
            let mut value: serde_json::Value = serde_json::from_slice(bytes).expect("json");
            if name == "standing" || name == "decisions" {
                let row = if name == "standing" {
                    Some(&mut value)
                } else {
                    value.get_mut("standing")
                };
                if let Some(fields) = row.and_then(serde_json::Value::as_object_mut) {
                    fields.remove("seconds_standing");
                }
            }
            value
        };
        assert!(
            comparable(was) == comparable(now),
            "{name} changed because a person acted:\nbefore {}\nafter  {}",
            String::from_utf8_lossy(was),
            String::from_utf8_lossy(now)
        );
    }
    let standing = change_decision::read(&ledger, PERSON_CHANGE)
        .expect("decisions")
        .standing
        .expect("an approval stands");
    assert_eq!(
        standing.seconds_standing,
        Some(1),
        "the second put by put_a_row_in_every_table is counted on the standing approval"
    );
}

/// The same claim for second approvals, through the real functions instead of
/// column values: seconds put on the standing approval, and one taken back,
/// leave every verdict, count and readiness answer — and which approval
/// stands, and every key it serves but the one derived count — exactly where
/// the same store without them leaves them.
#[test]
fn seconds_through_the_real_functions_move_no_result() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = a_store_with_work(dir.path());
    let before = results(&ledger);
    let standing_before = change_decision::read(&ledger, PERSON_CHANGE)
        .expect("decisions")
        .standing
        .expect("an approval stands");

    for (who, at) in [
        ("alex", "2026-10-07T10:00:00Z"),
        ("kim", "2026-10-07T10:05:00Z"),
    ] {
        let decide = Decide {
            actor: who.to_string(),
            at: at.to_string(),
            seen_revision: PERSON_VERSION.to_string(),
            note: None,
        };
        assert!(matches!(
            change_decision_second::second(&ledger, PERSON_CHANGE, &decide).expect("a second"),
            Seconding::Recorded { .. }
        ));
    }
    let withdrawn =
        change_decision_second::withdraw(&ledger, PERSON_CHANGE, "alex", "2026-10-07T10:10:00Z")
            .expect("a withdrawal");
    assert!(matches!(
        withdrawn,
        change_decision_second::Withdrawing::Recorded { .. }
    ));

    let after = results(&ledger);
    for name in ["statements", "project view", "coverage", "readiness"] {
        let find = |r: &Vec<(String, Vec<u8>)>| {
            r.iter()
                .find(|(n, _)| n == name)
                .expect("a reading")
                .1
                .clone()
        };
        assert!(
            find(&before) == find(&after),
            "{name} changed because a second person approved"
        );
    }
    // The same approval stands, row for row; the only new key is the count.
    let mut standing_after = change_decision::read(&ledger, PERSON_CHANGE)
        .expect("decisions")
        .standing
        .expect("the same approval stands");
    assert_eq!(standing_after.seconds_standing, Some(1));
    standing_after.seconds_standing = None;
    assert_eq!(standing_after, standing_before);
    // The approval-availability answer the Change screen reads is unmoved too:
    // the version the record stands at, and whether the code moved since.
    let view_before = results_view(&ledger);
    assert_eq!(view_before.0, standing_before.target_revision);
    assert!(!view_before.1, "a second does not move the code");
}

/// The version the record stands at, and whether the standing approval's
/// version is no longer it.
fn results_view(ledger: &Ledger) -> (String, bool) {
    let view = change_decision::read(ledger, PERSON_CHANGE).expect("decisions");
    (
        view.target_revision.clone().unwrap_or_default(),
        view.code_changed_since(),
    )
}

/// The same claim for notes, through the real functions instead of column
/// values: a note kept on the Change, and a note kept with a withdrawal and with
/// the approval that follows it, leave every verdict, count and readiness
/// answer — and which approval stands — exactly where the same acts without
/// words leave them.
#[test]
fn notes_through_the_real_functions_move_no_result() {
    let act = |note: Option<&str>, at: &str| Decide {
        actor: "sam".to_string(),
        at: at.to_string(),
        seen_revision: PERSON_VERSION.to_string(),
        note: note.map(str::to_string),
    };
    let withdraw_and_approve = |ledger: &Ledger, note: Option<&str>| {
        for outcome in [
            change_decision::withdraw(ledger, PERSON_CHANGE, &act(note, "2026-10-07T09:00:00Z")),
            change_decision::approve(ledger, PERSON_CHANGE, &act(note, "2026-10-07T09:30:00Z")),
        ] {
            assert!(
                matches!(
                    outcome.expect("the act"),
                    change_decision::Outcome::Recorded { .. }
                ),
                "the act must be recorded"
            );
        }
    };
    // What a person's words may not touch: all but the decision rows' own ids.
    let standing = |ledger: &Ledger| {
        let mut row = serde_json::to_value(
            change_decision::read(ledger, PERSON_CHANGE)
                .expect("decisions")
                .standing
                .expect("an approval stands"),
        )
        .expect("json");
        let fields = row.as_object_mut().expect("an object");
        for volatile in ["id", "record_path", "note"] {
            fields.remove(volatile);
        }
        row
    };

    let dir = tempfile::tempdir().unwrap();
    let ledger = a_store_with_work(dir.path());
    let before = results(&ledger);

    // 1. a note on the Change itself moves nothing, not even the decisions.
    thruline_core::change_note::add(
        &ledger,
        PERSON_CHANGE,
        "alex",
        "Read it twice; fine by me.",
        "2026-10-07T08:00:00Z",
    )
    .expect("the ledger answers")
    .expect("a note is kept");
    let after = results(&ledger);
    for ((name, was), (_, now)) in before.iter().zip(after.iter()) {
        assert!(was == now, "{name} changed because a note was kept");
    }

    // 2. the same withdrawal and re-approval, once without words and once with.
    withdraw_and_approve(&ledger, None);
    let plain = results(&ledger);
    let plain_standing = standing(&ledger);
    withdraw_and_approve(&ledger, Some("Looked again after the agent's change."));
    let noted = results(&ledger);
    for name in ["statements", "project view", "coverage", "readiness"] {
        let find = |r: &Vec<(String, Vec<u8>)>| {
            r.iter()
                .find(|(n, _)| n == name)
                .expect("a reading")
                .1
                .clone()
        };
        assert!(
            find(&plain) == find(&noted),
            "{name} changed because the act carried words"
        );
    }
    assert_eq!(plain_standing, standing(&ledger));
    let kept: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM change_notes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        kept,
        1 + 2,
        "one on the Change and one with each of the two acts"
    );
}

/// An objection is a person's words about a statement, never a result: no
/// column a verdict could be copied into exists, and none of the other five
/// tables has one either.
#[test]
fn an_objection_has_no_place_to_put_a_result() {
    let ledger = Ledger::open_in_memory().unwrap();
    let columns = |table: &str| -> Vec<String> {
        let mut s = ledger
            .conn()
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap();
        s.query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
    };
    assert_eq!(
        columns("statement_objections"),
        vec![
            "id",
            "act",
            "statement_id",
            "event_id",
            "claim_ordinal",
            "statement_sha256",
            "actor",
            "reason",
            "recorded_at",
            "withdraws",
        ],
        "an objection is a statement's address, its sentence's digest, who, why, \
         when and what it takes back — nothing else"
    );
    for table in TABLES {
        let names = columns(table);
        assert!(!names.is_empty(), "{table} does not exist");
        for forbidden in ["verdict", "result", "outcome", "passed", "status", "proven"] {
            assert!(
                !names.iter().any(|n| n.contains(forbidden)),
                "{table} must not be able to carry a result: {forbidden}"
            );
        }
    }
}

/// Q16: withdrawing an objection is not drawn and has no route, but the table
/// keeps the act, pointing at what it takes back.
#[test]
fn an_objection_keeps_withdrawn_in_the_table() {
    let ledger = Ledger::open_in_memory().unwrap();
    let sql: String = ledger
        .conn()
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='statement_objections'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(sql.contains("'objected','withdrawn'"), "{sql}");
}

/* ── the six tables are append-only, and say so in their own words ────── */

#[test]
fn every_r_ledger_table_refuses_update_delete_and_replace() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = a_store_with_work(dir.path());
    put_a_row_in_every_table(&ledger);
    for (table, words) in [
        ("change_notes", "notes are append-only"),
        ("statement_objections", "objections are append-only"),
        (
            "change_decision_seconds",
            "second approvals are append-only",
        ),
        (
            "change_agent_messages",
            "messages for the agent are append-only",
        ),
        ("capture_holds", "capture holds are append-only"),
        ("inbox_hides", "hides are append-only"),
    ] {
        let rows: i64 = ledger
            .conn()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert!(rows > 0, "{table} holds no row to protect");
        let update = ledger
            .conn()
            .execute(&format!("UPDATE {table} SET actor='mallory'"), [])
            .expect_err("an update must be refused");
        assert!(format!("{update}").contains(words), "{table}: {update}");
        let delete = ledger
            .conn()
            .execute(&format!("DELETE FROM {table}"), [])
            .expect_err("a delete must be refused");
        assert!(format!("{delete}").contains(words), "{table}: {delete}");
        let replace = ledger
            .conn()
            .execute(
                &format!("INSERT OR REPLACE INTO {table} SELECT * FROM {table}"),
                [],
            )
            .expect_err("a replace must be refused");
        assert!(format!("{replace}").contains(words), "{table}: {replace}");
    }
    // Every table carries all three guards, under the names the registry claims.
    let triggers: Vec<String> = {
        let mut s = ledger
            .conn()
            .prepare("SELECT name FROM sqlite_master WHERE type='trigger'")
            .unwrap();
        s.query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
    };
    for table in TABLES {
        for suffix in ["no_replace", "no_update", "no_delete"] {
            let name = format!("{table}_{suffix}");
            assert!(triggers.contains(&name), "missing {name}");
        }
    }
}

/* ── a second: three refusals, and it never moves the standing ────────── */

#[test]
fn a_second_has_three_refusals_and_never_makes_an_approval_stand() {
    let dir = tempfile::tempdir().unwrap();
    let path = base_commit_store(dir.path());
    drop(Ledger::open_upgrading(&path, &mut |_, _, _| {}).unwrap());
    let ledger = Ledger::open(&path).unwrap();
    let rows = |ledger: &Ledger| -> i64 {
        ledger
            .conn()
            .query_row("SELECT COUNT(*) FROM change_decision_seconds", [], |r| {
                r.get(0)
            })
            .unwrap()
    };

    // The screen was showing another version: nothing is appended, and the
    // answer is approve's own shape.
    match change_decision_second::second(&ledger, OLD_CHANGE, &decide("alex", "snap_other"))
        .unwrap()
    {
        Seconding::Moved { seen, now, .. } => {
            assert_eq!(seen, "snap_other");
            assert_eq!(now, OLD_VERSION);
        }
        other => panic!("expected Moved, got {other:?}"),
    }
    assert_eq!(rows(&ledger), 0);

    // A second joins a person: alex once, then alex again is the same person.
    assert!(matches!(
        change_decision_second::second(&ledger, OLD_CHANGE, &decide("alex", OLD_VERSION)).unwrap(),
        Seconding::Recorded { .. }
    ));
    assert_eq!(
        change_decision_second::second(&ledger, OLD_CHANGE, &decide("  ALEX ", OLD_VERSION))
            .unwrap(),
        Seconding::SamePerson {
            with: "alex".to_string()
        },
        "a standing second's actor cannot second again"
    );
    assert_eq!(rows(&ledger), 1);
    let seconds = change_decision_second::seconds_for(&ledger, OLD_APPROVAL).unwrap();
    assert_eq!(change_decision_second::standing(&seconds).len(), 1);

    // A nameless second is refused before anything is read.
    assert!(
        change_decision_second::second(&ledger, OLD_CHANGE, &decide("   ", OLD_VERSION)).is_err()
    );

    // Withdraw the approval: nothing stands, so there is nothing to second,
    // and the earlier second did not make anything stand.
    change_decision::withdraw(&ledger, OLD_CHANGE, &decide("sam", OLD_VERSION)).unwrap();
    assert!(change_decision::read(&ledger, OLD_CHANGE)
        .unwrap()
        .standing
        .is_none());
    assert_eq!(
        change_decision_second::second(&ledger, OLD_CHANGE, &decide("kim", OLD_VERSION)).unwrap(),
        Seconding::NothingToSecond
    );
    assert_eq!(rows(&ledger), 1, "a refusal appends nothing");
}

/* ── the capture-hold chain ──────────────────────────────────────────── */

fn hold(folder: &str) -> Hold<'_> {
    Hold {
        project_id: "hq-project",
        project_folder: folder,
        actor: "sam",
        recorded_at: "2026-10-07T12:00:00Z",
    }
}

fn appended(recorded: Recorded) -> capture_hold::HoldRow {
    match recorded {
        Recorded::Appended(row) => *row,
        Recorded::Refused(why) => panic!("expected an appended row, got {why:?}"),
    }
}

#[test]
fn a_hold_that_changes_nothing_is_refused_and_the_chain_adds_up() {
    let ledger = Ledger::open_in_memory().unwrap();
    let here = hold("/work/app");
    assert_eq!(
        capture_hold::record(&ledger, HoldAct::Resumed, &here, None).unwrap(),
        Recorded::Refused(Refusal::NotPaused)
    );
    let paused = appended(capture_hold::record(&ledger, HoldAct::Paused, &here, None).unwrap());
    assert_eq!(
        capture_hold::record(&ledger, HoldAct::Paused, &here, None).unwrap(),
        Recorded::Refused(Refusal::AlreadyPaused)
    );
    assert_eq!(
        capture_hold::record(&ledger, HoldAct::Included, &here, Some("env")).unwrap(),
        Recorded::Refused(Refusal::NotExcluded)
    );
    let excluded =
        appended(capture_hold::record(&ledger, HoldAct::Excluded, &here, Some("env")).unwrap());
    assert_eq!(
        capture_hold::record(&ledger, HoldAct::Excluded, &here, Some("env")).unwrap(),
        Recorded::Refused(Refusal::AlreadyExcluded)
    );
    let standing = capture_hold::standing(&ledger, "/work/app").unwrap();
    assert_eq!(
        standing.paused.as_ref().map(|r| r.id.clone()),
        Some(paused.id.clone())
    );
    assert_eq!(standing.exclusions.len(), 1);

    let resumed = appended(capture_hold::record(&ledger, HoldAct::Resumed, &here, None).unwrap());
    assert_eq!(
        resumed.ends.as_deref(),
        Some(paused.id.as_str()),
        "a resume names its pause"
    );
    let included =
        appended(capture_hold::record(&ledger, HoldAct::Included, &here, Some("env")).unwrap());
    assert_eq!(included.ends.as_deref(), Some(excluded.id.as_str()));
    let standing = capture_hold::standing(&ledger, "/work/app").unwrap();
    assert!(standing.paused.is_none() && standing.exclusions.is_empty());

    // Another folder has its own standing, on the same chain.
    appended(capture_hold::record(&ledger, HoldAct::Paused, &hold("/work/other"), None).unwrap());
    assert!(capture_hold::standing(&ledger, "/work/app")
        .unwrap()
        .paused
        .is_none());
    assert_eq!(capture_hold::verify_chain(&ledger).unwrap(), 5);

    // Bad patterns never reach the table.
    for bad in ["../secrets", "/etc/passwd", "src/?.rs", "a[b]", "", "   "] {
        assert!(
            capture_hold::record(&ledger, HoldAct::Excluded, &here, Some(bad)).is_err(),
            "{bad:?} must be refused"
        );
    }
    // A pause carries no pattern, and an exclusion must.
    assert!(capture_hold::record(&ledger, HoldAct::Paused, &here, Some("env")).is_err());
    assert!(capture_hold::record(&ledger, HoldAct::Excluded, &here, None).is_err());
    assert_eq!(capture_hold::verify_chain(&ledger).unwrap(), 5);
}

#[test]
fn a_tampered_hold_breaks_the_chain() {
    let ledger = Ledger::open_in_memory().unwrap();
    let here = hold("/work/app");
    appended(capture_hold::record(&ledger, HoldAct::Paused, &here, None).unwrap());
    appended(capture_hold::record(&ledger, HoldAct::Resumed, &here, None).unwrap());
    assert_eq!(capture_hold::verify_chain(&ledger).unwrap(), 2);
    ledger
        .conn()
        .execute_batch(
            "DROP TRIGGER capture_holds_no_update;
             UPDATE capture_holds SET actor='mallory' WHERE seq=1;",
        )
        .unwrap();
    let err = capture_hold::verify_chain(&ledger).expect_err("a tampered row must not add up");
    assert!(format!("{err}").contains("seq 1"), "{err}");
}

/* ── the derivation, the rebuild, the speaker and MCP name none of them ── */

fn workspace_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("the workspace root")
        .to_path_buf()
}

/// The text between two markers of a file, both required: a marker that moved
/// makes this test fail by name rather than check nothing.
fn between(file: &str, start: &str, end: &str) -> String {
    let text = std::fs::read_to_string(workspace_root().join(file))
        .unwrap_or_else(|e| panic!("{file} must be readable: {e}"));
    let from = text
        .find(start)
        .unwrap_or_else(|| panic!("{file} no longer contains {start:?}"));
    let rest = &text[from..];
    let to = rest[start.len()..]
        .find(end)
        .unwrap_or_else(|| panic!("{file} has nothing after {start:?} matching {end:?}"));
    rest[..start.len() + to].to_string()
}

#[test]
fn derive_rebuild_speaker_and_mcp_name_none_of_the_six_tables() {
    let whole = |file: &str| -> (String, String) {
        (
            file.to_string(),
            std::fs::read_to_string(workspace_root().join(file))
                .unwrap_or_else(|e| panic!("{file} must be readable: {e}")),
        )
    };
    let read = vec![
        // The derivation, and `derive::rebuild` with it.
        whole("crates/core/src/derive.rs"),
        // The speaker and its stop gate.
        whole("crates/core/src/speaker.rs"),
        whole("crates/core/src/speaker_gate.rs"),
        // The MCP tools.
        whole("crates/core/src/mcp.rs"),
        // `thruline rebuild`: the verb, and the chain positions it rebuilds.
        (
            "crates/cli/src/main.rs (Command::Rebuild)".to_string(),
            between(
                "crates/cli/src/main.rs",
                "Command::Rebuild { project, all } =>",
                "Command::Model { command } =>",
            ),
        ),
        (
            "crates/core/src/ledger.rs (rebuild_heads)".to_string(),
            between(
                "crates/core/src/ledger.rs",
                "pub fn rebuild_heads(",
                "\n    pub fn ",
            ),
        ),
    ];
    let mut offenders = Vec::new();
    for (file, text) in &read {
        assert!(!text.is_empty(), "{file} read as empty");
        for table in TABLES {
            if text.contains(table) {
                offenders.push(format!("{file} names {table}"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "a person's act is never read by the derivation, the rebuild, the speaker or MCP: \
         {offenders:?}"
    );
}

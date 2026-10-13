//! HQ record search (C30 R-ROUTES "Record search", census §3.08 row 4): what
//! ⌘K's Find rows find across one project's Changes, `Read` and `Version`
//! kinds included — against synthetic Claude Code recordings ingested through
//! the real ingest path. Nothing here opens a real store.

mod scratch;

use std::path::PathBuf;

use scratch::TempTree;
use serde_json::{json, Value};
use thruline_core::hq;
use thruline_core::hq::search::{Corpus, FindKind, RecordSearch};
use thruline_core::ingest::ingest;
use thruline_core::Ledger;

const ROOT: &str = "/work/shop";
const CANARY: &str = "ghp_TLCANARYaaZZ11bbYY22ccXX33ddWW44e";

/// One recorded step of a synthetic Claude Code session.
enum Step {
    Prompt(&'static str),
    Say(&'static str),
    /// A tool call and, when `Some`, its result.
    Tool(&'static str, Value, Option<&'static str>),
}

fn session(dir: &std::path::Path, id: &str, day: &str, steps: &[Step]) {
    let mut lines = Vec::new();
    let mut n = 0u32;
    let line = |kind: &str, message: Value, n: &mut u32| {
        *n += 1;
        json!({
            "parentUuid": null, "isSidechain": false,
            "uuid": format!("{id}-{n:04}"),
            "timestamp": format!("{day}T09:{:02}:{:02}.000Z", *n / 60, *n % 60),
            "cwd": ROOT, "sessionId": id, "version": "2.1.0", "userType": "external",
            "type": kind, "message": message,
        })
        .to_string()
    };
    for (k, step) in steps.iter().enumerate() {
        match step {
            Step::Prompt(text) => {
                lines.push(line("user", json!({"role": "user", "content": text}), &mut n))
            }
            Step::Say(text) => lines.push(line(
                "assistant",
                json!({"role": "assistant", "model": "m", "content": [{"type": "text", "text": text}]}),
                &mut n,
            )),
            Step::Tool(name, input, result) => {
                let tu = format!("{id}-tu{k}");
                lines.push(line(
                    "assistant",
                    json!({"role": "assistant", "model": "m", "content": [
                        {"type": "tool_use", "id": tu, "name": name, "input": input}]}),
                    &mut n,
                ));
                if let Some(r) = result {
                    lines.push(line(
                        "user",
                        json!({"role": "user", "content": [
                            {"type": "tool_result", "tool_use_id": tu, "content": r, "is_error": false}]}),
                        &mut n,
                    ));
                }
            }
        }
    }
    std::fs::write(dir.join(format!("{id}.jsonl")), lines.join("\n") + "\n").expect("write");
}

struct Store {
    _tmp: TempTree,
    db: PathBuf,
}

/// Two Changes in one project: fixing the checkout total (thread A) and
/// writing the migration guide (thread B), a day apart.
fn store() -> Store {
    let tmp = TempTree::new();
    let dir = tmp.path().join("claude/-work-shop");
    std::fs::create_dir_all(&dir).expect("dir");
    let read = |p: &str| json!({ "file_path": format!("{ROOT}/{p}") });
    session(
        &dir,
        "cccccccc-5a5a-4a5a-8a5a-000000000001",
        "2026-10-01",
        &[
            Step::Prompt(
                "Fix the checkout total in `cart/totals.py`; the discount is applied twice.",
            ),
            Step::Tool("Read", read("cart/totals.py"), Some("def total(cart): ...")),
            Step::Tool("Read", read("cart/totals.py"), Some("def total(cart): ...")),
            Step::Tool("Read", json!({"path": "nowhere"}), Some("no file")),
            Step::Tool(
                "Edit",
                json!({"file_path": format!("{ROOT}/cart/totals.py"), "old_string": "a", "new_string": "b"}),
                Some("The file has been updated successfully."),
            ),
            Step::Tool(
                "Bash",
                json!({"command": "pytest -q tests/test_totals.py"}),
                Some("....\n4 passed in 0.12s"),
            ),
            Step::Tool(
                "Bash",
                json!({"command": "git add -A && git commit -m \"Apply the discount once\""}),
                Some("[main 3f9a1c2] Apply the discount once\n 1 file changed, 1 insertion(+)"),
            ),
            Step::Say("Checkout total fixed. All 4 tests pass."),
            Step::Tool(
                "Bash",
                json!({"command": "rm -rf .pytest_cache && pytest -q"}),
                None,
            ),
        ],
    );
    session(
        &dir,
        "cccccccc-5a5a-4a5a-8a5a-000000000002",
        "2026-10-02",
        &[
            Step::Prompt("Write the migration guide in docs/migrate.md."),
            Step::Tool("Read", read("docs/old-guide.md"), Some("# Old guide")),
            Step::Tool(
                "Write",
                json!({"file_path": format!("{ROOT}/docs/migrate.md"), "content": "# Migrating\n"}),
                Some("File created successfully."),
            ),
            Step::Tool(
                "Bash",
                json!({"command": "git commit -m \"Add the migration guide\""}),
                Some("[main 9e8d7c6] Add the migration guide\n 1 file changed"),
            ),
            Step::Say("Done. I added the migration guide to `docs/migrate.md`."),
            Step::Prompt("Also rotate the token ghp_TLCANARYaaZZ11bbYY22ccXX33ddWW44e before you publish it."),
            Step::Say("Understood."),
        ],
    );
    let db = tmp.path().join("ledger.db");
    let mut ledger = Ledger::open(&db).expect("open");
    ingest(&mut ledger, &[dir], &tmp.path().join("mirror")).expect("ingest");
    Store { _tmp: tmp, db }
}

fn now() -> i64 {
    hq::parse_ms("2026-10-07T00:00:00Z").expect("instant")
}

struct Searchable {
    _store: Store,
    ledger: Ledger,
    index: hq::Index,
    corpus: Corpus,
}

fn searchable() -> Searchable {
    let store = store();
    let ledger = Ledger::open(&store.db).expect("open");
    let projects = hq::hq_projects(&ledger).expect("projects");
    assert_eq!(projects.len(), 1, "one project");
    let index = hq::index(&ledger, &projects[0]).expect("index");
    let corpus = Corpus::build(&ledger, &index).expect("corpus");
    Searchable {
        _store: store,
        ledger,
        index,
        corpus,
    }
}

/// The thread whose title starts with `prefix`.
fn thread(s: &Searchable, prefix: &str) -> String {
    s.index
        .view(now())
        .threads
        .iter()
        .find(|t| t.title.starts_with(prefix))
        .unwrap_or_else(|| panic!("a thread titled {prefix}…"))
        .id
        .clone()
}

fn kinds(doc: &RecordSearch) -> Vec<(FindKind, String, String)> {
    doc.results
        .iter()
        .map(|h| (h.kind, h.thread_id.clone(), h.text.clone()))
        .collect()
}

#[test]
fn a_file_the_agent_read_is_found_with_its_change_and_how_often() {
    let s = searchable();
    let a = thread(&s, "Fix the checkout total");
    let b = thread(&s, "Write the migration guide");
    assert_ne!(a, b, "two Changes");

    let doc = s.corpus.search("totals.py", 50, None);
    let read = doc
        .results
        .iter()
        .find(|h| h.kind == FindKind::Read)
        .unwrap_or_else(|| panic!("a Read hit: {:?}", kinds(&doc)));
    assert_eq!(read.text, "cart/totals.py", "relative to the project root");
    assert_eq!(read.thread_id, a);
    assert_eq!(read.count, 2, "read twice, found once");
    assert_eq!(read.source, "claude_code");
    assert!(read.at.is_some() && !read.event_id.is_empty());
    for k in [FindKind::Request, FindKind::File, FindKind::Run] {
        assert!(
            doc.results.iter().any(|h| h.kind == k),
            "{k:?} in {:?}",
            kinds(&doc)
        );
    }

    // the other Change's reads are found too
    let doc = s.corpus.search("old-guide", 50, None);
    let read = doc
        .results
        .iter()
        .find(|h| h.kind == FindKind::Read)
        .expect("Read in B");
    assert_eq!(
        (read.thread_id.as_str(), read.text.as_str()),
        (b.as_str(), "docs/old-guide.md")
    );
    assert_eq!(
        s.corpus
            .search("old-guide", 50, None)
            .searched
            .reads_without_a_path,
        1
    );
}

#[test]
fn a_version_is_found_by_its_commit_and_by_its_message() {
    let s = searchable();
    let a = thread(&s, "Fix the checkout total");
    let b = thread(&s, "Write the migration guide");

    let doc = s.corpus.search("3f9a1c2", 50, None);
    let v = doc
        .results
        .iter()
        .find(|h| h.kind == FindKind::Version)
        .unwrap_or_else(|| panic!("a Version hit: {:?}", kinds(&doc)));
    assert_eq!(v.thread_id, a);
    assert_eq!(v.text, "3f9a1c2");
    let ver = v.version.as_ref().expect("version");
    assert_eq!(ver.short.as_deref(), Some("3f9a1c2"));
    assert!(ver.committed);
    assert_eq!(ver.message.as_deref(), Some("Apply the discount once"));
    assert!(
        !ver.current,
        "rows follow the commit, so a later version is current"
    );

    let doc = s.corpus.search("migration guide", 50, None);
    let v = doc
        .results
        .iter()
        .find(|h| h.kind == FindKind::Version && h.text == "9e8d7c6")
        .unwrap_or_else(|| panic!("found by its message: {:?}", kinds(&doc)));
    assert_eq!(v.thread_id, b);

    // every thread's current version is findable by its id, and marked current
    for t in [&a, &b] {
        let doc = s.corpus.search("wt-", 100, None);
        assert!(
            doc.results.iter().any(|h| h.kind == FindKind::Version
                && &h.thread_id == t
                && h.version
                    .as_ref()
                    .is_some_and(|v| v.current && !v.committed)),
            "{t}: {:?}",
            kinds(&doc)
        );
    }
}

#[test]
fn the_open_changes_own_rows_are_left_to_its_view_but_reads_and_versions_are_not() {
    let s = searchable();
    let a = thread(&s, "Fix the checkout total");
    let open = s.corpus.search("totals", 100, Some(&a));
    assert_eq!(open.thread.as_deref(), Some(a.as_str()));
    // only what the agent read and each version: named here, not asked of the code under test
    let mine: Vec<FindKind> = open
        .results
        .iter()
        .filter(|h| h.thread_id == a)
        .map(|h| h.kind)
        .collect();
    assert!(!mine.is_empty(), "the open Change's reads are still served");
    for k in mine {
        assert!(
            matches!(k, FindKind::Read | FindKind::Version),
            "{k:?} of the open Change is the view's to draw"
        );
    }
    for k in [
        FindKind::Request,
        FindKind::Said,
        FindKind::File,
        FindKind::Run,
        FindKind::Command,
    ] {
        assert!(k.in_thread_view(), "{k:?}");
    }
    assert!(!FindKind::Read.in_thread_view() && !FindKind::Version.in_thread_view());
    assert!(open
        .results
        .iter()
        .any(|h| h.kind == FindKind::Read && h.open_thread));
    let versions = s.corpus.search("3f9a1c2", 100, Some(&a));
    assert!(versions
        .results
        .iter()
        .any(|h| h.kind == FindKind::Version && h.open_thread));

    // the other Change keeps every kind, and the open one's hits come first
    let doc = s.corpus.search("guide", 100, Some(&a));
    for k in [
        FindKind::Request,
        FindKind::Read,
        FindKind::Said,
        FindKind::Version,
    ] {
        assert!(
            doc.results.iter().any(|h| h.kind == k && !h.open_thread),
            "{k:?}: {:?}",
            kinds(&doc)
        );
    }
    let doc = s.corpus.search("migrate.md", 100, Some(&a));
    assert!(
        doc.results
            .iter()
            .any(|h| h.kind == FindKind::File && !h.open_thread),
        "{:?}",
        kinds(&doc)
    );
    let all = s.corpus.search("a", 100, Some(&a));
    let first_other = all
        .results
        .iter()
        .position(|h| !h.open_thread)
        .unwrap_or(all.results.len());
    assert!(
        all.results[first_other..].iter().all(|h| !h.open_thread),
        "open Change first"
    );
}

#[test]
fn a_masked_secret_is_never_found_and_what_was_masked_is_counted() {
    let s = searchable();
    let doc = s.corpus.search(CANARY, 50, None);
    assert_eq!(doc.total, 0, "{:?}", kinds(&doc));
    assert_eq!(s.corpus.search("TLCANARY", 50, None).total, 0);

    let doc = s.corpus.search("rotate the token", 50, None);
    let hit = doc
        .results
        .iter()
        .find(|h| h.kind == FindKind::Request)
        .unwrap_or_else(|| panic!("the prompt: {:?}", kinds(&doc)));
    assert!(hit.text.contains("[masked:"), "{}", hit.text);
    assert!(!hit.text.contains("TLCANARY"), "{}", hit.text);
    assert_eq!(
        hit.work,
        Some(false),
        "a prompt that did no work is conversation"
    );
    assert!(doc.masking.masked >= 1, "{:?}", doc.masking);
    assert!(doc.masking.families.contains("github"), "{:?}", doc.masking);
    let served = serde_json::to_string(&doc).expect("json");
    assert!(!served.contains("TLCANARY"), "{served}");
}

#[test]
fn total_by_kind_and_limit_agree() {
    let s = searchable();
    let doc = s.corpus.search("a", 3, None);
    assert_eq!(doc.results.len(), 3);
    assert_eq!(doc.limit, 3);
    assert!(doc.total > 3, "{}", doc.total);
    assert_eq!(doc.by_kind.values().sum::<u64>(), doc.total);
    assert_eq!(
        doc.by_kind.len(),
        FindKind::ALL.len(),
        "every kind is present"
    );
    let all = s.corpus.search("a", 1_000, None);
    assert_eq!(all.limit, hq::search::MAX_LIMIT);
    assert_eq!(all.total, doc.total);
    // newest first
    let ats: Vec<i64> = all
        .results
        .iter()
        .map(|h| hq::parse_ms(h.at.as_deref().unwrap_or("")).unwrap_or(0))
        .collect();
    assert!(ats.windows(2).all(|w| w[0] >= w[1]), "{ats:?}");
}

#[test]
fn every_hit_names_a_change_of_the_project_by_its_inbox_title() {
    let s = searchable();
    let view = s.index.view(now());
    let doc = s.corpus.search("a", 100, None);
    assert!(!doc.results.is_empty());
    for h in &doc.results {
        let t = view
            .threads
            .iter()
            .find(|t| t.id == h.thread_id)
            .unwrap_or_else(|| panic!("{} is a thread of the project", h.thread_id));
        assert_eq!(h.thread_title, t.title);
    }
}

#[test]
fn a_statement_carries_the_verdict_the_statements_route_serves() {
    let s = searchable();
    let statements = s.index.statements();
    let doc = s.corpus.search("tests pass", 50, None);
    let said: Vec<_> = doc
        .results
        .iter()
        .filter(|h| h.kind == FindKind::Said)
        .collect();
    assert!(!said.is_empty(), "{:?}", kinds(&doc));
    for h in said {
        let row = statements
            .claims
            .iter()
            .find(|c| c.id == h.event_id)
            .expect("the same claim");
        assert_eq!(Some(row.verdict), h.verdict);
        assert_eq!(Some(row.kind), h.claim_kind);
    }
}

#[test]
fn a_run_and_a_command_say_how_they_ended() {
    let s = searchable();
    let doc = s.corpus.search("pytest", 50, None);
    let run = doc
        .results
        .iter()
        .find(|h| h.kind == FindKind::Run && h.text == "pytest -q tests/test_totals.py")
        .unwrap_or_else(|| panic!("{:?}", kinds(&doc)));
    assert_eq!(run.outcome, Some(hq::Outcome::Succeeded));
    let t = run.tally.expect("pytest's own summary");
    assert_eq!((t.passed, t.failed), (4, 0));
    let pending = doc
        .results
        .iter()
        .find(|h| h.text.starts_with("rm -rf"))
        .unwrap_or_else(|| panic!("{:?}", kinds(&doc)));
    assert_eq!(
        pending.outcome,
        Some(hq::Outcome::Unknown),
        "no result yet, never a pass"
    );
}

#[test]
fn the_match_rule_is_the_palettes() {
    let s = searchable();
    // "was a migration run?" searches for "migration" only
    let q = s.corpus.search("was a migration run?", 100, None);
    assert!(q.total > 0);
    for h in &q.results {
        let text = format!(
            "{} {}",
            h.text,
            h.version
                .as_ref()
                .and_then(|v| v.message.clone())
                .unwrap_or_default()
        );
        assert!(text.to_lowercase().contains("migrat"), "{text}");
    }
    // a word under four characters is never searched on its own
    assert_eq!(
        s.corpus.search("zzz guide", 100, None).total,
        s.corpus.search("guide", 100, None).total
    );
    assert_eq!(s.corpus.search("   ", 100, None).total, 0);
}

#[test]
fn the_same_record_and_query_give_the_same_answer_and_nothing_is_written() {
    let s = searchable();
    let changes: i64 = s
        .ledger
        .conn()
        .query_row("SELECT total_changes()", [], |r| r.get(0))
        .expect("changes");
    let again = Corpus::build(&s.ledger, &s.index).expect("again");
    for q in ["totals", "guide", "3f9a1c2", "a"] {
        let x = serde_json::to_string(&s.corpus.search(q, 20, None)).expect("json");
        let y = serde_json::to_string(&again.search(q, 20, None)).expect("json");
        assert_eq!(x, y, "{q}");
    }
    let after: i64 = s
        .ledger
        .conn()
        .query_row("SELECT total_changes()", [], |r| r.get(0))
        .expect("changes");
    assert_eq!(changes, after, "building and searching wrote nothing");
}

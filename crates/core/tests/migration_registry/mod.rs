//! One directory read in place of five hand-written lists (R-2026-09-24-21).
//!
//! Every migration test in this layer asks the same question — "did MY task
//! alter anything that was already there?" — and to ask it, each one had to
//! know which tables tasks *after* it had added. Each carried its own
//! `ADDED_BY_LATER_TASKS` array. Five copies of one fact, maintained by hand:
//! a task that added a table and updated four of the five lists turned the
//! fifth red for a reason that had nothing to do with the fifth task, and a
//! task that updated none of them would have been caught only by whichever
//! test happened to run.
//!
//! The facts now live once, in `crates/core/migrations/` — one small file per
//! DDL task, naming the task, its place in the order, the objects it added,
//! and the commit and DDL digest its migration test pinned. This module reads
//! that directory and answers the one question the tests ask.
//!
//! A task that renames an object adds no table, and says so in its own file:
//! `[[renames]]` names what became what, the guard checks the new name is in
//! this tree's DDL and the old one is gone, and `tables_added_after` is
//! unaffected — a rename adds nothing (R-2026-09-24-22).
//!
//! It is deliberately loud: a malformed file, a duplicate `order`, a filename
//! whose numeric prefix disagrees with its `order`, or a table claimed by two
//! files is a panic with the offending name in it, not a silently smaller set.

#![allow(dead_code)]

use serde::Deserialize;

/// One object a task renamed: `kind` is `table`, `index` or `trigger`.
///
/// A rename is not an addition and not a removal; it is the same object under
/// a new name, and the registry says so in one place so that no later reader
/// has to infer it from two files disagreeing.
#[derive(Debug, Clone, Deserialize)]
pub struct Rename {
    pub kind: String,
    pub from: String,
    pub to: String,
}

/// One DDL task, as `crates/core/migrations/<order>-<task>.toml` states it.
#[derive(Debug, Clone, Deserialize)]
pub struct Entry {
    /// The task card's ID, e.g. `T-3-2b`.
    pub task: String,
    /// Where the task sits in the order the tasks landed.
    pub order: u32,
    /// The commit the task was built on, as its migration test pins it. Empty
    /// when the task has no migration test of its own — a stated debt.
    #[serde(default)]
    pub built_on_commit: String,
    /// The pre-task DDL digest its migration test pins. Empty for the same
    /// reason.
    #[serde(default)]
    pub pinned_schema_digest: String,
    /// The file that is this task's own compatibility evidence, if one exists.
    #[serde(default)]
    pub migration_test: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub tables: Vec<String>,
    #[serde(default)]
    pub indexes: Vec<String>,
    #[serde(default)]
    pub triggers: Vec<String>,
    /// The objects this task renamed, if it renamed any.
    #[serde(default)]
    pub renames: Vec<Rename>,
    /// The file this entry was read from, for error messages.
    #[serde(skip)]
    pub source_file: String,
}

/// Where the registry lives.
pub fn registry_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations")
}

/// Every task in the registry, in the order they landed.
///
/// Panics — with the file name in the message — on anything that would make
/// the answer quietly wrong.
pub fn entries() -> Vec<Entry> {
    let dir = registry_dir();
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| {
            panic!(
                "the DDL task registry at {} must be readable: {e}",
                dir.display()
            )
        })
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("toml"))
        .collect();
    files.sort();
    assert!(
        !files.is_empty(),
        "the DDL task registry at {} is empty — no migration test can know what later tasks added",
        dir.display()
    );

    let mut out: Vec<Entry> = Vec::new();
    for path in files {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("a registry filename must be UTF-8")
            .to_string();
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("registry file {name} must be readable: {e}"));
        let mut entry: Entry = toml::from_str(&text).unwrap_or_else(|e| {
            panic!("registry file {name} is not a well-formed task record: {e}")
        });
        entry.source_file = name.clone();

        // The filename's numeric prefix is how a human reads the order; the
        // `order` field is how the tests read it. They must agree, or the
        // directory listing lies about the sequence.
        let prefix: String = name.chars().take_while(|c| c.is_ascii_digit()).collect();
        let prefix: u32 = prefix.parse().unwrap_or_else(|_| {
            panic!("registry file {name} must begin with its order, e.g. 040-t-3-2b.toml")
        });
        assert_eq!(
            prefix, entry.order,
            "registry file {name} says order = {} but its filename says {prefix}",
            entry.order
        );
        assert!(
            !entry.tables.is_empty() || !entry.renames.is_empty(),
            "registry file {name} claims no table and no rename — a DDL task that \
             did neither does not belong here"
        );
        out.push(entry);
    }

    out.sort_by_key(|e| e.order);
    for pair in out.windows(2) {
        assert_ne!(
            pair[0].order, pair[1].order,
            "two registry files share order {}: {} and {}",
            pair[0].order, pair[0].source_file, pair[1].source_file
        );
    }

    // One table, one owner. Two files claiming a table would make
    // `tables_added_after` right by accident and the guard's "exactly one"
    // meaningless.
    let mut seen: Vec<(String, String)> = Vec::new();
    for entry in &out {
        for table in &entry.tables {
            if let Some((_, first)) = seen.iter().find(|(t, _)| t == table) {
                panic!(
                    "table `{table}` is claimed by two registry files: {first} and {}",
                    entry.source_file
                );
            }
            seen.push((table.clone(), entry.source_file.clone()));
        }
    }

    out
}

fn entry_for(task: &str) -> Entry {
    entries()
        .into_iter()
        .find(|e| e.task == task)
        .unwrap_or_else(|| {
            panic!(
                "no registry file names the task `{task}` — add one under crates/core/migrations/"
            )
        })
}

/// The tables tasks AFTER `task` added, sorted.
///
/// This is the set each migration test used to spell out by hand.
pub fn tables_added_after(task: &str) -> Vec<String> {
    let cutoff = entry_for(task).order;
    let mut out: Vec<String> = entries()
        .into_iter()
        .filter(|e| e.order > cutoff)
        .flat_map(|e| e.tables)
        .collect();
    out.sort();
    out
}

/// Every object (table, index, trigger) tasks AFTER `task` added, sorted.
///
/// For the migration tests that compare whole object sets rather than tables
/// (T-4-1's and T-4-2's), so a later task's index and triggers are read from
/// its registry file instead of being copied into their hand-written lists.
pub fn objects_added_after(task: &str) -> Vec<String> {
    let cutoff = entry_for(task).order;
    let mut out: Vec<String> = entries()
        .into_iter()
        .filter(|e| e.order > cutoff)
        .flat_map(|e| {
            e.tables
                .into_iter()
                .chain(e.indexes)
                .chain(e.triggers)
                .collect::<Vec<_>>()
        })
        .collect();
    out.sort();
    out
}

/// Every rename any task in the registry recorded, oldest task first.
pub fn renames() -> Vec<Rename> {
    entries().into_iter().flat_map(|e| e.renames).collect()
}

/// The tables `task` itself added, sorted.
pub fn tables_added_by(task: &str) -> Vec<String> {
    let mut out = entry_for(task).tables;
    out.sort();
    out
}

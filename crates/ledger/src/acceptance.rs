//! Append-only acceptance history, scoped to a repository namespace.
//!
//! This is local tamper evidence. A process that controls the entire database
//! can replace a chain; these rows do not assert external identity or custody.

use rusqlite::{params, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};

use crate::{Ledger, LedgerError, RepoId};

const GENESIS: &str = "thruline.acceptance.history.v1.genesis";
const MAX_DOCUMENT: usize = 65_536;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revision {
    pub seq: u64,
    pub document: String,
    pub source_commit: Option<String>,
    pub source_tree: Option<String>,
    pub accepted_by: String,
    pub accepted_at: String,
    pub prev_hash: String,
    pub hash: String,
}

fn hash(repo: &RepoId, r: &Revision) -> String {
    // Length framing and explicit option presence prevent separator/null
    // ambiguity. Hash the exact document bytes and all acceptance metadata.
    let seq = r.seq.to_string();
    let source_present = if r.source_commit.is_some() {
        "present"
    } else {
        "absent"
    };
    let mut bytes = Vec::new();
    for value in [
        "thruline.acceptance.history.v1",
        repo.as_str(),
        &seq,
        &r.document,
        source_present,
        r.source_commit.as_deref().unwrap_or(""),
        &r.accepted_by,
        &r.accepted_at,
        &r.prev_hash,
    ] {
        bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
        bytes.extend_from_slice(value.as_bytes());
    }
    if let Some(tree) = &r.source_tree {
        for value in ["thruline.acceptance.baseline.v1", tree.as_str()] {
            bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
            bytes.extend_from_slice(value.as_bytes());
        }
    }
    thruline_core::chain::sha256_hex(&bytes)
}

impl Ledger {
    /// Returns only a verified history. Broken history is never an empty one.
    pub fn acceptance_history(&self, repo: &RepoId) -> Result<Vec<Revision>, LedgerError> {
        if self.schema_version()? < 3 {
            return Ok(Vec::new());
        }
        let sql = if self.schema_version()? >= 4 {
            "SELECT seq, document, source_commit, accepted_by, accepted_at, prev_hash, hash, source_tree FROM acceptance_revisions WHERE repo_id = ?1 ORDER BY seq"
        } else {
            "SELECT seq, document, source_commit, accepted_by, accepted_at, prev_hash, hash, NULL FROM acceptance_revisions WHERE repo_id = ?1 ORDER BY seq"
        };
        let mut statement = self.conn().prepare(sql)?;
        let rows = statement.query_map([repo.as_str()], |row| {
            Ok(Revision {
                seq: row.get::<_, i64>(0)? as u64,
                document: row.get(1)?,
                source_commit: row.get(2)?,
                source_tree: row.get(7)?,
                accepted_by: row.get(3)?,
                accepted_at: row.get(4)?,
                prev_hash: row.get(5)?,
                hash: row.get(6)?,
            })
        })?;
        let mut history: Vec<Revision> = Vec::new();
        for row in rows {
            let row = row?;
            let expected_previous = history.last().map_or(GENESIS, |r| r.hash.as_str());
            if row.seq != history.len() as u64 + 1
                || row.prev_hash != expected_previous
                || row.document.len() > MAX_DOCUMENT
                || row.hash != hash(repo, &row)
            {
                return Err(LedgerError::AcceptanceHistory(format!(
                    "broken revision {}",
                    row.seq
                )));
            }
            history.push(row);
        }
        Ok(history)
    }

    /// Compare-and-append prevents an acceptance made against an outdated
    /// preview from silently replacing a concurrent acceptance. Command/policy
    /// validation belongs to the caller; the store seals its exact bytes.
    pub fn accept_requirements(
        &self,
        repo: &RepoId,
        document: &str,
        source_commit: Option<&str>,
        accepted_by: &str,
        expected_previous: Option<&str>,
    ) -> Result<Revision, LedgerError> {
        self.accept_requirements_at(
            repo,
            document,
            source_commit,
            None,
            accepted_by,
            expected_previous,
        )
    }

    pub fn accept_requirements_at(
        &self,
        repo: &RepoId,
        document: &str,
        source_commit: Option<&str>,
        source_tree: Option<&str>,
        accepted_by: &str,
        expected_previous: Option<&str>,
    ) -> Result<Revision, LedgerError> {
        if document.is_empty()
            || document.len() > MAX_DOCUMENT
            || accepted_by.trim().is_empty()
            || accepted_by.len() > 128
            || accepted_by.chars().any(char::is_control)
        {
            return Err(LedgerError::AcceptanceHistory(
                "invalid document size or local actor label".into(),
            ));
        }
        let tx = Transaction::new_unchecked(self.conn(), TransactionBehavior::Immediate)?;
        let history = self.acceptance_history(repo)?;
        let previous = history.last();
        if previous.map(|r| r.hash.as_str()) != expected_previous {
            return Err(LedgerError::AcceptanceChanged);
        }
        let mut row = Revision {
            seq: history.len() as u64 + 1,
            document: document.into(),
            source_commit: source_commit.map(str::to_string),
            source_tree: source_tree.map(str::to_string),
            accepted_by: accepted_by.into(),
            accepted_at: tx.query_row("SELECT datetime('now')", [], |row| row.get(0))?,
            prev_hash: previous.map_or_else(|| GENESIS.into(), |r| r.hash.clone()),
            hash: String::new(),
        };
        row.hash = hash(repo, &row);
        let seq_sql = i64::try_from(row.seq)
            .map_err(|_| LedgerError::AcceptanceHistory("revision sequence overflow".into()))?;
        tx.execute(
            "INSERT INTO acceptance_revisions
            (repo_id, seq, document, source_commit, accepted_by, accepted_at, prev_hash, hash, source_tree)
            VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                repo.as_str(),
                seq_sql,
                row.document,
                row.source_commit,
                row.accepted_by,
                row.accepted_at,
                row.prev_hash,
                row.hash,
                row.source_tree
            ],
        )?;
        tx.commit()?;
        Ok(row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revisions_survive_reopen_and_cannot_be_edited() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("exec.db");
        let ledger = Ledger::open(&path).unwrap();
        let repo = ledger.repo("project-a", None).unwrap();
        let first = ledger
            .accept_requirements(
                &repo,
                "{\"revision\":1}",
                Some("commit-a"),
                "local-developer",
                None,
            )
            .unwrap();
        let second = ledger
            .accept_requirements(
                &repo,
                "{\"revision\":2}",
                Some("commit-b"),
                "local-developer",
                Some(&first.hash),
            )
            .unwrap();
        assert_eq!(second.prev_hash, first.hash);
        assert!(ledger
            .conn()
            .execute("DELETE FROM acceptance_revisions", [])
            .is_err());
        assert!(ledger
            .conn()
            .execute("UPDATE acceptance_revisions SET document = '{}'", [])
            .is_err());
        drop(ledger);
        let reader = Ledger::open_read_only(&path).unwrap();
        assert_eq!(
            reader.acceptance_history(&repo).unwrap(),
            vec![first, second]
        );
        assert!(reader
            .conn()
            .execute("CREATE TABLE unwanted(id INTEGER)", [])
            .is_err());
        assert!(reader
            .acceptance_history(&RepoId::from_key("different-project"))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn stale_acceptance_cannot_overwrite_another_revision() {
        let ledger = Ledger::open_in_memory().unwrap();
        let repo = ledger.repo("project", None).unwrap();
        let first = ledger
            .accept_requirements(&repo, "v1", None, "developer", None)
            .unwrap();
        assert!(matches!(
            ledger.accept_requirements(&repo, "v2", None, "developer", None),
            Err(LedgerError::AcceptanceChanged)
        ));
        assert_eq!(ledger.acceptance_history(&repo).unwrap(), vec![first]);
    }

    #[test]
    fn broken_history_is_not_read_or_extended_as_clean() {
        let ledger = Ledger::open_in_memory().unwrap();
        let repo = ledger.repo("project", None).unwrap();
        let first = ledger
            .accept_requirements(&repo, "v1", None, "developer", None)
            .unwrap();
        ledger.conn().execute_batch("DROP TRIGGER acceptance_no_update; UPDATE acceptance_revisions SET accepted_by = 'forged'").unwrap();
        assert!(matches!(
            ledger.acceptance_history(&repo),
            Err(LedgerError::AcceptanceHistory(_))
        ));
        assert!(ledger
            .accept_requirements(&repo, "v2", None, "developer", Some(&first.hash))
            .is_err());
    }

    #[test]
    fn read_only_open_does_not_create_or_migrate_a_store() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("absent.db");
        assert!(Ledger::open_read_only(&path).is_err());
        assert!(!path.exists());
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection.execute_batch("PRAGMA user_version = 2").unwrap();
        drop(connection);
        let ledger = Ledger::open_read_only(&path).unwrap();
        assert_eq!(ledger.schema_version().unwrap(), 2);
        assert!(ledger
            .acceptance_history(&RepoId::from_key("project"))
            .unwrap()
            .is_empty());
        let n: i64 = ledger
            .conn()
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn v3_history_migrates_without_inventing_an_accepted_tree() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("exec.db");
        let ledger = Ledger::open(&path).unwrap();
        let repo = ledger.repo("project", None).unwrap();
        let old = ledger
            .accept_requirements(&repo, "v1", None, "developer", None)
            .unwrap();
        ledger
            .conn()
            .execute_batch(
                "ALTER TABLE acceptance_revisions DROP COLUMN source_tree; PRAGMA user_version=3",
            )
            .unwrap();
        drop(ledger);
        let reader = Ledger::open_read_only(&path).unwrap();
        assert_eq!(reader.schema_version().unwrap(), 3);
        assert_eq!(reader.acceptance_history(&repo).unwrap(), vec![old.clone()]);
        drop(reader);
        let writer = Ledger::open(&path).unwrap();
        let next = writer
            .accept_requirements_at(
                &repo,
                "v2",
                None,
                Some("accepted-tree"),
                "developer",
                Some(&old.hash),
            )
            .unwrap();
        assert_eq!(writer.acceptance_history(&repo).unwrap(), vec![old, next]);
        writer.conn().execute_batch("DROP TRIGGER acceptance_no_update; UPDATE acceptance_revisions SET source_tree='forged' WHERE seq=2").unwrap();
        assert!(writer.acceptance_history(&repo).is_err());
    }
}

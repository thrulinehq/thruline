//! The replay store: one SQLite file recording every licence issued and every
//! event that asked for one.
//!
//! Two keys, and both matter:
//!
//! - **(provider, event id)** — a provider re-delivers the same event when it
//!   did not see a success. That delivery gets the licence already issued.
//! - **(provider, order id)** — a provider can send a *different* event for an
//!   order already paid (a re-sent notification, a second webhook endpoint).
//!   That event gets the licence already issued, too, and no second
//!   `licence_id` is minted. Keying on the event alone would hand one payment
//!   two licences.
//!
//! Both tables are append-only (triggers refuse `UPDATE` and `DELETE`), and
//! every write for one event is one `BEGIN IMMEDIATE` transaction: a refusal
//! anywhere before the commit leaves the file exactly as it was.
//!
//! Versioned: `PRAGMA user_version` is [`SCHEMA_VERSION`]. A file with no
//! version is created; a file from a newer issuer is refused, never guessed at.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use sha2::{Digest, Sha256};

use crate::{licence_id_of, Issued, PaidOrder, Refusal};

/// The schema this build writes and reads.
pub const SCHEMA_VERSION: i64 = 1;

/// Version 1, exactly as created. A compatibility test builds a file from this
/// text by hand and holds [`Store::open`] to reading it.
pub const SCHEMA_V1: &str = "
CREATE TABLE licences (
    provider       TEXT NOT NULL,
    order_id       TEXT NOT NULL,
    licence_id     TEXT NOT NULL UNIQUE,
    first_event_id TEXT NOT NULL,
    licence        BLOB NOT NULL,
    issued_at      INTEGER NOT NULL,
    PRIMARY KEY (provider, order_id)
);
CREATE TABLE events (
    provider    TEXT NOT NULL,
    event_id    TEXT NOT NULL,
    order_id    TEXT NOT NULL,
    received_at INTEGER NOT NULL,
    PRIMARY KEY (provider, event_id),
    FOREIGN KEY (provider, order_id) REFERENCES licences (provider, order_id)
);
CREATE TRIGGER licences_no_update BEFORE UPDATE ON licences
    BEGIN SELECT RAISE(ABORT, 'licences is append-only'); END;
CREATE TRIGGER licences_no_delete BEFORE DELETE ON licences
    BEGIN SELECT RAISE(ABORT, 'licences is append-only'); END;
CREATE TRIGGER events_no_update BEFORE UPDATE ON events
    BEGIN SELECT RAISE(ABORT, 'events is append-only'); END;
CREATE TRIGGER events_no_delete BEFORE DELETE ON events
    BEGIN SELECT RAISE(ABORT, 'events is append-only'); END;
";

/// Why the store could not be opened.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("the issued-licence store could not be opened: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error(
        "the issued-licence store was written by a newer issuer (schema {found}, this one reads {SCHEMA_VERSION}); nothing was changed"
    )]
    Newer { found: i64 },
}

/// The store, open.
pub struct Store {
    conn: Connection,
}

fn store_err(e: rusqlite::Error) -> Refusal {
    Refusal::Store(e.to_string())
}

impl Store {
    /// Open (or create) the store at `path`.
    pub fn open(path: &Path) -> Result<Store, StoreError> {
        let mut conn = Connection::open(path)?;
        // Rollback journal, not WAL: a committed write is in the file itself,
        // so "the file's bytes did not change" is a real test of "nothing was
        // written".
        conn.pragma_update(None, "journal_mode", "DELETE")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        let found: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        match found {
            0 => {
                let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
                tx.execute_batch(SCHEMA_V1)?;
                tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
                tx.commit()?;
            }
            SCHEMA_VERSION => {}
            newer => return Err(StoreError::Newer { found: newer }),
        }
        Ok(Store { conn })
    }

    /// Issue at most once per order. Returns the licence already issued for
    /// this event or this order when there is one; otherwise calls `mint` and
    /// records what it returns. If `mint` refuses, nothing is written.
    pub(crate) fn issue_once<F>(
        &mut self,
        order: &PaidOrder,
        now: i64,
        mint: F,
    ) -> Result<Issued, Refusal>
    where
        F: FnOnce(&PaidOrder) -> Result<Vec<u8>, Refusal>,
    {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_err)?;
        let replay = |licence: Vec<u8>, order_id: String, licence_id: String| Issued {
            provider: order.provider,
            order_id,
            licence_id,
            licence,
            replayed: true,
        };

        // 1. The same event again.
        let by_event: Option<(Vec<u8>, String, String)> = tx
            .query_row(
                "SELECT l.licence, l.order_id, l.licence_id FROM events e
                 JOIN licences l ON l.provider = e.provider AND l.order_id = e.order_id
                 WHERE e.provider = ?1 AND e.event_id = ?2",
                params![order.provider, order.event_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(store_err)?;
        if let Some((licence, order_id, licence_id)) = by_event {
            return Ok(replay(licence, order_id, licence_id)); // nothing written
        }

        // 2. A different event for an order already paid.
        let by_order: Option<(Vec<u8>, String)> = tx
            .query_row(
                "SELECT licence, licence_id FROM licences WHERE provider = ?1 AND order_id = ?2",
                params![order.provider, order.order_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(store_err)?;
        if let Some((licence, licence_id)) = by_order {
            tx.execute(
                "INSERT INTO events (provider, event_id, order_id, received_at) VALUES (?1, ?2, ?3, ?4)",
                params![order.provider, order.event_id, order.order_id, now],
            )
            .map_err(store_err)?;
            tx.commit().map_err(store_err)?;
            return Ok(replay(licence, order.order_id.clone(), licence_id));
        }

        // 3. A new order: mint, then record the licence and the event together.
        let licence = mint(order)?;
        let licence_id = licence_id_of(&licence)
            .ok_or_else(|| Refusal::Seal("the licence carries no licence_id".into()))?;
        tx.execute(
            "INSERT INTO licences (provider, order_id, licence_id, first_event_id, licence, issued_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                order.provider,
                order.order_id,
                licence_id,
                order.event_id,
                licence,
                now
            ],
        )
        .map_err(store_err)?;
        tx.execute(
            "INSERT INTO events (provider, event_id, order_id, received_at) VALUES (?1, ?2, ?3, ?4)",
            params![order.provider, order.event_id, order.order_id, now],
        )
        .map_err(store_err)?;
        tx.commit().map_err(store_err)?;
        Ok(Issued {
            provider: order.provider,
            order_id: order.order_id.clone(),
            licence_id,
            licence,
            replayed: false,
        })
    }

    /// How many licences have been issued.
    pub fn licences(&self) -> Result<u64, rusqlite::Error> {
        self.count("licences")
    }

    /// How many events have asked for one.
    pub fn events(&self) -> Result<u64, rusqlite::Error> {
        self.count("events")
    }

    fn count(&self, table: &str) -> Result<u64, rusqlite::Error> {
        let n: i64 = self
            .conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?;
        Ok(u64::try_from(n).unwrap_or(0))
    }

    /// SHA-256 over every row of both tables in key order, so "the store is
    /// unchanged" is one comparison.
    pub fn digest(&self) -> Result<String, rusqlite::Error> {
        let mut h = Sha256::new();
        let mut rows = self.conn.prepare(
            "SELECT provider, order_id, licence_id, first_event_id, licence, issued_at
             FROM licences ORDER BY provider, order_id",
        )?;
        let mut it = rows.query([])?;
        h.update(b"licences\0");
        while let Some(r) = it.next()? {
            for i in 0..4 {
                h.update(r.get::<_, String>(i)?.as_bytes());
                h.update(b"\0");
            }
            h.update(r.get::<_, Vec<u8>>(4)?);
            h.update(b"\0");
            h.update(r.get::<_, i64>(5)?.to_le_bytes());
        }
        let mut rows = self.conn.prepare(
            "SELECT provider, event_id, order_id, received_at FROM events ORDER BY provider, event_id",
        )?;
        let mut it = rows.query([])?;
        h.update(b"events\0");
        while let Some(r) = it.next()? {
            for i in 0..3 {
                h.update(r.get::<_, String>(i)?.as_bytes());
                h.update(b"\0");
            }
            h.update(r.get::<_, i64>(3)?.to_le_bytes());
        }
        Ok(thruline_licence::to_hex(&h.finalize()))
    }

    /// The licence issued for one order, if any.
    pub fn licence_for(
        &self,
        provider: &str,
        order_id: &str,
    ) -> Result<Option<Vec<u8>>, rusqlite::Error> {
        self.conn
            .query_row(
                "SELECT licence FROM licences WHERE provider = ?1 AND order_id = ?2",
                params![provider, order_id],
                |r| r.get(0),
            )
            .optional()
    }
}

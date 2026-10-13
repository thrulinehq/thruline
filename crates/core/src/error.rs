use std::path::Path;

/// Core never panics on bad input — everything recoverable is an error value.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("i/o error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("unknown event kind {0:?}")]
    UnknownEventKind(String),

    /// The ledger disagrees with the raw files. Never recovered from silently.
    #[error("integrity failure: {0}")]
    Integrity(String),

    #[error("{0}")]
    Config(String),

    /// A project removal owns this live store until its explicit release or
    /// recovery. Callers must not issue even their usual open-time pragmas.
    #[error("ThruLine is removing a project's records; try again when it finishes")]
    StoreHeld { path: String },

    /// The ledger on disk was written by a different schema and needs the full
    /// migration path, which this open refuses to pay (A193 / Q-A191.1).
    ///
    /// Every command that opens a ledger inherits this refusal, which is the
    /// point: a schema upgrade is tens of seconds of work on a real corpus, and
    /// before A193 it was charged silently to whoever happened to open the
    /// ledger first after an upgrade — including a hook with a five-second
    /// timeout. It is now paid only by `thruline upgrade`, deliberately.
    #[error(
        "schema upgrade pending — run `thruline upgrade` ({estimate}). \
         Nothing was read or written."
    )]
    UpgradePending { path: String, estimate: String },
}

impl CoreError {
    /// Another connection held the store's write lock for longer than this
    /// connection was willing to wait (`SQLITE_BUSY` / `SQLITE_LOCKED`).
    pub fn is_busy(&self) -> bool {
        match self {
            CoreError::Sqlite(rusqlite::Error::SqliteFailure(e, _)) => matches!(
                e.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            ),
            _ => false,
        }
    }

    pub fn io(path: impl AsRef<Path>, source: std::io::Error) -> Self {
        CoreError::Io {
            path: path.as_ref().display().to_string(),
            source,
        }
    }
}

pub type Result<T> = std::result::Result<T, CoreError>;

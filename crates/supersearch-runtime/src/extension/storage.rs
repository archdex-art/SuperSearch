//! # Isolated per-extension storage (ADR-003: SQLite)
//!
//! Each extension is granted a private SQLite database, physically isolated
//! on disk as `<root>/<extension_id>.sqlite`. Extension A can never read
//! Extension B's data because it is never handed a connection outside its own
//! namespace — isolation is enforced by construction, not by runtime checks.
//!
//! The store exposes a minimal, ACID-backed key/value surface (the
//! `LocalStorage` primitive from the SDK spec). Richer relational access is a
//! later milestone; the schema is intentionally forward-compatible.
//!
//! ## Why SQLite over a KV store
//! Extensions in a search product frequently need to *query* their own cached
//! data (filter, sort, search) rather than load an entire dataset into the V8
//! heap. SQLite pushes that computation to the database, keeping isolate memory
//! bounded (see ADR-003).

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension};

/// Errors surfaced by the storage layer.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// The requested extension id is not a safe filesystem slug.
    #[error("unsafe extension id for storage: `{0}`")]
    UnsafeId(String),
    /// An underlying SQLite failure.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// The storage root could not be created.
    #[error("failed to prepare storage root `{path}`: {source}")]
    Io {
        /// The path we failed to create.
        path: String,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}

/// Owns the on-disk root under which every extension's database lives and mints
/// per-extension [`ExtensionStore`] handles.
///
/// The provider itself holds no open connections — it is a cheap, cloneable
/// factory. Connections are opened lazily when an extension first touches
/// storage, so idle extensions cost zero file handles.
#[derive(Debug, Clone)]
pub struct StorageProvider {
    root: PathBuf,
}

impl StorageProvider {
    /// Create a provider rooted at `root`, creating the directory if needed.
    ///
    /// # Errors
    /// Returns [`StorageError::Io`] if the root cannot be created.
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, StorageError> {
        let root = root.into();
        std::fs::create_dir_all(&root).map_err(|source| StorageError::Io {
            path: root.display().to_string(),
            source,
        })?;
        Ok(Self { root })
    }

    /// Open (or create) the private database for `extension_id`.
    ///
    /// The id is validated as a filesystem-safe slug so a malicious id can
    /// never escape the storage root via path traversal.
    ///
    /// # Errors
    /// - [`StorageError::UnsafeId`] if the id is not a plain slug.
    /// - [`StorageError::Sqlite`] if the database cannot be opened/migrated.
    pub fn open(&self, extension_id: &str) -> Result<ExtensionStore, StorageError> {
        if !is_safe_id(extension_id) {
            return Err(StorageError::UnsafeId(extension_id.to_string()));
        }
        let path = self.root.join(format!("{extension_id}.sqlite"));
        ExtensionStore::open(&path)
    }
}

/// A single extension's private, ACID key/value database.
#[derive(Debug)]
pub struct ExtensionStore {
    conn: Connection,
}

impl ExtensionStore {
    /// Open the database at `path`, applying performance PRAGMAs and migrating
    /// the schema. WAL mode is enabled for concurrent read performance; a
    /// bounded mmap keeps hot reads off the syscall path (Phase 10 caching).
    fn open(path: &Path) -> Result<Self, StorageError> {
        let conn = Connection::open(path)?;
        // WAL: concurrent readers never block the single writer.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        // NORMAL is durable under WAL and avoids an fsync per transaction.
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        // Memory-map up to 64 MiB of the DB for zero-syscall hot reads.
        conn.pragma_update(None, "mmap_size", 64 * 1024 * 1024)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS kv (
                 key   TEXT PRIMARY KEY NOT NULL,
                 value BLOB NOT NULL
             ) WITHOUT ROWID",
            [],
        )?;
        Ok(Self { conn })
    }

    /// Store `value` under `key`, overwriting any existing entry (upsert).
    ///
    /// # Errors
    /// Returns [`StorageError::Sqlite`] on write failure.
    pub fn set(&self, key: &str, value: &[u8]) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO kv (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }

    /// Fetch the value stored under `key`, or `None` if absent.
    ///
    /// # Errors
    /// Returns [`StorageError::Sqlite`] on read failure.
    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        let value = self
            .conn
            .query_row("SELECT value FROM kv WHERE key = ?1", [key], |row| {
                row.get::<_, Vec<u8>>(0)
            })
            .optional()?;
        Ok(value)
    }

    /// Delete the entry under `key`. Returns `true` if a row was removed.
    ///
    /// # Errors
    /// Returns [`StorageError::Sqlite`] on write failure.
    pub fn remove(&self, key: &str) -> Result<bool, StorageError> {
        let affected = self.conn.execute("DELETE FROM kv WHERE key = ?1", [key])?;
        Ok(affected > 0)
    }

    /// Number of stored keys. Primarily used for telemetry and tests.
    ///
    /// # Errors
    /// Returns [`StorageError::Sqlite`] on read failure.
    pub fn len(&self) -> Result<u64, StorageError> {
        let n = self
            .conn
            .query_row("SELECT COUNT(*) FROM kv", [], |row| row.get::<_, i64>(0))?;
        Ok(n as u64)
    }

    /// Whether the store holds no keys.
    ///
    /// # Errors
    /// Returns [`StorageError::Sqlite`] on read failure.
    pub fn is_empty(&self) -> Result<bool, StorageError> {
        Ok(self.len()? == 0)
    }
}

/// A safe storage id is a non-empty ASCII slug — the same rule the manifest
/// enforces for the on-disk directory name, applied here so the two can never
/// diverge.
fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> (tempfile::TempDir, StorageProvider) {
        let dir = tempfile::tempdir().expect("tempdir");
        let provider = StorageProvider::new(dir.path()).expect("provider");
        (dir, provider)
    }

    #[test]
    fn set_get_roundtrip() {
        let (_dir, provider) = provider();
        let store = provider.open("weather").expect("open");
        store.set("last_query", b"berlin").expect("set");
        assert_eq!(
            store.get("last_query").unwrap().as_deref(),
            Some(&b"berlin"[..])
        );
    }

    #[test]
    fn upsert_overwrites() {
        let (_dir, provider) = provider();
        let store = provider.open("weather").expect("open");
        store.set("k", b"v1").unwrap();
        store.set("k", b"v2").unwrap();
        assert_eq!(store.get("k").unwrap().as_deref(), Some(&b"v2"[..]));
        assert_eq!(store.len().unwrap(), 1);
    }

    #[test]
    fn remove_reports_presence() {
        let (_dir, provider) = provider();
        let store = provider.open("weather").expect("open");
        store.set("k", b"v").unwrap();
        assert!(store.remove("k").unwrap());
        assert!(!store.remove("k").unwrap());
        assert!(store.is_empty().unwrap());
    }

    #[test]
    fn extensions_are_physically_isolated() {
        let (_dir, provider) = provider();
        let a = provider.open("alpha").expect("open a");
        let b = provider.open("beta").expect("open b");
        a.set("secret", b"only-alpha").unwrap();
        // Beta's database has no knowledge of alpha's keys.
        assert_eq!(b.get("secret").unwrap(), None);
    }

    #[test]
    fn rejects_unsafe_id() {
        let (_dir, provider) = provider();
        let err = provider.open("../escape").unwrap_err();
        assert!(matches!(err, StorageError::UnsafeId(_)));
    }
}

use std::path::Path;

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::OpenFlags;

use crate::error::{CoreError, Result};
use crate::parse::ParsedFile;
use crate::CACHE_SCHEMA_VERSION;

pub struct Cache {
    pool: Pool<SqliteConnectionManager>,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS parsed (
    hash     TEXT PRIMARY KEY,
    language INTEGER NOT NULL,
    payload  TEXT NOT NULL,
    used_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS parsed_used_at ON parsed(used_at);
";

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl Cache {
    pub fn open(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| CoreError::io(parent, e))?;
            ignore_itself(parent);
        }

        let manager = SqliteConnectionManager::file(db_path)
            .with_flags(
                OpenFlags::SQLITE_OPEN_READ_WRITE
                    | OpenFlags::SQLITE_OPEN_CREATE
                    | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .with_init(|conn| {
                conn.execute_batch(
                    "PRAGMA journal_mode = WAL;
                     PRAGMA synchronous = NORMAL;
                     PRAGMA busy_timeout = 5000;
                     PRAGMA cache_size = -8000;",
                )
            });

        let pool = Pool::builder()
            .max_size(8)
            .build(manager)
            .map_err(|e| CoreError::Cache(e.to_string()))?;

        let cache = Self { pool };
        cache.migrate()?;
        Ok(cache)
    }

    pub fn in_memory() -> Result<Self> {
        let manager = SqliteConnectionManager::memory();
        let pool = Pool::builder()
            .max_size(1)
            .build(manager)
            .map_err(|e| CoreError::Cache(e.to_string()))?;
        let cache = Self { pool };
        cache.migrate()?;
        Ok(cache)
    }

    fn conn(&self) -> Result<r2d2::PooledConnection<SqliteConnectionManager>> {
        self.pool.get().map_err(|e| CoreError::Cache(e.to_string()))
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.conn()?;
        conn.execute_batch(SCHEMA)
            .map_err(|e| CoreError::Cache(e.to_string()))?;

        let found: Option<u32> = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |r| r.get::<_, String>(0),
            )
            .ok()
            .and_then(|s| s.parse().ok());

        match found {
            Some(v) if v == CACHE_SCHEMA_VERSION => {}
            Some(v) if v > CACHE_SCHEMA_VERSION => {
                return Err(CoreError::CacheFromTheFuture {
                    found: v,
                    expected: CACHE_SCHEMA_VERSION,
                })
            }

            _ => {
                conn.execute_batch("DELETE FROM parsed;")
                    .map_err(|e| CoreError::Cache(e.to_string()))?;
                conn.execute(
                    "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', ?1)",
                    [CACHE_SCHEMA_VERSION.to_string()],
                )
                .map_err(|e| CoreError::Cache(e.to_string()))?;
            }
        }
        Ok(())
    }

    pub fn get(&self, hash: &str) -> Option<ParsedFile> {
        let conn = self.conn().ok()?;
        let payload: String = conn
            .query_row("SELECT payload FROM parsed WHERE hash = ?1", [hash], |r| {
                r.get(0)
            })
            .ok()?;
        serde_json::from_str(&payload).ok()
    }

    pub fn put(&self, hash: &str, language: u8, parsed: &ParsedFile) -> Result<()> {
        let payload = serde_json::to_string(parsed).map_err(|e| CoreError::Cache(e.to_string()))?;
        let conn = self.conn()?;
        conn.execute(
            "INSERT OR REPLACE INTO parsed (hash, language, payload, used_at)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![hash, language as i64, payload, now()],
        )
        .map_err(|e| CoreError::Cache(e.to_string()))?;
        Ok(())
    }

    pub fn put_many(&self, rows: &[(String, u8, ParsedFile)]) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn()?;
        let tx = conn
            .transaction()
            .map_err(|e| CoreError::Cache(e.to_string()))?;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT OR REPLACE INTO parsed (hash, language, payload, used_at)
                     VALUES (?1, ?2, ?3, ?4)",
                )
                .map_err(|e| CoreError::Cache(e.to_string()))?;
            let stamp = now();
            for (hash, language, parsed) in rows {
                let payload =
                    serde_json::to_string(parsed).map_err(|e| CoreError::Cache(e.to_string()))?;
                stmt.execute(rusqlite::params![hash, *language as i64, payload, stamp])
                    .map_err(|e| CoreError::Cache(e.to_string()))?;
            }
        }
        tx.commit().map_err(|e| CoreError::Cache(e.to_string()))?;
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.conn()
            .ok()
            .and_then(|c| {
                c.query_row("SELECT COUNT(*) FROM parsed", [], |r| r.get::<_, i64>(0))
                    .ok()
            })
            .unwrap_or(0) as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn evict_older_than(&self, before: i64) -> Result<usize> {
        let conn = self.conn()?;
        let n = conn
            .execute("DELETE FROM parsed WHERE used_at < ?1", [before])
            .map_err(|e| CoreError::Cache(e.to_string()))?;
        Ok(n)
    }
}

fn ignore_itself(dir: &Path) {
    let marker = dir.join(".gitignore");
    if marker.exists() {
        return;
    }
    let _ = std::fs::write(
        &marker,
        "# ZAtlas analysis cache. Regenerated on demand; safe to delete.\n*\n",
    );
}

pub fn content_hash(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{ExportItem, ImportKind, ImportedNames, RawImport};

    fn sample() -> ParsedFile {
        ParsedFile {
            imports: vec![RawImport {
                specifier: "./b".into(),
                line: 1,
                kind: ImportKind::Static,
                names: ImportedNames::Named(vec!["x".into()]),
                type_only: false,
                scope: None,
            }],
            exports: vec![ExportItem {
                name: "a".into(),
                from: None,
                line: 2,
            }],
            loc: 12,
            total_lines: 15,
            ..Default::default()
        }
    }

    #[test]
    fn a_stored_parse_comes_back_identical() {
        let cache = Cache::in_memory().unwrap();
        cache.put("hash1", 0, &sample()).unwrap();
        assert_eq!(cache.get("hash1"), Some(sample()));
    }

    #[test]
    fn a_missing_hash_returns_nothing_rather_than_failing() {
        let cache = Cache::in_memory().unwrap();
        assert_eq!(cache.get("nope"), None);
    }

    #[test]
    fn the_same_content_at_two_paths_is_one_entry() {
        let cache = Cache::in_memory().unwrap();
        let hash = content_hash(b"export const a = 1;");
        cache.put(&hash, 0, &sample()).unwrap();
        cache.put(&hash, 0, &sample()).unwrap();
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn different_content_hashes_differently() {
        assert_ne!(content_hash(b"a"), content_hash(b"b"));
        assert_eq!(content_hash(b"a"), content_hash(b"a"));
    }

    #[test]
    fn a_batch_write_stores_every_row() {
        let cache = Cache::in_memory().unwrap();
        let rows: Vec<(String, u8, ParsedFile)> =
            (0..500).map(|i| (format!("h{i}"), 0u8, sample())).collect();
        cache.put_many(&rows).unwrap();
        assert_eq!(cache.len(), 500);
        assert_eq!(cache.get("h499"), Some(sample()));
    }

    #[test]
    fn an_empty_batch_is_not_an_error() {
        let cache = Cache::in_memory().unwrap();
        cache.put_many(&[]).unwrap();
        assert!(cache.is_empty());
    }

    #[test]
    fn a_cache_survives_being_closed_and_reopened() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".zatlas/cache.sqlite");
        {
            let cache = Cache::open(&path).unwrap();
            cache.put("h", 0, &sample()).unwrap();
        }
        let reopened = Cache::open(&path).unwrap();
        assert_eq!(reopened.get("h"), Some(sample()));
    }

    #[test]
    fn opening_creates_the_directory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deep/nested/.zatlas/cache.sqlite");
        let _ = Cache::open(&path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn the_cache_directory_ignores_itself() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".zatlas/cache.sqlite");
        let _ = Cache::open(&path).unwrap();
        let marker = dir.path().join(".zatlas/.gitignore");
        assert!(marker.exists(), "the cache should ignore itself");
        assert!(std::fs::read_to_string(&marker).unwrap().contains('*'));
    }

    #[test]
    fn an_existing_gitignore_in_the_cache_directory_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join(".zatlas");
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(cache_dir.join(".gitignore"), "mine\n").unwrap();
        let _ = Cache::open(&cache_dir.join("cache.sqlite")).unwrap();
        assert_eq!(
            std::fs::read_to_string(cache_dir.join(".gitignore")).unwrap(),
            "mine\n"
        );
    }

    #[test]
    fn a_cache_from_a_newer_zatlas_is_refused_rather_than_misread() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.sqlite");
        {
            let cache = Cache::open(&path).unwrap();
            let conn = cache.conn().unwrap();
            conn.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '999')",
                [],
            )
            .unwrap();
        }
        let err = match Cache::open(&path) {
            Err(e) => e,
            Ok(_) => panic!("a newer cache must be refused"),
        };
        assert!(
            matches!(err, CoreError::CacheFromTheFuture { found: 999, .. }),
            "got {err}"
        );
    }

    #[test]
    fn a_cache_from_an_older_zatlas_is_rebuilt_rather_than_reinterpreted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.sqlite");
        {
            let cache = Cache::open(&path).unwrap();
            cache.put("h", 0, &sample()).unwrap();
            let conn = cache.conn().unwrap();
            conn.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '0')",
                [],
            )
            .unwrap();
        }
        let reopened = Cache::open(&path).unwrap();
        assert!(reopened.is_empty(), "stale rows must be dropped");
        assert_eq!(reopened.get("h"), None);
    }

    #[test]
    fn eviction_drops_only_the_old_entries() {
        let cache = Cache::in_memory().unwrap();
        cache.put("old", 0, &sample()).unwrap();
        {
            let conn = cache.conn().unwrap();
            conn.execute("UPDATE parsed SET used_at = 0 WHERE hash = 'old'", [])
                .unwrap();
        }
        cache.put("new", 0, &sample()).unwrap();
        let removed = cache.evict_older_than(now() - 1).unwrap();
        assert_eq!(removed, 1);
        assert_eq!(cache.get("new"), Some(sample()));
        assert_eq!(cache.get("old"), None);
    }
}

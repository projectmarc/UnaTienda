use std::path::{Path, PathBuf};

use chrono::Utc;
use deadpool_sqlite::{Config, Hook, Pool, Runtime};
use rusqlite::Connection;

use crate::domain::error::AppError;

use std::collections::HashSet;

const MIGRATIONS: [(i64, &str); 3] = [
    (1, include_str!("../../migrations/001_initial.sql")),
    (
        2,
        include_str!("../../migrations/002_upgrade_closed_session_immutability.sql"),
    ),
    (
        3,
        include_str!("../../migrations/003_upgrade_closed_session_insert_guards.sql"),
    ),
];

/// Maximum number of pre-migration snapshots to retain.
///
/// Local recovery needs only a handful of recent snapshots; older ones are
/// removed after a successful migration to bound disk growth.
const SNAPSHOT_RETENTION_LIMIT: usize = 5;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("rusqlite error: {0}")]
    Rusqlite(#[from] rusqlite::Error),
    #[error("pool error: {0}")]
    Pool(#[from] deadpool_sqlite::PoolError),
    #[error("interact error: {0}")]
    Interact(#[from] deadpool_sqlite::InteractError),
    #[error("create pool error: {0}")]
    CreatePool(#[from] deadpool_sqlite::CreatePoolError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<DbError> for AppError {
    fn from(err: DbError) -> Self {
        match err {
            DbError::Rusqlite(e) => AppError::Database(e.to_string()),
            DbError::Pool(e) => AppError::Database(e.to_string()),
            DbError::Interact(e) => AppError::Database(e.to_string()),
            DbError::CreatePool(e) => AppError::Database(e.to_string()),
            DbError::Io(e) => e.into(),
        }
    }
}

/// Per-connection SQLite pragmas required by the design.
fn initialize_connection(conn: &mut Connection) -> Result<(), rusqlite::Error> {
    // WAL mode for concurrent readers and durability.
    conn.pragma_update(None, "journal_mode", "WAL")?;
    // Full synchronous mode for durability.
    conn.pragma_update(None, "synchronous", "FULL")?;
    // Foreign key enforcement is per connection and must be set explicitly.
    conn.pragma_update(None, "foreign_keys", "ON")?;
    // Busy timeout for serialized writers.
    conn.pragma_update(None, "busy_timeout", "5000")?;
    // Strict type checking for application-defined tables.
    conn.pragma_update(None, "strict_tables", "ON")?;
    Ok(())
}

/// Open a SQLite connection pool at the given path.
///
/// Every created connection is initialized with the design-required pragmas:
/// WAL, synchronous=FULL, foreign_keys=ON, busy_timeout=5000, strict_tables=ON.
pub async fn open_pool(path: &Path) -> Result<Pool, DbError> {
    let path_str = path.to_string_lossy().to_string();
    let cfg = Config::new(path_str);
    let pool = cfg
        .builder(Runtime::Tokio1)
        .map_err(deadpool_sqlite::CreatePoolError::Config)?
        .post_create(Hook::sync_fn(|conn, _| {
            let mut guard = conn
                .lock()
                .map_err(|e| deadpool_sqlite::HookError::Message(e.to_string().into()))?;
            initialize_connection(&mut *guard).map_err(deadpool_sqlite::HookError::Backend)
        }))
        .runtime(Runtime::Tokio1)
        .build()
        .map_err(deadpool_sqlite::CreatePoolError::Build)?;
    // Force the first connection to validate the path and pragmas.
    let _conn = pool.get().await?;
    Ok(pool)
}

/// Create a pre-migration snapshot of the live database using SQLite's online backup API.
///
/// The snapshot is a self-contained database copy; the WAL/SHM trio is never copied raw.
/// The snapshot is written next to the live database path.
fn snapshot_before_migration(live: &Connection, db_path: &Path) -> Result<PathBuf, DbError> {
    let stamp = Utc::now().format("%Y%m%d%H%M%S").to_string();
    let parent = db_path.parent().unwrap_or_else(|| Path::new("."));
    let file_stem = db_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unatienda");
    let snapshot_path = parent.join(format!("{}.pre-migration.{}.db", file_stem, stamp));
    live.backup(rusqlite::DatabaseName::Main, &snapshot_path, None)?;
    Ok(snapshot_path)
}

/// Check whether a migration version is not yet recorded.
///
/// Returns `true` if the `schema_migrations` table does not exist or does not
/// contain the requested version.
fn is_migration_pending(conn: &Connection, version: i64) -> Result<bool, rusqlite::Error> {
    let table_exists: bool = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='schema_migrations'",
        [],
        |row| {
            let count: i64 = row.get(0)?;
            Ok(count > 0)
        },
    )?;
    if !table_exists {
        return Ok(true);
    }
    let count: i64 = conn.query_row(
        "SELECT count(*) FROM schema_migrations WHERE version = ?1",
        [version],
        |row| row.get(0),
    )?;
    Ok(count == 0)
}

/// Extract the pre-migration timestamp from a snapshot filename.
fn snapshot_timestamp(path: &Path) -> Option<String> {
    let name = path.file_stem()?.to_str()?;
    let parts: Vec<&str> = name.split('.').collect();
    if parts.len() == 3 && parts[1] == "pre-migration" {
        Some(parts[2].to_string())
    } else {
        None
    }
}

/// Enforce a bounded retention policy for pre-migration snapshots.
///
/// Keeps the newest `limit` snapshots, sorted by the timestamp embedded in their
/// filename, and removes older ones. This bounds disk growth for local recovery.
fn enforce_snapshot_retention(db_path: &Path, limit: usize) -> Result<(), DbError> {
    let parent = db_path.parent().unwrap_or_else(|| Path::new("."));
    let file_stem = db_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unatienda");
    let prefix = format!("{}.pre-migration.", file_stem);

    let mut snapshots: Vec<(PathBuf, String)> = std::fs::read_dir(parent)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.starts_with(&prefix) && name.ends_with(".db")
        })
        .filter_map(|e| {
            let path = e.path();
            snapshot_timestamp(&path).map(|ts| (path, ts))
        })
        .collect();

    snapshots.sort_by(|a, b| a.1.cmp(&b.1));

    if snapshots.len() > limit {
        for (path, _) in snapshots.iter().take(snapshots.len() - limit) {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}

/// Run pending numbered migrations in order.
///
/// Each migration is applied inside its own transaction and its version is
/// recorded atomically in `schema_migrations`. A pre-migration snapshot is taken
/// only when at least one migration is actually pending and the database already
/// contains tables, so fresh installs do not create empty snapshots and
/// already-applied migrations do not create repeated snapshots. After successful
/// migrations, older snapshots are cleaned up according to `SNAPSHOT_RETENTION_LIMIT`.
pub async fn run_migrations(pool: &Pool, db_path: &Path) -> Result<(), DbError> {
    let db_path_buf = db_path.to_path_buf();
    let conn = pool.get().await?;
    conn.interact(move |conn| {
        let mut pending_versions = Vec::new();
        for (version, _) in MIGRATIONS.iter() {
            if is_migration_pending(conn, *version)? {
                pending_versions.push(*version);
            }
        }

        if !pending_versions.is_empty() {
            // Snapshot before migration only if the database is not empty (has tables already).
            let table_count: i64 = conn.query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
                [],
                |row| row.get(0),
            )?;
            if table_count > 0 {
                let _snapshot_path = snapshot_before_migration(conn, &db_path_buf)?;
            }
        }

        let pending_set: HashSet<i64> = pending_versions.iter().copied().collect();
        for (version, sql) in MIGRATIONS.iter() {
            if pending_set.contains(version) {
                let tx = conn.transaction()?;
                tx.execute_batch(sql)?;
                tx.commit()?;
            }
        }

        if !pending_versions.is_empty() {
            enforce_snapshot_retention(&db_path_buf, SNAPSHOT_RETENTION_LIMIT)?;
        }
        Ok::<_, DbError>(())
    })
    .await??;
    Ok(())
}

/// Application-owned database handle.
#[derive(Clone)]
pub struct AppDb {
    pool: Pool,
}

impl AppDb {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &Pool {
        &self.pool
    }

    pub async fn health_check(&self) -> Result<(), DbError> {
        let conn = self.pool.get().await?;
        conn.interact(|conn| {
            conn.pragma_query_value(None, "journal_mode", |row| {
                let mode: String = row.get(0)?;
                Ok(mode)
            })?;
            conn.pragma_query_value(None, "foreign_keys", |row| {
                let enabled: i32 = row.get(0)?;
                Ok(enabled)
            })?;
            Ok::<_, DbError>(())
        })
        .await??;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn pool_initializes_pragmas() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        let conn = pool.get().await.unwrap();
        conn.interact(|conn| {
            let journal_mode: String = conn
                .pragma_query_value(None, "journal_mode", |row| row.get(0))
                .unwrap();
            assert_eq!(journal_mode.to_lowercase(), "wal");
            let foreign_keys: i32 = conn
                .pragma_query_value(None, "foreign_keys", |row| row.get(0))
                .unwrap();
            assert_eq!(foreign_keys, 1);
            let busy_timeout: i32 = conn
                .pragma_query_value(None, "busy_timeout", |row| row.get(0))
                .unwrap();
            assert_eq!(busy_timeout, 5000);
            let synchronous: i32 = conn
                .pragma_query_value(None, "synchronous", |row| row.get(0))
                .unwrap();
            assert_eq!(synchronous, 2);
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn migration_creates_schema_tables() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool, &db_path).await.unwrap();
        let conn = pool.get().await.unwrap();
        conn.interact(|conn| {
            let tables: Vec<String> = conn
                .prepare("SELECT name FROM sqlite_master WHERE type='table'")
                .unwrap()
                .query_map([], |row| row.get(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert!(tables.contains(&"cash_sessions".to_string()));
            assert!(tables.contains(&"sales".to_string()));
            assert!(tables.contains(&"payments".to_string()));
            assert!(tables.contains(&"drafts".to_string()));
            assert!(tables.contains(&"movements".to_string()));
            assert!(tables.contains(&"returns".to_string()));
            assert!(tables.contains(&"payment_reversals".to_string()));
            assert!(tables.contains(&"idempotency_keys".to_string()));
            assert!(tables.contains(&"schema_migrations".to_string()));
            assert!(tables.contains(&"backup_runs".to_string()));

            for version in [1, 2, 3] {
                let migrations: i64 = conn
                    .query_row(
                        "SELECT count(*) FROM schema_migrations WHERE version = ?1",
                        [version],
                        |row| row.get(0),
                    )
                    .unwrap();
                assert_eq!(
                    migrations, 1,
                    "migration version {} must be recorded",
                    version
                );
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn one_open_session_index_enforced() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool, &db_path).await.unwrap();
        let conn = pool.get().await.unwrap();
        let inner = conn
            .interact(|conn| {
                conn.execute(
                    "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, ?2, ?3, ?4)",
                    ["session-1", "open", "1000", "2024-01-01T00:00:00Z"],
                )?;
                conn.execute(
                    "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, ?2, ?3, ?4)",
                    ["session-2", "open", "500", "2024-01-01T00:00:00Z"],
                )?;
                Ok::<_, rusqlite::Error>(())
            })
            .await;
        assert!(inner.is_ok(), "interact must run the closure");
        assert!(
            inner.unwrap().is_err(),
            "two open sessions must violate the partial unique index"
        );
    }

    /// Prove JD-A-001: payment_reversals accepts the schema and enforces the XOR source constraint.
    #[tokio::test]
    async fn payment_reversals_xor_source_constraint() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool, &db_path).await.unwrap();
        let conn = pool.get().await.unwrap();
        conn.interact(|conn| {
            // Minimal session/sale/payment graph for a valid reversal.
            conn.execute(
                "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, ?2, ?3, ?4)",
                ["session-1", "open", "1000", "2024-01-01T00:00:00Z"],
            )?;
            conn.execute(
                "INSERT INTO sales (id, session_id, total_amount, status) VALUES (?1, ?2, ?3, ?4)",
                ["sale-1", "session-1", "1000", "confirmed"],
            )?;
            conn.execute(
                "INSERT INTO payments (id, sale_id, method, amount) VALUES (?1, ?2, ?3, ?4)",
                ["payment-1", "sale-1", "cash", "1000"],
            )?;
            conn.execute(
                "INSERT INTO returns (id, session_id, sale_id, return_kind, amount) VALUES (?1, ?2, ?3, ?4, ?5)",
                ["return-1", "session-1", "sale-1", "total", "1000"],
            )?;

            // Valid reversal: return source only.
            conn.execute(
                "INSERT INTO payment_reversals (id, payment_id, amount, return_id) VALUES (?1, ?2, ?3, ?4)",
                ["rev-1", "payment-1", "1000", "return-1"],
            )?;

            // Violation: both return and annulment sources set.
            let both = conn.execute(
                "INSERT INTO payment_reversals (id, payment_id, amount, return_id, annulment_sale_id) VALUES (?1, ?2, ?3, ?4, ?5)",
                ["rev-2", "payment-1", "1000", "return-1", "sale-1"],
            );
            assert!(both.is_err(), "XOR constraint must reject both sources");

            // Violation: neither source set.
            let neither = conn.execute(
                "INSERT INTO payment_reversals (id, payment_id, amount) VALUES (?1, ?2, ?3)",
                ["rev-3", "payment-1", "1000"],
            );
            assert!(neither.is_err(), "XOR constraint must reject missing source");

            Ok::<_, rusqlite::Error>(())
        })
        .await
        .unwrap()
        .unwrap();
    }

    /// Prove JD-A-002 / R3-001: rows created while a session is open become
    /// immutable once the session is closed, and new rows cannot be inserted
    /// against the closed session.
    #[tokio::test]
    async fn closed_session_immutability_triggers() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool, &db_path).await.unwrap();
        let conn = pool.get().await.unwrap();
        conn.interact(|conn| {
            // Open a session and create all dependent rows while it is open.
            conn.execute(
                "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, ?2, ?3, ?4)",
                ["session-1", "open", "1000", "2024-01-01T00:00:00Z"],
            )?;
            conn.execute(
                "INSERT INTO sales (id, session_id, total_amount, status) VALUES (?1, ?2, ?3, ?4)",
                ["sale-1", "session-1", "1000", "confirmed"],
            )?;
            conn.execute(
                "INSERT INTO payments (id, sale_id, method, amount) VALUES (?1, ?2, ?3, ?4)",
                ["payment-1", "sale-1", "cash", "1000"],
            )?;
            conn.execute(
                "INSERT INTO movements (id, session_id, kind, amount) VALUES (?1, ?2, ?3, ?4)",
                ["movement-1", "session-1", "income", "100"],
            )?;
            conn.execute(
                "INSERT INTO returns (id, session_id, sale_id, return_kind, amount) VALUES (?1, ?2, ?3, ?4, ?5)",
                ["return-1", "session-1", "sale-1", "total", "1000"],
            )?;
            conn.execute(
                "INSERT INTO payment_reversals (id, payment_id, amount, return_id) VALUES (?1, ?2, ?3, ?4)",
                ["rev-1", "payment-1", "1000", "return-1"],
            )?;
            conn.execute(
                "INSERT INTO drafts (id, session_id, total_amount) VALUES (?1, ?2, ?3)",
                ["draft-1", "session-1", "500"],
            )?;

            // Legitimate closure transition must succeed.
            conn.execute(
                "UPDATE cash_sessions SET status = 'closed', counted_cash = 1000, difference = 0, closed_at = ?2 WHERE id = ?1 AND status = 'open'",
                ["session-1", "2024-01-01T23:00:00Z"],
            )?;

            // Closed cash_session itself is immutable.
            assert!(conn.execute("UPDATE cash_sessions SET counted_cash = 999 WHERE id = ?1", ["session-1"]).is_err(),
                "UPDATE on closed cash_session must be blocked");
            assert!(conn.execute("DELETE FROM cash_sessions WHERE id = ?1", ["session-1"]).is_err(),
                "DELETE on closed cash_session must be blocked");

            // Sales: new INSERTs, UPDATE on any column, and DELETE are blocked.
            assert!(conn.execute(
                "INSERT INTO sales (id, session_id, total_amount, status) VALUES (?1, ?2, ?3, ?4)",
                ["sale-2", "session-1", "500", "confirmed"],
            ).is_err(), "INSERT into closed-session sales must be blocked");
            assert!(conn.execute("UPDATE sales SET total_amount = 999 WHERE id = ?1", ["sale-1"]).is_err(),
                "UPDATE on closed-session sale total_amount must be blocked");
            assert!(conn.execute("UPDATE sales SET annulment_reason = 'test' WHERE id = ?1", ["sale-1"]).is_err(),
                "UPDATE on closed-session sale annulment_reason must be blocked");
            assert!(conn.execute("DELETE FROM sales WHERE id = ?1", ["sale-1"]).is_err(),
                "DELETE on closed-session sale must be blocked");

            // Payments: INSERT, UPDATE, DELETE blocked.
            assert!(conn.execute(
                "INSERT INTO payments (id, sale_id, method, amount) VALUES (?1, ?2, ?3, ?4)",
                ["payment-2", "sale-1", "cash", "100"],
            ).is_err(), "INSERT into closed-session payments must be blocked");
            let update_payment = conn.execute(
                "UPDATE payments SET amount = 999 WHERE id = ?1",
                ["payment-1"],
            );
            assert!(update_payment.is_err(), "UPDATE on closed-session payment must be blocked");
            let delete_payment = conn.execute("DELETE FROM payments WHERE id = ?1", ["payment-1"]);
            assert!(delete_payment.is_err(), "DELETE on closed-session payment must be blocked");

            // Movements: INSERT, UPDATE, DELETE blocked.
            assert!(conn.execute(
                "INSERT INTO movements (id, session_id, kind, amount) VALUES (?1, ?2, ?3, ?4)",
                ["movement-2", "session-1", "expense", "50"],
            ).is_err(), "INSERT into closed-session movements must be blocked");
            assert!(conn.execute("UPDATE movements SET amount = 999 WHERE id = ?1", ["movement-1"]).is_err(),
                "UPDATE on closed-session movement must be blocked");
            assert!(conn.execute("DELETE FROM movements WHERE id = ?1", ["movement-1"]).is_err(),
                "DELETE on closed-session movement must be blocked");

            // Returns: INSERT, UPDATE, DELETE blocked.
            assert!(conn.execute(
                "INSERT INTO returns (id, session_id, sale_id, return_kind, amount) VALUES (?1, ?2, ?3, ?4, ?5)",
                ["return-2", "session-1", "sale-1", "partial", "100"],
            ).is_err(), "INSERT into closed-session returns must be blocked");
            assert!(conn.execute("UPDATE returns SET amount = 999 WHERE id = ?1", ["return-1"]).is_err(),
                "UPDATE on closed-session return must be blocked");
            assert!(conn.execute("DELETE FROM returns WHERE id = ?1", ["return-1"]).is_err(),
                "DELETE on closed-session return must be blocked");

            // Payment reversals: INSERT, UPDATE, DELETE blocked.
            assert!(conn.execute(
                "INSERT INTO payment_reversals (id, payment_id, amount, return_id) VALUES (?1, ?2, ?3, ?4)",
                ["rev-2", "payment-1", "1000", "return-1"],
            ).is_err(), "INSERT into closed-session payment_reversals must be blocked");
            assert!(conn.execute("UPDATE payment_reversals SET amount = 999 WHERE id = ?1", ["rev-1"]).is_err(),
                "UPDATE on closed-session payment_reversal must be blocked");
            assert!(conn.execute("DELETE FROM payment_reversals WHERE id = ?1", ["rev-1"]).is_err(),
                "DELETE on closed-session payment_reversal must be blocked");

            // Drafts: INSERT, UPDATE, DELETE blocked.
            assert!(conn.execute(
                "INSERT INTO drafts (id, session_id, total_amount) VALUES (?1, ?2, ?3)",
                ["draft-2", "session-1", "100"],
            ).is_err(), "INSERT into closed-session drafts must be blocked");
            assert!(conn.execute("UPDATE drafts SET total_amount = 999 WHERE id = ?1", ["draft-1"]).is_err(),
                "UPDATE on closed-session draft must be blocked");
            assert!(conn.execute("DELETE FROM drafts WHERE id = ?1", ["draft-1"]).is_err(),
                "DELETE on closed-session draft must be blocked");

            Ok::<_, rusqlite::Error>(())
        })
        .await
        .unwrap()
        .unwrap();
    }

    /// Prove JD-B-006: a fresh database does not create a pre-migration snapshot.
    #[tokio::test]
    async fn snapshot_not_created_on_fresh_database() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool, &db_path).await.unwrap();
        let snapshots: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("pre-migration"))
            .collect();
        assert!(
            snapshots.is_empty(),
            "fresh database must not produce a snapshot"
        );
    }

    /// Prove JD-B-006: a snapshot is created only when the migration is pending.
    #[tokio::test]
    async fn snapshot_created_only_when_migration_pending() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool, &db_path).await.unwrap();

        // Simulate a pending migration by removing the applied record.
        let conn = pool.get().await.unwrap();
        conn.interact(|conn| {
            conn.execute("DELETE FROM schema_migrations WHERE version = 1", [])
                .unwrap();
            Ok::<_, rusqlite::Error>(())
        })
        .await
        .unwrap()
        .unwrap();

        run_migrations(&pool, &db_path).await.unwrap();

        let snapshots: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("test.pre-migration")
            })
            .collect();
        assert_eq!(
            snapshots.len(),
            1,
            "pending migration on a populated database must produce exactly one snapshot"
        );

        // Re-running migrations when already applied must not create another snapshot.
        run_migrations(&pool, &db_path).await.unwrap();
        let snapshots: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("test.pre-migration")
            })
            .collect();
        assert_eq!(
            snapshots.len(),
            1,
            "already-applied migration must not create another snapshot"
        );
    }

    /// Prove JD-B-006: snapshot retention keeps only the newest N snapshots.
    #[tokio::test]
    async fn snapshot_retention_keeps_newest() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");

        // Seed fake older snapshots with deterministic timestamps.
        for i in 0..7 {
            let path = dir
                .path()
                .join(format!("test.pre-migration.202401010000{:02}.db", i));
            std::fs::write(&path, b"fake").unwrap();
        }

        enforce_snapshot_retention(&db_path, 5).unwrap();

        let mut names: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("test.pre-migration")
            })
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        assert_eq!(names.len(), 5, "retention must keep exactly 5 snapshots");
        assert!(
            names.iter().all(|n| {
                n.ends_with("02.db")
                    || n.ends_with("03.db")
                    || n.ends_with("04.db")
                    || n.ends_with("05.db")
                    || n.ends_with("06.db")
            }),
            "retention must keep the 5 newest snapshots"
        );
    }

    /// Prove REM-A-001: an existing version-1 database with legacy incomplete
    /// same-name immutability triggers is upgraded by migration 2, which drops the
    /// legacy triggers and recreates the complete guards. The schema version is
    /// recorded, a single pre-migration snapshot is created, and the previously
    /// mutable closed-session paths (closed cash_session UPDATE/DELETE, sale
    /// annulment_reason UPDATE, sale DELETE, draft UPDATE/DELETE) are now blocked.
    #[tokio::test]
    async fn migration_v2_upgrades_legacy_triggers() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");

        // Seed a representative version-1 database: full schema, version 1 recorded,
        // but the immutability triggers for cash_sessions, sales, and drafts are
        // missing or incomplete (matching the legacy definitions).
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(include_str!("../../migrations/001_initial.sql"))
                .unwrap();
            conn.execute_batch(
                r#"
                DROP TRIGGER IF EXISTS trg_guard_closed_session_cash_sessions_update;
                DROP TRIGGER IF EXISTS trg_guard_closed_session_cash_sessions_delete;
                DROP TRIGGER IF EXISTS trg_guard_closed_session_sales_update;
                DROP TRIGGER IF EXISTS trg_guard_closed_session_sales_delete;
                DROP TRIGGER IF EXISTS trg_guard_closed_session_drafts_update;
                DROP TRIGGER IF EXISTS trg_guard_closed_session_drafts_delete;
                DROP TRIGGER IF EXISTS trg_guard_closed_session_payments_update;
                DROP TRIGGER IF EXISTS trg_guard_closed_session_payments_delete;
                DROP TRIGGER IF EXISTS trg_guard_closed_session_movements_update;
                DROP TRIGGER IF EXISTS trg_guard_closed_session_movements_delete;
                DROP TRIGGER IF EXISTS trg_guard_closed_session_returns_update;
                DROP TRIGGER IF EXISTS trg_guard_closed_session_returns_delete;
                DROP TRIGGER IF EXISTS trg_guard_closed_session_payment_reversals_update;
                DROP TRIGGER IF EXISTS trg_guard_closed_session_payment_reversals_delete;

                -- Legacy sales update trigger guarded only a column list, omitting annulment_reason.
                CREATE TRIGGER trg_guard_closed_session_sales_update
                BEFORE UPDATE OF total_amount, status, details, notes, updated_at ON sales
                FOR EACH ROW
                WHEN EXISTS (
                    SELECT 1 FROM cash_sessions WHERE id = OLD.session_id AND status = 'closed'
                )
                BEGIN
                    SELECT RAISE(ABORT, 'closed session records are immutable');
                END;
                "#,
            )
            .unwrap();
        }

        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool, &db_path).await.unwrap();

        let dir_path = dir.path().to_path_buf();
        let conn = pool.get().await.unwrap();
        conn.interact(move |conn| {
            // Migration version 2 is recorded atomically.
            let version_2: i64 = conn
                .query_row(
                    "SELECT count(*) FROM schema_migrations WHERE version = 2",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(version_2, 1, "migration v2 must be recorded");

            // A pre-migration snapshot was created because the DB was populated.
            let snapshots: Vec<_> = std::fs::read_dir(&dir_path)
                .unwrap()
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .starts_with("test.pre-migration")
                })
                .collect();
            assert_eq!(
                snapshots.len(),
                1,
                "upgrade from a populated v1 database must produce exactly one snapshot"
            );

            // Seed an open session with rows, then close it, to verify the legacy
            // mutable paths are now protected on the closed session.
            conn.execute(
                "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, ?2, ?3, ?4)",
                ["session-1", "open", "1000", "2024-01-01T00:00:00Z"],
            ).unwrap();
            conn.execute(
                "INSERT INTO sales (id, session_id, total_amount, status) VALUES (?1, ?2, ?3, ?4)",
                ["sale-1", "session-1", "1000", "confirmed"],
            ).unwrap();
            conn.execute(
                "INSERT INTO drafts (id, session_id, total_amount) VALUES (?1, ?2, ?3)",
                ["draft-1", "session-1", "500"],
            ).unwrap();
            conn.execute(
                "UPDATE cash_sessions SET status = 'closed', counted_cash = 1000, difference = 0, closed_at = ?2 WHERE id = ?1 AND status = 'open'",
                ["session-1", "2024-01-01T23:00:00Z"],
            ).unwrap();

            // Paths that the legacy v1 triggers left mutable are now blocked.
            assert!(
                conn.execute("UPDATE cash_sessions SET counted_cash = 999 WHERE id = ?1", ["session-1"]).is_err(),
                "UPDATE on closed cash_session must be blocked after v2"
            );
            assert!(
                conn.execute("DELETE FROM cash_sessions WHERE id = ?1", ["session-1"]).is_err(),
                "DELETE on closed cash_session must be blocked after v2"
            );
            assert!(
                conn.execute("UPDATE sales SET annulment_reason = 'test' WHERE id = ?1", ["sale-1"]).is_err(),
                "UPDATE on closed-session sale annulment_reason must be blocked after v2"
            );
            assert!(
                conn.execute("DELETE FROM sales WHERE id = ?1", ["sale-1"]).is_err(),
                "DELETE on closed-session sale must be blocked after v2"
            );
            assert!(
                conn.execute("UPDATE drafts SET total_amount = 999 WHERE id = ?1", ["draft-1"]).is_err(),
                "UPDATE on closed-session draft must be blocked after v2"
            );
            assert!(
                conn.execute("DELETE FROM drafts WHERE id = ?1", ["draft-1"]).is_err(),
                "DELETE on closed-session draft must be blocked after v2"
            );

            Ok::<_, rusqlite::Error>(())
        })
        .await
        .unwrap()
        .unwrap();
    }

    /// Prove R3-001: a database that already has migrations 1 and 2 applied
    /// receives the INSERT guards from migration 3, and new rows cannot be
    /// inserted against a closed session.
    #[tokio::test]
    async fn migration_v3_adds_closed_session_insert_guards() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");

        // Seed a representative version-2 database: full schema and immutability
        // triggers, but no INSERT guards. Rows are inserted while the guards are
        // absent so the fixture survives; migration 3 then reinstalls the guards.
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(include_str!("../../migrations/001_initial.sql"))
                .unwrap();
            conn.execute_batch(include_str!(
                "../../migrations/002_upgrade_closed_session_immutability.sql"
            ))
            .unwrap();
            // Simulate the pre-fix state: drop the INSERT triggers that 001 may have
            // contained in a patched development build, leaving only UPDATE/DELETE guards.
            conn.execute_batch(
                r#"
                DROP TRIGGER IF EXISTS trg_guard_closed_session_sales_insert;
                DROP TRIGGER IF EXISTS trg_guard_closed_session_payments_insert;
                DROP TRIGGER IF EXISTS trg_guard_closed_session_movements_insert;
                DROP TRIGGER IF EXISTS trg_guard_closed_session_drafts_insert;
                DROP TRIGGER IF EXISTS trg_guard_closed_session_returns_insert;
                DROP TRIGGER IF EXISTS trg_guard_closed_session_payment_reversals_insert;
                "#,
            )
            .unwrap();

            // Seed a closed session with representative rows before migration 3
            // restores the INSERT guards.
            conn.execute(
                "INSERT INTO cash_sessions (id, status, opening_amount, counted_cash, difference, opened_at, closed_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                ["session-1", "closed", "1000", "1000", "0", "2024-01-01T00:00:00Z", "2024-01-01T23:00:00Z"],
            ).unwrap();
            conn.execute(
                "INSERT INTO sales (id, session_id, total_amount, status) VALUES (?1, ?2, ?3, ?4)",
                ["sale-1", "session-1", "1000", "confirmed"],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO payments (id, sale_id, method, amount) VALUES (?1, ?2, ?3, ?4)",
                ["payment-1", "sale-1", "cash", "1000"],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO movements (id, session_id, kind, amount) VALUES (?1, ?2, ?3, ?4)",
                ["movement-1", "session-1", "income", "100"],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO returns (id, session_id, sale_id, return_kind, amount) VALUES (?1, ?2, ?3, ?4, ?5)",
                ["return-1", "session-1", "sale-1", "total", "1000"],
            ).unwrap();
            conn.execute(
                "INSERT INTO payment_reversals (id, payment_id, amount, return_id) VALUES (?1, ?2, ?3, ?4)",
                ["rev-1", "payment-1", "1000", "return-1"],
            ).unwrap();
            conn.execute(
                "INSERT INTO drafts (id, session_id, total_amount) VALUES (?1, ?2, ?3)",
                ["draft-1", "session-1", "500"],
            )
            .unwrap();
        }

        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool, &db_path).await.unwrap();

        let conn = pool.get().await.unwrap();
        conn.interact(|conn| {
            // Migration version 3 is recorded atomically.
            let version_3: i64 = conn
                .query_row(
                    "SELECT count(*) FROM schema_migrations WHERE version = 3",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(version_3, 1, "migration v3 must be recorded");

            // INSERTs against the closed session are now blocked.
            assert!(
                conn.execute(
                    "INSERT INTO sales (id, session_id, total_amount, status) VALUES (?1, ?2, ?3, ?4)",
                    ["sale-2", "session-1", "500", "confirmed"],
                ).is_err(),
                "INSERT into closed-session sales must be blocked after v3"
            );
            assert!(
                conn.execute(
                    "INSERT INTO payments (id, sale_id, method, amount) VALUES (?1, ?2, ?3, ?4)",
                    ["payment-2", "sale-1", "cash", "100"],
                ).is_err(),
                "INSERT into closed-session payments must be blocked after v3"
            );
            assert!(
                conn.execute(
                    "INSERT INTO movements (id, session_id, kind, amount) VALUES (?1, ?2, ?3, ?4)",
                    ["movement-2", "session-1", "expense", "50"],
                ).is_err(),
                "INSERT into closed-session movements must be blocked after v3"
            );
            assert!(
                conn.execute(
                    "INSERT INTO returns (id, session_id, sale_id, return_kind, amount) VALUES (?1, ?2, ?3, ?4, ?5)",
                    ["return-2", "session-1", "sale-1", "partial", "100"],
                ).is_err(),
                "INSERT into closed-session returns must be blocked after v3"
            );
            assert!(
                conn.execute(
                    "INSERT INTO payment_reversals (id, payment_id, amount, return_id) VALUES (?1, ?2, ?3, ?4)",
                    ["rev-2", "payment-1", "1000", "return-1"],
                ).is_err(),
                "INSERT into closed-session payment_reversals must be blocked after v3"
            );
            assert!(
                conn.execute(
                    "INSERT INTO drafts (id, session_id, total_amount) VALUES (?1, ?2, ?3)",
                    ["draft-2", "session-1", "100"],
                ).is_err(),
                "INSERT into closed-session drafts must be blocked after v3"
            );

            Ok::<_, rusqlite::Error>(())
        })
        .await
        .unwrap()
        .unwrap();
    }

    /// Prove R3-001 round 2: payment_reversals INSERT ownership follows the
    /// source session, not the original payment's sale session.
    ///
    /// 1. A historical sale/payment belongs to a closed session.
    /// 2. A return in a separate current open session references that sale.
    /// 3. A payment reversal linked to the open-session return succeeds.
    /// 4. After closing the return's session, another return-based reversal fails.
    /// 5. Annulment-based reversal succeeds while the annulled sale's session is
    ///    open and fails after closure.
    /// 6. Existing XOR and post-close protections remain intact.
    #[tokio::test]
    async fn payment_reversal_source_ownership_fresh_schema() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool, &db_path).await.unwrap();
        let conn = pool.get().await.unwrap();
        conn.interact(|conn| {
            // 1. Historical sale/payment belongs to a closed session.
            // Create the session open, insert the sale and payment, then close it.
            conn.execute(
                "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, ?2, ?3, ?4)",
                ["session-hist", "open", "1000", "2024-01-01T00:00:00Z"],
            )?;
            conn.execute(
                "INSERT INTO sales (id, session_id, total_amount, status) VALUES (?1, ?2, ?3, ?4)",
                ["sale-hist", "session-hist", "1000", "confirmed"],
            )?;
            conn.execute(
                "INSERT INTO payments (id, sale_id, method, amount) VALUES (?1, ?2, ?3, ?4)",
                ["payment-hist", "sale-hist", "cash", "1000"],
            )?;
            conn.execute(
                "UPDATE cash_sessions SET status = 'closed', counted_cash = 1000, difference = 0, closed_at = ?2 WHERE id = ?1 AND status = 'open'",
                ["session-hist", "2024-01-01T23:00:00Z"],
            )?;

            // 2. A return in a separate current open session references the historical sale.
            conn.execute(
                "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, ?2, ?3, ?4)",
                ["session-return", "open", "1000", "2024-01-02T00:00:00Z"],
            )?;
            conn.execute(
                "INSERT INTO returns (id, session_id, sale_id, return_kind, amount) VALUES (?1, ?2, ?3, ?4, ?5)",
                ["return-1", "session-return", "sale-hist", "total", "1000"],
            )?;

            // 3. Payment reversal linked to the open-session return succeeds.
            conn.execute(
                "INSERT INTO payment_reversals (id, payment_id, amount, return_id) VALUES (?1, ?2, ?3, ?4)",
                ["rev-return-open", "payment-hist", "1000", "return-1"],
            )?;

            // 4. After closing the return's session, another return-based reversal fails.
            conn.execute(
                "UPDATE cash_sessions SET status = 'closed', counted_cash = 1000, difference = 0, closed_at = ?2 WHERE id = ?1 AND status = 'open'",
                ["session-return", "2024-01-02T23:00:00Z"],
            )?;
            assert!(
                conn.execute(
                    "INSERT INTO payment_reversals (id, payment_id, amount, return_id) VALUES (?1, ?2, ?3, ?4)",
                    ["rev-return-closed", "payment-hist", "1000", "return-1"],
                )
                .is_err(),
                "return-based reversal must fail when the return's session is closed"
            );

            // 5. Annulment-based reversal succeeds while annulled sale's session is open.
            conn.execute(
                "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, ?2, ?3, ?4)",
                ["session-annul", "open", "1000", "2024-01-03T00:00:00Z"],
            )?;
            conn.execute(
                "INSERT INTO sales (id, session_id, total_amount, status) VALUES (?1, ?2, ?3, ?4)",
                ["sale-annul", "session-annul", "1000", "confirmed"],
            )?;
            conn.execute(
                "INSERT INTO payments (id, sale_id, method, amount) VALUES (?1, ?2, ?3, ?4)",
                ["payment-annul", "sale-annul", "cash", "1000"],
            )?;
            conn.execute(
                "INSERT INTO payment_reversals (id, payment_id, amount, annulment_sale_id) VALUES (?1, ?2, ?3, ?4)",
                ["rev-annul-open", "payment-annul", "1000", "sale-annul"],
            )?;

            // After closing the annulled sale's session, another annulment-based reversal fails.
            conn.execute(
                "UPDATE cash_sessions SET status = 'closed', counted_cash = 1000, difference = 0, closed_at = ?2 WHERE id = ?1 AND status = 'open'",
                ["session-annul", "2024-01-03T23:00:00Z"],
            )?;
            assert!(
                conn.execute(
                    "INSERT INTO payment_reversals (id, payment_id, amount, annulment_sale_id) VALUES (?1, ?2, ?3, ?4)",
                    ["rev-annul-closed", "payment-annul", "1000", "sale-annul"],
                )
                .is_err(),
                "annulment-based reversal must fail when the annulled sale's session is closed"
            );

            // 6. XOR constraint remains intact.
            assert!(
                conn.execute(
                    "INSERT INTO payment_reversals (id, payment_id, amount, return_id, annulment_sale_id) VALUES (?1, ?2, ?3, ?4, ?5)",
                    ["rev-both", "payment-hist", "1000", "return-1", "sale-annul"],
                )
                .is_err(),
                "XOR constraint must reject both sources"
            );
            assert!(
                conn.execute(
                    "INSERT INTO payment_reversals (id, payment_id, amount) VALUES (?1, ?2, ?3)",
                    ["rev-neither", "payment-hist", "1000"],
                )
                .is_err(),
                "XOR constraint must reject missing source"
            );

            // Post-close UPDATE/DELETE protections remain intact for the source-owned reversal.
            assert!(
                conn.execute(
                    "UPDATE payment_reversals SET amount = 999 WHERE id = ?1",
                    ["rev-return-open"],
                )
                .is_err(),
                "UPDATE on closed-session payment_reversal must be blocked"
            );
            assert!(
                conn.execute(
                    "DELETE FROM payment_reversals WHERE id = ?1",
                    ["rev-return-open"],
                )
                .is_err(),
                "DELETE on closed-session payment_reversal must be blocked"
            );

            Ok::<_, rusqlite::Error>(())
        })
        .await
        .unwrap()
        .unwrap();
    }

    /// Prove R3-001 round 2 on the genuine pre-v3 upgrade path: the same
    /// source-session ownership rules are installed by migration 3.
    #[tokio::test]
    async fn payment_reversal_source_ownership_upgrade_path() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");

        // Seed a representative version-2 database: full schema and immutability
        // triggers, but no INSERT guards. Seed rows before migration 3 installs them.
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(include_str!("../../migrations/001_initial.sql"))
                .unwrap();
            conn.execute_batch(include_str!(
                "../../migrations/002_upgrade_closed_session_immutability.sql"
            ))
            .unwrap();
            conn.execute_batch(
                r#"
                DROP TRIGGER IF EXISTS trg_guard_closed_session_sales_insert;
                DROP TRIGGER IF EXISTS trg_guard_closed_session_payments_insert;
                DROP TRIGGER IF EXISTS trg_guard_closed_session_movements_insert;
                DROP TRIGGER IF EXISTS trg_guard_closed_session_drafts_insert;
                DROP TRIGGER IF EXISTS trg_guard_closed_session_returns_insert;
                DROP TRIGGER IF EXISTS trg_guard_closed_session_payment_reversals_insert;
                "#,
            )
            .unwrap();

            // 1. Historical sale/payment belongs to a closed session.
            // Create the session open, insert the sale and payment, then close it.
            conn.execute(
                "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, ?2, ?3, ?4)",
                ["session-hist", "open", "1000", "2024-01-01T00:00:00Z"],
            ).unwrap();
            conn.execute(
                "INSERT INTO sales (id, session_id, total_amount, status) VALUES (?1, ?2, ?3, ?4)",
                ["sale-hist", "session-hist", "1000", "confirmed"],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO payments (id, sale_id, method, amount) VALUES (?1, ?2, ?3, ?4)",
                ["payment-hist", "sale-hist", "cash", "1000"],
            )
            .unwrap();
            conn.execute(
                "UPDATE cash_sessions SET status = 'closed', counted_cash = 1000, difference = 0, closed_at = ?2 WHERE id = ?1 AND status = 'open'",
                ["session-hist", "2024-01-01T23:00:00Z"],
            ).unwrap();

            // 2. A return in a separate current open session references the historical sale.
            conn.execute(
                "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, ?2, ?3, ?4)",
                ["session-return", "open", "1000", "2024-01-02T00:00:00Z"],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO returns (id, session_id, sale_id, return_kind, amount) VALUES (?1, ?2, ?3, ?4, ?5)",
                ["return-1", "session-return", "sale-hist", "total", "1000"],
            )
            .unwrap();
        }

        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool, &db_path).await.unwrap();

        let conn = pool.get().await.unwrap();
        conn.interact(|conn| {
            // 3. Return-based reversal linked to the open-session return succeeds.
            conn.execute(
                "INSERT INTO payment_reversals (id, payment_id, amount, return_id) VALUES (?1, ?2, ?3, ?4)",
                ["rev-return-open", "payment-hist", "1000", "return-1"],
            )?;

            // 4. After closing the return's session, another return-based reversal fails.
            conn.execute(
                "UPDATE cash_sessions SET status = 'closed', counted_cash = 1000, difference = 0, closed_at = ?2 WHERE id = ?1 AND status = 'open'",
                ["session-return", "2024-01-02T23:00:00Z"],
            )?;
            assert!(
                conn.execute(
                    "INSERT INTO payment_reversals (id, payment_id, amount, return_id) VALUES (?1, ?2, ?3, ?4)",
                    ["rev-return-closed", "payment-hist", "1000", "return-1"],
                )
                .is_err(),
                "return-based reversal must fail when the return's session is closed after v3 upgrade"
            );

            // 5. Annulment-based reversal succeeds while annulled sale's session is open,
            //    and fails after closure.
            conn.execute(
                "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, ?2, ?3, ?4)",
                ["session-annul", "open", "1000", "2024-01-03T00:00:00Z"],
            )?;
            conn.execute(
                "INSERT INTO sales (id, session_id, total_amount, status) VALUES (?1, ?2, ?3, ?4)",
                ["sale-annul", "session-annul", "1000", "confirmed"],
            )?;
            conn.execute(
                "INSERT INTO payments (id, sale_id, method, amount) VALUES (?1, ?2, ?3, ?4)",
                ["payment-annul", "sale-annul", "cash", "1000"],
            )?;
            conn.execute(
                "INSERT INTO payment_reversals (id, payment_id, amount, annulment_sale_id) VALUES (?1, ?2, ?3, ?4)",
                ["rev-annul-open", "payment-annul", "1000", "sale-annul"],
            )?;
            conn.execute(
                "UPDATE cash_sessions SET status = 'closed', counted_cash = 1000, difference = 0, closed_at = ?2 WHERE id = ?1 AND status = 'open'",
                ["session-annul", "2024-01-03T23:00:00Z"],
            )?;
            assert!(
                conn.execute(
                    "INSERT INTO payment_reversals (id, payment_id, amount, annulment_sale_id) VALUES (?1, ?2, ?3, ?4)",
                    ["rev-annul-closed", "payment-annul", "1000", "sale-annul"],
                )
                .is_err(),
                "annulment-based reversal must fail when the annulled sale's session is closed after v3 upgrade"
            );

            // 6. XOR and post-close protections remain intact.
            assert!(
                conn.execute(
                    "INSERT INTO payment_reversals (id, payment_id, amount, return_id, annulment_sale_id) VALUES (?1, ?2, ?3, ?4, ?5)",
                    ["rev-both", "payment-hist", "1000", "return-1", "sale-annul"],
                )
                .is_err(),
                "XOR constraint must reject both sources after v3 upgrade"
            );
            assert!(
                conn.execute(
                    "INSERT INTO payment_reversals (id, payment_id, amount) VALUES (?1, ?2, ?3)",
                    ["rev-neither", "payment-hist", "1000"],
                )
                .is_err(),
                "XOR constraint must reject missing source after v3 upgrade"
            );
            assert!(
                conn.execute(
                    "UPDATE payment_reversals SET amount = 999 WHERE id = ?1",
                    ["rev-return-open"],
                )
                .is_err(),
                "UPDATE on closed-session payment_reversal must be blocked after v3 upgrade"
            );
            assert!(
                conn.execute(
                    "DELETE FROM payment_reversals WHERE id = ?1",
                    ["rev-return-open"],
                )
                .is_err(),
                "DELETE on closed-session payment_reversal must be blocked after v3 upgrade"
            );

            Ok::<_, rusqlite::Error>(())
        })
        .await
        .unwrap()
        .unwrap();
    }

    /// Prove R3-001 remediation: annulment-based payment reversals are owned by the
    /// annulled *sale*'s session, not by the original *payment*'s sale session.
    ///
    /// The payment belongs to sale A/session A; `annulment_sale_id` points to sale B
    /// in a different session B. Both cases deliberately invert A's state so the only
    /// way the test can pass is if the guard follows B's session.
    ///
    /// This test would fail under the former payment-sale ownership predicate:
    ///   SELECT ... FROM cash_sessions cs
    ///     JOIN payments p ON p.id = NEW.payment_id
    ///     JOIN sales s ON s.id = p.sale_id
    ///     WHERE cs.id = s.session_id AND cs.status = 'closed'
    /// because in case 1 session A is closed, so the old predicate would reject the
    /// insert. The current annulment-sale predicate checks only sale B's session.
    #[tokio::test]
    async fn payment_reversal_annulment_ownership_follows_annulment_sale_fresh_schema() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool, &db_path).await.unwrap();
        let conn = pool.get().await.unwrap();
        conn.interact(|conn| {
            // Case 1: session A (payment's sale) is CLOSED, session B (annulled sale) is OPEN.
            // The reversal must be allowed because B is open.
            conn.execute(
                "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, ?2, ?3, ?4)",
                ["session-a1", "open", "1000", "2024-01-01T00:00:00Z"],
            )?;
            conn.execute(
                "INSERT INTO sales (id, session_id, total_amount, status) VALUES (?1, ?2, ?3, ?4)",
                ["sale-a1", "session-a1", "1000", "confirmed"],
            )?;
            conn.execute(
                "INSERT INTO payments (id, sale_id, method, amount) VALUES (?1, ?2, ?3, ?4)",
                ["payment-a1", "sale-a1", "cash", "1000"],
            )?;
            conn.execute(
                "UPDATE cash_sessions SET status = 'closed', counted_cash = 1000, difference = 0, closed_at = ?2 WHERE id = ?1 AND status = 'open'",
                ["session-a1", "2024-01-01T23:00:00Z"],
            )?;

            conn.execute(
                "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, ?2, ?3, ?4)",
                ["session-b1", "open", "1000", "2024-01-02T00:00:00Z"],
            )?;
            conn.execute(
                "INSERT INTO sales (id, session_id, total_amount, status) VALUES (?1, ?2, ?3, ?4)",
                ["sale-b1", "session-b1", "1000", "confirmed"],
            )?;

            conn.execute(
                "INSERT INTO payment_reversals (id, payment_id, amount, annulment_sale_id) VALUES (?1, ?2, ?3, ?4)",
                ["rev-b1-open", "payment-a1", "1000", "sale-b1"],
            )?;

            // Close B1 before opening the next session, because idx_one_open_session
            // allows only one open session at a time.
            conn.execute(
                "UPDATE cash_sessions SET status = 'closed', counted_cash = 1000, difference = 0, closed_at = ?2 WHERE id = ?1 AND status = 'open'",
                ["session-b1", "2024-01-02T23:00:00Z"],
            )?;

            // Case 2: session A (payment's sale) is OPEN, session B (annulled sale) is CLOSED.
            // The reversal must be rejected because B is closed, even though A is open.
            // Create the closed B2 session first so A2 can remain open afterwards.
            conn.execute(
                "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, ?2, ?3, ?4)",
                ["session-b2", "open", "1000", "2024-01-04T00:00:00Z"],
            )?;
            conn.execute(
                "INSERT INTO sales (id, session_id, total_amount, status) VALUES (?1, ?2, ?3, ?4)",
                ["sale-b2", "session-b2", "1000", "confirmed"],
            )?;
            conn.execute(
                "UPDATE cash_sessions SET status = 'closed', counted_cash = 1000, difference = 0, closed_at = ?2 WHERE id = ?1 AND status = 'open'",
                ["session-b2", "2024-01-04T23:00:00Z"],
            )?;

            conn.execute(
                "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, ?2, ?3, ?4)",
                ["session-a2", "open", "1000", "2024-01-03T00:00:00Z"],
            )?;
            conn.execute(
                "INSERT INTO sales (id, session_id, total_amount, status) VALUES (?1, ?2, ?3, ?4)",
                ["sale-a2", "session-a2", "1000", "confirmed"],
            )?;
            conn.execute(
                "INSERT INTO payments (id, sale_id, method, amount) VALUES (?1, ?2, ?3, ?4)",
                ["payment-a2", "sale-a2", "cash", "1000"],
            )?;

            assert!(
                conn.execute(
                    "INSERT INTO payment_reversals (id, payment_id, amount, annulment_sale_id) VALUES (?1, ?2, ?3, ?4)",
                    ["rev-b2-closed", "payment-a2", "1000", "sale-b2"],
                )
                .is_err(),
                "annulment-based reversal must be rejected when the annulled sale's session is closed, regardless of the payment's sale session being open"
            );

            // Return-source and XOR protections remain intact in the same transaction.
            conn.execute(
                "INSERT INTO returns (id, session_id, sale_id, return_kind, amount) VALUES (?1, ?2, ?3, ?4, ?5)",
                ["return-a2", "session-a2", "sale-b1", "total", "1000"],
            )?;
            conn.execute(
                "INSERT INTO payment_reversals (id, payment_id, amount, return_id) VALUES (?1, ?2, ?3, ?4)",
                ["rev-return-a2", "payment-a1", "1000", "return-a2"],
            )?;
            assert!(
                conn.execute(
                    "INSERT INTO payment_reversals (id, payment_id, amount, return_id, annulment_sale_id) VALUES (?1, ?2, ?3, ?4, ?5)",
                    ["rev-both", "payment-a1", "1000", "return-a2", "sale-b1"],
                )
                .is_err(),
                "XOR constraint must reject both sources"
            );
            assert!(
                conn.execute(
                    "INSERT INTO payment_reversals (id, payment_id, amount) VALUES (?1, ?2, ?3)",
                    ["rev-neither", "payment-a1", "1000"],
                )
                .is_err(),
                "XOR constraint must reject missing source"
            );

            Ok::<_, rusqlite::Error>(())
        })
        .await
        .unwrap()
        .unwrap();
    }

    /// Same annulment-sale ownership discrimination on the genuine pre-v3 upgrade path.
    /// The database is seeded with migrations 1 and 2 but no INSERT guards, then
    /// migration 3 installs the source-owned predicates. After the upgrade, the same
    /// sequential fixture creation proves the annulment-sale predicate governs.
    #[tokio::test]
    async fn payment_reversal_annulment_ownership_follows_annulment_sale_upgrade_path() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");

        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(include_str!("../../migrations/001_initial.sql"))
                .unwrap();
            conn.execute_batch(include_str!(
                "../../migrations/002_upgrade_closed_session_immutability.sql"
            ))
            .unwrap();
            // Simulate the pre-v3 state: drop the INSERT triggers that 001 may have
            // contained in a patched development build.
            conn.execute_batch(
                r#"
                DROP TRIGGER IF EXISTS trg_guard_closed_session_sales_insert;
                DROP TRIGGER IF EXISTS trg_guard_closed_session_payments_insert;
                DROP TRIGGER IF EXISTS trg_guard_closed_session_movements_insert;
                DROP TRIGGER IF EXISTS trg_guard_closed_session_drafts_insert;
                DROP TRIGGER IF EXISTS trg_guard_closed_session_returns_insert;
                DROP TRIGGER IF EXISTS trg_guard_closed_session_payment_reversals_insert;
                "#,
            )
            .unwrap();
        }

        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool, &db_path).await.unwrap();

        let conn = pool.get().await.unwrap();
        conn.interact(|conn| {
            // Case 1: session A (payment's sale) is CLOSED, session B (annulled sale) is OPEN.
            conn.execute(
                "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, ?2, ?3, ?4)",
                ["session-a1", "open", "1000", "2024-01-01T00:00:00Z"],
            )?;
            conn.execute(
                "INSERT INTO sales (id, session_id, total_amount, status) VALUES (?1, ?2, ?3, ?4)",
                ["sale-a1", "session-a1", "1000", "confirmed"],
            )?;
            conn.execute(
                "INSERT INTO payments (id, sale_id, method, amount) VALUES (?1, ?2, ?3, ?4)",
                ["payment-a1", "sale-a1", "cash", "1000"],
            )?;
            conn.execute(
                "UPDATE cash_sessions SET status = 'closed', counted_cash = 1000, difference = 0, closed_at = ?2 WHERE id = ?1 AND status = 'open'",
                ["session-a1", "2024-01-01T23:00:00Z"],
            )?;

            conn.execute(
                "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, ?2, ?3, ?4)",
                ["session-b1", "open", "1000", "2024-01-02T00:00:00Z"],
            )?;
            conn.execute(
                "INSERT INTO sales (id, session_id, total_amount, status) VALUES (?1, ?2, ?3, ?4)",
                ["sale-b1", "session-b1", "1000", "confirmed"],
            )?;

            conn.execute(
                "INSERT INTO payment_reversals (id, payment_id, amount, annulment_sale_id) VALUES (?1, ?2, ?3, ?4)",
                ["rev-b1-open", "payment-a1", "1000", "sale-b1"],
            )?;

            conn.execute(
                "UPDATE cash_sessions SET status = 'closed', counted_cash = 1000, difference = 0, closed_at = ?2 WHERE id = ?1 AND status = 'open'",
                ["session-b1", "2024-01-02T23:00:00Z"],
            )?;

            // Case 2: session A (payment's sale) is OPEN, session B (annulled sale) is CLOSED.
            // Create B2 closed first so A2 can remain open for the return-source checks.
            conn.execute(
                "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, ?2, ?3, ?4)",
                ["session-b2", "open", "1000", "2024-01-04T00:00:00Z"],
            )?;
            conn.execute(
                "INSERT INTO sales (id, session_id, total_amount, status) VALUES (?1, ?2, ?3, ?4)",
                ["sale-b2", "session-b2", "1000", "confirmed"],
            )?;
            conn.execute(
                "UPDATE cash_sessions SET status = 'closed', counted_cash = 1000, difference = 0, closed_at = ?2 WHERE id = ?1 AND status = 'open'",
                ["session-b2", "2024-01-04T23:00:00Z"],
            )?;

            conn.execute(
                "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, ?2, ?3, ?4)",
                ["session-a2", "open", "1000", "2024-01-03T00:00:00Z"],
            )?;
            conn.execute(
                "INSERT INTO sales (id, session_id, total_amount, status) VALUES (?1, ?2, ?3, ?4)",
                ["sale-a2", "session-a2", "1000", "confirmed"],
            )?;
            conn.execute(
                "INSERT INTO payments (id, sale_id, method, amount) VALUES (?1, ?2, ?3, ?4)",
                ["payment-a2", "sale-a2", "cash", "1000"],
            )?;

            assert!(
                conn.execute(
                    "INSERT INTO payment_reversals (id, payment_id, amount, annulment_sale_id) VALUES (?1, ?2, ?3, ?4)",
                    ["rev-b2-closed", "payment-a2", "1000", "sale-b2"],
                )
                .is_err(),
                "annulment-based reversal must be rejected when the annulled sale's session is closed after v3 upgrade, regardless of the payment's sale session being open"
            );

            // Return-source and XOR protections remain intact after upgrade.
            conn.execute(
                "INSERT INTO returns (id, session_id, sale_id, return_kind, amount) VALUES (?1, ?2, ?3, ?4, ?5)",
                ["return-a2", "session-a2", "sale-b1", "total", "1000"],
            )?;
            conn.execute(
                "INSERT INTO payment_reversals (id, payment_id, amount, return_id) VALUES (?1, ?2, ?3, ?4)",
                ["rev-return-a2", "payment-a1", "1000", "return-a2"],
            )?;
            assert!(
                conn.execute(
                    "INSERT INTO payment_reversals (id, payment_id, amount, return_id, annulment_sale_id) VALUES (?1, ?2, ?3, ?4, ?5)",
                    ["rev-both", "payment-a1", "1000", "return-a2", "sale-b1"],
                )
                .is_err(),
                "XOR constraint must reject both sources after v3 upgrade"
            );
            assert!(
                conn.execute(
                    "INSERT INTO payment_reversals (id, payment_id, amount) VALUES (?1, ?2, ?3)",
                    ["rev-neither", "payment-a1", "1000"],
                )
                .is_err(),
                "XOR constraint must reject missing source after v3 upgrade"
            );

            Ok::<_, rusqlite::Error>(())
        })
        .await
        .unwrap()
        .unwrap();
    }
}

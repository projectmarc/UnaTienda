use chrono::{DateTime, Duration, Utc};
use deadpool_sqlite::Pool;
use rusqlite::{Connection, DatabaseName, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;
use thiserror::Error;
use tokio::task;
use uuid::Uuid;

use crate::domain::error::AppError;
use crate::persistence::connection::{open_pool, AppDb};

const APP_SCHEMA: &str = "unatienda";
const BACKUP_FORMAT: &str = "1";
const CURRENT_SCHEMA_VERSION: i64 = 6;
const RETENTION_DAYS: i64 = 30;
const JOURNAL_FILE: &str = "restore_swap.json";

// Validate the columns used by runtime SQL instead of comparing full CREATE TABLE text.
const REQUIRED_COLUMNS: &[(&str, &str)] = &[
    ("schema_migrations", "version"),
    (
        "cash_sessions",
        "id,status,opening_amount,counted_cash,difference,opened_at,closed_at",
    ),
    (
        "sales",
        "id,session_id,total_amount,category,details,status,annulment_reason,created_at,updated_at",
    ),
    (
        "payments",
        "id,sale_id,method,amount,recipient_account,created_at",
    ),
    ("drafts", "session_id"),
    ("movements", "id,session_id,kind,amount,note,created_at"),
    ("returns", "id,session_id,sale_id,return_kind,amount"),
    (
        "payment_reversals",
        "id,payment_id,amount,return_id,annulment_sale_id",
    ),
    (
        "idempotency_keys",
        "idempotency_key,command,request_hash,response_json",
    ),
    ("backup_runs", "id,run_at,path,success,error,kind,managed"),
    ("app_metadata", "key,value"),
];

const REQUIRED_FOREIGN_KEYS: &[(&str, &str, &str, &str)] = &[
    ("sales", "session_id", "cash_sessions", "id"),
    ("payments", "sale_id", "sales", "id"),
    ("drafts", "session_id", "cash_sessions", "id"),
    ("movements", "session_id", "cash_sessions", "id"),
    ("returns", "session_id", "cash_sessions", "id"),
    ("returns", "sale_id", "sales", "id"),
    ("payment_reversals", "payment_id", "payments", "id"),
    ("payment_reversals", "return_id", "returns", "id"),
    ("payment_reversals", "annulment_sale_id", "sales", "id"),
];

const REQUIRED_TRIGGER_OPERATIONS: &[(&str, &str)] = &[
    ("cash_sessions", "update,delete"),
    ("sales", "update,delete,insert"),
    ("payments", "update,delete,insert"),
    ("movements", "update,delete,insert"),
    ("drafts", "update,delete,insert"),
    ("returns", "update,delete,insert"),
    ("payment_reversals", "update,delete,insert"),
];

#[derive(Debug, Error)]
pub enum BackupError {
    #[error("backup I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("backup database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("backup pool error: {0}")]
    Pool(#[from] deadpool_sqlite::PoolError),
    #[error("backup interaction error: {0}")]
    Interact(#[from] deadpool_sqlite::InteractError),
    #[error("backup task failed: {0}")]
    Task(String),
    #[error("incompatible backup: {0}")]
    Incompatible(String),
    #[error("restore is blocked while a cash session is open")]
    RestoreBlockedOpenSession,
    #[error("restore recovery failed: {0}")]
    Recovery(String),
    #[error("no backup is selected")]
    NoPendingRestore,
    #[error("invalid export destination: {0}")]
    InvalidDestination(String),
}

impl BackupError {
    pub fn to_app_error(&self) -> AppError {
        match self {
            Self::Incompatible(_) => AppError::IncompatibleBackup,
            Self::RestoreBlockedOpenSession => AppError::RestoreBlockedOpenSession,
            Self::NoPendingRestore => AppError::Validation {
                message: self.to_string(),
                field: None,
            },
            _ => AppError::Backup(self.to_string()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupKind {
    Daily,
    SessionClose,
    PreRestore,
}

impl BackupKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::SessionClose => "session_close",
            Self::PreRestore => "pre_restore",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSummary {
    pub backup_timestamp: String,
    pub file_name: String,
    pub schema_version: i64,
    pub session_count: i64,
    pub sale_count: i64,
    pub movement_count: i64,
    pub return_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupStatus {
    pub id: String,
    pub run_at: String,
    pub kind: String,
    pub success: bool,
    pub available: bool,
    pub file_name: String,
    pub size_bytes: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupPanel {
    pub last_successful: Option<BackupStatus>,
    pub backups: Vec<BackupStatus>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreSelection {
    pub summary: BackupSummary,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    pub summary: BackupSummary,
    pub restart_required: bool,
}

#[derive(Debug, Clone)]
pub struct PendingRestore {
    path: PathBuf,
    size: u64,
    modified: Option<u128>,
}

#[derive(Debug, Clone)]
pub struct BackupManager {
    live_path: PathBuf,
    managed_dir: PathBuf,
    journal_path: PathBuf,
    faults: Arc<Mutex<FaultState>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RestorePhase {
    // Before the swap, recovery keeps the current database in place.
    Staged,
    // Every non-terminal phase is an instruction to reinstall snapshot_path.
    PreSwap,
    PostSwap,
    // Verified is the only committed terminal phase.
    Verified,
    // Rollback means the snapshot installation is in progress, not cleanup-only.
    Rollback,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RestoreJournal {
    phase: RestorePhase,
    live_path: PathBuf,
    staged_path: PathBuf,
    snapshot_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum FaultPoint {
    JournalWrite,
    Install,
    Fsync,
    Rollback,
    PoolReopen,
}

#[derive(Debug, Default)]
struct FaultState {
    counts: HashMap<FaultPoint, usize>,
    fail_at: HashSet<(FaultPoint, usize)>,
}

impl BackupManager {
    pub fn new(live_path: impl Into<PathBuf>) -> Self {
        let live_path = live_path.into();
        let parent = live_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        Self {
            managed_dir: parent.join("backups"),
            journal_path: parent.join(JOURNAL_FILE),
            live_path,
            faults: Arc::new(Mutex::new(FaultState::default())),
        }
    }

    pub fn managed_dir(&self) -> &Path {
        &self.managed_dir
    }

    pub fn journal_path(&self) -> &Path {
        &self.journal_path
    }

    fn check_fault(&self, point: FaultPoint) -> Result<(), BackupError> {
        let mut faults = self
            .faults
            .lock()
            .map_err(|_| BackupError::Task("fault controller is poisoned".into()))?;
        let occurrence = {
            let count = faults.counts.entry(point).or_insert(0);
            *count += 1;
            *count
        };
        if faults.fail_at.remove(&(point, occurrence)) {
            return Err(BackupError::Task(format!(
                "injected restore fault at {:?} occurrence {}",
                point, occurrence
            )));
        }
        Ok(())
    }

    #[cfg(test)]
    fn inject_fault(&self, point: FaultPoint, occurrence: usize) {
        self.faults
            .lock()
            .unwrap()
            .fail_at
            .insert((point, occurrence));
    }

    fn write_restore_journal(&self, journal: &RestoreJournal) -> Result<(), BackupError> {
        self.check_fault(FaultPoint::JournalWrite)?;
        write_journal_file(&self.journal_path, journal)
    }

    fn install_restore_database(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<(), BackupError> {
        self.check_fault(FaultPoint::Install)?;
        self.check_fault(FaultPoint::Fsync)?;
        install_database_file(source, destination)
    }

    fn install_snapshot_database(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<(), BackupError> {
        self.check_fault(FaultPoint::Install)?;
        self.check_fault(FaultPoint::Fsync)?;
        let temporary =
            destination.with_file_name(format!(".restore-rollback-{}.db", Uuid::new_v4()));
        let result = copy_and_sync(source, &temporary)
            .and_then(|_| install_database_file(&temporary, destination));
        let _ = fs::remove_file(&temporary);
        result
    }

    fn stage_restore(&self, source: &Path, destination: &Path) -> Result<(), BackupError> {
        self.check_fault(FaultPoint::Fsync)?;
        copy_and_sync(source, destination)
    }

    async fn reopen_pool(&self, path: &Path) -> Result<Pool, BackupError> {
        self.check_fault(FaultPoint::PoolReopen)?;
        open_pool(path)
            .await
            .map_err(|error| BackupError::Task(error.to_string()))
    }

    pub async fn run_startup_backup(
        &self,
        pool: &Pool,
        now: DateTime<Utc>,
    ) -> Result<(), BackupError> {
        let today = now.format("%Y-%m-%d").to_string();
        let already_done = self.has_successful_daily(pool, &today).await?;
        let result = if already_done {
            Ok(())
        } else {
            self.create_managed_backup(pool, BackupKind::Daily, now)
                .await
                .map(|_| ())
        };

        let prune_result = self.prune(pool, now).await;
        result.and(prune_result)
    }

    pub async fn create_session_close_backup(
        &self,
        pool: &Pool,
        now: DateTime<Utc>,
    ) -> Result<(), BackupError> {
        self.create_managed_backup(pool, BackupKind::SessionClose, now)
            .await
            .map(|_| ())
    }

    pub async fn panel(&self, pool: &Pool) -> Result<BackupPanel, BackupError> {
        let backups = self.list(pool).await?;
        let last_successful = backups.iter().find(|backup| backup.success).cloned();
        Ok(BackupPanel {
            last_successful,
            backups,
        })
    }

    pub async fn export_to(
        &self,
        pool: &Pool,
        destination: PathBuf,
    ) -> Result<BackupSummary, BackupError> {
        self.validate_export_destination(&destination)?;
        fs::create_dir_all(&self.managed_dir)?;

        let snapshot_path = self
            .managed_dir
            .join(format!(".export-{}.db", Uuid::new_v4()));
        let result = async {
            self.create_snapshot_file(pool, &snapshot_path).await?;
            let summary = self.validate_async(snapshot_path.clone()).await?;
            let destination_parent = destination.parent().ok_or_else(|| {
                BackupError::InvalidDestination("missing parent directory".into())
            })?;
            fs::create_dir_all(destination_parent)?;
            let temporary_destination = destination_parent.join(format!(
                ".{}.export-{}.tmp",
                destination
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("backup"),
                Uuid::new_v4()
            ));
            copy_and_sync(&snapshot_path, &temporary_destination)?;
            if let Err(error) = fs::rename(&temporary_destination, &destination) {
                let _ = fs::remove_file(&temporary_destination);
                return Err(BackupError::Io(error));
            }
            sync_directory(destination_parent)?;
            if let Err(error) = self.validate_async(destination.clone()).await {
                let _ = fs::remove_file(&destination);
                return Err(error);
            }
            Ok(summary)
        }
        .await;

        let _ = fs::remove_file(&snapshot_path);
        result
    }

    pub async fn inspect_backup(
        &self,
        path: PathBuf,
    ) -> Result<(PendingRestore, RestoreSelection), BackupError> {
        let path = fs::canonicalize(path)?;
        if fs::canonicalize(&self.live_path)
            .map(|live_path| live_path == path)
            .unwrap_or(false)
        {
            return Err(BackupError::Incompatible(
                "the live database is not a valid restore source".into(),
            ));
        }
        let metadata = fs::metadata(&path)?;
        if !metadata.is_file() {
            return Err(BackupError::Incompatible(
                "the selected path is not a file".into(),
            ));
        }
        let summary = self.validate_async(path.clone()).await?;
        let (size, modified) = fingerprint(&path)?;
        Ok((
            PendingRestore {
                path,
                size,
                modified,
            },
            RestoreSelection { summary },
        ))
    }

    pub async fn restore_live(
        &self,
        db: &mut AppDb,
        pending: &PendingRestore,
    ) -> Result<RestoreResult, BackupError> {
        if fs::canonicalize(&self.live_path)
            .map(|live_path| live_path == pending.path)
            .unwrap_or(false)
        {
            return Err(BackupError::Incompatible(
                "the live database is not a valid restore source".into(),
            ));
        }
        if !fingerprint_matches(&pending.path, pending.size, pending.modified)? {
            return Err(BackupError::Incompatible(
                "the selected backup changed after validation".into(),
            ));
        }

        let summary = self.validate_async(pending.path.clone()).await?;
        let pool = db
            .pool()
            .map_err(|error| BackupError::Recovery(error.to_string()))?;
        if self.has_open_session(pool).await? {
            return Err(BackupError::RestoreBlockedOpenSession);
        }

        fs::create_dir_all(&self.managed_dir)?;
        let snapshot_path = self
            .managed_dir
            .join(format!("unatienda-pre-restore-{}.db", Uuid::new_v4()));
        let pool = db
            .pool()
            .map_err(|error| BackupError::Recovery(error.to_string()))?;
        self.create_snapshot_file(pool, &snapshot_path).await?;
        self.validate_async(snapshot_path.clone()).await?;
        let pool = db
            .pool()
            .map_err(|error| BackupError::Recovery(error.to_string()))?;
        self.record_run(
            pool,
            BackupKind::PreRestore,
            &snapshot_path,
            true,
            None,
            Utc::now(),
        )
        .await?;

        let staged_path = self
            .managed_dir
            .join(format!(".restore-stage-{}.db", Uuid::new_v4()));
        let mut journal = RestoreJournal {
            phase: RestorePhase::Staged,
            live_path: self.live_path.clone(),
            staged_path: staged_path.clone(),
            snapshot_path: snapshot_path.clone(),
        };
        self.write_restore_journal(&journal)?;

        if let Err(error) = self
            .stage_restore(&pending.path, &staged_path)
            .and_then(|_| Ok(sync_directory(&self.managed_dir)?))
            .and_then(|_| validate_backup_path(&staged_path).map(|_| ()))
        {
            let _ = fs::remove_file(&staged_path);
            if let Err(cleanup) = remove_journal(&self.journal_path) {
                return Err(BackupError::Recovery(format!(
                    "{error}; restore journal cleanup failed: {cleanup}"
                )));
            }
            return Err(error.into());
        }

        journal.phase = RestorePhase::PreSwap;
        self.write_restore_journal(&journal)?;
        db.close();

        if let Err(error) = self.install_restore_database(&staged_path, &self.live_path) {
            return Err(self.rollback_after_swap(db, &journal, error.into()).await);
        }

        journal.phase = RestorePhase::PostSwap;
        if let Err(error) = self.write_restore_journal(&journal) {
            return Err(self.rollback_after_swap(db, &journal, error).await);
        }

        let replacement_pool = match self.reopen_pool(&self.live_path).await {
            Ok(pool) => pool,
            Err(error) => {
                return Err(self.rollback_after_swap(db, &journal, error).await);
            }
        };

        if let Err(error) = self.validate_async(self.live_path.clone()).await {
            replacement_pool.close();
            return Err(self.rollback_after_swap(db, &journal, error).await);
        }

        // Keep the preventive snapshot visible after the restored database becomes active.
        if let Err(error) = self
            .record_run(
                &replacement_pool,
                BackupKind::PreRestore,
                &snapshot_path,
                true,
                None,
                Utc::now(),
            )
            .await
        {
            replacement_pool.close();
            return Err(self.rollback_after_swap(db, &journal, error).await);
        }

        journal.phase = RestorePhase::Verified;
        if let Err(error) = self.write_restore_journal(&journal) {
            replacement_pool.close();
            return Err(self
                .rollback_after_swap(
                    db,
                    &journal,
                    BackupError::Recovery(format!(
                        "restored database was verified but the terminal restore journal could not be committed: {error}"
                    )),
                )
                .await);
        }

        db.replace_pool(replacement_pool);
        let staged_cleanup_failed = fs::remove_file(&staged_path)
            .err()
            .filter(|error| error.kind() != io::ErrorKind::NotFound);
        if staged_cleanup_failed.is_some() {
            return Ok(RestoreResult {
                summary,
                restart_required: true,
            });
        }
        let journal_cleanup_failed = remove_journal(&self.journal_path).is_err();

        Ok(RestoreResult {
            summary,
            restart_required: journal_cleanup_failed,
        })
    }

    pub fn recover_interrupted_restore(&self) -> Result<(), BackupError> {
        if !self.journal_path.exists() {
            return Ok(());
        }

        let bytes = fs::read(&self.journal_path)?;
        let journal: RestoreJournal = serde_json::from_slice(&bytes)
            .map_err(|error| BackupError::Recovery(error.to_string()))?;

        match journal.phase {
            RestorePhase::Staged
            | RestorePhase::PreSwap
            | RestorePhase::PostSwap
            | RestorePhase::Rollback => {
                // These phases are all non-terminal. Reinstalling the validated snapshot is
                // idempotent and removes ambiguity about whether a crash happened before or
                // after the live-file replacement.
                self.install_snapshot_database(&journal.snapshot_path, &journal.live_path)?;
                validate_backup_path(&journal.live_path)?;
                remove_file_if_present(&journal.staged_path)?;
                remove_journal(&self.journal_path)?;
            }
            RestorePhase::Verified => {
                remove_file_if_present(&journal.staged_path)?;
                remove_journal(&self.journal_path)?;
            }
        }

        Ok(())
    }

    async fn create_managed_backup(
        &self,
        pool: &Pool,
        kind: BackupKind,
        now: DateTime<Utc>,
    ) -> Result<PathBuf, BackupError> {
        let final_path = self.managed_path(kind, now);
        let temporary_path = self.managed_dir.join(format!(
            ".{}.tmp",
            final_path.file_name().unwrap().to_string_lossy()
        ));

        let result: Result<PathBuf, BackupError> = async {
            fs::create_dir_all(&self.managed_dir)?;
            self.create_snapshot_file(pool, &temporary_path).await?;
            self.validate_async(temporary_path.clone()).await?;
            fs::rename(&temporary_path, &final_path)?;
            sync_directory(&self.managed_dir)?;
            self.validate_async(final_path.clone()).await?;
            self.record_run(pool, kind, &final_path, true, None, now)
                .await?;
            Ok(final_path.clone())
        }
        .await;

        if let Err(error) = result {
            let _ = fs::remove_file(&temporary_path);
            let _ = self
                .record_run(pool, kind, &final_path, false, Some(error.to_string()), now)
                .await;
            return Err(error);
        }

        result
    }

    async fn create_snapshot_file(&self, pool: &Pool, target: &Path) -> Result<(), BackupError> {
        let target = target.to_path_buf();
        let connection = pool.get().await?;
        connection
            .interact(move |connection| {
                if target.exists() {
                    fs::remove_file(&target)?;
                }
                connection.backup(DatabaseName::Main, &target, None)?;
                sync_file(&target)?;
                Ok::<_, BackupError>(())
            })
            .await??;
        Ok(())
    }

    async fn validate_async(&self, path: PathBuf) -> Result<BackupSummary, BackupError> {
        task::spawn_blocking(move || validate_backup_path(&path))
            .await
            .map_err(|error| BackupError::Task(error.to_string()))?
    }

    async fn has_successful_daily(&self, pool: &Pool, day: &str) -> Result<bool, BackupError> {
        let day = day.to_string();
        let connection = pool.get().await?;
        let paths = connection
            .interact(move |connection| {
                let mut statement = connection.prepare(
                    "SELECT path FROM backup_runs WHERE kind = 'daily' AND success = 1 AND managed = 1 AND substr(run_at, 1, 10) = ?1",
                )?;
                let paths = statement
                    .query_map([day], |row| row.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, rusqlite::Error>>()?;
                Ok::<_, rusqlite::Error>(paths)
            })
            .await??;

        for path in paths.into_iter().map(PathBuf::from) {
            if is_within(&path, &self.managed_dir)
                && path.is_file()
                && self.validate_async(path).await.is_ok()
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn has_open_session(&self, pool: &Pool) -> Result<bool, BackupError> {
        let connection = pool.get().await?;
        connection
            .interact(|connection| {
                let present: i64 = connection.query_row(
                    "SELECT EXISTS(SELECT 1 FROM cash_sessions WHERE status = 'open')",
                    [],
                    |row| row.get(0),
                )?;
                Ok::<_, rusqlite::Error>(present != 0)
            })
            .await?
            .map_err(BackupError::Sqlite)
    }

    async fn record_run(
        &self,
        pool: &Pool,
        kind: BackupKind,
        path: &Path,
        success: bool,
        error: Option<String>,
        run_at: DateTime<Utc>,
    ) -> Result<(), BackupError> {
        let id = Uuid::new_v4().to_string();
        let kind = kind.as_str().to_string();
        let path = path.to_string_lossy().to_string();
        let run_at = run_at.to_rfc3339();
        let connection = pool.get().await?;
        connection
            .interact(move |connection| {
                connection.execute(
                    "INSERT INTO backup_runs (id, run_at, path, success, error, kind, managed) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)",
                    rusqlite::params![id, run_at, path, success as i64, error, kind],
                )?;
                Ok::<_, rusqlite::Error>(())
            })
            .await??;
        Ok(())
    }

    async fn list(&self, pool: &Pool) -> Result<Vec<BackupStatus>, BackupError> {
        let connection = pool.get().await?;
        let rows = connection
            .interact(|connection| {
                let mut statement = connection.prepare(
                    "SELECT id, run_at, kind, success, path, error FROM backup_runs WHERE managed = 1 ORDER BY run_at DESC",
                )?;
                let rows = statement
                    .query_map([], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, i64>(3)? != 0,
                            PathBuf::from(row.get::<_, String>(4)?),
                            row.get::<_, Option<String>>(5)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, rusqlite::Error>>()?;
                Ok::<_, rusqlite::Error>(rows)
            })
            .await??;

        Ok(rows
            .into_iter()
            .map(|(id, run_at, kind, success, path, error)| {
                let metadata = fs::metadata(&path).ok();
                BackupStatus {
                    id,
                    run_at,
                    kind,
                    success,
                    available: metadata.is_some(),
                    file_name: path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("backup.db")
                        .to_string(),
                    size_bytes: metadata.map(|value| value.len()),
                    error,
                }
            })
            .collect())
    }

    async fn prune(&self, pool: &Pool, now: DateTime<Utc>) -> Result<(), BackupError> {
        let cutoff = (now - Duration::days(RETENTION_DAYS)).to_rfc3339();
        let connection = pool.get().await?;
        let old_runs = connection
            .interact(move |connection| {
                let mut statement = connection.prepare(
                    "SELECT id, path FROM backup_runs WHERE managed = 1 AND run_at < ?1",
                )?;
                let rows = statement
                    .query_map([cutoff], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            PathBuf::from(row.get::<_, String>(1)?),
                        ))
                    })?
                    .collect::<Result<Vec<_>, rusqlite::Error>>()?;
                Ok::<_, rusqlite::Error>(rows)
            })
            .await??;

        for (id, path) in old_runs {
            if !is_within(&path, &self.managed_dir) {
                continue;
            }
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            let connection = pool.get().await?;
            connection
                .interact(move |connection| {
                    connection.execute("DELETE FROM backup_runs WHERE id = ?1", [&id])?;
                    Ok::<_, rusqlite::Error>(())
                })
                .await??;
        }
        sync_directory(&self.managed_dir)?;
        Ok(())
    }

    fn managed_path(&self, kind: BackupKind, now: DateTime<Utc>) -> PathBuf {
        let label = match kind {
            BackupKind::Daily => "daily",
            BackupKind::SessionClose => "session-close",
            BackupKind::PreRestore => "pre-restore",
        };
        self.managed_dir.join(format!(
            "unatienda-{}-{}-{}.db",
            label,
            now.format("%Y%m%d-%H%M%S"),
            Uuid::new_v4()
        ))
    }

    async fn rollback_after_swap(
        &self,
        db: &mut AppDb,
        journal: &RestoreJournal,
        error: BackupError,
    ) -> BackupError {
        if let Err(injected) = self.check_fault(FaultPoint::Rollback) {
            db.fail_closed(injected.to_string());
            return BackupError::Recovery(format!(
                "{error}; rollback was not attempted: {}",
                injected
            ));
        }

        let mut rollback_errors = Vec::new();
        let mut rollback_journal = journal.clone();
        rollback_journal.phase = RestorePhase::Rollback;
        if let Err(rollback) = self.write_restore_journal(&rollback_journal) {
            rollback_errors.push(format!("rollback journal failed: {rollback}"));
        }
        if let Err(rollback) = self
            .install_snapshot_database(&journal.snapshot_path, &journal.live_path)
            .and_then(|_| validate_backup_path(&journal.live_path).map(|_| ()))
        {
            rollback_errors.push(format!("database reinstall failed: {rollback}"));
        }
        if rollback_errors.is_empty() {
            match self.reopen_pool(&self.live_path).await {
                Ok(pool) => db.replace_pool(pool),
                Err(rollback) => {
                    rollback_errors.push(format!("pool reopen failed: {rollback}"));
                }
            }
        }
        if rollback_errors.is_empty() {
            if let Err(cleanup) = remove_file_if_present(&journal.staged_path) {
                rollback_errors.push(format!("staged-file cleanup failed: {cleanup}"));
            }
        }
        if rollback_errors.is_empty() {
            if let Err(cleanup) = remove_journal(&self.journal_path) {
                rollback_errors.push(format!("rollback journal cleanup failed: {cleanup}"));
            }
        }
        if rollback_errors.is_empty() {
            error
        } else {
            let details = rollback_errors.join("; ");
            db.fail_closed(details.clone());
            BackupError::Recovery(format!(
                "{error}; original database rollback failed: {details}; restart the application to recover"
            ))
        }
    }

    fn validate_export_destination(&self, destination: &Path) -> Result<(), BackupError> {
        if destination.as_os_str().is_empty() {
            return Err(BackupError::InvalidDestination("empty path".into()));
        }
        if destination == self.live_path || is_within(destination, self.managed_dir.as_path()) {
            return Err(BackupError::InvalidDestination(
                "the application database directory cannot be exported to".into(),
            ));
        }
        if destination.exists() {
            return Err(BackupError::InvalidDestination(
                "the destination already exists; choose another file".into(),
            ));
        }
        Ok(())
    }
}

fn validate_backup_path(path: &Path) -> Result<BackupSummary, BackupError> {
    let metadata = fs::metadata(path)?;
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let integrity: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(BackupError::Incompatible(format!(
            "integrity_check returned {}",
            integrity
        )));
    }

    let foreign_key_violations: i64 = connection
        .prepare("PRAGMA foreign_key_check")?
        .query_map([], |_| Ok(()))?
        .count() as i64;
    if foreign_key_violations > 0 {
        return Err(BackupError::Incompatible(
            "foreign key validation failed".into(),
        ));
    }

    for table in [
        "cash_sessions",
        "sales",
        "payments",
        "drafts",
        "movements",
        "returns",
        "payment_reversals",
        "idempotency_keys",
        "backup_runs",
        "app_metadata",
        "schema_migrations",
    ] {
        let present: i64 = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [table],
            |row| row.get(0),
        )?;
        if present == 0 {
            return Err(BackupError::Incompatible(format!(
                "required table {} is missing",
                table
            )));
        }
    }

    validate_schema_contract(&connection)?;

    for version in 1..=CURRENT_SCHEMA_VERSION {
        let present: i64 = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
            [version],
            |row| row.get(0),
        )?;
        if present == 0 {
            return Err(BackupError::Incompatible(format!(
                "schema migration {} is missing",
                version
            )));
        }
    }

    let app_schema: String = connection.query_row(
        "SELECT value FROM app_metadata WHERE key = 'app_schema'",
        [],
        |row| row.get(0),
    )?;
    let backup_format: String = connection.query_row(
        "SELECT value FROM app_metadata WHERE key = 'backup_format'",
        [],
        |row| row.get(0),
    )?;
    if app_schema != APP_SCHEMA || backup_format != BACKUP_FORMAT {
        return Err(BackupError::Incompatible(
            "application metadata is not compatible".into(),
        ));
    }

    let schema_version: i64 =
        connection.query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })?;
    let session_count = count_rows(&connection, "cash_sessions")?;
    let sale_count = count_rows(&connection, "sales")?;
    let movement_count = count_rows(&connection, "movements")?;
    let return_count = count_rows(&connection, "returns")?;
    let timestamp = metadata
        .modified()
        .ok()
        .map(|value| DateTime::<Utc>::from(value).to_rfc3339())
        .unwrap_or_else(|| "desconocido".to_string());

    Ok(BackupSummary {
        backup_timestamp: timestamp,
        file_name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("backup.db")
            .to_string(),
        schema_version,
        session_count,
        sale_count,
        movement_count,
        return_count,
    })
}

fn validate_schema_contract(connection: &Connection) -> Result<(), BackupError> {
    for &(table, required_columns) in REQUIRED_COLUMNS {
        let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
        let present = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<HashSet<_>, _>>()?;
        for column in required_columns.split(',') {
            if !present.contains(column) {
                return Err(BackupError::Incompatible(format!(
                    "required column {table}.{column} is missing"
                )));
            }
        }
    }

    for &(table, from, referenced_table, to) in REQUIRED_FOREIGN_KEYS {
        let mut statement = connection.prepare(&format!("PRAGMA foreign_key_list({table})"))?;
        let present = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        if !present.iter().any(|(target, source, target_column)| {
            target == referenced_table && source == from && target_column == to
        }) {
            return Err(BackupError::Incompatible(format!(
                "required foreign key {table}.{from} -> {referenced_table}.{to} is missing"
            )));
        }
    }

    let mut statement = connection.prepare("PRAGMA index_list(cash_sessions)")?;
    let open_session_index = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .find(|(name, _, _)| name == "idx_one_open_session");
    if !matches!(open_session_index, Some((_, 1, 1))) {
        return Err(BackupError::Incompatible(
            "required partial unique index idx_one_open_session is missing or incompatible".into(),
        ));
    }

    for &(table, operations) in REQUIRED_TRIGGER_OPERATIONS {
        for operation in operations.split(',') {
            let name = format!("trg_guard_closed_session_{table}_{operation}");
            let trigger: Option<(String, String)> = connection
                .query_row(
                    "SELECT tbl_name, sql FROM sqlite_master WHERE type = 'trigger' AND name = ?1",
                    [&name],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let Some((target_table, definition)) = trigger else {
                return Err(BackupError::Incompatible(format!(
                    "required trigger {name} is missing"
                )));
            };
            let normalized = definition
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_ascii_lowercase();
            let signature = format!("before {operation} on {table}");
            if target_table != table
                || !normalized.contains(&signature)
                || !normalized.contains("for each row")
                || !normalized.contains("when")
                || !normalized.contains("raise(abort")
                || !normalized.contains("closed session")
            {
                return Err(BackupError::Incompatible(format!(
                    "required trigger {name} definition is incompatible"
                )));
            }
        }
    }

    Ok(())
}

fn count_rows(connection: &Connection, table: &str) -> Result<i64, rusqlite::Error> {
    connection.query_row(&format!("SELECT COUNT(*) FROM {}", table), [], |row| {
        row.get(0)
    })
}

fn fingerprint(path: &Path) -> Result<(u64, Option<u128>), BackupError> {
    let metadata = fs::metadata(path)?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_nanos());
    Ok((metadata.len(), modified))
}

fn fingerprint_matches(
    path: &Path,
    size: u64,
    modified: Option<u128>,
) -> Result<bool, BackupError> {
    let (current_size, current_modified) = fingerprint(path)?;
    Ok(current_size == size && current_modified == modified)
}

fn copy_and_sync(source: &Path, destination: &Path) -> Result<(), BackupError> {
    fs::copy(source, destination)?;
    sync_file(destination)?;
    Ok(())
}

fn install_database_file(source: &Path, destination: &Path) -> Result<(), BackupError> {
    remove_sidecar(destination, "-wal")?;
    remove_sidecar(destination, "-shm")?;
    #[cfg(unix)]
    {
        // Unix rename replaces the destination atomically after the journal is durable.
        fs::rename(source, destination)?;
    }
    #[cfg(windows)]
    {
        // MoveFileExW replaces in one filesystem operation and asks Windows to flush the
        // replacement. The journal remains the recovery authority if the platform cannot
        // guarantee the requested durability (for example, removable/network storage).
        replace_file_windows(source, destination)?;
    }
    #[cfg(all(not(unix), not(windows)))]
    {
        // This platform has no standard atomic replacement primitive. The durable journal
        // and pre-restore snapshot make the brief replacement gap recoverable on restart.
        if destination.exists() {
            fs::remove_file(destination)?;
        }
        fs::rename(source, destination)?;
    }
    sync_file(destination)?;
    if let Some(parent) = destination.parent() {
        sync_directory(parent)?;
    }
    Ok(())
}

#[cfg(windows)]
fn replace_file_windows(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: u32,
        ) -> i32;
    }

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn remove_sidecar(path: &Path, suffix: &str) -> Result<(), BackupError> {
    let sidecar = PathBuf::from(format!("{}{}", path.to_string_lossy(), suffix));
    match fs::remove_file(sidecar) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn write_journal_file(path: &Path, journal: &RestoreJournal) -> Result<(), BackupError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(journal)
        .map_err(|error| BackupError::Recovery(error.to_string()))?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    fs::rename(&temporary, path)?;
    sync_directory(parent)?;
    Ok(())
}

fn remove_journal(path: &Path) -> Result<(), BackupError> {
    match fs::remove_file(path) {
        Ok(()) => {
            if let Some(parent) = path.parent() {
                sync_directory(parent)?;
            }
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn remove_file_if_present(path: &Path) -> Result<(), BackupError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn sync_file(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

fn sync_directory(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        File::open(path)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        // Windows does not expose directory fsync through the standard library. File
        // write-through and the restore journal provide the strongest available guarantee;
        // startup recovery is required if directory metadata durability is uncertain.
        let _ = path;
        Ok(())
    }
}

fn is_within(path: &Path, directory: &Path) -> bool {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let directory = directory
        .canonicalize()
        .unwrap_or_else(|_| directory.to_path_buf());
    path.starts_with(directory)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::connection::{open_pool, run_migrations};
    use tempfile::TempDir;

    async fn setup() -> (TempDir, BackupManager, Pool) {
        let directory = TempDir::new().unwrap();
        let live_path = directory.path().join("unatienda.db");
        let pool = open_pool(&live_path).await.unwrap();
        run_migrations(&pool, &live_path).await.unwrap();
        (directory, BackupManager::new(live_path), pool)
    }

    #[test]
    fn restore_selection_serializes_as_an_explicit_summary_wrapper() {
        let selection = RestoreSelection {
            summary: BackupSummary {
                backup_timestamp: "2026-08-03T18:00:00Z".into(),
                file_name: "unatienda-daily-20260803.db".into(),
                schema_version: 6,
                session_count: 4,
                sale_count: 12,
                movement_count: 3,
                return_count: 1,
            },
        };

        assert_eq!(
            serde_json::to_value(selection).unwrap(),
            serde_json::json!({
                "summary": {
                    "backupTimestamp": "2026-08-03T18:00:00Z",
                    "fileName": "unatienda-daily-20260803.db",
                    "schemaVersion": 6,
                    "sessionCount": 4,
                    "saleCount": 12,
                    "movementCount": 3,
                    "returnCount": 1,
                },
            })
        );
    }

    #[tokio::test]
    async fn daily_backup_is_deduplicated() {
        let (_directory, manager, pool) = setup().await;
        let now = DateTime::parse_from_rfc3339("2026-08-03T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        manager.run_startup_backup(&pool, now).await.unwrap();
        manager.run_startup_backup(&pool, now).await.unwrap();

        let panel = manager.panel(&pool).await.unwrap();
        assert_eq!(
            panel
                .backups
                .iter()
                .filter(|backup| backup.kind == "daily" && backup.success)
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn session_close_backup_is_recorded() {
        let (_directory, manager, pool) = setup().await;
        crate::application::cash_session_use_case::open_session(&pool, 100, "open-hook")
            .await
            .unwrap();
        crate::application::cash_session_use_case::close_session(&pool, 100, "close-hook")
            .await
            .unwrap();
        manager
            .create_session_close_backup(
                &pool,
                DateTime::parse_from_rfc3339("2026-08-03T10:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            )
            .await
            .unwrap();

        let panel = manager.panel(&pool).await.unwrap();
        assert!(panel
            .backups
            .iter()
            .any(|backup| backup.kind == "session_close" && backup.success));
    }

    #[tokio::test]
    async fn rejects_corrupt_and_incompatible_backups() {
        let (directory, manager, _pool) = setup().await;
        let corrupt = directory.path().join("corrupt.db");
        fs::write(&corrupt, b"not sqlite").unwrap();
        assert!(matches!(
            manager.inspect_backup(corrupt).await,
            Err(BackupError::Sqlite(_)) | Err(BackupError::Incompatible(_))
        ));

        let incompatible = directory.path().join("incompatible.db");
        let connection = Connection::open(&incompatible).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE app_metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL); INSERT INTO app_metadata VALUES ('app_schema', 'other'); INSERT INTO app_metadata VALUES ('backup_format', '1');",
            )
            .unwrap();
        drop(connection);
        assert!(matches!(
            manager.inspect_backup(incompatible).await,
            Err(BackupError::Incompatible(_))
        ));
    }

    #[tokio::test]
    async fn rejects_backups_with_incomplete_runtime_schema() {
        for (label, mutation, expected) in [
            (
                "column",
                "ALTER TABLE payments DROP COLUMN recipient_account",
                "required column payments.recipient_account",
            ),
            (
                "trigger",
                "DROP TRIGGER trg_guard_closed_session_sales_insert",
                "required trigger trg_guard_closed_session_sales_insert",
            ),
        ] {
            let (directory, manager, pool) = setup().await;
            let malformed = directory.path().join(format!("malformed-{label}.db"));
            manager
                .create_snapshot_file(&pool, &malformed)
                .await
                .unwrap();
            let connection = Connection::open(&malformed).unwrap();
            connection.execute_batch(mutation).unwrap();
            drop(connection);

            let result = manager.inspect_backup(malformed).await;
            assert!(
                matches!(result, Err(BackupError::Incompatible(ref message)) if message.contains(expected)),
                "malformed {label} backup was accepted: {result:?}"
            );
        }
    }

    #[tokio::test]
    async fn export_round_trip_preserves_records() {
        let (directory, manager, pool) = setup().await;
        let session_id = Uuid::new_v4().to_string();
        pool.get()
            .await
            .unwrap()
            .interact({
                let session_id = session_id.clone();
                move |connection| {
                    connection.execute(
                        "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, 'closed', 100, '2026-08-03T10:00:00Z')",
                        [&session_id],
                    )?;
                    Ok::<_, rusqlite::Error>(())
                }
            })
            .await
            .unwrap()
            .unwrap();
        let destination = directory.path().join("export.db");
        let summary = manager.export_to(&pool, destination.clone()).await.unwrap();
        assert_eq!(summary.session_count, 1);
        assert!(destination.exists());
        validate_backup_path(&destination).unwrap();

        let exported =
            Connection::open_with_flags(&destination, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let exported_record: (String, String, i64) = exported
            .query_row(
                "SELECT id, status, opening_amount FROM cash_sessions",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(exported_record, (session_id.clone(), "closed".into(), 100));
        drop(exported);

        let changed_id = Uuid::new_v4().to_string();
        pool.get()
            .await
            .unwrap()
            .interact({
                let changed_id = changed_id.clone();
                move |connection| {
                    connection.execute(
                        "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, 'closed', 200, '2026-08-03T11:00:00Z')",
                        [&changed_id],
                    )?;
                    Ok::<_, rusqlite::Error>(())
                }
            })
            .await
            .unwrap()
            .unwrap();
        let (pending, _) = manager.inspect_backup(destination).await.unwrap();
        let mut db = AppDb::new(pool);
        manager.restore_live(&mut db, &pending).await.unwrap();
        let restored_record: (String, i64) = db
            .pool()
            .unwrap()
            .get()
            .await
            .unwrap()
            .interact(|connection| {
                connection.query_row("SELECT id, opening_amount FROM cash_sessions", [], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(restored_record, (session_id, 100));
    }

    #[tokio::test]
    async fn restore_is_blocked_with_open_session() {
        let (directory, manager, pool) = setup().await;
        let source = directory.path().join("source.db");
        manager.export_to(&pool, source.clone()).await.unwrap();
        let (pending, _) = manager.inspect_backup(source).await.unwrap();
        let session_id = Uuid::new_v4().to_string();
        pool.get()
            .await
            .unwrap()
            .interact(move |connection| {
                connection
                    .execute(
                        "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, 'open', 100, '2026-08-03T10:00:00Z')",
                        [&session_id],
                    )
                    .unwrap();
            })
            .await
            .unwrap();
        let mut db = AppDb::new(pool.clone());
        assert!(matches!(
            manager.restore_live(&mut db, &pending).await,
            Err(BackupError::RestoreBlockedOpenSession)
        ));
    }

    #[tokio::test]
    async fn restore_replaces_database_and_failure_keeps_current_data() {
        let (directory, manager, pool) = setup().await;
        let current_id = Uuid::new_v4().to_string();
        pool.get()
            .await
            .unwrap()
            .interact({
                let current_id = current_id.clone();
                move |connection| {
                    connection
                        .execute(
                            "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, 'closed', 100, '2026-08-03T10:00:00Z')",
                            [&current_id],
                        )
                        .unwrap();
                }
            })
            .await
            .unwrap();
        let source = directory.path().join("source.db");
        manager.export_to(&pool, source.clone()).await.unwrap();
        let source_id = Uuid::new_v4().to_string();
        pool.get()
            .await
            .unwrap()
            .interact(move |connection| {
                connection
                    .execute(
                        "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, 'closed', 200, '2026-08-03T11:00:00Z')",
                        [&source_id],
                    )
                    .unwrap();
            })
            .await
            .unwrap();

        let (pending, selection) = manager.inspect_backup(source).await.unwrap();
        let mut db = AppDb::new(pool);
        let result = manager.restore_live(&mut db, &pending).await.unwrap();
        assert_eq!(
            result.summary.session_count,
            selection.summary.session_count
        );
        let connection = db.pool().unwrap().get().await.unwrap();
        let count: i64 = connection
            .interact(|connection| {
                connection.query_row("SELECT COUNT(*) FROM cash_sessions", [], |row| row.get(0))
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(count, 1);

        let failed = directory.path().join("failed.db");
        fs::write(&failed, b"broken").unwrap();
        let error = manager.inspect_backup(failed).await.unwrap_err();
        assert!(matches!(
            error,
            BackupError::Sqlite(_) | BackupError::Incompatible(_)
        ));
        let connection = db.pool().unwrap().get().await.unwrap();
        let count: i64 = connection
            .interact(|connection| {
                connection.query_row("SELECT COUNT(*) FROM cash_sessions", [], |row| row.get(0))
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn rollback_restores_current_data_after_swap_failure() {
        let (directory, manager, pool) = setup().await;
        let session_id = Uuid::new_v4().to_string();
        pool.get()
            .await
            .unwrap()
            .interact({
                let session_id = session_id.clone();
                move |connection| {
                    connection
                        .execute(
                            "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, 'closed', 900, '2026-08-03T10:00:00Z')",
                            [&session_id],
                        )
                        .unwrap();
                }
            })
            .await
            .unwrap();
        let snapshot = directory.path().join("pre-restore.db");
        manager
            .create_snapshot_file(&pool, &snapshot)
            .await
            .unwrap();
        let live_path = directory.path().join("unatienda.db");
        let journal = RestoreJournal {
            phase: RestorePhase::PostSwap,
            live_path,
            staged_path: directory.path().join("staged.db"),
            snapshot_path: snapshot,
        };
        let mut db = AppDb::new(pool);
        db.close();
        let error = manager
            .rollback_after_swap(
                &mut db,
                &journal,
                BackupError::Incompatible("forced validation failure".into()),
            )
            .await;
        assert!(matches!(error, BackupError::Incompatible(_)));

        let connection = db.pool().unwrap().get().await.unwrap();
        let count: i64 = connection
            .interact(|connection| {
                connection.query_row("SELECT COUNT(*) FROM cash_sessions", [], |row| row.get(0))
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn interrupted_journal_recovers_from_post_swap_failure() {
        let (directory, manager, pool) = setup().await;
        let snapshot = directory.path().join("snapshot.db");
        manager
            .create_snapshot_file(&pool, &snapshot)
            .await
            .unwrap();
        let live_path = directory.path().join("unatienda.db");
        let staged = directory.path().join("staged.db");
        fs::write(&live_path, b"broken").unwrap();
        let journal = RestoreJournal {
            phase: RestorePhase::PostSwap,
            live_path,
            staged_path: staged,
            snapshot_path: snapshot,
        };
        write_journal_file(manager.journal_path(), &journal).unwrap();
        manager.recover_interrupted_restore().unwrap();
        validate_backup_path(&journal.live_path).unwrap();
        assert!(!manager.journal_path().exists());
    }

    #[tokio::test]
    async fn interrupted_nonterminal_phases_always_reinstall_snapshot() {
        for phase in [
            RestorePhase::Staged,
            RestorePhase::PreSwap,
            RestorePhase::PostSwap,
            RestorePhase::Rollback,
        ] {
            let (directory, manager, pool) = setup().await;
            let original_id = Uuid::new_v4().to_string();
            pool.get()
                .await
                .unwrap()
                .interact({
                    let original_id = original_id.clone();
                    move |connection| {
                        connection.execute(
                            "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, 'closed', 100, '2026-08-03T10:00:00Z')",
                            [&original_id],
                        )?;
                        Ok::<_, rusqlite::Error>(())
                    }
                })
                .await
                .unwrap()
                .unwrap();
            let snapshot = directory.path().join("snapshot.db");
            manager
                .create_snapshot_file(&pool, &snapshot)
                .await
                .unwrap();
            let replacement_id = Uuid::new_v4().to_string();
            pool.get()
                .await
                .unwrap()
                .interact({
                    let replacement_id = replacement_id.clone();
                    move |connection| {
                        connection.execute(
                            "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, 'closed', 200, '2026-08-03T11:00:00Z')",
                            [&replacement_id],
                        )?;
                        Ok::<_, rusqlite::Error>(())
                    }
                })
                .await
                .unwrap()
                .unwrap();
            pool.close();

            let journal = RestoreJournal {
                phase,
                live_path: directory.path().join("unatienda.db"),
                staged_path: directory.path().join("staged.db"),
                snapshot_path: snapshot,
            };
            write_journal_file(manager.journal_path(), &journal).unwrap();
            manager.recover_interrupted_restore().unwrap();

            let recovered = open_pool(&journal.live_path).await.unwrap();
            let count: i64 = recovered
                .get()
                .await
                .unwrap()
                .interact(|connection| {
                    connection.query_row("SELECT COUNT(*) FROM cash_sessions", [], |row| row.get(0))
                })
                .await
                .unwrap()
                .unwrap();
            assert_eq!(count, 1);
            assert!(!manager.journal_path().exists());
        }
    }

    #[tokio::test]
    async fn interrupted_verified_phase_keeps_verified_restore() {
        let (directory, manager, pool) = setup().await;
        let snapshot = directory.path().join("snapshot.db");
        manager
            .create_snapshot_file(&pool, &snapshot)
            .await
            .unwrap();
        let replacement_id = Uuid::new_v4().to_string();
        pool.get()
            .await
            .unwrap()
            .interact({
                let replacement_id = replacement_id.clone();
                move |connection| {
                    connection.execute(
                        "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, 'closed', 200, '2026-08-03T11:00:00Z')",
                        [&replacement_id],
                    )?;
                    Ok::<_, rusqlite::Error>(())
                }
            })
            .await
            .unwrap()
            .unwrap();
        pool.close();
        let journal = RestoreJournal {
            phase: RestorePhase::Verified,
            live_path: directory.path().join("unatienda.db"),
            staged_path: directory.path().join("staged.db"),
            snapshot_path: snapshot,
        };
        write_journal_file(manager.journal_path(), &journal).unwrap();

        manager.recover_interrupted_restore().unwrap();
        let recovered = open_pool(&journal.live_path).await.unwrap();
        let count: i64 = recovered
            .get()
            .await
            .unwrap()
            .interact(|connection| {
                connection.query_row("SELECT COUNT(*) FROM cash_sessions", [], |row| row.get(0))
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(count, 1);
        assert!(!manager.journal_path().exists());
    }

    async fn restore_with_injected_fault(
        point: FaultPoint,
        occurrence: usize,
    ) -> (TempDir, BackupManager, AppDb, BackupError) {
        let (directory, manager, pool) = setup().await;
        let current_id = Uuid::new_v4().to_string();
        pool.get()
            .await
            .unwrap()
            .interact({
                let current_id = current_id.clone();
                move |connection| {
                    connection.execute(
                        "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, 'closed', 100, '2026-08-03T10:00:00Z')",
                        [&current_id],
                    )?;
                    Ok::<_, rusqlite::Error>(())
                }
            })
            .await
            .unwrap()
            .unwrap();
        let source = directory.path().join("source.db");
        manager.export_to(&pool, source.clone()).await.unwrap();
        let changed_id = Uuid::new_v4().to_string();
        pool.get()
            .await
            .unwrap()
            .interact({
                let changed_id = changed_id.clone();
                move |connection| {
                    connection.execute(
                        "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, 'closed', 200, '2026-08-03T11:00:00Z')",
                        [&changed_id],
                    )?;
                    Ok::<_, rusqlite::Error>(())
                }
            })
            .await
            .unwrap()
            .unwrap();
        let (pending, _) = manager.inspect_backup(source).await.unwrap();
        manager.inject_fault(point, occurrence);
        let mut db = AppDb::new(pool);
        let error = manager.restore_live(&mut db, &pending).await.unwrap_err();
        (directory, manager, db, error)
    }

    #[tokio::test]
    async fn terminal_journal_write_failure_returns_error_and_reopens_original_pool() {
        let (directory, _manager, db, error) =
            restore_with_injected_fault(FaultPoint::JournalWrite, 4).await;
        assert!(matches!(error, BackupError::Recovery(_)));
        assert!(!directory.path().join("restore_swap.json").exists());
        let count: i64 = db
            .pool()
            .unwrap()
            .get()
            .await
            .unwrap()
            .interact(|connection| {
                connection.query_row("SELECT COUNT(*) FROM cash_sessions", [], |row| row.get(0))
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn install_and_fsync_faults_rollback_to_a_usable_original_pool() {
        for (point, occurrence) in [(FaultPoint::Install, 1), (FaultPoint::Fsync, 2)] {
            let (_directory, _manager, db, error) =
                restore_with_injected_fault(point, occurrence).await;
            assert!(!matches!(error, BackupError::Recovery(_)));
            assert!(db.pool().is_ok());
        }
    }

    #[tokio::test]
    async fn rollback_or_pool_reopen_faults_enter_explicit_fail_closed_state() {
        for (point, occurrence) in [(FaultPoint::Rollback, 1), (FaultPoint::PoolReopen, 2)] {
            let (directory, manager, db, error) = if point == FaultPoint::Rollback {
                manager_with_terminal_fault(FaultPoint::JournalWrite, 4, Some((point, occurrence)))
                    .await
            } else {
                manager_with_terminal_fault(FaultPoint::JournalWrite, 4, Some((point, occurrence)))
                    .await
            };
            assert!(matches!(error, BackupError::Recovery(_)));
            assert!(matches!(
                db.pool(),
                Err(AppError::DatabaseUnavailable { .. })
            ));
            assert!(directory.path().join("restore_swap.json").exists());
            manager.recover_interrupted_restore().unwrap();
            assert!(!directory.path().join("restore_swap.json").exists());
        }
    }

    async fn manager_with_terminal_fault(
        terminal_point: FaultPoint,
        terminal_occurrence: usize,
        rollback_fault: Option<(FaultPoint, usize)>,
    ) -> (TempDir, BackupManager, AppDb, BackupError) {
        let (directory, manager, pool) = setup().await;
        let source = directory.path().join("source.db");
        manager.export_to(&pool, source.clone()).await.unwrap();
        let changed_id = Uuid::new_v4().to_string();
        pool.get()
            .await
            .unwrap()
            .interact({
                let changed_id = changed_id.clone();
                move |connection| {
                    connection.execute(
                        "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, 'closed', 200, '2026-08-03T11:00:00Z')",
                        [&changed_id],
                    )?;
                    Ok::<_, rusqlite::Error>(())
                }
            })
            .await
            .unwrap()
            .unwrap();
        let (pending, _) = manager.inspect_backup(source).await.unwrap();
        manager.inject_fault(terminal_point, terminal_occurrence);
        if let Some((point, occurrence)) = rollback_fault {
            manager.inject_fault(point, occurrence);
        }
        let mut db = AppDb::new(pool);
        let error = manager.restore_live(&mut db, &pending).await.unwrap_err();
        (directory, manager, db, error)
    }

    #[tokio::test]
    async fn retention_prunes_only_old_managed_backups() {
        let (directory, manager, pool) = setup().await;
        manager
            .create_session_close_backup(
                &pool,
                DateTime::parse_from_rfc3339("2020-01-01T00:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            )
            .await
            .unwrap();
        let exported = directory.path().join("user-export.db");
        fs::write(&exported, b"keep").unwrap();
        manager
            .run_startup_backup(
                &pool,
                DateTime::parse_from_rfc3339("2026-08-03T00:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            )
            .await
            .unwrap();
        assert!(exported.exists());
        let panel = manager.panel(&pool).await.unwrap();
        assert!(panel
            .backups
            .iter()
            .all(|backup| backup.run_at.as_str() >= "2026-07-04"));
    }
}

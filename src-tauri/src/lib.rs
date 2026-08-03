use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::{Manager, State};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use tokio::sync::{Mutex, RwLock};

use crate::backup::{BackupManager, PendingRestore};
use crate::persistence::connection::{open_pool, run_migrations, AppDb};

mod application;
mod backup;
mod commands;
mod domain;
mod persistence;

pub struct AppState {
    pub db: Arc<RwLock<AppDb>>,
    pub backup: Arc<BackupManager>,
    pub pending_restore: Arc<Mutex<Option<PendingRestore>>>,
}

const STARTUP_ERROR_TITLE: &str = "UnaTienda could not start";
const STARTUP_DIAGNOSTIC_FILE: &str = "startup-errors.log";

#[derive(Debug, thiserror::Error)]
enum StartupError {
    #[error("cannot resolve the application data directory: {0}")]
    AppDataDirectory(#[source] tauri::Error),
    #[error("cannot create the application data directory at {path}: {source}")]
    CreateAppDataDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("database initialization failed: {0}")]
    Database(#[from] persistence::connection::DbError),
}

impl StartupError {
    fn stage(&self) -> &'static str {
        match self {
            Self::AppDataDirectory(_) => "application data directory resolution",
            Self::CreateAppDataDirectory { .. } => "application data directory creation",
            Self::Database(_) => "database initialization and migration",
        }
    }

    fn preferred_diagnostic_directory(&self) -> Option<&Path> {
        match self {
            Self::CreateAppDataDirectory { path, .. } => path.parent(),
            _ => None,
        }
    }
}

#[derive(Debug)]
struct StartupFailureReport {
    diagnostic_path: Option<PathBuf>,
}

impl StartupFailureReport {
    fn record(
        stage: &str,
        detail: &str,
        preferred_directory: Option<&Path>,
        fallback_directory: &Path,
    ) -> Self {
        let timestamp = chrono::Utc::now().to_rfc3339();
        let entry = format!("[{timestamp}] stage={stage}\nerror={detail}\n\n");

        let diagnostic_path = preferred_directory
            .and_then(|directory| write_startup_diagnostic(directory, &entry).ok())
            .or_else(|| write_startup_diagnostic(fallback_directory, &entry).ok());

        eprintln!("UnaTienda startup failure ({stage}): {detail}");
        if let Some(path) = &diagnostic_path {
            eprintln!("Startup diagnostics written to {}", path.display());
        } else {
            eprintln!("Startup diagnostics could not be written to disk");
        }

        Self { diagnostic_path }
    }

    fn user_message(&self) -> String {
        let diagnostic_guidance = match &self.diagnostic_path {
            Some(path) => format!(
                "If the problem continues, share the diagnostic file with support:\n{}",
                path.display()
            ),
            None => "Diagnostics could not be saved. Note this message before contacting support."
                .to_string(),
        };

        format!(
            "UnaTienda stopped database startup before the application became usable so it can fail safely.\n\nCheck available disk space and folder permissions, then restart the application.\n\n{diagnostic_guidance}"
        )
    }
}

fn write_startup_diagnostic(directory: &Path, entry: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(directory)?;
    let path = directory.join(STARTUP_DIAGNOSTIC_FILE);
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    file.write_all(entry.as_bytes())?;
    file.sync_all()?;
    Ok(path)
}

fn fallback_diagnostic_directory() -> PathBuf {
    std::env::temp_dir().join("UnaTienda")
}

fn report_startup_failure(
    app_handle: &tauri::AppHandle,
    error: &StartupError,
    preferred_directory: Option<&Path>,
) {
    let fallback_directory = fallback_diagnostic_directory();
    let report = StartupFailureReport::record(
        error.stage(),
        &error.to_string(),
        preferred_directory.or_else(|| error.preferred_diagnostic_directory()),
        &fallback_directory,
    );

    app_handle
        .dialog()
        .message(report.user_message())
        .kind(MessageDialogKind::Error)
        .title(STARTUP_ERROR_TITLE)
        .blocking_show();
}

fn app_data_dir(app_handle: &tauri::AppHandle) -> Result<PathBuf, StartupError> {
    app_handle
        .path()
        .app_data_dir()
        .map_err(StartupError::AppDataDirectory)
}

fn database_path(app_handle: &tauri::AppHandle) -> Result<PathBuf, StartupError> {
    let mut dir = app_data_dir(app_handle)?;
    std::fs::create_dir_all(&dir).map_err(|source| StartupError::CreateAppDataDirectory {
        path: dir.clone(),
        source,
    })?;
    dir.push("unatienda.db");
    Ok(dir)
}

/// Initialize the application database and state.
///
/// This is the async core of startup. It is driven synchronously inside Tauri's
/// setup hook via `tauri::async_runtime::block_on`, which is Tauri's supported
/// mechanism for async initialization. It never uses `Handle::current().block_on`.
pub async fn initialize_app(db_path: &Path) -> Result<AppState, persistence::connection::DbError> {
    let backup = Arc::new(BackupManager::new(db_path));
    backup.recover_interrupted_restore().map_err(|error| {
        persistence::connection::DbError::Io(std::io::Error::other(error.to_string()))
    })?;
    let pool = open_pool(db_path).await?;
    run_migrations(&pool, db_path).await?;
    let state = AppState {
        db: Arc::new(RwLock::new(AppDb::new(pool))),
        backup,
        pending_restore: Arc::new(Mutex::new(None)),
    };
    let db = state.db.read().await;
    let pool = db
        .pool()
        .map_err(|error| persistence::connection::DbError::Unavailable(error.to_string()))?;
    if let Err(error) = state
        .backup
        .run_startup_backup(pool, chrono::Utc::now())
        .await
    {
        eprintln!("UnaTienda startup backup failed: {error}");
    }
    drop(db);
    Ok(state)
}

#[tauri::command]
async fn health(state: State<'_, AppState>) -> Result<String, String> {
    state
        .db
        .read()
        .await
        .health_check()
        .await
        .map(|_| "ok".to_string())
        .map_err(|e| format!("{:?}", e))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_handle = app.handle();
            let db_path = match database_path(app_handle) {
                Ok(path) => path,
                Err(error) => {
                    report_startup_failure(app_handle, &error, None);
                    return Err(error.into());
                }
            };

            let state = match tauri::async_runtime::block_on(initialize_app(&db_path)) {
                Ok(state) => state,
                Err(error) => {
                    let error = StartupError::Database(error);
                    report_startup_failure(app_handle, &error, db_path.parent());
                    return Err(error.into());
                }
            };
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            health,
            commands::cash_session_commands::open_session,
            commands::cash_session_commands::close_session,
            commands::cash_session_commands::get_reconciliation,
            commands::cash_session_commands::get_session_history,
            commands::cash_session_commands::list_closed_sessions,
            commands::cash_session_commands::get_closed_session_detail,
            commands::sale_commands::record_sale,
            commands::sale_commands::record_cash_out,
            commands::sale_commands::annul_sale,
            commands::backup_commands::get_backup_panel,
            commands::backup_commands::export_backup,
            commands::backup_commands::select_backup,
            commands::backup_commands::restore_backup,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|error| eprintln!("UnaTienda runtime terminated: {error}"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Prove JD-A-006: `initialize_app` is a plain async function and can be
    /// driven to completion without relying on `tokio::runtime::Handle::current().block_on`.
    /// The production setup path uses Tauri's supported `async_runtime::block_on`.
    #[tokio::test]
    async fn initialize_app_does_not_depend_on_tokio_handle_block_on() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let state = initialize_app(&db_path).await.unwrap();
        state.db.read().await.health_check().await.unwrap();
    }

    #[test]
    fn configured_bundle_target_is_the_verified_macos_app_format() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(config["bundle"]["targets"], serde_json::json!(["app"]));
    }

    #[tokio::test]
    async fn startup_database_failure_is_fail_closed_and_actionable() {
        let dir = TempDir::new().unwrap();
        let invalid_db_path = dir.path().join("database-is-a-directory");
        std::fs::create_dir(&invalid_db_path).unwrap();

        let error = match initialize_app(&invalid_db_path).await {
            Ok(_) => panic!("a directory cannot be opened as the SQLite database"),
            Err(error) => error,
        };
        let startup_error = StartupError::Database(error);
        let diagnostics_dir = dir.path().join("diagnostics");
        let fallback_dir = dir.path().join("fallback");
        let report = StartupFailureReport::record(
            startup_error.stage(),
            &startup_error.to_string(),
            Some(&diagnostics_dir),
            &fallback_dir,
        );

        let diagnostic_path = report
            .diagnostic_path
            .as_ref()
            .expect("startup failure must leave durable diagnostics");
        let diagnostic = std::fs::read_to_string(diagnostic_path).unwrap();
        assert!(diagnostic.contains("stage=database initialization and migration"));
        assert!(diagnostic.contains("database initialization failed"));

        let guidance = report.user_message();
        assert!(guidance.contains("stopped database startup"));
        assert!(guidance.contains("disk space and folder permissions"));
        assert!(guidance.contains(&diagnostic_path.display().to_string()));
    }

    #[test]
    fn startup_diagnostics_fall_back_when_app_data_is_unwritable() {
        let dir = TempDir::new().unwrap();
        let unusable_directory = dir.path().join("not-a-directory");
        std::fs::write(&unusable_directory, b"blocks directory creation").unwrap();
        let fallback_directory = dir.path().join("fallback");

        let report = StartupFailureReport::record(
            "application data directory creation",
            "permission denied",
            Some(&unusable_directory),
            &fallback_directory,
        );

        let diagnostic_path = report
            .diagnostic_path
            .expect("fallback diagnostics must be written");
        assert_eq!(
            diagnostic_path,
            fallback_directory.join(STARTUP_DIAGNOSTIC_FILE)
        );
        assert!(std::fs::read_to_string(diagnostic_path)
            .unwrap()
            .contains("permission denied"));
    }
}

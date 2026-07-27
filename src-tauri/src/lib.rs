use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::{Manager, State};

use crate::persistence::connection::{open_pool, run_migrations, AppDb};

mod domain;
mod persistence;

pub struct AppState {
    pub db: Arc<AppDb>,
}

fn app_data_dir(app_handle: &tauri::AppHandle) -> PathBuf {
    app_handle
        .path()
        .app_data_dir()
        .expect("app data directory must be available")
}

fn database_path(app_handle: &tauri::AppHandle) -> PathBuf {
    let mut dir = app_data_dir(app_handle);
    std::fs::create_dir_all(&dir).expect("create app data dir");
    dir.push("unatienda.db");
    dir
}

/// Initialize the application database and state.
///
/// This is the async core of startup. It is driven synchronously inside Tauri's
/// setup hook via `tauri::async_runtime::block_on`, which is Tauri's supported
/// mechanism for async initialization. It never uses `Handle::current().block_on`.
pub async fn initialize_app(db_path: &Path) -> Result<AppState, persistence::connection::DbError> {
    let pool = open_pool(db_path).await?;
    run_migrations(&pool, db_path).await?;
    Ok(AppState {
        db: Arc::new(AppDb::new(pool)),
    })
}

#[tauri::command]
async fn health(state: State<'_, AppState>) -> Result<String, String> {
    state
        .db
        .health_check()
        .await
        .map(|_| "ok".to_string())
        .map_err(|e| format!("{:?}", e))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let db_path = database_path(&app.handle());
            let state = tauri::async_runtime::block_on(initialize_app(&db_path))?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![health])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
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
        state.db.health_check().await.unwrap();
    }
}

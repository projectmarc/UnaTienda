use std::path::PathBuf;

use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::backup::{BackupPanel, RestoreResult, RestoreSelection};
use crate::AppState;

fn backup_error(error: crate::backup::BackupError) -> String {
    super::app_error(error.to_app_error())
}

fn selected_path(path: Option<tauri_plugin_dialog::FilePath>) -> Result<Option<PathBuf>, String> {
    path.map(|path| path.into_path().map_err(|error| error.to_string()))
        .transpose()
}

#[tauri::command]
pub async fn get_backup_panel(state: State<'_, AppState>) -> Result<BackupPanel, String> {
    let db = state.db.read().await;
    let pool = super::usable_pool(&db)?;
    state.backup.panel(pool).await.map_err(backup_error)
}

#[tauri::command]
pub async fn export_backup(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<crate::backup::BackupSummary>, String> {
    let path = selected_path(
        app.dialog()
            .file()
            .add_filter("Copia de seguridad de UnaTienda", &["db"])
            .set_file_name("unatienda-backup.db")
            .blocking_save_file(),
    )?;
    let Some(path) = path else {
        return Ok(None);
    };

    let db = state.db.read().await;
    let pool = super::usable_pool(&db)?;
    state
        .backup
        .export_to(pool, path)
        .await
        .map(Some)
        .map_err(backup_error)
}

#[tauri::command]
pub async fn select_backup(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<RestoreSelection>, String> {
    let path = selected_path(
        app.dialog()
            .file()
            .add_filter("Copia de seguridad de UnaTienda", &["db"])
            .blocking_pick_file(),
    )?;
    let Some(path) = path else {
        return Ok(None);
    };

    let (pending, selection) = state
        .backup
        .inspect_backup(path)
        .await
        .map_err(backup_error)?;
    *state.pending_restore.lock().await = Some(pending);
    Ok(Some(selection))
}

#[tauri::command]
pub async fn restore_backup(state: State<'_, AppState>) -> Result<RestoreResult, String> {
    let pending = state
        .pending_restore
        .lock()
        .await
        .clone()
        .ok_or_else(|| backup_error(crate::backup::BackupError::NoPendingRestore))?;
    let mut db = state.db.write().await;
    let result = state.backup.restore_live(&mut db, &pending).await;
    if result.is_ok() {
        *state.pending_restore.lock().await = None;
    }
    result.map_err(backup_error)
}

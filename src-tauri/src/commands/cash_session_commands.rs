use tauri::State;

use crate::application::cash_session_use_case::{
    self, ClosedSessionDetailResponse, ClosedSessionResponse, ClosedSessionSummary,
    OpenSessionResponse, ReconciliationView, SessionHistoryResponse,
};
use crate::AppState;

/// Open a new cash session.
///
/// Returns the session ID and canonical opening amount string.
/// Rejects if a session is already open or the opening amount is negative.
#[tauri::command]
pub async fn open_session(
    state: State<'_, AppState>,
    opening_amount: String,
    idempotency_key: String,
) -> Result<OpenSessionResponse, String> {
    let amount = opening_amount
        .parse::<crate::domain::Money>()
        .map_err(|e| e.to_string())?;

    let db = state.db.read().await;
    let pool = super::usable_pool(&db)?;
    cash_session_use_case::open_session(pool, amount.cents(), &idempotency_key)
        .await
        .map_err(|e| {
            serde_json::to_string(&crate::domain::ErrorResponse::from(&e))
                .unwrap_or_else(|_| e.to_string())
        })
}

/// Close the current open session.
///
/// Returns the closed session with reconciliation data.
/// Rejects if no session is open or suspended drafts remain.
#[tauri::command]
pub async fn close_session(
    state: State<'_, AppState>,
    counted_cash: String,
    idempotency_key: String,
) -> Result<ClosedSessionResponse, String> {
    let cash = counted_cash
        .parse::<crate::domain::Money>()
        .map_err(|e| e.to_string())?;

    let db = state.db.read().await;
    let pool = super::usable_pool(&db)?;
    let result = cash_session_use_case::close_session(pool, cash.cents(), &idempotency_key).await;
    if result.is_ok() {
        let mut response = result.unwrap();
        if let Err(error) = state
            .backup
            .create_session_close_backup(pool, chrono::Utc::now())
            .await
        {
            eprintln!("UnaTienda post-close backup failed: {error}");
            response.backup_warning = Some(
                "La caja se cerró correctamente, pero no se pudo crear la copia de seguridad."
                    .to_string(),
            );
        }
        return Ok(response);
    }
    result.map_err(|e| {
        serde_json::to_string(&crate::domain::ErrorResponse::from(&e))
            .unwrap_or_else(|_| e.to_string())
    })
}

/// Get the reconciliation view for the current open session.
///
/// Returns expected cash and session metadata. When `counted_cash` is
/// provided, the response includes the exact difference computed via
/// integer arithmetic (no floating-point). No idempotency key needed
/// because this is a read-only query.
#[tauri::command]
pub async fn get_reconciliation(
    state: State<'_, AppState>,
    counted_cash: Option<String>,
) -> Result<ReconciliationView, String> {
    let db = state.db.read().await;
    let pool = super::usable_pool(&db)?;
    cash_session_use_case::get_reconciliation(pool, counted_cash.as_deref())
        .await
        .map_err(|e| {
            serde_json::to_string(&crate::domain::ErrorResponse::from(&e))
                .unwrap_or_else(|_| e.to_string())
        })
}

/// Get chronological operations, net payment totals, and expected cash for
/// the current open session.
#[tauri::command]
pub async fn get_session_history(
    state: State<'_, AppState>,
) -> Result<SessionHistoryResponse, String> {
    let db = state.db.read().await;
    let pool = super::usable_pool(&db)?;
    cash_session_use_case::get_session_history(pool)
        .await
        .map_err(|e| {
            serde_json::to_string(&crate::domain::ErrorResponse::from(&e))
                .unwrap_or_else(|_| e.to_string())
        })
}

/// List closed cash sessions in reverse chronological order.
#[tauri::command]
pub async fn list_closed_sessions(
    state: State<'_, AppState>,
) -> Result<Vec<ClosedSessionSummary>, String> {
    let db = state.db.read().await;
    let pool = super::usable_pool(&db)?;
    cash_session_use_case::list_closed_sessions(pool)
        .await
        .map_err(|e| {
            serde_json::to_string(&crate::domain::ErrorResponse::from(&e))
                .unwrap_or_else(|_| e.to_string())
        })
}

/// Get read-only operations and final reconciliation for one closed session.
#[tauri::command]
pub async fn get_closed_session_detail(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<ClosedSessionDetailResponse, String> {
    let db = state.db.read().await;
    let pool = super::usable_pool(&db)?;
    cash_session_use_case::get_closed_session_detail(pool, &session_id)
        .await
        .map_err(|e| {
            serde_json::to_string(&crate::domain::ErrorResponse::from(&e))
                .unwrap_or_else(|_| e.to_string())
        })
}

use tauri::State;

use crate::application::cash_session_use_case::{
    self, ClosedSessionResponse, OpenSessionResponse, ReconciliationView,
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

    cash_session_use_case::open_session(&state.db.pool(), amount.cents(), &idempotency_key)
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

    cash_session_use_case::close_session(&state.db.pool(), cash.cents(), &idempotency_key)
        .await
        .map_err(|e| {
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
    cash_session_use_case::get_reconciliation(&state.db.pool(), counted_cash.as_deref())
        .await
        .map_err(|e| {
            serde_json::to_string(&crate::domain::ErrorResponse::from(&e))
                .unwrap_or_else(|_| e.to_string())
        })
}

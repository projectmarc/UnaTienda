use tauri::State;

use crate::application::sale_use_case::{
    self, AnnulSaleRequest, CashOutRecordedResponse, RecordSaleRequest, SaleAnnulledResponse,
    SaleRecordedResponse,
};
use crate::AppState;

/// Record a sale in the current open session.
///
/// The sale has a mandatory total amount, mandatory category (ropa/libreria/mixto),
/// one or more payment parts, and an optional detail.
/// Payment parts must sum exactly to the sale total.
#[tauri::command]
pub async fn record_sale(
    state: State<'_, AppState>,
    request: RecordSaleRequest,
    idempotency_key: String,
) -> Result<SaleRecordedResponse, String> {
    let db = state.db.read().await;
    let pool = super::usable_pool(&db)?;
    sale_use_case::record_sale(pool, request, &idempotency_key)
        .await
        .map_err(|e| {
            serde_json::to_string(&crate::domain::ErrorResponse::from(&e))
                .unwrap_or_else(|_| e.to_string())
        })
}

/// Record a cash-out (withdrawal) in the current open session.
///
/// Cash-outs have a mandatory amount and an optional free-text reason.
/// They reduce expected physical cash.
#[tauri::command]
pub async fn record_cash_out(
    state: State<'_, AppState>,
    amount: String,
    detail: Option<String>,
    idempotency_key: String,
) -> Result<CashOutRecordedResponse, String> {
    let db = state.db.read().await;
    let pool = super::usable_pool(&db)?;
    sale_use_case::record_cash_out(pool, amount, detail, &idempotency_key)
        .await
        .map_err(|e| {
            serde_json::to_string(&crate::domain::ErrorResponse::from(&e))
                .unwrap_or_else(|_| e.to_string())
        })
}

/// Annul a confirmed sale while its owning session remains open.
#[tauri::command]
pub async fn annul_sale(
    state: State<'_, AppState>,
    request: AnnulSaleRequest,
    idempotency_key: String,
) -> Result<SaleAnnulledResponse, String> {
    let db = state.db.read().await;
    let pool = super::usable_pool(&db)?;
    sale_use_case::annul_sale(pool, request, &idempotency_key)
        .await
        .map_err(|e| {
            serde_json::to_string(&crate::domain::ErrorResponse::from(&e))
                .unwrap_or_else(|_| e.to_string())
        })
}

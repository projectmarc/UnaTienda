use deadpool_sqlite::Pool;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::domain::error::AppError;
use crate::domain::sale::{PaymentMethod, PaymentPart, SaleCategory};
use crate::domain::Money;
use crate::persistence::idempotency::with_idempotency;
use crate::persistence::sale_repo;

/// A single payment part in the request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentPartRequest {
    pub method: String,
    pub amount: String,
    pub recipient_account: Option<String>,
}

/// Request to record a sale.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordSaleRequest {
    pub total_amount: String,
    pub category: String,
    pub detail: Option<String>,
    pub payments: Vec<PaymentPartRequest>,
}

/// Response returned after recording a sale.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaleRecordedResponse {
    pub sale_id: String,
    pub total_amount: String,
}

/// Response returned after recording a cash-out.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CashOutRecordedResponse {
    pub movement_id: String,
    pub amount: String,
}

/// Request to annul a confirmed sale.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnulSaleRequest {
    pub sale_id: String,
    pub reason: Option<String>,
}

/// Response returned after annulling a sale.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaleAnnulledResponse {
    pub sale_id: String,
    pub status: String,
}

/// Record a sale in the current open session.
///
/// Validates that:
/// - A session is open.
/// - The total amount is positive.
/// - The category is one of the allowed values.
/// - Payment parts sum to the total.
/// - Transfer payments have a recipient account.
/// - At least one payment part is provided.
pub async fn record_sale(
    pool: &Pool,
    request: RecordSaleRequest,
    idempotency_key: &str,
) -> Result<SaleRecordedResponse, AppError> {
    let total: Money = request.total_amount.parse()?;
    let total_cents = total.cents();

    if total_cents <= 0 {
        return Err(AppError::validation_field(
            "sale total must be positive",
            "total_amount",
        ));
    }

    let category = SaleCategory::from_str(&request.category)?;

    if request.payments.is_empty() {
        return Err(AppError::validation_field(
            "at least one payment method is required",
            "payments",
        ));
    }

    let payment_parts: Vec<PaymentPart> = request
        .payments
        .iter()
        .map(|p| {
            let method = PaymentMethod::from_str(&p.method)?;
            let amount: Money = p.amount.parse()?;
            Ok(PaymentPart {
                method,
                amount: amount.cents(),
                recipient_account: p.recipient_account.clone(),
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;

    for part in &payment_parts {
        part.validate()?;
    }

    sale_repo::validate_payment_sum(total_cents, &payment_parts)?;

    let request_hash = format!(
        "record_sale:{}:{}:{:?}:{}",
        total_cents,
        category.as_str(),
        request.detail.as_deref(),
        payment_parts
            .iter()
            .map(|p| format!(
                "{}:{}:{:?}",
                p.method,
                p.amount,
                p.recipient_account.as_deref()
            ))
            .collect::<Vec<_>>()
            .join(",")
    );

    with_idempotency(
        pool,
        idempotency_key,
        "record_sale",
        &request_hash,
        move |tx| {
            // Verify the session is open.
            let session_id: String = tx
                .query_row(
                    "SELECT id FROM cash_sessions WHERE status = 'open' LIMIT 1",
                    [],
                    |row| row.get(0),
                )
                .map_err(|_| AppError::NoOpenSession)?;

            let sale_id = sale_repo::insert_sale(
                tx,
                &session_id,
                total_cents,
                category.as_str(),
                request.detail.as_deref(),
                &payment_parts,
            )?;

            Ok(SaleRecordedResponse {
                sale_id,
                total_amount: total.to_string(),
            })
        },
    )
    .await
}

/// Record a cash-out (withdrawal) in the current open session.
///
/// Validates that:
/// - A session is open.
/// - The amount is positive.
pub async fn record_cash_out(
    pool: &Pool,
    amount: String,
    detail: Option<String>,
    idempotency_key: &str,
) -> Result<CashOutRecordedResponse, AppError> {
    let money: Money = amount.parse()?;
    let cents = money.cents();

    if cents <= 0 {
        return Err(AppError::validation_field(
            "cash-out amount must be positive",
            "amount",
        ));
    }

    let request_hash = format!("record_cash_out:{}", cents);

    with_idempotency(
        pool,
        idempotency_key,
        "record_cash_out",
        &request_hash,
        move |tx| {
            let session_id: String = tx
                .query_row(
                    "SELECT id FROM cash_sessions WHERE status = 'open' LIMIT 1",
                    [],
                    |row| row.get(0),
                )
                .map_err(|_| AppError::NoOpenSession)?;

            let movement_id =
                sale_repo::insert_cash_out(tx, &session_id, cents, detail.as_deref())?;

            Ok(CashOutRecordedResponse {
                movement_id,
                amount: money.to_string(),
            })
        },
    )
    .await
}

/// Annul a sale while its owning cash session remains open.
///
/// The sale row is retained and marked annulled. Each original payment gets an
/// immutable reversal in the same transaction, so the expected cash and net
/// payment totals immediately reflect the annulment.
pub async fn annul_sale(
    pool: &Pool,
    request: AnnulSaleRequest,
    idempotency_key: &str,
) -> Result<SaleAnnulledResponse, AppError> {
    if request.sale_id.trim().is_empty() {
        return Err(AppError::validation_field("sale id is required", "sale_id"));
    }

    let sale_id = request.sale_id;
    let reason = request.reason.and_then(|value| {
        let trimmed = value.trim().to_string();
        (!trimmed.is_empty()).then_some(trimmed)
    });
    let request_hash = format!(
        "annul_sale:{}:{}",
        sale_id,
        reason.as_deref().unwrap_or_default()
    );

    with_idempotency(
        pool,
        idempotency_key,
        "annul_sale",
        &request_hash,
        move |tx| {
            let state: Option<(String, String)> = tx
                .query_row(
                    "SELECT cs.status, s.status FROM sales s JOIN cash_sessions cs ON cs.id = s.session_id WHERE s.id = ?1",
                    [&sale_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let (session_status, sale_status) = state.ok_or(AppError::NotFound)?;

            if session_status != "open" {
                return Err(AppError::SessionClosed);
            }
            if sale_status == "annulled" {
                return Err(AppError::conflict("sale is already annulled"));
            }
            if sale_status != "confirmed" {
                return Err(AppError::conflict("sale cannot be annulled"));
            }

            sale_repo::annul_sale(tx, &sale_id, reason.as_deref())?;

            Ok(SaleAnnulledResponse {
                sale_id,
                status: "annulled".to_string(),
            })
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::cash_session_use_case::open_session;
    use crate::persistence::connection::{open_pool, run_migrations};
    use tempfile::TempDir;

    async fn setup_db() -> (TempDir, Pool) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool, &db_path).await.unwrap();
        (dir, pool)
    }

    #[tokio::test]
    async fn record_sale_single_cash_payment() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 10000, "key-open").await.unwrap();

        let resp = record_sale(
            &pool,
            RecordSaleRequest {
                total_amount: "50.00".to_string(),
                category: "ropa".to_string(),
                detail: Some("campera negra".to_string()),
                payments: vec![PaymentPartRequest {
                    method: "cash".to_string(),
                    amount: "50.00".to_string(),
                    recipient_account: None,
                }],
            },
            "key-sale-1",
        )
        .await
        .unwrap();

        assert_eq!(resp.total_amount, "50.00");
        assert!(!resp.sale_id.is_empty());
    }

    #[tokio::test]
    async fn record_sale_split_payment() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 10000, "key-open").await.unwrap();

        let resp = record_sale(
            &pool,
            RecordSaleRequest {
                total_amount: "100.00".to_string(),
                category: "mixto".to_string(),
                detail: None,
                payments: vec![
                    PaymentPartRequest {
                        method: "cash".to_string(),
                        amount: "60.00".to_string(),
                        recipient_account: None,
                    },
                    PaymentPartRequest {
                        method: "debit".to_string(),
                        amount: "40.00".to_string(),
                        recipient_account: None,
                    },
                ],
            },
            "key-sale-split",
        )
        .await
        .unwrap();

        assert_eq!(resp.total_amount, "100.00");
    }

    #[tokio::test]
    async fn record_sale_rejects_payment_sum_mismatch() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 10000, "key-open").await.unwrap();

        let err = record_sale(
            &pool,
            RecordSaleRequest {
                total_amount: "100.00".to_string(),
                category: "ropa".to_string(),
                detail: None,
                payments: vec![PaymentPartRequest {
                    method: "cash".to_string(),
                    amount: "80.00".to_string(),
                    recipient_account: None,
                }],
            },
            "key-sale-mismatch",
        )
        .await
        .unwrap_err();

        assert!(matches!(err, AppError::Validation { .. }));
    }

    #[tokio::test]
    async fn record_sale_rejects_empty_payments() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 10000, "key-open").await.unwrap();

        let err = record_sale(
            &pool,
            RecordSaleRequest {
                total_amount: "50.00".to_string(),
                category: "libreria".to_string(),
                detail: None,
                payments: vec![],
            },
            "key-sale-empty",
        )
        .await
        .unwrap_err();

        assert!(matches!(err, AppError::Validation { .. }));
    }

    #[tokio::test]
    async fn record_sale_rejects_invalid_category() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 10000, "key-open").await.unwrap();

        let err = record_sale(
            &pool,
            RecordSaleRequest {
                total_amount: "50.00".to_string(),
                category: "electronica".to_string(),
                detail: None,
                payments: vec![PaymentPartRequest {
                    method: "cash".to_string(),
                    amount: "50.00".to_string(),
                    recipient_account: None,
                }],
            },
            "key-sale-cat",
        )
        .await
        .unwrap_err();

        assert!(matches!(err, AppError::Validation { .. }));
    }

    #[tokio::test]
    async fn record_sale_transfer_requires_recipient() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 10000, "key-open").await.unwrap();

        let err = record_sale(
            &pool,
            RecordSaleRequest {
                total_amount: "50.00".to_string(),
                category: "mixto".to_string(),
                detail: None,
                payments: vec![PaymentPartRequest {
                    method: "transfer".to_string(),
                    amount: "50.00".to_string(),
                    recipient_account: None,
                }],
            },
            "key-sale-xfer",
        )
        .await
        .unwrap_err();

        assert!(matches!(err, AppError::Validation { .. }));
    }

    #[tokio::test]
    async fn record_sale_transfer_with_recipient_succeeds() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 10000, "key-open").await.unwrap();

        let resp = record_sale(
            &pool,
            RecordSaleRequest {
                total_amount: "30.00".to_string(),
                category: "ropa".to_string(),
                detail: None,
                payments: vec![PaymentPartRequest {
                    method: "transfer".to_string(),
                    amount: "30.00".to_string(),
                    recipient_account: Some("Juan".to_string()),
                }],
            },
            "key-sale-xfer-ok",
        )
        .await
        .unwrap();

        assert_eq!(resp.total_amount, "30.00");
    }

    #[tokio::test]
    async fn record_sale_rejects_when_no_session_open() {
        let (_dir, pool) = setup_db().await;

        let err = record_sale(
            &pool,
            RecordSaleRequest {
                total_amount: "50.00".to_string(),
                category: "ropa".to_string(),
                detail: None,
                payments: vec![PaymentPartRequest {
                    method: "cash".to_string(),
                    amount: "50.00".to_string(),
                    recipient_account: None,
                }],
            },
            "key-sale-no-session",
        )
        .await
        .unwrap_err();

        assert!(matches!(err, AppError::NoOpenSession));
    }

    #[tokio::test]
    async fn record_sale_affects_expected_cash() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 10000, "key-open").await.unwrap();

        record_sale(
            &pool,
            RecordSaleRequest {
                total_amount: "50.00".to_string(),
                category: "ropa".to_string(),
                detail: None,
                payments: vec![PaymentPartRequest {
                    method: "cash".to_string(),
                    amount: "50.00".to_string(),
                    recipient_account: None,
                }],
            },
            "key-sale-cash",
        )
        .await
        .unwrap();

        let view = crate::application::cash_session_use_case::get_reconciliation(&pool, None)
            .await
            .unwrap();
        // expected = 10000 opening + 5000 cash payment = 15000
        assert_eq!(view.expected_cash, "150.00");
    }

    #[tokio::test]
    async fn record_sale_non_cash_payment_does_not_affect_expected_cash() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 10000, "key-open").await.unwrap();

        record_sale(
            &pool,
            RecordSaleRequest {
                total_amount: "50.00".to_string(),
                category: "mixto".to_string(),
                detail: None,
                payments: vec![PaymentPartRequest {
                    method: "debit".to_string(),
                    amount: "50.00".to_string(),
                    recipient_account: None,
                }],
            },
            "key-sale-debit",
        )
        .await
        .unwrap();

        let view = crate::application::cash_session_use_case::get_reconciliation(&pool, None)
            .await
            .unwrap();
        // expected = 10000 opening only (debit doesn't add cash)
        assert_eq!(view.expected_cash, "100.00");
    }

    #[tokio::test]
    async fn record_cash_out_basic() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 10000, "key-open").await.unwrap();

        let resp = record_cash_out(
            &pool,
            "20.00".to_string(),
            Some("compra de insumos".to_string()),
            "key-co-1",
        )
        .await
        .unwrap();

        assert_eq!(resp.amount, "20.00");
        assert!(!resp.movement_id.is_empty());
    }

    #[tokio::test]
    async fn record_cash_out_rejects_when_no_session_open() {
        let (_dir, pool) = setup_db().await;

        let err = record_cash_out(&pool, "10.00".to_string(), None, "key-co-no-session")
            .await
            .unwrap_err();

        assert!(matches!(err, AppError::NoOpenSession));
    }

    #[tokio::test]
    async fn record_cash_out_rejects_zero_amount() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 10000, "key-open").await.unwrap();

        let err = record_cash_out(&pool, "0.00".to_string(), None, "key-co-zero")
            .await
            .unwrap_err();

        assert!(matches!(err, AppError::Validation { .. }));
    }

    #[tokio::test]
    async fn record_cash_out_reduces_expected_cash() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 10000, "key-open").await.unwrap();

        record_cash_out(
            &pool,
            "20.00".to_string(),
            Some("cambio".to_string()),
            "key-co-1",
        )
        .await
        .unwrap();

        let view = crate::application::cash_session_use_case::get_reconciliation(&pool, None)
            .await
            .unwrap();
        // expected = 10000 opening - 2000 withdrawal = 8000
        assert_eq!(view.expected_cash, "80.00");
    }

    #[tokio::test]
    async fn record_sale_idempotency_replays() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 10000, "key-open").await.unwrap();

        let first = record_sale(
            &pool,
            RecordSaleRequest {
                total_amount: "50.00".to_string(),
                category: "ropa".to_string(),
                detail: None,
                payments: vec![PaymentPartRequest {
                    method: "cash".to_string(),
                    amount: "50.00".to_string(),
                    recipient_account: None,
                }],
            },
            "key-idem",
        )
        .await
        .unwrap();

        let second = record_sale(
            &pool,
            RecordSaleRequest {
                total_amount: "50.00".to_string(),
                category: "ropa".to_string(),
                detail: None,
                payments: vec![PaymentPartRequest {
                    method: "cash".to_string(),
                    amount: "50.00".to_string(),
                    recipient_account: None,
                }],
            },
            "key-idem",
        )
        .await
        .unwrap();

        assert_eq!(first.sale_id, second.sale_id);
    }

    #[tokio::test]
    async fn record_cash_out_idempotency_replays() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 10000, "key-open").await.unwrap();

        let first = record_cash_out(&pool, "10.00".to_string(), None, "key-co-idem")
            .await
            .unwrap();
        let second = record_cash_out(&pool, "10.00".to_string(), None, "key-co-idem")
            .await
            .unwrap();

        assert_eq!(first.movement_id, second.movement_id);
    }

    #[tokio::test]
    async fn record_sale_rejects_negative_amount() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 10000, "key-open").await.unwrap();

        let err = record_sale(
            &pool,
            RecordSaleRequest {
                total_amount: "-10.00".to_string(),
                category: "ropa".to_string(),
                detail: None,
                payments: vec![PaymentPartRequest {
                    method: "cash".to_string(),
                    amount: "-10.00".to_string(),
                    recipient_account: None,
                }],
            },
            "key-sale-neg",
        )
        .await
        .unwrap_err();

        assert!(matches!(err, AppError::Validation { .. }));
    }

    #[tokio::test]
    async fn record_sale_qr_payment() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 10000, "key-open").await.unwrap();

        let resp = record_sale(
            &pool,
            RecordSaleRequest {
                total_amount: "75.00".to_string(),
                category: "libreria".to_string(),
                detail: Some("cuadernos".to_string()),
                payments: vec![PaymentPartRequest {
                    method: "qr".to_string(),
                    amount: "75.00".to_string(),
                    recipient_account: None,
                }],
            },
            "key-sale-qr",
        )
        .await
        .unwrap();

        assert_eq!(resp.total_amount, "75.00");
    }

    #[tokio::test]
    async fn annul_sale_marks_sale_and_reverses_each_payment() {
        let (_dir, pool) = setup_db().await;
        open_session(&pool, 10000, "key-open").await.unwrap();

        let sale = record_sale(
            &pool,
            RecordSaleRequest {
                total_amount: "100.00".to_string(),
                category: "ropa".to_string(),
                detail: Some("campera".to_string()),
                payments: vec![
                    PaymentPartRequest {
                        method: "cash".to_string(),
                        amount: "60.00".to_string(),
                        recipient_account: None,
                    },
                    PaymentPartRequest {
                        method: "debit".to_string(),
                        amount: "40.00".to_string(),
                        recipient_account: None,
                    },
                ],
            },
            "key-sale-annul",
        )
        .await
        .unwrap();

        let response = annul_sale(
            &pool,
            AnnulSaleRequest {
                sale_id: sale.sale_id.clone(),
                reason: Some("error de carga".to_string()),
            },
            "key-annul",
        )
        .await
        .unwrap();

        assert_eq!(response.sale_id, sale.sale_id);
        assert_eq!(response.status, "annulled");

        let history = crate::application::cash_session_use_case::get_session_history(&pool)
            .await
            .unwrap();
        assert_eq!(history.operations[0].status.as_deref(), Some("annulled"));
        assert_eq!(
            history.operations[0].annulment_reason.as_deref(),
            Some("error de carga")
        );
        assert_eq!(history.payment_totals.cash, "0.00");
        assert_eq!(history.payment_totals.debit, "0.00");
        assert_eq!(history.expected_cash, "100.00");

        let reversal_count: i64 = pool
            .get()
            .await
            .unwrap()
            .interact(move |conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM payment_reversals WHERE annulment_sale_id = ?1",
                    [&sale.sale_id],
                    |row| row.get(0),
                )
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(reversal_count, 2);
    }

    #[tokio::test]
    async fn annul_sale_rejects_repeated_annulment() {
        let (_dir, pool) = setup_db().await;
        open_session(&pool, 10000, "key-open").await.unwrap();
        let sale = record_sale(
            &pool,
            RecordSaleRequest {
                total_amount: "25.00".to_string(),
                category: "libreria".to_string(),
                detail: None,
                payments: vec![PaymentPartRequest {
                    method: "cash".to_string(),
                    amount: "25.00".to_string(),
                    recipient_account: None,
                }],
            },
            "key-sale-repeat",
        )
        .await
        .unwrap();

        annul_sale(
            &pool,
            AnnulSaleRequest {
                sale_id: sale.sale_id.clone(),
                reason: None,
            },
            "key-annul-first",
        )
        .await
        .unwrap();

        let err = annul_sale(
            &pool,
            AnnulSaleRequest {
                sale_id: sale.sale_id,
                reason: None,
            },
            "key-annul-second",
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AppError::Conflict { .. }));
    }

    #[tokio::test]
    async fn annul_sale_rejects_when_owning_session_is_closed() {
        let (_dir, pool) = setup_db().await;
        open_session(&pool, 10000, "key-open").await.unwrap();
        let sale = record_sale(
            &pool,
            RecordSaleRequest {
                total_amount: "25.00".to_string(),
                category: "ropa".to_string(),
                detail: None,
                payments: vec![PaymentPartRequest {
                    method: "cash".to_string(),
                    amount: "25.00".to_string(),
                    recipient_account: None,
                }],
            },
            "key-sale-closed",
        )
        .await
        .unwrap();
        crate::application::cash_session_use_case::close_session(&pool, 12500, "key-close")
            .await
            .unwrap();

        let err = annul_sale(
            &pool,
            AnnulSaleRequest {
                sale_id: sale.sale_id,
                reason: Some("too late".to_string()),
            },
            "key-annul-closed",
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AppError::SessionClosed));
    }

    #[tokio::test]
    async fn annul_sale_adjusts_expected_cash_for_split_payment() {
        let (_dir, pool) = setup_db().await;
        open_session(&pool, 10000, "key-open").await.unwrap();
        let sale = record_sale(
            &pool,
            RecordSaleRequest {
                total_amount: "100.00".to_string(),
                category: "mixto".to_string(),
                detail: None,
                payments: vec![
                    PaymentPartRequest {
                        method: "cash".to_string(),
                        amount: "60.00".to_string(),
                        recipient_account: None,
                    },
                    PaymentPartRequest {
                        method: "debit".to_string(),
                        amount: "40.00".to_string(),
                        recipient_account: None,
                    },
                ],
            },
            "key-sale-split-annul",
        )
        .await
        .unwrap();

        let before = crate::application::cash_session_use_case::get_reconciliation(&pool, None)
            .await
            .unwrap();
        assert_eq!(before.expected_cash, "160.00");

        annul_sale(
            &pool,
            AnnulSaleRequest {
                sale_id: sale.sale_id,
                reason: None,
            },
            "key-annul-split",
        )
        .await
        .unwrap();

        let after = crate::application::cash_session_use_case::get_reconciliation(&pool, None)
            .await
            .unwrap();
        assert_eq!(after.expected_cash, "100.00");
    }
}

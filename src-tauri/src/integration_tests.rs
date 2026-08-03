use std::path::Path;

use deadpool_sqlite::Pool;
use rusqlite::Connection;
use tempfile::TempDir;

use crate::application::cash_session_use_case as cash;
use crate::application::sale_use_case as sales;
use crate::domain::error::AppError;
use crate::initialize_app;
use crate::persistence::connection::{open_pool, run_migrations};

const V1: &str = include_str!("../migrations/001_initial.sql");
const V2: &str = include_str!("../migrations/002_upgrade_closed_session_immutability.sql");
const V3: &str = include_str!("../migrations/003_upgrade_closed_session_insert_guards.sql");
const V4: &str = include_str!("../migrations/004_upgrade_closed_session_reassignment_guards.sql");

async fn setup_db() -> (TempDir, Pool) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.db");
    let pool = open_pool(&path).await.unwrap();
    run_migrations(&pool, &path).await.unwrap();
    (dir, pool)
}

fn cash_sale(total: &str, detail: &str) -> sales::RecordSaleRequest {
    sales::RecordSaleRequest {
        total_amount: total.to_string(),
        category: "ropa".to_string(),
        detail: Some(detail.to_string()),
        payments: vec![sales::PaymentPartRequest {
            method: "cash".to_string(),
            amount: total.to_string(),
            recipient_account: None,
        }],
    }
}

fn split_sale() -> sales::RecordSaleRequest {
    sales::RecordSaleRequest {
        total_amount: "100.00".to_string(),
        category: "mixto".to_string(),
        detail: Some("campera y cuaderno".to_string()),
        payments: vec![
            sales::PaymentPartRequest {
                method: "cash".to_string(),
                amount: "60.00".to_string(),
                recipient_account: None,
            },
            sales::PaymentPartRequest {
                method: "debit".to_string(),
                amount: "40.00".to_string(),
                recipient_account: None,
            },
        ],
    }
}

fn apply_sql(path: &Path, migrations: &[&str]) {
    let conn = Connection::open(path).unwrap();
    for sql in migrations {
        conn.execute_batch(sql).unwrap();
    }
}

#[tokio::test]
async fn record_sale_canonicalizes_amounts_and_rejects_changed_business_fields() {
    let (_dir, pool) = setup_db().await;
    cash::open_session(&pool, 10000, "hash-fields-open")
        .await
        .unwrap();
    let request = cash_sale("50.00", "campera");
    let first = sales::record_sale(&pool, request.clone(), "hash-fields-sale")
        .await
        .unwrap();
    for amount in ["50", "50.0"] {
        let replay = sales::record_sale(&pool, cash_sale(amount, "campera"), "hash-fields-sale")
            .await
            .unwrap();
        assert_eq!(replay.sale_id, first.sale_id);
    }

    let mut changed_category = request.clone();
    changed_category.category = "libreria".to_string();
    assert!(matches!(
        sales::record_sale(&pool, changed_category, "hash-fields-sale").await,
        Err(AppError::IdempotencyKeyReuse)
    ));

    let mut changed_detail = request;
    changed_detail.detail = Some("bufanda".to_string());
    assert!(matches!(
        sales::record_sale(&pool, changed_detail, "hash-fields-sale").await,
        Err(AppError::IdempotencyKeyReuse)
    ));

    let counts: (i64, i64) = pool
        .get()
        .await
        .unwrap()
        .interact(|conn| {
            conn.query_row(
                "SELECT (SELECT COUNT(*) FROM sales), (SELECT COUNT(*) FROM payments)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(counts, (1, 1));
}

#[tokio::test]
async fn integral_cash_session_lifecycle_and_closed_guards() {
    let (_dir, pool) = setup_db().await;
    let opened = cash::open_session(&pool, 10000, "integral-open")
        .await
        .unwrap();
    let sale = sales::record_sale(&pool, cash_sale("50.00", "campera"), "integral-sale")
        .await
        .unwrap();
    let withdrawal = sales::record_cash_out(
        &pool,
        "20.00".to_string(),
        Some("insumos".to_string()),
        "integral-withdrawal",
    )
    .await
    .unwrap();

    let reconciliation = cash::get_reconciliation(&pool, Some("130.00"))
        .await
        .unwrap();
    assert_eq!(reconciliation.session_id, opened.session_id);
    assert_eq!(reconciliation.expected_cash, "130.00");
    assert_eq!(reconciliation.difference.as_deref(), Some("0.00"));

    let closed = cash::close_session(&pool, 13000, "integral-close")
        .await
        .unwrap();
    assert_eq!(closed.session_id, opened.session_id);
    assert_eq!(closed.expected_cash, "130.00");
    assert_eq!(closed.difference, "0.00");

    let summaries = cash::list_closed_sessions(&pool).await.unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].session_id, opened.session_id);
    assert_eq!(summaries[0].counted_cash, "130.00");

    let detail = cash::get_closed_session_detail(&pool, &opened.session_id)
        .await
        .unwrap();
    assert_eq!(detail.operations.len(), 2);
    assert!(detail
        .operations
        .iter()
        .any(|operation| operation.id == sale.sale_id));
    assert!(detail
        .operations
        .iter()
        .any(|operation| operation.id == withdrawal.movement_id));
    assert_eq!(detail.payment_totals.cash, "50.00");

    assert!(matches!(
        cash::get_reconciliation(&pool, None).await,
        Err(AppError::NoOpenSession)
    ));
    assert!(matches!(
        cash::get_session_history(&pool).await,
        Err(AppError::NoOpenSession)
    ));
    assert!(matches!(
        sales::record_sale(&pool, cash_sale("10.00", "after close"), "after-close-sale").await,
        Err(AppError::NoOpenSession)
    ));
    assert!(matches!(
        sales::record_cash_out(&pool, "1.00".to_string(), None, "after-close-withdrawal").await,
        Err(AppError::NoOpenSession)
    ));
    assert!(matches!(
        sales::annul_sale(
            &pool,
            sales::AnnulSaleRequest {
                sale_id: sale.sale_id,
                reason: Some("too late".to_string()),
            },
            "after-close-annul",
        )
        .await,
        Err(AppError::SessionClosed)
    ));
    assert!(matches!(
        cash::close_session(&pool, 13000, "after-close-close").await,
        Err(AppError::NoOpenSession)
    ));
}

#[tokio::test]
async fn restart_preserves_open_session_sale_and_annulment_replay() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("restart.db");
    let first = initialize_app(&path).await.unwrap();
    let (session_id, sale_id, annulled) = {
        let db = first.db.read().await;
        let pool = db.pool().unwrap();
        let opened = cash::open_session(pool, 10000, "restart-open")
            .await
            .unwrap();
        let sale = sales::record_sale(pool, split_sale(), "restart-sale")
            .await
            .unwrap();
        let annulled = sales::annul_sale(
            pool,
            sales::AnnulSaleRequest {
                sale_id: sale.sale_id.clone(),
                reason: Some("error de carga".to_string()),
            },
            "restart-annul",
        )
        .await
        .unwrap();
        (opened.session_id, sale.sale_id, annulled)
    };
    drop(first);

    let second = initialize_app(&path).await.unwrap();
    let db = second.db.read().await;
    let pool = db.pool().unwrap();
    let schema_versions: i64 = pool
        .get()
        .await
        .unwrap()
        .interact(|conn| {
            conn.query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(schema_versions, 6);

    let view = cash::get_reconciliation(pool, None).await.unwrap();
    assert_eq!(view.session_id, session_id);
    assert_eq!(view.expected_cash, "100.00");
    let replayed_sale = sales::record_sale(pool, split_sale(), "restart-sale")
        .await
        .unwrap();
    assert_eq!(replayed_sale.sale_id, sale_id);
    let replayed_annulment = sales::annul_sale(
        pool,
        sales::AnnulSaleRequest {
            sale_id: sale_id.clone(),
            reason: Some("error de carga".to_string()),
        },
        "restart-annul",
    )
    .await
    .unwrap();
    assert_eq!(replayed_annulment.sale_id, annulled.sale_id);

    let err = sales::annul_sale(
        pool,
        sales::AnnulSaleRequest {
            sale_id: sale_id.clone(),
            reason: Some("different request".to_string()),
        },
        "restart-annul",
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::IdempotencyKeyReuse));

    let history = cash::get_session_history(pool).await.unwrap();
    assert_eq!(history.operations.len(), 1);
    assert_eq!(history.operations[0].id, sale_id);
    assert_eq!(history.operations[0].status.as_deref(), Some("annulled"));
    let reversal_count: i64 = pool
        .get()
        .await
        .unwrap()
        .interact(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM payment_reversals WHERE annulment_sale_id = ?1",
                [sale_id],
                |row| row.get(0),
            )
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reversal_count, 2);
    assert!(matches!(
        cash::open_session(pool, 1, "restart-second-open").await,
        Err(AppError::Conflict { .. })
    ));
}

#[tokio::test]
async fn migration_v4_to_v5_rebuild_preserves_payment_data() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("v4.db");
    apply_sql(&path, &[V1, V2, V3, V4]);
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES ('session-v4', 'open', 10000, '2026-08-03T09:00:00Z');
             INSERT INTO sales (id, session_id, total_amount, status, details) VALUES ('sale-v4', 'session-v4', 12345, 'confirmed', 'legacy sale');
             INSERT INTO payments (id, sale_id, method, amount, account, created_at) VALUES ('payment-v4', 'sale-v4', 'transfer', 12345, 'legacy-account', '2026-08-03T09:01:00Z');",
        )
        .unwrap();
    }
    let pool = open_pool(&path).await.unwrap();
    run_migrations(&pool, &path).await.unwrap();
    let conn = pool.get().await.unwrap();
    conn.interact(|conn| {
        let payment: (String, String, String, i64, Option<String>, Option<String>, String) = conn
            .query_row(
                "SELECT id, sale_id, method, amount, account, recipient_account, created_at FROM payments",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )?;
        assert_eq!(payment, ("payment-v4".into(), "sale-v4".into(), "transfer".into(), 12345, Some("legacy-account".into()), None, "2026-08-03T09:01:00Z".into()));
        let category: String = conn.query_row("SELECT category FROM sales WHERE id = 'sale-v4'", [], |row| row.get(0))?;
        assert_eq!(category, "mixto");
        let recipient_column: i64 = conn.query_row("SELECT COUNT(*) FROM pragma_table_info('payments') WHERE name = 'recipient_account'", [], |row| row.get(0))?;
        assert_eq!(recipient_column, 1);
        Ok::<_, rusqlite::Error>(())
    })
    .await
    .unwrap()
    .unwrap();
}

#[tokio::test]
async fn migration_v1_to_current_preserves_representative_data() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("v1.db");
    apply_sql(&path, &[V1]);
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES ('session-v1', 'open', 5000, '2026-08-03T08:00:00Z');
             INSERT INTO sales (id, session_id, total_amount, status, details) VALUES ('sale-v1', 'session-v1', 2500, 'confirmed', 'legacy');
             INSERT INTO payments (id, sale_id, method, amount, account) VALUES ('payment-v1', 'sale-v1', 'cash', 2500, 'legacy-cash');
             INSERT INTO movements (id, session_id, kind, amount, note) VALUES ('movement-v1', 'session-v1', 'withdrawal', 500, 'legacy withdrawal');
             INSERT INTO idempotency_keys (idempotency_key, command, request_hash, response_json) VALUES ('key-v1', 'record_sale', 'hash-v1', '{\"sale_id\":\"sale-v1\"}');
             INSERT INTO backup_runs (id, run_at, path, success, error) VALUES ('backup-v1', '2026-08-03T08:02:00Z', '/legacy.db', 1, NULL);",
        )
        .unwrap();
    }
    let pool = open_pool(&path).await.unwrap();
    run_migrations(&pool, &path).await.unwrap();
    let conn = pool.get().await.unwrap();
    conn.interact(|conn| {
        let session: (String, i64) = conn.query_row(
            "SELECT status, opening_amount FROM cash_sessions WHERE id = 'session-v1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(session, ("open".into(), 5000));
        let sale: (i64, String, String) = conn.query_row(
            "SELECT total_amount, details, category FROM sales WHERE id = 'sale-v1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        assert_eq!(sale, (2500, "legacy".into(), "mixto".into()));
        let movement: i64 = conn.query_row(
            "SELECT amount FROM movements WHERE id = 'movement-v1'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(movement, 500);
        let idem: String = conn.query_row(
            "SELECT response_json FROM idempotency_keys WHERE idempotency_key = 'key-v1'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(idem, "{\"sale_id\":\"sale-v1\"}");
        let backup: (String, i64) = conn.query_row(
            "SELECT kind, managed FROM backup_runs WHERE id = 'backup-v1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(backup, ("automatic".into(), 1));
        let versions: i64 =
            conn.query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })?;
        assert_eq!(versions, 6);
        Ok::<_, rusqlite::Error>(())
    })
    .await
    .unwrap()
    .unwrap();
}

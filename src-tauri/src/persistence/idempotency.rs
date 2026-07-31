use deadpool_sqlite::Pool;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::error::AppError;

/// Public status for an idempotency key lookup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum MutationStatus {
    Committed { response: Value },
    NotFound,
}

/// Lookup the committed result for an idempotency key.
///
/// Returns `Committed` with the stored JSON response if the key exists,
/// otherwise `NotFound`.
pub async fn get_mutation_status(pool: &Pool, key: &str) -> Result<MutationStatus, AppError> {
    let key = key.to_string();
    let result = pool
        .get()
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .interact(move |conn| {
            conn.query_row(
                "SELECT response_json FROM idempotency_keys WHERE idempotency_key = ?1",
                [&key],
                |row| {
                    let response: String = row.get(0)?;
                    Ok(response)
                },
            )
            .optional()
        })
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    match result {
        Ok(Some(response_json)) => {
            let response = serde_json::from_str(&response_json)?;
            Ok(MutationStatus::Committed { response })
        }
        Ok(None) => Ok(MutationStatus::NotFound),
        Err(e) => Err(AppError::Database(e.to_string())),
    }
}

/// Execute a mutation under an idempotency key, atomically inside one transaction.
///
/// The caller provides:
/// - `key`: the client-generated idempotency key.
/// - `command`: a command name for diagnostics.
/// - `request_hash`: a hash of the canonical request bytes.
/// - `f`: a closure that performs the financial mutation and returns a serializable response.
///
/// Rules:
/// 1. If the key exists with a matching request hash, the stored response is replayed.
/// 2. If the key exists with a different request hash, `IDEMPOTENCY_KEY_REUSE` is returned.
/// 3. Otherwise the mutation runs and the key + response are inserted in the same transaction.
///
/// Because the whole flow runs inside `BEGIN IMMEDIATE`, concurrent writers are serialized.
/// A rollback leaves neither the mutation nor the idempotency row committed.
pub async fn with_idempotency<R, F>(
    pool: &Pool,
    key: &str,
    command: &str,
    request_hash: &str,
    f: F,
) -> Result<R, AppError>
where
    R: Serialize + for<'de> Deserialize<'de> + Send + 'static,
    F: FnOnce(&mut rusqlite::Transaction<'_>) -> Result<R, AppError> + Send + 'static,
{
    let key = key.to_string();
    let command = command.to_string();
    let request_hash = request_hash.to_string();

    let response = pool
        .get()
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .interact(move |conn| {
            let mut tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

            let existing: Option<(String, String)> = tx
                .query_row(
                    "SELECT request_hash, response_json FROM idempotency_keys WHERE idempotency_key = ?1",
                    [&key],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;

            if let Some((stored_hash, stored_response)) = existing {
                if stored_hash == request_hash {
                    let response: R = serde_json::from_str(&stored_response)?;
                    return Ok(response);
                } else {
                    return Err(AppError::IdempotencyKeyReuse);
                }
            }

            let result = f(&mut tx)?;
            let response_json = serde_json::to_string(&result)?;

            tx.execute(
                "INSERT INTO idempotency_keys (idempotency_key, command, request_hash, response_json) VALUES (?1, ?2, ?3, ?4)",
                [&key, &command, &request_hash, &response_json],
            )?;

            tx.commit()?;
            Ok(result)
        })
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::connection::{open_pool, run_migrations};
    use serde_json::json;
    use tempfile::TempDir;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct DummyResponse {
        id: String,
        amount: i64,
    }

    #[tokio::test]
    async fn idempotency_commits_mutation_and_key_together() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool, &db_path).await.unwrap();

        let response: DummyResponse = with_idempotency(
            &pool,
            "key-1",
            "create_sale",
            "hash-a",
            |tx| {
                tx.execute(
                    "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, ?2, ?3, ?4)",
                    ["s1", "open", "1000", "2024-01-01T00:00:00Z"],
                )?;
                Ok(DummyResponse {
                    id: "s1".to_string(),
                    amount: 1000,
                })
            },
        )
        .await
        .unwrap();

        assert_eq!(response.id, "s1");

        let status = get_mutation_status(&pool, "key-1").await.unwrap();
        assert_eq!(
            status,
            MutationStatus::Committed {
                response: json!({"id":"s1","amount":1000})
            }
        );
    }

    #[tokio::test]
    async fn idempotency_replays_matching_hash() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool, &db_path).await.unwrap();

        let first: DummyResponse = with_idempotency(&pool, "key-2", "cmd", "hash-a", |tx| {
            tx.execute(
                "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, ?2, ?3, ?4)",
                ["s2", "open", "500", "2024-01-01T00:00:00Z"],
            )?;
            Ok(DummyResponse {
                id: "s2".to_string(),
                amount: 500,
            })
        })
        .await
        .unwrap();

        let second: DummyResponse = with_idempotency(&pool, "key-2", "cmd", "hash-a", |_tx| {
            panic!("mutation must not rerun when key matches")
        })
        .await
        .unwrap();

        assert_eq!(first, second);
    }

    #[tokio::test]
    async fn idempotency_rejects_different_hash() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool, &db_path).await.unwrap();

        with_idempotency(&pool, "key-3", "cmd", "hash-a", |tx| {
            tx.execute(
                "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, ?2, ?3, ?4)",
                ["s3", "open", "100", "2024-01-01T00:00:00Z"],
            )?;
            Ok(DummyResponse {
                id: "s3".to_string(),
                amount: 100,
            })
        })
        .await
        .unwrap();

        let err = with_idempotency(&pool, "key-3", "cmd", "hash-b", |_tx| {
            Ok(DummyResponse {
                id: "x".to_string(),
                amount: 0,
            })
        })
        .await
        .unwrap_err();

        assert!(matches!(err, AppError::IdempotencyKeyReuse));
    }

    #[tokio::test]
    async fn idempotency_rollback_leaves_no_key() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool, &db_path).await.unwrap();

        let err = with_idempotency(&pool, "key-rollback", "cmd", "hash-a", |_tx| {
            Err::<DummyResponse, _>(AppError::validation("forced failure"))
        })
        .await
        .unwrap_err();

        assert!(matches!(err, AppError::Validation { .. }));

        let status = get_mutation_status(&pool, "key-rollback").await.unwrap();
        assert_eq!(status, MutationStatus::NotFound);
    }

    #[tokio::test]
    async fn not_found_for_unknown_key() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool, &db_path).await.unwrap();

        let status = get_mutation_status(&pool, "unknown-key").await.unwrap();
        assert_eq!(status, MutationStatus::NotFound);
    }
}

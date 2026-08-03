use deadpool_sqlite::Pool;
use rusqlite::Transaction;

use crate::domain::error::AppError;
use crate::domain::sale::PaymentPart;
use crate::domain::Money;

/// Record a sale with its payment parts inside a transaction.
///
/// Returns the sale_id.
pub fn insert_sale(
    tx: &mut Transaction<'_>,
    session_id: &str,
    total_amount: i64,
    category: &str,
    detail: Option<&str>,
    payments: &[PaymentPart],
) -> Result<String, AppError> {
    let sale_id = uuid::Uuid::new_v4().to_string();

    tx.execute(
        "INSERT INTO sales (id, session_id, total_amount, category, details) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![sale_id, session_id, total_amount, category, detail],
    )?;

    for part in payments {
        let payment_id = uuid::Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO payments (id, sale_id, method, amount, recipient_account) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                payment_id,
                sale_id,
                part.method.as_str(),
                part.amount,
                part.recipient_account
            ],
        )?;
    }

    Ok(sale_id)
}

/// Record a cash-out movement inside a transaction.
///
/// Returns the movement_id.
pub fn insert_cash_out(
    tx: &mut Transaction<'_>,
    session_id: &str,
    amount: i64,
    detail: Option<&str>,
) -> Result<String, AppError> {
    let movement_id = uuid::Uuid::new_v4().to_string();

    // Cash-outs are recorded as 'withdrawal' movements.
    tx.execute(
        "INSERT INTO movements (id, session_id, kind, amount, note) VALUES (?1, ?2, 'withdrawal', ?3, ?4)",
        rusqlite::params![movement_id, session_id, amount, detail],
    )?;

    Ok(movement_id)
}

/// Annul a sale and create one immutable payment reversal per original payment.
pub fn annul_sale(
    tx: &mut Transaction<'_>,
    sale_id: &str,
    reason: Option<&str>,
) -> Result<(), AppError> {
    let payment_parts: Vec<(String, i64)> = {
        let mut statement = tx.prepare("SELECT id, amount FROM payments WHERE sale_id = ?1")?;
        let rows = statement
            .query_map([sale_id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };

    let rows_changed = tx.execute(
        "UPDATE sales SET status = 'annulled', annulment_reason = ?2, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = ?1 AND status = 'confirmed'",
        rusqlite::params![sale_id, reason],
    )?;
    if rows_changed == 0 {
        return Err(AppError::conflict("sale is no longer confirmed"));
    }

    for (payment_id, amount) in payment_parts {
        tx.execute(
            "INSERT INTO payment_reversals (id, payment_id, amount, annulment_sale_id) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![uuid::Uuid::new_v4().to_string(), payment_id, amount, sale_id],
        )?;
    }

    Ok(())
}

/// Validate that the payment parts sum to the sale total.
pub fn validate_payment_sum(total_amount: i64, payments: &[PaymentPart]) -> Result<(), AppError> {
    let sum: i64 = payments.iter().map(|p| p.amount).sum();

    if sum != total_amount {
        return Err(AppError::validation(format!(
            "payment parts sum ({}) does not match sale total ({})",
            Money::from_cents(sum),
            Money::from_cents(total_amount),
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::sale::{PaymentMethod, PaymentPart};

    #[test]
    fn validate_payment_sum_accepts_matching_total() {
        let payments = vec![
            PaymentPart {
                method: PaymentMethod::Cash,
                amount: 3000,
                recipient_account: None,
            },
            PaymentPart {
                method: PaymentMethod::Debit,
                amount: 2000,
                recipient_account: None,
            },
        ];
        assert!(validate_payment_sum(5000, &payments).is_ok());
    }

    #[test]
    fn validate_payment_sum_rejects_mismatch() {
        let payments = vec![PaymentPart {
            method: PaymentMethod::Cash,
            amount: 3000,
            recipient_account: None,
        }];
        assert!(validate_payment_sum(5000, &payments).is_err());
    }

    #[test]
    fn validate_payment_sum_accepts_single_payment() {
        let payments = vec![PaymentPart {
            method: PaymentMethod::Qr,
            amount: 10000,
            recipient_account: None,
        }];
        assert!(validate_payment_sum(10000, &payments).is_ok());
    }
}

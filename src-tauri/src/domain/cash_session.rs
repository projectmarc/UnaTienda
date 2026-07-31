use chrono::Utc;

use crate::domain::error::AppError;

/// Domain model for a cash session.
///
/// A cash session represents a period of operation on a single computer.
/// At most one session may be open at a time (enforced by a partial unique
/// index in SQLite). The session tracks the opening amount, reconciliation
/// data, and lifecycle state.
#[derive(Debug, Clone, PartialEq)]
pub struct CashSession {
    pub id: String,
    pub status: SessionStatus,
    pub opening_amount: i64,
    pub counted_cash: Option<i64>,
    pub difference: Option<i64>,
    pub opened_at: String,
    pub closed_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    Open,
    Closed,
}

impl SessionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionStatus::Open => "open",
            SessionStatus::Closed => "closed",
        }
    }
}

impl CashSession {
    /// Open a new cash session with the given opening amount.
    ///
    /// The opening amount must be non-negative. The caller supplies a
    /// pre-generated UUID; the domain does not own identity generation.
    pub fn open(id: String, opening_amount: i64) -> Result<Self, AppError> {
        if opening_amount < 0 {
            return Err(AppError::validation_field(
                "opening amount must be non-negative",
                "opening_amount",
            ));
        }

        Ok(Self {
            id,
            status: SessionStatus::Open,
            opening_amount,
            counted_cash: None,
            difference: None,
            opened_at: Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            closed_at: None,
        })
    }

    /// Close this session.
    ///
    /// The caller must supply `expected_cash` (computed by the persistence
    /// layer from aggregations) and `draft_count` (also from persistence).
    /// Closure is rejected when:
    /// - The session is not open.
    /// - Suspended drafts remain (`draft_count > 0`).
    ///
    /// On success the session records `counted_cash`, the `difference` between
    /// counted and expected cash, and a `closed_at` timestamp.
    pub fn close(
        &mut self,
        expected_cash: i64,
        counted_cash: i64,
        draft_count: i64,
    ) -> Result<(), AppError> {
        if self.status != SessionStatus::Open {
            return Err(AppError::SessionClosed);
        }

        if draft_count > 0 {
            return Err(AppError::DraftsBlockClose);
        }

        // JD2-A-003 fix: use checked subtraction to prevent i64 overflow on extreme values.
        let difference = counted_cash
            .checked_sub(expected_cash)
            .ok_or_else(|| AppError::validation("money subtraction overflow in difference"))?;

        self.status = SessionStatus::Closed;
        self.counted_cash = Some(counted_cash);
        self.difference = Some(difference);
        self.closed_at = Some(Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());

        Ok(())
    }

    /// Compute the reconciliation difference for display.
    ///
    /// Returns the difference between counted cash and expected cash using
    /// checked subtraction. The caller supplies `expected_cash` from the
    /// persistence layer.
    pub fn reconcile(&self, expected_cash: i64, counted_cash: i64) -> Result<i64, AppError> {
        counted_cash
            .checked_sub(expected_cash)
            .ok_or_else(|| AppError::validation("money subtraction overflow in reconcile"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_session_succeeds() {
        let session = CashSession::open("test-id".to_string(), 1000).unwrap();
        assert_eq!(session.status, SessionStatus::Open);
        assert_eq!(session.opening_amount, 1000);
        assert_eq!(session.counted_cash, None);
        assert_eq!(session.difference, None);
        assert!(session.closed_at.is_none());
    }

    #[test]
    fn open_session_zero_amount() {
        let session = CashSession::open("test-id".to_string(), 0).unwrap();
        assert_eq!(session.opening_amount, 0);
    }

    #[test]
    fn open_session_rejects_negative_amount() {
        let err = CashSession::open("test-id".to_string(), -1).unwrap_err();
        assert!(matches!(err, AppError::Validation { .. }));
        assert!(err.to_string().contains("non-negative"));
    }

    #[test]
    fn close_session_records_difference() {
        let mut session = CashSession::open("test-id".to_string(), 1000).unwrap();
        // expected_cash = 1000 opening + 500 cash payments = 1500
        session.close(1500, 1600, 0).unwrap();

        assert_eq!(session.status, SessionStatus::Closed);
        assert_eq!(session.counted_cash, Some(1600));
        assert_eq!(session.difference, Some(100)); // 1600 - 1500
        assert!(session.closed_at.is_some());
    }

    #[test]
    fn close_session_with_zero_difference() {
        let mut session = CashSession::open("test-id".to_string(), 1000).unwrap();
        session.close(1000, 1000, 0).unwrap();
        assert_eq!(session.difference, Some(0));
    }

    #[test]
    fn close_session_rejects_when_not_open() {
        let mut session = CashSession::open("test-id".to_string(), 1000).unwrap();
        session.close(1000, 1000, 0).unwrap();

        let err = session.close(1000, 1000, 0).unwrap_err();
        assert!(matches!(err, AppError::SessionClosed));
    }

    #[test]
    fn close_session_rejects_when_drafts_remain() {
        let mut session = CashSession::open("test-id".to_string(), 1000).unwrap();
        let err = session.close(1000, 1000, 2).unwrap_err();
        assert!(matches!(err, AppError::DraftsBlockClose));
    }

    #[test]
    fn reconcile_computes_difference() {
        let session = CashSession::open("test-id".to_string(), 1000).unwrap();
        assert_eq!(session.reconcile(1500, 1600).unwrap(), 100);
        assert_eq!(session.reconcile(1500, 1400).unwrap(), -100);
        assert_eq!(session.reconcile(1500, 1500).unwrap(), 0);
    }

    #[test]
    fn reconcile_rejects_overflow() {
        let session = CashSession::open("test-id".to_string(), 0).unwrap();
        // i64::MIN - i64::MAX would overflow.
        let err = session.reconcile(i64::MAX, i64::MIN).unwrap_err();
        assert!(matches!(err, AppError::Validation { .. }));
    }

    /// JD2-A-003 regression: extreme Money values that would overflow
    /// i64 subtraction must be rejected, not silently wrap.
    #[test]
    fn close_session_rejects_overflowing_difference() {
        let mut session = CashSession::open("test-id".to_string(), 0).unwrap();
        // i64::MIN counted - i64::MAX expected would overflow in unchecked subtraction.
        let err = session.close(i64::MAX, i64::MIN, 0).unwrap_err();
        assert!(matches!(err, AppError::Validation { .. }));
        assert!(err.to_string().contains("overflow"));
    }

    /// JD2-A-003 regression: values that do NOT overflow must still succeed.
    #[test]
    fn close_session_accepts_valid_extreme_difference() {
        let mut session = CashSession::open("test-id".to_string(), 0).unwrap();
        // i64::MAX - 0 = i64::MAX (no overflow)
        session.close(0, i64::MAX, 0).unwrap();
        assert_eq!(session.difference, Some(i64::MAX));
    }
}

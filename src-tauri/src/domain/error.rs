use serde::Serialize;

/// Application error domain with a stable public JSON contract for IPC.
#[derive(Debug, thiserror::Error, Clone, PartialEq)]
pub enum AppError {
    #[error("validation error: {message}")]
    Validation {
        message: String,
        field: Option<String>,
    },
    #[error("no open session")]
    NoOpenSession,
    #[error("session is closed")]
    SessionClosed,
    #[error("drafts block session close")]
    DraftsBlockClose,
    #[error("conflict: {message}")]
    Conflict { message: String },
    #[error("idempotency key reuse")]
    IdempotencyKeyReuse,
    #[error("io error: {0}")]
    Io(String),
    #[error("incompatible backup")]
    IncompatibleBackup,
    #[error("backup error: {0}")]
    Backup(String),
    #[error("restore is blocked while a cash session is open")]
    RestoreBlockedOpenSession,
    #[error("database error: {0}")]
    Database(String),
    #[error("database is unavailable and needs recovery: {message}")]
    DatabaseUnavailable { message: String },
    #[error("not found")]
    NotFound,
}

impl AppError {
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation {
            message: message.into(),
            field: None,
        }
    }

    pub fn validation_field(message: impl Into<String>, field: impl Into<String>) -> Self {
        Self::Validation {
            message: message.into(),
            field: Some(field.into()),
        }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict {
            message: message.into(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            AppError::Validation { .. } => "VALIDATION",
            AppError::NoOpenSession => "NO_OPEN_SESSION",
            AppError::SessionClosed => "SESSION_CLOSED",
            AppError::DraftsBlockClose => "DRAFTS_BLOCK_CLOSE",
            AppError::Conflict { .. } => "CONFLICT",
            AppError::IdempotencyKeyReuse => "IDEMPOTENCY_KEY_REUSE",
            AppError::Io(_) => "IO",
            AppError::IncompatibleBackup => "INCOMPATIBLE_BACKUP",
            AppError::Backup(_) => "BACKUP_FAILED",
            AppError::RestoreBlockedOpenSession => "RESTORE_BLOCKED_OPEN_SESSION",
            AppError::Database(_) => "DATABASE",
            AppError::DatabaseUnavailable { .. } => "DATABASE_UNAVAILABLE",
            AppError::NotFound => "NOT_FOUND",
        }
    }

    fn retryable(&self) -> bool {
        match self {
            AppError::Conflict { .. } => true,
            AppError::Io(_) => true,
            AppError::Backup(_) => true,
            AppError::Database(_) => true,
            AppError::DatabaseUnavailable { .. } => true,
            _ => false,
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Io(err.to_string())
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(err: rusqlite::Error) -> Self {
        AppError::Database(err.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::Validation {
            message: err.to_string(),
            field: None,
        }
    }
}

/// Public JSON shape: `{ code, message, field?, retryable }`.
#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ErrorResponse {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    pub retryable: bool,
}

impl From<&AppError> for ErrorResponse {
    fn from(err: &AppError) -> Self {
        Self {
            code: err.code().to_string(),
            message: err.to_string(),
            field: match err {
                AppError::Validation { field, .. } => field.clone(),
                _ => None,
            },
            retryable: err.retryable(),
        }
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        ErrorResponse::from(self).serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn error_serializes_public_shape() {
        let err = AppError::validation_field("amount is required", "amount");
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "VALIDATION");
        assert_eq!(json["message"], "validation error: amount is required");
        assert_eq!(json["field"], "amount");
        assert_eq!(json["retryable"], false);
    }

    #[test]
    fn io_error_is_retryable() {
        let err = AppError::Io("disk full".to_string());
        assert_eq!(err.code(), "IO");
        assert!(err.retryable());
    }

    #[test]
    fn validation_omits_field_when_absent() {
        let err = AppError::validation("bad input");
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "VALIDATION");
        assert!(json.get("field").is_none());
    }
}

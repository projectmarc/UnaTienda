pub mod backup_commands;
pub mod cash_session_commands;
pub mod sale_commands;

use deadpool_sqlite::Pool;

use crate::domain::error::{AppError, ErrorResponse};
use crate::persistence::connection::AppDb;

pub(crate) fn app_error(error: AppError) -> String {
    serde_json::to_string(&ErrorResponse::from(&error)).unwrap_or_else(|_| error.to_string())
}

pub(crate) fn usable_pool(db: &AppDb) -> Result<&Pool, String> {
    db.pool().map_err(app_error)
}

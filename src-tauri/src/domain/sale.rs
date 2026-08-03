use serde::{Deserialize, Serialize};

use crate::domain::error::AppError;

/// Sale category — exactly one of three allowed values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SaleCategory {
    Ropa,
    Libreria,
    Mixto,
}

impl SaleCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            SaleCategory::Ropa => "ropa",
            SaleCategory::Libreria => "libreria",
            SaleCategory::Mixto => "mixto",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, AppError> {
        match s {
            "ropa" => Ok(SaleCategory::Ropa),
            "libreria" => Ok(SaleCategory::Libreria),
            "mixto" => Ok(SaleCategory::Mixto),
            _ => Err(AppError::validation_field(
                "invalid category; expected ropa, libreria, or mixto",
                "category",
            )),
        }
    }
}

impl std::fmt::Display for SaleCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for SaleCategory {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        SaleCategory::from_str(s)
    }
}

/// Payment method — one of the five allowed values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentMethod {
    Cash,
    Transfer,
    Debit,
    Credit,
    Qr,
}

impl PaymentMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            PaymentMethod::Cash => "cash",
            PaymentMethod::Transfer => "transfer",
            PaymentMethod::Debit => "debit",
            PaymentMethod::Credit => "credit",
            PaymentMethod::Qr => "qr",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, AppError> {
        match s {
            "cash" => Ok(PaymentMethod::Cash),
            "transfer" => Ok(PaymentMethod::Transfer),
            "debit" => Ok(PaymentMethod::Debit),
            "credit" => Ok(PaymentMethod::Credit),
            "qr" => Ok(PaymentMethod::Qr),
            _ => Err(AppError::validation_field(
                "invalid payment method; expected cash, transfer, debit, credit, or qr",
                "method",
            )),
        }
    }

    /// Transfer payments require a recipient account.
    pub fn requires_recipient(&self) -> bool {
        matches!(self, PaymentMethod::Transfer)
    }
}

impl std::fmt::Display for PaymentMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for PaymentMethod {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        PaymentMethod::from_str(s)
    }
}

/// A payment part within a sale.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentPart {
    pub method: PaymentMethod,
    pub amount: i64,
    pub recipient_account: Option<String>,
}

impl PaymentPart {
    /// Validate this payment part.
    pub fn validate(&self) -> Result<(), AppError> {
        if self.amount <= 0 {
            return Err(AppError::validation_field(
                "payment amount must be positive",
                "amount",
            ));
        }
        if self.method.requires_recipient() && self.recipient_account.is_none() {
            return Err(AppError::validation_field(
                "transfer payments require a recipient account",
                "recipient_account",
            ));
        }
        Ok(())
    }
}

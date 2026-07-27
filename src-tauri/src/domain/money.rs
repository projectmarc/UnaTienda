use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::domain::error::AppError;

/// Exact monetary amount in the smallest currency unit (e.g., centavos).
///
/// Internally `i64` so that SQLite can store it exactly and Rust can detect
/// overflow. Displayed and parsed as a decimal string over the IPC boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Money(i64);

impl Money {
    pub const ZERO: Money = Money(0);

    pub fn from_cents(cents: i64) -> Self {
        Money(cents)
    }

    pub fn cents(&self) -> i64 {
        self.0
    }

    pub fn is_negative(&self) -> bool {
        self.0 < 0
    }

    pub fn is_zero(&self) -> bool {
        self.0 == 0
    }

    pub fn is_positive(&self) -> bool {
        self.0 > 0
    }

    /// Checked addition returning `AppError` on overflow.
    pub fn checked_add(&self, other: &Money) -> Result<Money, AppError> {
        self.0
            .checked_add(other.0)
            .map(Money)
            .ok_or_else(|| AppError::validation("money addition overflow"))
    }

    /// Checked subtraction returning `AppError` on overflow.
    pub fn checked_sub(&self, other: &Money) -> Result<Money, AppError> {
        self.0
            .checked_sub(other.0)
            .map(Money)
            .ok_or_else(|| AppError::validation("money subtraction overflow"))
    }

    /// Wrapping addition for contexts where overflow is impossible by construction.
    pub fn add(&self, other: &Money) -> Money {
        Money(self.0.saturating_add(other.0))
    }

    /// Wrapping subtraction for contexts where overflow is impossible by construction.
    pub fn sub(&self, other: &Money) -> Money {
        Money(self.0.saturating_sub(other.0))
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        // Use i128 to avoid `i64::MIN.abs()` overflow while preserving exact magnitude.
        let abs = (self.0 as i128).unsigned_abs() as u64;
        let units = abs / 100;
        let cents = abs % 100;
        write!(f, "{}{}.{:02}", sign, units, cents)
    }
}

impl Serialize for Money {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Money {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Money::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl FromStr for Money {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() {
            return Err(AppError::validation_field("amount is empty", "amount"));
        }

        let negative = s.starts_with('-');
        let digits = if negative { &s[1..] } else { s };

        let parts: Vec<&str> = digits.split('.').collect();
        if parts.len() > 2 {
            return Err(AppError::validation_field(
                "amount has more than one decimal point",
                "amount",
            ));
        }

        let units = parts[0].parse::<u64>().map_err(|_| {
            AppError::validation_field("amount units are not a valid integer", "amount")
        })?;

        let fraction = if parts.len() == 2 {
            let frac = parts[1];
            if frac.len() > 2 {
                return Err(AppError::validation_field(
                    "amount has more than two decimal places",
                    "amount",
                ));
            }
            let padded = format!("{:0<2}", frac);
            padded
                .parse::<u64>()
                .map_err(|_| AppError::validation_field("amount fraction is not valid", "amount"))?
        } else {
            0u64
        };

        let abs_total = units
            .checked_mul(100)
            .and_then(|u| u.checked_add(fraction))
            .ok_or_else(|| AppError::validation("amount is too large"))?;

        let cents = if negative {
            let min_abs = (i64::MAX as u64)
                .checked_add(1)
                .expect("i64::MAX + 1 fits in u64");
            if abs_total > min_abs {
                return Err(AppError::validation("amount is too large"));
            }
            if abs_total == min_abs {
                i64::MIN
            } else {
                -(abs_total as i64)
            }
        } else if abs_total > i64::MAX as u64 {
            return Err(AppError::validation("amount is too large"));
        } else {
            abs_total as i64
        };

        Ok(Money::from_cents(cents))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_cents_round_trip() {
        let m = Money::from_cents(12345);
        assert_eq!(m.cents(), 12345);
        assert_eq!(m.to_string(), "123.45");
    }

    #[test]
    fn display_zero_pads_cents() {
        assert_eq!(Money::from_cents(5).to_string(), "0.05");
        assert_eq!(Money::from_cents(100).to_string(), "1.00");
        assert_eq!(Money::from_cents(-99).to_string(), "-0.99");
    }

    #[test]
    fn parse_valid_decimal() {
        assert_eq!(Money::from_str("10.00").unwrap(), Money::from_cents(1000));
        assert_eq!(Money::from_str("0.05").unwrap(), Money::from_cents(5));
        assert_eq!(Money::from_str("123").unwrap(), Money::from_cents(12300));
        assert_eq!(Money::from_str("-7.50").unwrap(), Money::from_cents(-750));
    }

    #[test]
    fn parse_rejects_excess_precision() {
        let err = Money::from_str("1.001").unwrap_err();
        assert!(matches!(err, AppError::Validation { .. }));
        assert!(err.to_string().contains("more than two decimal places"));
    }

    #[test]
    fn parse_rejects_invalid_input() {
        assert!(Money::from_str("").is_err());
        assert!(Money::from_str("abc").is_err());
        assert!(Money::from_str("1.2.3").is_err());
    }

    #[test]
    fn checked_add_detects_overflow() {
        let a = Money::from_cents(i64::MAX);
        let b = Money::from_cents(1);
        assert!(a.checked_add(&b).is_err());
    }

    #[test]
    fn checked_sub_detects_overflow() {
        let a = Money::from_cents(i64::MIN);
        let b = Money::from_cents(1);
        assert!(a.checked_sub(&b).is_err());
    }

    #[test]
    fn add_and_sub_wrap_saturate() {
        let a = Money::from_cents(100);
        let b = Money::from_cents(50);
        assert_eq!(a.add(&b), Money::from_cents(150));
        assert_eq!(a.sub(&b), Money::from_cents(50));
    }

    #[test]
    fn checked_sum_cascades() {
        let values = [
            Money::from_cents(100),
            Money::from_cents(200),
            Money::from_cents(50),
        ];
        let total = values
            .iter()
            .try_fold(Money::ZERO, |acc, v| acc.checked_add(v))
            .unwrap();
        assert_eq!(total, Money::from_cents(350));
    }

    #[test]
    fn parse_negative_zero() {
        assert_eq!(Money::from_str("-0.00").unwrap(), Money::ZERO);
    }

    /// Prove JD-A-008: Money serializes as a decimal string over IPC.
    #[test]
    fn serde_serializes_decimal_string() {
        let money = Money::from_cents(12345);
        let json = serde_json::to_string(&money).unwrap();
        assert_eq!(json, "\"123.45\"");
    }

    /// Prove JD-A-008: Money deserializes from a decimal string.
    #[test]
    fn serde_deserializes_decimal_string() {
        let money: Money = serde_json::from_str("\"123.45\"").unwrap();
        assert_eq!(money, Money::from_cents(12345));
        assert_eq!(Money::from_str("0.05").unwrap(), Money::from_cents(5));
    }

    /// Prove JD-A-008: excess precision is rejected during deserialization.
    #[test]
    fn serde_rejects_excess_precision() {
        let result: Result<Money, _> = serde_json::from_str("\"1.001\"");
        assert!(result.is_err());
    }

    /// Prove JD-A-008: numeric cents are rejected; the contract requires strings.
    #[test]
    fn serde_rejects_numeric_cents() {
        let result: Result<Money, _> = serde_json::from_str("12345");
        assert!(
            result.is_err(),
            "numeric cents must be rejected; contract requires decimal string"
        );
    }

    /// Prove R3-002: i64::MAX displays without overflow and round-trips.
    #[test]
    fn display_upper_boundary() {
        let m = Money::from_cents(i64::MAX);
        assert_eq!(m.to_string(), "92233720368547758.07");
        assert_eq!(Money::from_str("92233720368547758.07").unwrap(), m);
    }

    /// Prove R3-002: i64::MIN displays without panic and round-trips.
    #[test]
    fn display_lower_boundary() {
        let m = Money::from_cents(i64::MIN);
        assert_eq!(m.to_string(), "-92233720368547758.08");
        assert_eq!(Money::from_str("-92233720368547758.08").unwrap(), m);
    }

    /// Prove R3-002: Serde round-trips the complete i64 domain including MIN.
    #[test]
    fn serde_round_trips_full_domain() {
        for cents in [i64::MIN, i64::MIN + 1, -1, 0, 1, i64::MAX - 1, i64::MAX] {
            let original = Money::from_cents(cents);
            let json = serde_json::to_string(&original).unwrap();
            let deserialized: Money = serde_json::from_str(&json).unwrap();
            assert_eq!(
                deserialized, original,
                "Money({}) must serialize and deserialize exactly",
                cents
            );
        }
    }

    /// Prove R3-002: values just past the upper and lower boundaries are rejected.
    #[test]
    fn parse_rejects_out_of_domain() {
        assert!(
            Money::from_str("92233720368547758.08").is_err(),
            "just above i64::MAX"
        );
        assert!(
            Money::from_str("-92233720368547758.09").is_err(),
            "just below i64::MIN"
        );
    }
}

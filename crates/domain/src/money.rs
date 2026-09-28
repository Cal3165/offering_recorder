use crate::errors::{DomainError, DomainResult};
use serde::{Deserialize, Deserializer, Serialize};
use std::ops::{Add, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Money {
    pence: i64,
}

impl<'de> Deserialize<'de> for Money {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct MoneyHelper {
            pence: i64,
        }
        // Accept both the struct form {"pence": N} and a bare integer N
        // (persistence/IPC layers historically use both).
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum MoneyRepr {
            Struct(MoneyHelper),
            Bare(i64),
        }
        let repr = MoneyRepr::deserialize(deserializer)?;
        let pence = match repr {
            MoneyRepr::Struct(h) => h.pence,
            MoneyRepr::Bare(p) => p,
        };
        Money::from_pence(pence).map_err(serde::de::Error::custom)
    }
}

impl Money {
    #[must_use]
    pub const fn zero() -> Self {
        Money { pence: 0 }
    }

    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn from_pence(pence: i64) -> DomainResult<Self> {
        if pence < 0 {
            return Err(DomainError::NegativeValue);
        }
        Ok(Money { pence })
    }

    /// Trusted-construction path for internal decoders (e.g. reading INTEGER
    /// pence columns that the schema CHECKs constrain to be non-negative).
    /// Validates in ALL builds (not just debug): a negative is a data bug and
    /// panics loudly instead of constructing an impossible value. Application/
    /// domain logic must use [`Money::from_pence`], which rejects negatives
    /// with an error (review O7).
    ///
    /// # Panics
    ///
    /// Panics when `pence` is negative.
    #[must_use]
    pub fn from_pence_unchecked(pence: i64) -> Self {
        assert!(pence >= 0, "from_pence_unchecked with negative value");
        Money { pence }
    }

    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn from_decimal(pounds: i64, pence: i64) -> DomainResult<Self> {
        if !(0..100).contains(&pence) {
            return Err(DomainError::Validation(
                "pence component must be between 0 and 99".into(),
            ));
        }
        let total = pounds
            .checked_mul(100)
            .and_then(|t| t.checked_add(pence))
            .ok_or(DomainError::ArithmeticOverflow)?;
        if total < 0 {
            return Err(DomainError::NegativeValue);
        }
        Ok(Money { pence: total })
    }

    #[must_use]
    pub fn pence(&self) -> i64 {
        self.pence
    }

    #[must_use]
    pub fn pounds(&self) -> i64 {
        self.pence / 100
    }

    #[must_use]
    pub fn pence_part(&self) -> i64 {
        self.pence.abs() % 100
    }

    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.pence == 0
    }

    #[must_use]
    pub fn is_positive(&self) -> bool {
        self.pence > 0
    }

    #[must_use]
    pub fn abs(&self) -> Self {
        Money {
            pence: self.pence.checked_abs().unwrap_or(i64::MAX),
        }
    }

    #[must_use]
    pub fn display(&self) -> String {
        self.to_string()
    }

    /// Saturating addition for non-accounting display ergonomics ONLY.
    /// Deprecated for accounting paths: use checked [`Money::add`] /
    /// [`Money::sum`], which return errors on overflow (Phase 3.2).
    #[must_use]
    pub fn saturating_add(&self, other: &Money) -> Self {
        Money {
            pence: self.pence.saturating_add(other.pence),
        }
    }

    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn add(&self, other: &Money) -> DomainResult<Self> {
        let result = self
            .pence
            .checked_add(other.pence)
            .ok_or(DomainError::ArithmeticOverflow)?;
        Ok(Money { pence: result })
    }

    /// Single checked subtraction path (Phase 3.10): `checked_subtract` is
    /// kept as a thin alias so both historic names collapse onto one
    /// implementation.
    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn subtract(&self, other: &Money) -> DomainResult<Self> {
        let result = self
            .pence
            .checked_sub(other.pence)
            .ok_or(DomainError::ArithmeticUnderflow)?;
        if result < 0 {
            return Err(DomainError::NegativeValue);
        }
        Ok(Money { pence: result })
    }

    /// Alias of [`Money::subtract`] (single implementation, Phase 3.10).
    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn checked_subtract(&self, other: &Money) -> DomainResult<Self> {
        self.subtract(other)
    }

    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn multiply(&self, factor: i32) -> DomainResult<Self> {
        if factor < 0 {
            return Err(DomainError::NegativeValue);
        }
        let result = self
            .pence
            .checked_mul(i64::from(factor))
            .ok_or(DomainError::ArithmeticOverflow)?;
        Ok(Money { pence: result })
    }

    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn sum<I>(iter: I) -> DomainResult<Money>
    where
        I: IntoIterator<Item = Money>,
    {
        let mut total = 0i64;
        for m in iter {
            total = total
                .checked_add(m.pence)
                .ok_or(DomainError::ArithmeticOverflow)?;
        }
        Ok(Money { pence: total })
    }
}

impl Add for Money {
    type Output = Self;

    /// Saturating addition for ergonomics in aggregations. Accounting paths
    /// that must detect overflow use [`Money::add`] / [`Money::sum`], which
    /// return errors (review O7).
    fn add(self, rhs: Self) -> Self {
        self.saturating_add(&rhs)
    }
}

impl Add<&Money> for &Money {
    type Output = Money;

    /// See the owned [`Add`] impl: saturating; prefer `add`/`sum` in
    /// accounting paths.
    fn add(self, rhs: &Money) -> Money {
        self.saturating_add(rhs)
    }
}

impl Sub for Money {
    type Output = DomainResult<Self>;

    fn sub(self, rhs: Self) -> DomainResult<Self> {
        if rhs.pence > self.pence {
            Err(DomainError::NegativeValue)
        } else {
            Ok(Money {
                pence: self.pence - rhs.pence,
            })
        }
    }
}

impl Default for Money {
    fn default() -> Self {
        Money::zero()
    }
}

impl std::fmt::Display for Money {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.pence < 0 {
            write!(
                f,
                "-£{}.{:02}",
                self.pence.abs() / 100,
                self.pence.abs() % 100
            )
        } else {
            write!(f, "£{}.{:02}", self.pence / 100, self.pence % 100)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_is_zero_pence() {
        let z = Money::zero();
        assert_eq!(z.pence(), 0);
        assert!(z.is_zero());
        assert!(!z.is_positive());
    }

    #[test]
    fn from_pence_creates_correct_value() {
        let m = Money::from_pence(1050).unwrap();
        assert_eq!(m.pence(), 1050);
        assert_eq!(m.pounds(), 10);
        assert_eq!(m.pence_part(), 50);
    }

    #[test]
    fn from_decimal_creates_correct_value() {
        let m = Money::from_decimal(10, 50).unwrap();
        assert_eq!(m.pence(), 1050);
        assert_eq!(m.pounds(), 10);
        assert_eq!(m.pence_part(), 50);
    }

    #[test]
    fn from_decimal_rejects_overflow() {
        assert!(matches!(
            Money::from_decimal(i64::MAX / 50, 0),
            Err(DomainError::ArithmeticOverflow)
        ));
    }

    #[test]
    fn from_decimal_rejects_pence_over_99() {
        assert!(Money::from_decimal(0, 100).is_err());
        assert!(Money::from_decimal(0, -1).is_err());
    }

    #[test]
    fn from_pence_rejects_negative() {
        assert!(matches!(
            Money::from_pence(-1),
            Err(DomainError::NegativeValue)
        ));
    }

    #[test]
    fn from_decimal_rejects_negative_total() {
        assert!(matches!(
            Money::from_decimal(-1, 0),
            Err(DomainError::NegativeValue)
        ));
    }

    #[test]
    fn addition_works() {
        let a = Money::from_pence(1050).unwrap();
        let b = Money::from_pence(500).unwrap();
        let result = a + b;
        assert_eq!(result.pence(), 1550);
    }

    #[test]
    fn addition_saturates_on_overflow() {
        let a = Money { pence: i64::MAX };
        let b = Money::from_pence(1).unwrap();
        let r = a + b;
        assert_eq!(r.pence(), i64::MAX);
    }

    #[test]
    fn saturating_add_method_saturates_on_overflow() {
        let a = Money { pence: i64::MAX };
        let b = Money::from_pence(1).unwrap();
        let r = Money::saturating_add(&a, &b);
        assert_eq!(r.pence(), i64::MAX);
    }

    #[test]
    fn subtraction_succeeds_when_positive_result() {
        let a = Money::from_pence(1050).unwrap();
        let b = Money::from_pence(500).unwrap();
        let result = (a - b).unwrap();
        assert_eq!(result.pence(), 550);
    }

    #[test]
    fn subtraction_fails_when_negative_result() {
        let a = Money::from_pence(400).unwrap();
        let b = Money::from_pence(500).unwrap();
        assert!(matches!(a - b, Err(DomainError::NegativeValue)));
    }

    #[test]
    fn comparison_works() {
        let a = Money::from_pence(100).unwrap();
        let b = Money::from_pence(200).unwrap();
        let c = Money::from_pence(100).unwrap();
        assert!(a < b);
        assert!(b > a);
        assert_eq!(a, c);
    }

    #[test]
    fn display_format() {
        let m = Money::from_pence(1234).unwrap();
        assert_eq!(m.display(), "£12.34");
        assert_eq!(format!("{m}"), "£12.34");
    }

    #[test]
    fn display_with_zero_pence() {
        let m = Money::from_pence(1200).unwrap();
        assert_eq!(m.display(), "£12.00");
    }

    #[test]
    fn display_zero() {
        assert_eq!(Money::zero().display(), "£0.00");
    }

    #[test]
    fn multiply_by_positive_factor() {
        let m = Money::from_pence(100).unwrap();
        let result = m.multiply(5).unwrap();
        assert_eq!(result.pence(), 500);
    }

    #[test]
    fn multiply_by_zero() {
        let m = Money::from_pence(100).unwrap();
        let result = m.multiply(0).unwrap();
        assert!(result.is_zero());
    }

    #[test]
    fn multiply_rejects_negative_factor() {
        let m = Money::from_pence(100).unwrap();
        assert!(matches!(m.multiply(-1), Err(DomainError::NegativeValue)));
    }

    #[test]
    fn sum_aggregates_correctly() {
        let items = vec![
            Money::from_pence(100).unwrap(),
            Money::from_pence(250).unwrap(),
            Money::from_pence(750).unwrap(),
        ];
        let total = Money::sum(items).unwrap();
        assert_eq!(total.pence(), 1100);
    }

    #[test]
    fn sum_empty_is_zero() {
        let total = Money::sum(Vec::<Money>::new()).unwrap();
        assert!(total.is_zero());
    }

    #[test]
    fn checked_subtract_boundary_zero() {
        let a = Money::from_pence(500).unwrap();
        let b = Money::from_pence(500).unwrap();
        let result = a.checked_subtract(&b).unwrap();
        assert!(result.is_zero());
    }

    #[test]
    fn is_positive_correctly() {
        let zero = Money::zero();
        let pos = Money::from_pence(1).unwrap();
        assert!(!zero.is_positive());
        assert!(pos.is_positive());
    }

    #[test]
    fn abs_works() {
        let m = Money::from_pence(500).unwrap();
        assert_eq!(m.abs().pence(), 500);
    }

    #[test]
    fn serialize_deserialize() {
        let m = Money::from_pence(1250).unwrap();
        let json = serde_json::to_string(&m).unwrap();
        let deserialized: Money = serde_json::from_str(&json).unwrap();
        assert_eq!(m, deserialized);
    }

    #[test]
    fn deserialize_rejects_negative() {
        // Phase 3.2: custom Deserialize routes through `from_pence`, so a
        // negative on the wire fails instead of constructing bad money.
        assert!(serde_json::from_str::<Money>(r#"{"pence":-1}"#).is_err());
        assert!(serde_json::from_str::<Money>("-5").is_err());
    }

    #[test]
    fn checked_add_rejects_overflow() {
        // Phase 3.2: accounting paths use checked `add`/`sum`, never
        // saturating arithmetic. (UFCS: `Add` is in scope via the parent.)
        let a = Money::from_pence(i64::MAX).unwrap();
        let b = Money::from_pence(1).unwrap();
        assert!(matches!(
            Money::add(&a, &b),
            Err(DomainError::ArithmeticOverflow)
        ));
        assert!(matches!(
            Money::sum(vec![a, b]),
            Err(DomainError::ArithmeticOverflow)
        ));
    }

    #[test]
    fn default_is_zero() {
        assert!(Money::default().is_zero());
    }
}

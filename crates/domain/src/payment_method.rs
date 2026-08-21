use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum PaymentMethod {
    #[default]
    Cash,
    Cheque,
    BankTransfer,
    Contactless,
    ChipAndPin,
}

impl std::fmt::Display for PaymentMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PaymentMethod::Cash => write!(f, "Cash"),
            PaymentMethod::Cheque => write!(f, "Cheque"),
            PaymentMethod::BankTransfer => write!(f, "Bank transfer"),
            PaymentMethod::Contactless => write!(f, "Contactless"),
            PaymentMethod::ChipAndPin => write!(f, "Chip & PIN"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_variant_names() {
        assert_eq!(PaymentMethod::Cash.to_string(), "Cash");
        assert_eq!(PaymentMethod::Cheque.to_string(), "Cheque");
        assert_eq!(PaymentMethod::BankTransfer.to_string(), "Bank transfer");
        assert_eq!(PaymentMethod::Contactless.to_string(), "Contactless");
        assert_eq!(PaymentMethod::ChipAndPin.to_string(), "Chip & PIN");
    }

    #[test]
    fn default_is_cash() {
        assert_eq!(PaymentMethod::default(), PaymentMethod::Cash);
    }
}

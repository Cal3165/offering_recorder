use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum DonationType {
    Tithe,
    #[default]
    General,
    Special,
    Offering,
    Other,
}

impl std::fmt::Display for DonationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DonationType::Tithe => write!(f, "Tithe"),
            DonationType::General => write!(f, "General"),
            DonationType::Special => write!(f, "Special"),
            DonationType::Offering => write!(f, "Offering"),
            DonationType::Other => write!(f, "Other"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_variant_names() {
        assert_eq!(DonationType::Tithe.to_string(), "Tithe");
        assert_eq!(DonationType::General.to_string(), "General");
        assert_eq!(DonationType::Special.to_string(), "Special");
        assert_eq!(DonationType::Other.to_string(), "Other");
    }

    #[test]
    fn default_is_general() {
        assert_eq!(DonationType::default(), DonationType::General);
    }

    #[test]
    fn serialize_all_variants() {
        assert_eq!(
            serde_json::to_string(&DonationType::Tithe).unwrap(),
            r#""Tithe""#
        );
        assert_eq!(
            serde_json::to_string(&DonationType::General).unwrap(),
            r#""General""#
        );
        assert_eq!(
            serde_json::to_string(&DonationType::Special).unwrap(),
            r#""Special""#
        );
        assert_eq!(
            serde_json::to_string(&DonationType::Offering).unwrap(),
            r#""Offering""#
        );
        assert_eq!(
            serde_json::to_string(&DonationType::Other).unwrap(),
            r#""Other""#
        );
    }
}

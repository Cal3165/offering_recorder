use crate::money::Money;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Denomination {
    Coin1p,
    Coin2p,
    Coin5p,
    Coin10p,
    Coin20p,
    Coin50p,
    Coin100p,
    Coin200p,
    Note500p,
    Note1000p,
    Note2000p,
    Note5000p,
}

impl Denomination {
    pub fn pence(&self) -> i64 {
        match self {
            Denomination::Coin1p => 1,
            Denomination::Coin2p => 2,
            Denomination::Coin5p => 5,
            Denomination::Coin10p => 10,
            Denomination::Coin20p => 20,
            Denomination::Coin50p => 50,
            Denomination::Coin100p => 100,
            Denomination::Coin200p => 200,
            Denomination::Note500p => 500,
            Denomination::Note1000p => 1000,
            Denomination::Note2000p => 2000,
            Denomination::Note5000p => 5000,
        }
    }

    pub fn display(&self) -> String {
        match self {
            Denomination::Coin1p => "1p".to_string(),
            Denomination::Coin2p => "2p".to_string(),
            Denomination::Coin5p => "5p".to_string(),
            Denomination::Coin10p => "10p".to_string(),
            Denomination::Coin20p => "20p".to_string(),
            Denomination::Coin50p => "50p".to_string(),
            Denomination::Coin100p => "£1".to_string(),
            Denomination::Coin200p => "£2".to_string(),
            Denomination::Note500p => "£5".to_string(),
            Denomination::Note1000p => "£10".to_string(),
            Denomination::Note2000p => "£20".to_string(),
            Denomination::Note5000p => "£50".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CashCount {
    pub denomination: Denomination,
    pub count: u32,
}

impl CashCount {
    pub fn new(denomination: Denomination, count: u32) -> Self {
        CashCount {
            denomination,
            count,
        }
    }

    pub fn total(&self) -> Money {
        let pence = self.denomination.pence() * self.count as i64;
        Money::from_pence_unchecked(pence)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CashTotal {
    counts: BTreeMap<Denomination, u32>,
}

impl CashTotal {
    pub fn new() -> Self {
        CashTotal {
            counts: BTreeMap::new(),
        }
    }

    pub fn add(&mut self, denomination: Denomination, count: u32) {
        if count == 0 {
            return;
        }
        let entry = self.counts.entry(denomination).or_insert(0);
        *entry += count;
        if *entry == 0 {
            self.counts.remove(&denomination);
        }
    }

    pub fn set(&mut self, denomination: Denomination, count: u32) {
        if count == 0 {
            self.counts.remove(&denomination);
        } else {
            self.counts.insert(denomination, count);
        }
    }

    pub fn get(&self, denomination: &Denomination) -> u32 {
        self.counts.get(denomination).copied().unwrap_or(0)
    }

    pub fn total(&self) -> Money {
        let pence: i64 = self
            .counts
            .iter()
            .map(|(denom, count)| denom.pence() * *count as i64)
            .sum();
        Money::from_pence_unchecked(pence)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Denomination, &u32)> {
        self.counts.iter()
    }

    pub fn to_counts(&self) -> Vec<CashCount> {
        self.counts
            .iter()
            .map(|(d, c)| CashCount {
                denomination: *d,
                count: *c,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denomination_values() {
        assert_eq!(Denomination::Coin1p.pence(), 1);
        assert_eq!(Denomination::Coin100p.pence(), 100);
        assert_eq!(Denomination::Note5000p.pence(), 5000);
    }

    #[test]
    fn denomination_display() {
        assert_eq!(Denomination::Coin1p.display(), "1p");
        assert_eq!(Denomination::Note2000p.display(), "£20");
    }

    #[test]
    fn cash_count_total() {
        let cc = CashCount::new(Denomination::Note1000p, 10);
        assert_eq!(cc.total().pence(), 10000);
    }

    #[test]
    fn cash_total_add_and_total() {
        let mut ct = CashTotal::new();
        ct.add(Denomination::Coin100p, 5);
        ct.add(Denomination::Note1000p, 3);
        assert_eq!(ct.total().pence(), 500 + 3000);
    }

    #[test]
    fn cash_total_set_replaces() {
        let mut ct = CashTotal::new();
        ct.set(Denomination::Coin100p, 5);
        ct.set(Denomination::Coin100p, 3);
        assert_eq!(ct.get(&Denomination::Coin100p), 3);
    }

    #[test]
    fn cash_total_set_zero_removes() {
        let mut ct = CashTotal::new();
        ct.set(Denomination::Coin100p, 5);
        ct.set(Denomination::Coin100p, 0);
        assert_eq!(ct.get(&Denomination::Coin100p), 0);
    }

    #[test]
    fn cash_total_empty_is_zero() {
        let ct = CashTotal::new();
        assert!(ct.total().is_zero());
    }

    #[test]
    fn cash_total_to_counts() {
        let mut ct = CashTotal::new();
        ct.add(Denomination::Coin100p, 2);
        let counts = ct.to_counts();
        assert_eq!(counts.len(), 1);
        assert_eq!(counts[0].denomination, Denomination::Coin100p);
        assert_eq!(counts[0].count, 2);
    }
}

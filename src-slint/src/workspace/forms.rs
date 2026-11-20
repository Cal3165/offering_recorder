use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, Utc};
use offering_application::{
    community_buildings::CommunityBuildingInput, donors::CreateDonorInput, error::AppResult,
    gift_aid::NewDeclarationDetails,
};
use offering_domain::{
    address::Address,
    entities::{cash_denomination::CashTotal, donor::Donor, gift_aid::DeclarationType},
    postcode::Postcode,
    tax_year::TaxYear,
};
use serde::de::DeserializeOwned;
use uuid::Uuid;

use crate::{backend::validation, parse_decimal, DENOMINATIONS};

use super::catalog::Operation;

pub type Values = BTreeMap<String, String>;

pub struct Input(pub Values);

impl Input {
    pub fn get(&self, key: &str) -> &str {
        self.0.get(key).map_or("", |value| value.trim())
    }

    pub fn required(&self, key: &str, label: &str) -> AppResult<&str> {
        self.optional(key)
            .ok_or_else(|| validation(format!("Enter {label}.")))
    }

    pub fn optional(&self, key: &str) -> Option<&str> {
        let value = self.get(key);
        (!value.is_empty()).then_some(value)
    }

    pub fn owned(&self, key: &str) -> Option<String> {
        self.optional(key).map(str::to_owned)
    }

    pub fn id(&self, key: &str) -> AppResult<Uuid> {
        self.optional_id(key)?
            .ok_or_else(|| validation(format!("Choose a {key}.")))
    }

    pub fn optional_id(&self, key: &str) -> AppResult<Option<Uuid>> {
        self.optional(key)
            .map(|value| {
                Uuid::parse_str(value).map_err(|_| validation(format!("Choose a valid {key}.")))
            })
            .transpose()
    }

    pub fn optional_count(&self, key: &str, label: &str) -> AppResult<Option<u32>> {
        self.optional(key)
            .map(|value| parse_count(value, label))
            .transpose()
    }

    pub fn count(&self, key: &str, label: &str) -> AppResult<u32> {
        parse_count(self.get(key), label)
    }

    pub fn signed_in_day(&self, key: &str) -> AppResult<DateTime<Utc>> {
        let date = self.date(key)?;
        let local = date.and_hms_opt(12, 0, 0).unwrap();
        let utc = local
            .and_local_timezone(Local)
            .single()
            .ok_or_else(|| validation("The selected local date is invalid."))?
            .with_timezone(&Utc);
        let now = Utc::now();
        if date == Local::now().date_naive() && utc > now {
            Ok(now)
        } else {
            Ok(utc)
        }
    }

    pub fn date(&self, key: &str) -> AppResult<NaiveDate> {
        NaiveDate::parse_from_str(self.get(key), "%Y-%m-%d")
            .map_err(|_| validation(format!("Enter {key} as YYYY-MM-DD.")))
    }

    pub fn range(&self) -> AppResult<(NaiveDate, NaiveDate)> {
        let (start, end) = (self.date("start")?, self.date("end")?);
        if end < start {
            return Err(validation(
                "The end date must be on or after the start date.",
            ));
        }
        Ok((start, end))
    }

    pub fn year(&self) -> AppResult<TaxYear> {
        let start = self
            .get("year")
            .parse()
            .map_err(|_| validation("Enter the four-digit tax year start."))?;
        TaxYear::try_new(start).map_err(Into::into)
    }

    pub fn choice<T: DeserializeOwned>(&self, key: &str) -> AppResult<T> {
        decode_choice(self.get(key), key)
    }

    pub fn multiple<T>(
        &self,
        key: &str,
        parse: impl Fn(&str) -> AppResult<T>,
    ) -> AppResult<Vec<T>> {
        self.get(key)
            .split(',')
            .filter(|part| !part.is_empty())
            .map(parse)
            .collect()
    }

    pub fn yes(&self, key: &str) -> bool {
        self.get(key) == "yes"
    }

    pub fn donor(&self) -> AppResult<CreateDonorInput> {
        Ok(CreateDonorInput {
            first_name: self.get("first").into(),
            last_name: self.get("last").into(),
            preferred_name: self.owned("preferred"),
            donor_type: self.get("type").parse()?,
            address_line1: self.owned("line1"),
            address_line2: self.owned("line2"),
            address_line3: self.owned("line3"),
            postcode: self.owned("postcode"),
            email: self.owned("email"),
            phone: self.owned("phone"),
            lawful_basis: self.optional("basis").map(str::parse).transpose()?,
            privacy_notice_version: self.owned("notice"),
        })
    }

    pub fn declaration(&self) -> AppResult<NewDeclarationDetails> {
        let declaration_type = self.choice("type")?;
        let donor_address = self
            .optional("line1")
            .map(|line1| -> AppResult<Address> {
                let postcode =
                    Postcode::new(self.required("postcode", "the declaration postcode")?)?;
                Ok(Address::new(
                    line1.into(),
                    self.owned("line2"),
                    self.owned("line3"),
                    postcode,
                )?)
            })
            .transpose()?;
        Ok(NewDeclarationDetails {
            declaration_type,
            effective_from: self.signed_in_day("date")?,
            charity_name: self.owned("charity"),
            donor_full_name: self.owned("name"),
            donor_address,
            declaration_method: self.choice("method")?,
            received_at: self
                .optional("received")
                .map(|_| self.signed_in_day("received"))
                .transpose()?,
            declaration_text_version: self.owned("wording"),
            single_donation_id: if declaration_type == DeclarationType::Single {
                Some(self.id("gift")?)
            } else {
                None
            },
        })
    }

    pub fn building(&self) -> AppResult<CommunityBuildingInput> {
        if self.optional("line1").is_some() != self.optional("postcode").is_some() {
            return Err(validation("Enter both an address line and a postcode."));
        }
        Ok(CommunityBuildingInput {
            name: self.get("name").into(),
            address_line1: self.owned("line1"),
            address_line2: self.owned("line2"),
            address_line3: self.owned("line3"),
            postcode: self.owned("postcode"),
            local_authority: self.owned("authority"),
        })
    }

    pub fn pence(&self, key: &str) -> AppResult<i64> {
        Ok(parse_decimal(self.get(key))?.pence())
    }

    pub fn cash(&self) -> AppResult<CashTotal> {
        let mut cash = CashTotal::new();
        for denomination in DENOMINATIONS {
            let key = denomination.pence().to_string();
            let count = self
                .optional_count(&key, &denomination.display())?
                .unwrap_or_default();
            cash.set(denomination, count);
        }
        Ok(cash)
    }
}

fn parse_count(value: &str, label: &str) -> AppResult<u32> {
    value
        .parse()
        .map_err(|_| validation(format!("Enter a whole number for {label}.")))
}

pub fn decode_choice<T: DeserializeOwned>(value: &str, name: &str) -> AppResult<T> {
    serde_json::from_value(serde_json::Value::String(value.into()))
        .map_err(|_| validation(format!("Choose a valid {name}.")))
}

pub fn form_defaults(operation: &Operation) -> Values {
    let today = Local::now().date_naive();
    operation
        .fields
        .iter()
        .map(|field| {
            let initial = match field.initial {
                "$today" => today.to_string(),
                "$month" => NaiveDate::from_ymd_opt(today.year(), today.month(), 1)
                    .unwrap()
                    .to_string(),
                "$year" => TaxYear::from_date(today).start_year().to_string(),
                literal => literal.to_owned(),
            };
            (field.key.to_string(), initial)
        })
        .collect()
}

pub fn apply_date_preset(preset: &str, values: &mut Values) {
    let today = Local::now().date_naive();
    let (start, end, year) = match preset {
        "today" => (today, today, None),
        "week" => {
            let offset = i64::from(today.weekday().num_days_from_monday());
            let monday = today - Duration::days(offset);
            let sunday = monday + Duration::days(6);
            (monday, sunday, None)
        }
        "month" => {
            let first = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap();
            let next_month = if today.month() == 12 {
                NaiveDate::from_ymd_opt(today.year() + 1, 1, 1).unwrap()
            } else {
                NaiveDate::from_ymd_opt(today.year(), today.month() + 1, 1).unwrap()
            };
            (first, next_month - Duration::days(1), None)
        }
        "taxyear" => {
            let y = TaxYear::from_date(today).start_year();
            let start = NaiveDate::from_ymd_opt(y, 4, 6).unwrap();
            let end = NaiveDate::from_ymd_opt(y + 1, 4, 5).unwrap();
            (start, end, Some(y))
        }
        "prev_taxyear" => {
            let y = TaxYear::from_date(today).start_year() - 1;
            let start = NaiveDate::from_ymd_opt(y, 4, 6).unwrap();
            let end = NaiveDate::from_ymd_opt(y + 1, 4, 5).unwrap();
            (start, end, Some(y))
        }
        _ => return,
    };
    values.insert("start".into(), start.to_string());
    values.insert("end".into(), end.to_string());
    if let Some(y) = year {
        if values.contains_key("year") {
            values.insert("year".into(), y.to_string());
        }
    }
}

pub fn donor_profile(values: &mut Values, donor: &Donor) {
    for (key, value) in [
        ("first", donor.first_name.clone()),
        ("last", donor.last_name.clone()),
        (
            "preferred",
            donor.preferred_name.clone().unwrap_or_default(),
        ),
        ("type", donor.donor_type.to_string()),
        ("email", donor.email.clone().unwrap_or_default()),
        ("phone", donor.phone.clone().unwrap_or_default()),
        (
            "basis",
            donor
                .lawful_basis
                .map(|basis| basis.to_string())
                .unwrap_or_default(),
        ),
        (
            "notice",
            donor.privacy_notice_version.clone().unwrap_or_default(),
        ),
    ] {
        values.insert(key.to_owned(), value);
    }
    address_profile(values, donor.address.as_ref());
}

pub fn address_profile(values: &mut Values, address: Option<&Address>) {
    for (key, value) in [
        ("line1", address.map(|addr| addr.line1.clone())),
        ("line2", address.and_then(|addr| addr.line2.clone())),
        ("line3", address.and_then(|addr| addr.line3.clone())),
        ("postcode", address.map(|addr| addr.postcode.to_string())),
    ] {
        values.insert(key.to_owned(), value.unwrap_or_default());
    }
}

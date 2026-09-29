use offering_domain::entities::donor::LawfulBasis;
use offering_domain::entities::user::User;

use crate::donors::{CreateDonorInput, DonorService};
use crate::error::{unauthorized, AppError, AppResult};
use crate::permissions::can_manage_migration;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum CsvRowStatus {
    Created,
    Duplicate,
    Error,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CsvImportRow {
    pub row: usize,
    pub name: String,
    pub status: CsvRowStatus,
    pub message: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct CsvImportSummary {
    pub created: usize,
    pub duplicates: usize,
    pub errors: usize,
    pub rows: Vec<CsvImportRow>,
    /// Transparency note (plan Phases 5.2/6.3): the operator chooses ONE
    /// lawful basis for the whole batch (or explicitly "Not recorded"); the
    /// note records that choice and flags per-donor review. The UI surfaces
    /// this note.
    pub basis_note: String,
}

#[derive(Clone)]
pub struct MigrationService<'a> {
    donors: DonorService<'a>,
}

impl<'a> MigrationService<'a> {
    #[must_use]
    pub fn new(donors: DonorService<'a>) -> Self {
        MigrationService { donors }
    }

    /// Import donors from CSV text. The first row must be a header containing
    /// first-name and last-name columns (aliases such as "Forename"/"Surname"
    /// are recognised). Rows whose first/last name matches an existing donor
    /// are skipped as duplicates; invalid rows are reported per-row.
    ///
    /// `lawful_basis` is the operator's per-batch basis choice (plan Phase
    /// 6.3): one basis for the whole batch, passed explicitly — there is no
    /// silent default. `None` means "Not recorded": the rows import with no
    /// basis and MUST be completed per donor (the summary note says so).
    /// # Errors
    ///
    /// Returns [`AppError`] for unauthorized actors, missing records, conflicting state, or repository failures.
    pub async fn import_donors_csv(
        &self,
        actor: &User,
        csv: &str,
        lawful_basis: Option<LawfulBasis>,
    ) -> AppResult<CsvImportSummary> {
        if !can_manage_migration(actor) {
            return Err(unauthorized("manage migration"));
        }
        // Excel's "CSV UTF-8" export prepends a byte-order mark; without
        // stripping it the first header cell never matches its aliases and
        // the import fails with a misleading header error (review O11).
        let csv = csv.strip_prefix('\u{feff}').unwrap_or(csv);
        let rows = parse_csv(csv);
        if rows.is_empty() {
            return Err(AppError::Validation("CSV is empty".into()));
        }
        let map = detect_header(&rows[0])?;
        let mut summary = CsvImportSummary::default();
        for (i, raw) in rows.iter().enumerate().skip(1) {
            if raw.iter().all(|f| f.trim().is_empty()) {
                continue;
            }
            let row_no = i + 1;
            match build_input(raw, &map, lawful_basis) {
                Ok(input) => {
                    let name = format!("{} {}", input.first_name, input.last_name);
                    match self.donors.create_donor_migrated(actor, input, false).await {
                        Ok(d) => {
                            summary.created += 1;
                            summary.rows.push(CsvImportRow {
                                row: row_no,
                                name: d.display_name(),
                                status: CsvRowStatus::Created,
                                message: None,
                            });
                        }
                        Err(AppError::Conflict(msg)) => {
                            summary.duplicates += 1;
                            summary.rows.push(CsvImportRow {
                                row: row_no,
                                name: name.clone(),
                                status: CsvRowStatus::Duplicate,
                                message: Some(msg),
                            });
                        }
                        Err(e) => {
                            summary.errors += 1;
                            summary.rows.push(CsvImportRow {
                                row: row_no,
                                name,
                                status: CsvRowStatus::Error,
                                message: Some(e.to_string()),
                            });
                        }
                    }
                }
                Err(msg) => {
                    summary.errors += 1;
                    summary.rows.push(CsvImportRow {
                        row: row_no,
                        name: String::new(),
                        status: CsvRowStatus::Error,
                        message: Some(msg),
                    });
                }
            }
        }
        if summary.created > 0 {
            summary.basis_note = match lawful_basis {
                Some(basis) => format!(
                    "{} imported donor(s) had their lawful basis recorded as {basis} per the batch choice — review each donor's basis and privacy notice.",
                    summary.created
                ),
                None => format!(
                    "{} imported donor(s) have NO lawful basis recorded — complete each donor's basis and privacy notice before using their data.",
                    summary.created
                ),
            };
        }
        Ok(summary)
    }
}

// --- CSV parsing (dependency-free) ---

fn parse_csv(input: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                if in_quotes {
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        field.push('"');
                    } else {
                        in_quotes = false;
                    }
                } else {
                    in_quotes = true;
                }
            }
            ',' => {
                if in_quotes {
                    field.push(',');
                } else {
                    row.push(std::mem::take(&mut field));
                }
            }
            '\n' => {
                if in_quotes {
                    field.push('\n');
                } else {
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                }
            }
            '\r' if !in_quotes => {}
            other => field.push(other),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(std::mem::take(&mut field));
        rows.push(row);
    }
    rows
}

// --- column mapping ---

const FIRST_NAME: &[&str] = &[
    "first_name",
    "firstname",
    "first name",
    "forename",
    "given name",
    "given",
];
const LAST_NAME: &[&str] = &[
    "last_name",
    "lastname",
    "last name",
    "surname",
    "family name",
    "family",
];
const PREFERRED_NAME: &[&str] = &["preferred_name", "preferred name", "preferred", "known as"];
const ADDRESS_LINE1: &[&str] = &[
    "address_line1",
    "address line 1",
    "address1",
    "address",
    "line1",
    "line 1",
    "addr1",
];
const ADDRESS_LINE2: &[&str] = &[
    "address_line2",
    "address line 2",
    "address2",
    "line2",
    "line 2",
    "addr2",
];
const ADDRESS_LINE3: &[&str] = &[
    "address_line3",
    "address line 3",
    "address3",
    "line3",
    "line 3",
    "addr3",
];
const POSTCODE: &[&str] = &["postcode", "post code", "postal code", "zip", "zip code"];
const EMAIL: &[&str] = &["email", "email address", "e-mail", "e mail"];
const PHONE: &[&str] = &["phone", "telephone", "tel", "phone number", "mobile"];

#[derive(Debug, Clone, Copy, Default)]
struct ColumnMap {
    first: Option<usize>,
    last: Option<usize>,
    preferred: Option<usize>,
    address_line1: Option<usize>,
    address_line2: Option<usize>,
    address_line3: Option<usize>,
    postcode: Option<usize>,
    email: Option<usize>,
    phone: Option<usize>,
}

fn normalize_header(h: &str) -> String {
    // Excel's "CSV UTF-8" export prepends U+FEFF, which lands in the first
    // header cell and is not char::is_whitespace — strip it explicitly so the
    // first column still matches its aliases (review O11).
    let h = h.trim_start_matches('\u{feff}');
    h.trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn field_index(norm: &str, aliases: &[&str]) -> bool {
    aliases.contains(&norm)
}

fn detect_header(first_row: &[String]) -> AppResult<ColumnMap> {
    let mut map = ColumnMap::default();
    let mut found = 0usize;
    for (i, h) in first_row.iter().enumerate() {
        let norm = normalize_header(h);
        if norm.is_empty() {
            continue;
        }
        if field_index(&norm, FIRST_NAME) {
            map.first = Some(i);
            found += 1;
        } else if field_index(&norm, LAST_NAME) {
            map.last = Some(i);
            found += 1;
        } else if field_index(&norm, PREFERRED_NAME) {
            map.preferred = Some(i);
        } else if field_index(&norm, ADDRESS_LINE1) {
            map.address_line1 = Some(i);
        } else if field_index(&norm, ADDRESS_LINE2) {
            map.address_line2 = Some(i);
        } else if field_index(&norm, ADDRESS_LINE3) {
            map.address_line3 = Some(i);
        } else if field_index(&norm, POSTCODE) {
            map.postcode = Some(i);
        } else if field_index(&norm, EMAIL) {
            map.email = Some(i);
        } else if field_index(&norm, PHONE) {
            map.phone = Some(i);
        }
    }
    if found == 0 {
        return Err(AppError::Validation(
            "Could not detect a CSV header row. Expected a first column named e.g. first_name, first name, or forename, and a surname/last_name column."
                .into(),
        ));
    }
    if map.first.is_none() || map.last.is_none() {
        return Err(AppError::Validation(
            "CSV header must include both a first-name and a last-name column".into(),
        ));
    }
    Ok(map)
}

fn cell(raw: &[String], idx: Option<usize>) -> &str {
    idx.and_then(|i| raw.get(i))
        .map_or("", std::string::String::as_str)
}

fn opt_cell(raw: &[String], idx: Option<usize>) -> Option<String> {
    let v = cell(raw, idx).trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

fn build_input(
    raw: &[String],
    map: &ColumnMap,
    lawful_basis: Option<LawfulBasis>,
) -> Result<CreateDonorInput, String> {
    let first = cell(raw, map.first).trim().to_string();
    let last = cell(raw, map.last).trim().to_string();
    if first.is_empty() || last.is_empty() {
        return Err("first and last name required".into());
    }
    Ok(CreateDonorInput {
        first_name: first,
        last_name: last,
        preferred_name: opt_cell(raw, map.preferred),
        donor_type: offering_domain::entities::donor::DonorType::Unknown,
        address_line1: opt_cell(raw, map.address_line1),
        address_line2: opt_cell(raw, map.address_line2),
        address_line3: opt_cell(raw, map.address_line3),
        postcode: opt_cell(raw, map.postcode),
        email: opt_cell(raw, map.email),
        phone: opt_cell(raw, map.phone),
        // Plan Phase 6.3: the batch basis chosen by the operator (or
        // "Not recorded" when None) — never a silent Legitimate interest.
        lawful_basis,
        privacy_notice_version: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_csv() {
        let rows = parse_csv("a,b\nc,d\n");
        assert_eq!(rows, vec![vec!["a", "b"], vec!["c", "d"]]);
    }

    #[test]
    fn parse_quoted_commas_and_newlines() {
        let rows = parse_csv("first,last\n\"Smith, John\",X\n");
        assert_eq!(rows[1][0], "Smith, John");
        assert_eq!(rows[1][1], "X");
    }

    #[test]
    fn parse_escaped_quotes() {
        let rows = parse_csv("a\n\"say \"\"hi\"\"\"\n");
        assert_eq!(rows[1][0], "say \"hi\"");
    }

    #[test]
    fn parse_crlf() {
        let rows = parse_csv("a,b\r\nc,d\r\n");
        assert_eq!(rows, vec![vec!["a", "b"], vec!["c", "d"]]);
    }

    #[test]
    fn parse_no_trailing_newline() {
        let rows = parse_csv("a,b\nc,d");
        assert_eq!(rows, vec![vec!["a", "b"], vec!["c", "d"]]);
    }

    #[test]
    fn detect_header_aliases() {
        let header = parse_csv("Forename,Surname,Email,Postcode\n")[0].clone();
        let map = detect_header(&header).unwrap();
        assert_eq!(map.first, Some(0));
        assert_eq!(map.last, Some(1));
        assert_eq!(map.email, Some(2));
        assert_eq!(map.postcode, Some(3));
    }

    #[test]
    fn header_with_bom_still_detected() {
        // Excel UTF-8 CSVs start with U+FEFF; the service strips it before
        // parsing (review O11).
        let map =
            detect_header(&["\u{feff}First Name".to_string(), "Last Name".to_string()]).unwrap();
        assert_eq!(map.first, Some(0));
        assert_eq!(map.last, Some(1));
    }

    #[test]
    fn detect_header_missing_name_columns() {
        let header = parse_csv("email,phone\n")[0].clone();
        assert!(detect_header(&header).is_err());
    }

    #[test]
    fn build_input_requires_names() {
        let map = ColumnMap {
            first: Some(0),
            last: Some(1),
            preferred: Some(2),
            ..Default::default()
        };
        assert!(build_input(&["a".into(), String::new()], &map, None).is_err());
        let input = build_input(&["A".into(), "B".into(), "Pref".into()], &map, None).unwrap();
        assert_eq!(input.first_name, "A");
        assert_eq!(input.preferred_name.as_deref(), Some("Pref"));
        // Plan Phase 6.3: an unchosen basis stays "Not recorded", never a
        // silent default.
        assert_eq!(input.lawful_basis, None);
    }

    #[test]
    fn build_input_optional_fields() {
        let map = ColumnMap {
            first: Some(0),
            last: Some(1),
            email: Some(2),
            ..Default::default()
        };
        let input = build_input(&["A".into(), "B".into(), String::new()], &map, None).unwrap();
        assert_eq!(input.email, None);
    }

    #[test]
    fn build_input_carries_batch_basis_choice() {
        // Plan Phase 6.3: the operator's per-batch choice lands on every row.
        let map = ColumnMap {
            first: Some(0),
            last: Some(1),
            ..Default::default()
        };
        let input =
            build_input(&["A".into(), "B".into()], &map, Some(LawfulBasis::Consent)).unwrap();
        assert_eq!(input.lawful_basis, Some(LawfulBasis::Consent));
    }
}

use crate::retention::RETENTION_YEARS;
use chrono::{DateTime, Months, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

fn retention_until() -> DateTime<Utc> {
    Utc::now()
        .checked_add_months(Months::new((RETENTION_YEARS as u32) * 12))
        .expect("retention deadline in range")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackupStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Restored,
}

impl std::fmt::Display for BackupStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackupStatus::Pending => write!(f, "Pending"),
            BackupStatus::InProgress => write!(f, "In Progress"),
            BackupStatus::Completed => write!(f, "Completed"),
            BackupStatus::Failed => write!(f, "Failed"),
            BackupStatus::Restored => write!(f, "Restored"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Backup {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub backup_path: String,
    pub status: BackupStatus,
    pub size_bytes: Option<u64>,
    pub is_automatic: bool,
    pub encryption_enabled: bool,
    pub retained_until: Option<DateTime<Utc>>,
}

impl Backup {
    pub fn create_manual(path: String, encryption_enabled: bool) -> Self {
        let now = Utc::now();
        Backup {
            id: Uuid::new_v4(),
            created_at: now,
            backup_path: path,
            status: BackupStatus::Completed,
            size_bytes: None,
            is_automatic: false,
            encryption_enabled,
            retained_until: Some(retention_until()),
        }
    }

    pub fn create_automatic(path: String) -> Self {
        let now = Utc::now();
        Backup {
            id: Uuid::new_v4(),
            created_at: now,
            backup_path: path,
            status: BackupStatus::Pending,
            size_bytes: None,
            is_automatic: true,
            encryption_enabled: true,
            retained_until: Some(retention_until()),
        }
    }

    pub fn mark_in_progress(&mut self) {
        self.status = BackupStatus::InProgress;
    }

    pub fn mark_completed(&mut self, size: u64) {
        self.size_bytes = Some(size);
        self.status = BackupStatus::Completed;
    }

    pub fn mark_failed(&mut self) {
        self.status = BackupStatus::Failed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    #[test]
    fn create_manual_backup() {
        let b = Backup::create_manual("/backups/manual.zip".to_string(), true);
        assert!(!b.is_automatic);
        assert!(b.encryption_enabled);
        assert_eq!(b.status, BackupStatus::Completed);
    }

    #[test]
    fn create_automatic_backup() {
        let b = Backup::create_automatic("/backups/auto.zip".to_string());
        assert!(b.is_automatic);
        assert_eq!(b.status, BackupStatus::Pending);
    }

    #[test]
    fn status_transitions() {
        let mut b = Backup::create_automatic("/backups/auto.zip".to_string());
        b.mark_in_progress();
        assert_eq!(b.status, BackupStatus::InProgress);
        b.mark_completed(5000);
        assert_eq!(b.status, BackupStatus::Completed);
        assert_eq!(b.size_bytes, Some(5000));
    }

    #[test]
    fn retained_until_is_seven_calendar_years() {
        let b = Backup::create_manual("/backups/x.zip".to_string(), true);
        let created = b.created_at.date_naive();
        let retained = b.retained_until.unwrap().date_naive();
        assert_eq!(retained, created + chrono::Months::new(84));
        assert_eq!(retained.day(), created.day());
    }
}

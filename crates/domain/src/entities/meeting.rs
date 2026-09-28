use crate::errors::{DomainError, DomainResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MeetingEventType {
    #[default]
    RegularService,
    SpecialService,
    CommunityEvent,
}

impl std::fmt::Display for MeetingEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MeetingEventType::RegularService => write!(f, "Regular Service"),
            MeetingEventType::SpecialService => write!(f, "Special Service"),
            MeetingEventType::CommunityEvent => write!(f, "Community Event"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meeting {
    pub id: Uuid,
    pub name: String,
    pub day_of_week: u8,
    pub time_of_day: String,
    pub is_active: bool,
    pub community_building_id: Option<Uuid>,
    pub event_type: MeetingEventType,
    pub estimated_attendance: Option<u32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Meeting {
    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn create(name: &str, day_of_week: u8, time_of_day: &str) -> DomainResult<Self> {
        if name.trim().is_empty() {
            return Err(DomainError::Validation(
                "meeting name cannot be empty".into(),
            ));
        }
        if day_of_week > 6 {
            return Err(DomainError::Validation("day of week must be 0-6".into()));
        }
        if time_of_day.trim().is_empty() {
            return Err(DomainError::Validation(
                "time of day cannot be empty".into(),
            ));
        }
        let now = Utc::now();
        Ok(Meeting {
            id: Uuid::new_v4(),
            name: name.to_string(),
            day_of_week,
            time_of_day: time_of_day.to_string(),
            is_active: true,
            community_building_id: None,
            event_type: MeetingEventType::RegularService,
            estimated_attendance: None,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn set_community_building(&mut self, building_id: Option<Uuid>) {
        self.community_building_id = building_id;
        self.updated_at = Utc::now();
    }

    /// # Errors
    ///
    /// Returns [`DomainError`] if the input violates a domain invariant.
    pub fn update_schedule(
        &mut self,
        name: &str,
        day_of_week: u8,
        time_of_day: &str,
    ) -> DomainResult<()> {
        if name.trim().is_empty() {
            return Err(DomainError::Validation(
                "meeting name cannot be empty".into(),
            ));
        }
        if day_of_week > 6 {
            return Err(DomainError::Validation("day of week must be 0-6".into()));
        }
        if time_of_day.trim().is_empty() {
            return Err(DomainError::Validation(
                "time of day cannot be empty".into(),
            ));
        }
        self.name = name.to_string();
        self.day_of_week = day_of_week;
        self.time_of_day = time_of_day.to_string();
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn set_event_type(&mut self, event_type: MeetingEventType) {
        self.event_type = event_type;
        self.updated_at = Utc::now();
    }

    pub fn set_estimated_attendance(&mut self, attendance: Option<u32>) {
        self.estimated_attendance = attendance;
        self.updated_at = Utc::now();
    }

    pub fn deactivate(&mut self) {
        self.is_active = false;
        self.updated_at = Utc::now();
    }

    pub fn activate(&mut self) {
        self.is_active = true;
        self.updated_at = Utc::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_valid_meeting() {
        let m = Meeting::create("Sunday Service", 0, "10:00").unwrap();
        assert_eq!(m.name, "Sunday Service");
        assert_eq!(m.day_of_week, 0);
        assert!(m.is_active);
    }

    #[test]
    fn reject_empty_name() {
        assert!(matches!(
            Meeting::create("", 0, "10:00"),
            Err(DomainError::Validation(_))
        ));
    }

    #[test]
    fn reject_invalid_day() {
        assert!(matches!(
            Meeting::create("Test", 7, "10:00"),
            Err(DomainError::Validation(_))
        ));
    }

    #[test]
    fn reject_empty_time() {
        assert!(matches!(
            Meeting::create("Test", 0, "  "),
            Err(DomainError::Validation(_))
        ));
    }

    #[test]
    fn activate_deactivate() {
        let mut m = Meeting::create("Test", 0, "10:00").unwrap();
        m.deactivate();
        assert!(!m.is_active);
        m.activate();
        assert!(m.is_active);
    }

    #[test]
    fn new_fields_default() {
        let m = Meeting::create("Test", 0, "10:00").unwrap();
        assert!(m.community_building_id.is_none());
        assert_eq!(m.event_type, MeetingEventType::RegularService);
        assert!(m.estimated_attendance.is_none());
    }

    #[test]
    fn setters_update_fields() {
        let mut m = Meeting::create("Test", 0, "10:00").unwrap();
        let building = Uuid::new_v4();
        m.set_community_building(Some(building));
        m.set_event_type(MeetingEventType::CommunityEvent);
        m.set_estimated_attendance(Some(120));
        assert_eq!(m.community_building_id, Some(building));
        assert_eq!(m.event_type, MeetingEventType::CommunityEvent);
        assert_eq!(m.estimated_attendance, Some(120));
    }

    #[test]
    fn event_type_display() {
        assert_eq!(
            MeetingEventType::RegularService.to_string(),
            "Regular Service"
        );
        assert_eq!(
            MeetingEventType::CommunityEvent.to_string(),
            "Community Event"
        );
    }
}

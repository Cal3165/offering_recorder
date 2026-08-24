use offering_domain::entities::audit_event::AuditAction;
use offering_domain::entities::meeting::{Meeting, MeetingEventType};
use offering_domain::entities::user::User;
use offering_persistence::{AuditRepository, MeetingRepository};
use uuid::Uuid;

use crate::audit::AuditLog;
use crate::error::{unauthorized, AppResult};
use crate::permissions::can_record_offering;

#[derive(Clone)]
pub struct MeetingService<'a> {
    meetings: &'a dyn MeetingRepository,
    audit: AuditLog<'a>,
}

impl<'a> MeetingService<'a> {
    pub fn new(meetings: &'a dyn MeetingRepository, audit_repo: &'a dyn AuditRepository) -> Self {
        MeetingService {
            meetings,
            audit: AuditLog::new(audit_repo),
        }
    }

    pub async fn create_meeting(
        &self,
        actor: &User,
        name: &str,
        day_of_week: u8,
        time_of_day: &str,
        estimated_attendance: Option<u32>,
    ) -> AppResult<Meeting> {
        if !can_record_offering(actor) {
            return Err(unauthorized("record offering"));
        }
        let mut meeting = Meeting::create(name, day_of_week, time_of_day)?;
        meeting.set_estimated_attendance(estimated_attendance);
        let meeting = self.meetings.create(meeting).await?;
        self.audit
            .record(
                actor,
                AuditAction::MeetingCreated,
                "Meeting",
                Some(meeting.id),
                None,
                crate::audit::state_json(&meeting),
                Some("meeting created".into()),
            )
            .await?;
        Ok(meeting)
    }

    pub async fn list_meetings(&self) -> AppResult<Vec<Meeting>> {
        Ok(self.meetings.list().await?)
    }

    pub async fn list_active_meetings(&self) -> AppResult<Vec<Meeting>> {
        Ok(self.meetings.list_active().await?)
    }

    pub async fn update_meeting(
        &self,
        actor: &User,
        meeting_id: Uuid,
        name: &str,
        day_of_week: u8,
        time_of_day: &str,
    ) -> AppResult<Meeting> {
        if !can_record_offering(actor) {
            return Err(unauthorized("record offering"));
        }
        let mut meeting = self
            .meetings
            .get(meeting_id)
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound("meeting not found".into()))?;
        let old = crate::audit::state_json(&meeting);
        meeting.update_schedule(name, day_of_week, time_of_day)?;
        let updated = self.meetings.update(meeting).await?;
        self.audit
            .record(
                actor,
                AuditAction::MeetingChanged,
                "Meeting",
                Some(updated.id),
                old,
                crate::audit::state_json(&updated),
                Some("meeting updated".into()),
            )
            .await?;
        Ok(updated)
    }

    pub async fn set_active(
        &self,
        actor: &User,
        meeting_id: Uuid,
        active: bool,
    ) -> AppResult<Meeting> {
        if !can_record_offering(actor) {
            return Err(unauthorized("record offering"));
        }
        let mut meeting = self
            .meetings
            .get(meeting_id)
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound("meeting not found".into()))?;
        let old = crate::audit::state_json(&meeting);
        if active {
            meeting.activate();
        } else {
            meeting.deactivate();
        }
        let updated = self.meetings.update(meeting).await?;
        self.audit
            .record(
                actor,
                AuditAction::MeetingChanged,
                "Meeting",
                Some(updated.id),
                old,
                crate::audit::state_json(&updated),
                Some(format!("set active = {active}")),
            )
            .await?;
        Ok(updated)
    }

    /// Set the GASDS-relevant event details on a meeting: the community
    /// building it is associated with, the event type, and the estimated
    /// attendance.
    pub async fn set_event_details(
        &self,
        actor: &User,
        meeting_id: Uuid,
        community_building_id: Option<Uuid>,
        event_type: MeetingEventType,
        estimated_attendance: Option<u32>,
    ) -> AppResult<Meeting> {
        if !can_record_offering(actor) {
            return Err(unauthorized("record offering"));
        }
        let mut meeting = self
            .meetings
            .get(meeting_id)
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound("meeting not found".into()))?;
        let old = crate::audit::state_json(&meeting);
        meeting.set_community_building(community_building_id);
        meeting.set_event_type(event_type);
        meeting.set_estimated_attendance(estimated_attendance);
        let updated = self.meetings.update(meeting).await?;
        self.audit
            .record(
                actor,
                AuditAction::MeetingChanged,
                "Meeting",
                Some(updated.id),
                old,
                crate::audit::state_json(&updated),
                Some("meeting event details updated".into()),
            )
            .await?;
        Ok(updated)
    }
}
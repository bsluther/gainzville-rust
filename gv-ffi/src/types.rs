use gv_core::{
    models::activity::{Activity, ActivityName},
    queries::{AllActivities, AnyQuery, AnyQueryResponse},
};
use uuid::Uuid;

// --- Errors ---

#[derive(uniffi::Error, thiserror::Error, Debug)]
pub enum FfiError {
    #[error("{0}")]
    Generic(String),
}

impl From<gv_core::error::DomainError> for FfiError {
    fn from(e: gv_core::error::DomainError) -> Self {
        FfiError::Generic(e.to_string())
    }
}

// --- Activity ---

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiActivity {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    pub description: Option<String>,
    pub source_activity_id: Option<String>,
}

impl From<Activity> for FfiActivity {
    fn from(a: Activity) -> Self {
        FfiActivity {
            id: a.id.to_string(),
            owner_id: a.owner_id.to_string(),
            name: a.name.to_string(),
            description: a.description,
            source_activity_id: a.source_activity_id.map(|id| id.to_string()),
        }
    }
}

// --- Queries ---

#[derive(uniffi::Enum, Clone)]
pub enum FfiAnyQuery {
    AllActivities,
}

#[derive(uniffi::Enum)]
pub enum FfiAnyQueryResponse {
    AllActivities(Vec<FfiActivity>),
}

impl From<FfiAnyQuery> for AnyQuery {
    fn from(q: FfiAnyQuery) -> Self {
        match q {
            FfiAnyQuery::AllActivities => AnyQuery::AllActivities(AllActivities {}),
        }
    }
}

impl From<AnyQueryResponse> for FfiAnyQueryResponse {
    fn from(r: AnyQueryResponse) -> Self {
        match r {
            AnyQueryResponse::AllActivities(v) => {
                FfiAnyQueryResponse::AllActivities(v.into_iter().map(FfiActivity::from).collect())
            }
        }
    }
}

// --- Actions ---

/// Minimal action surface for the PoC — only CreateActivity.
/// Additional variants will be added as the experiment progresses.
#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiCreateActivity {
    /// UUID string for the new activity.
    pub id: String,
    /// Display name (1–49 chars).
    pub name: String,
    /// Optional free-text description.
    pub description: Option<String>,
}

#[derive(uniffi::Enum, Debug, Clone)]
pub enum FfiAction {
    CreateActivity(FfiCreateActivity),
}

// --- Helpers ---

pub(crate) fn parse_uuid(s: &str) -> Result<Uuid, FfiError> {
    Uuid::parse_str(s).map_err(|e| FfiError::Generic(format!("invalid UUID '{}': {}", s, e)))
}

pub(crate) fn parse_activity_name(s: &str) -> Result<ActivityName, FfiError> {
    ActivityName::parse(s.to_string())
        .map_err(|e| FfiError::Generic(format!("invalid activity name '{}': {}", s, e)))
}

pub(crate) fn ffi_action_to_core(
    action: FfiAction,
    actor_id: Uuid,
) -> Result<gv_core::actions::Action, FfiError> {
    match action {
        FfiAction::CreateActivity(a) => {
            let id = parse_uuid(&a.id)?;
            let name = parse_activity_name(&a.name)?;
            let activity = gv_core::models::activity::Activity {
                id,
                owner_id: actor_id,
                name,
                description: a.description,
                source_activity_id: None,
            };
            Ok(gv_core::actions::CreateActivity { actor_id, activity }.into())
        }
    }
}

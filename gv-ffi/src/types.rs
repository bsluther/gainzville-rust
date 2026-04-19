use chrono::{DateTime, Utc};
use gv_core::{
    models::{
        activity::{Activity, ActivityName},
        attribute::{
            Attribute, AttributeConfig, AttributeValue, MassConfig, MassMeasurement, MassUnit,
            MassValue, NumericConfig, NumericValue, SelectConfig, SelectValue, Value,
        },
        attribute_pair::{
            AttributePair, MassAttributePair, NumericAttributePair, SelectAttributePair,
        },
        entry::{Entry, Position, Temporal},
        entry_join::EntryJoin,
        user::User,
    },
    queries::{
        AllActorIds, AllActivities, AllAttributes, AllEntries, AnyQuery, AnyQueryResponse,
        EntriesRootedInTimeInterval, FindActivityById, FindAncestors, FindAttributeById,
        FindAttributePairsForEntry, FindAttributesByOwner, FindDescendants, FindEntryById,
        FindEntryJoinById, FindUserById, FindUserByUsername, FindValueByKey, FindValuesForEntries,
        FindValuesForEntry, IsEmailRegistered,
    },
    validation::{Email, Username},
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

// --- Helpers ---

pub(crate) fn parse_uuid(s: &str) -> Result<Uuid, FfiError> {
    Uuid::parse_str(s).map_err(|e| FfiError::Generic(format!("invalid UUID '{}': {}", s, e)))
}

pub(crate) fn parse_activity_name(s: &str) -> Result<ActivityName, FfiError> {
    ActivityName::parse(s.to_string())
        .map_err(|e| FfiError::Generic(format!("invalid activity name '{}': {}", s, e)))
}

pub(crate) fn parse_timestamp_ms(ms: i64) -> Result<DateTime<Utc>, FfiError> {
    DateTime::<Utc>::from_timestamp_millis(ms)
        .ok_or_else(|| FfiError::Generic(format!("invalid timestamp milliseconds: {ms}")))
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
        FfiAction::MoveEntry(a) => {
            let entry_id = parse_uuid(&a.entry_id)?;
            let position = a.position.map(ffi_position_to_core).transpose()?;
            let temporal = ffi_temporal_to_core(a.temporal)?;
            Ok(gv_core::actions::MoveEntry { actor_id, entry_id, position, temporal }.into())
        }
    }
}

// --- User ---

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiUser {
    pub actor_id: String,
    pub username: String,
    pub email: String,
}

impl From<User> for FfiUser {
    fn from(u: User) -> Self {
        FfiUser {
            actor_id: u.actor_id.to_string(),
            username: u.username.as_str().to_string(),
            email: u.email.as_str().to_string(),
        }
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

// --- Entry ---

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiPosition {
    pub parent_id: String,
    pub frac_index: String,
}

impl From<Position> for FfiPosition {
    fn from(p: Position) -> Self {
        FfiPosition {
            parent_id: p.parent_id.to_string(),
            frac_index: p.frac_index.to_string(),
        }
    }
}

/// Timestamps are Unix milliseconds (UTC).
#[derive(uniffi::Enum, Debug, Clone)]
pub enum FfiTemporal {
    None,
    Start { start: i64 },
    End { end: i64 },
    Duration { duration: u32 },
    StartAndEnd { start: i64, end: i64 },
    StartAndDuration { start: i64, duration_ms: u32 },
    DurationAndEnd { duration_ms: u32, end: i64 },
}

impl From<Temporal> for FfiTemporal {
    fn from(t: Temporal) -> Self {
        match t {
            Temporal::None => FfiTemporal::None,
            Temporal::Start { start } => FfiTemporal::Start { start: start.timestamp_millis() },
            Temporal::End { end } => FfiTemporal::End { end: end.timestamp_millis() },
            Temporal::Duration { duration } => FfiTemporal::Duration { duration },
            Temporal::StartAndEnd { start, end } => FfiTemporal::StartAndEnd {
                start: start.timestamp_millis(),
                end: end.timestamp_millis(),
            },
            Temporal::StartAndDuration { start, duration_ms } => FfiTemporal::StartAndDuration {
                start: start.timestamp_millis(),
                duration_ms,
            },
            Temporal::DurationAndEnd { duration_ms, end } => FfiTemporal::DurationAndEnd {
                duration_ms,
                end: end.timestamp_millis(),
            },
        }
    }
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiEntry {
    pub id: String,
    pub activity_id: Option<String>,
    pub owner_id: String,
    pub position: Option<FfiPosition>,
    pub is_template: bool,
    pub display_as_sets: bool,
    pub is_sequence: bool,
    pub is_complete: bool,
    pub temporal: FfiTemporal,
}

impl From<Entry> for FfiEntry {
    fn from(e: Entry) -> Self {
        FfiEntry {
            id: e.id.to_string(),
            activity_id: e.activity_id.map(|id| id.to_string()),
            owner_id: e.owner_id.to_string(),
            position: e.position.map(FfiPosition::from),
            is_template: e.is_template,
            display_as_sets: e.display_as_sets,
            is_sequence: e.is_sequence,
            is_complete: e.is_complete,
            temporal: FfiTemporal::from(e.temporal),
        }
    }
}

// --- Attribute ---

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiNumericConfig {
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub integer: bool,
    pub default: Option<f64>,
}

impl From<NumericConfig> for FfiNumericConfig {
    fn from(c: NumericConfig) -> Self {
        FfiNumericConfig { min: c.min, max: c.max, integer: c.integer, default: c.default }
    }
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiSelectConfig {
    pub options: Vec<String>,
    pub ordered: bool,
    pub default: Option<String>,
}

impl From<SelectConfig> for FfiSelectConfig {
    fn from(c: SelectConfig) -> Self {
        FfiSelectConfig { options: c.options, ordered: c.ordered, default: c.default }
    }
}

#[derive(uniffi::Enum, Debug, Clone)]
pub enum FfiMassUnit {
    Gram,
    Kilogram,
    Pound,
}

impl From<MassUnit> for FfiMassUnit {
    fn from(u: MassUnit) -> Self {
        match u {
            MassUnit::Gram => FfiMassUnit::Gram,
            MassUnit::Kilogram => FfiMassUnit::Kilogram,
            MassUnit::Pound => FfiMassUnit::Pound,
        }
    }
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiMassConfig {
    pub default_units: Vec<FfiMassUnit>,
}

impl From<MassConfig> for FfiMassConfig {
    fn from(c: MassConfig) -> Self {
        FfiMassConfig {
            default_units: c.default_units.into_iter().map(FfiMassUnit::from).collect(),
        }
    }
}

#[derive(uniffi::Enum, Debug, Clone)]
pub enum FfiAttributeConfig {
    Numeric(FfiNumericConfig),
    Select(FfiSelectConfig),
    Mass(FfiMassConfig),
}

impl From<AttributeConfig> for FfiAttributeConfig {
    fn from(c: AttributeConfig) -> Self {
        match c {
            AttributeConfig::Numeric(c) => FfiAttributeConfig::Numeric(c.into()),
            AttributeConfig::Select(c) => FfiAttributeConfig::Select(c.into()),
            AttributeConfig::Mass(c) => FfiAttributeConfig::Mass(c.into()),
        }
    }
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiAttribute {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    pub config: FfiAttributeConfig,
}

impl From<Attribute> for FfiAttribute {
    fn from(a: Attribute) -> Self {
        FfiAttribute {
            id: a.id.to_string(),
            owner_id: a.owner_id.to_string(),
            name: a.name,
            config: FfiAttributeConfig::from(a.config),
        }
    }
}

// --- Value ---

#[derive(uniffi::Enum, Debug, Clone)]
pub enum FfiNumericValue {
    Exact { value: f64 },
    Range { min: f64, max: f64 },
}

impl From<NumericValue> for FfiNumericValue {
    fn from(v: NumericValue) -> Self {
        match v {
            NumericValue::Exact(value) => FfiNumericValue::Exact { value },
            NumericValue::Range { min, max } => FfiNumericValue::Range { min, max },
        }
    }
}

#[derive(uniffi::Enum, Debug, Clone)]
pub enum FfiSelectValue {
    Exact { value: String },
    Range { min: String, max: String },
}

impl From<SelectValue> for FfiSelectValue {
    fn from(v: SelectValue) -> Self {
        match v {
            SelectValue::Exact(value) => FfiSelectValue::Exact { value },
            SelectValue::Range { min, max } => FfiSelectValue::Range { min, max },
        }
    }
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiMassMeasurement {
    pub unit: FfiMassUnit,
    pub value: f64,
}

impl From<MassMeasurement> for FfiMassMeasurement {
    fn from(m: MassMeasurement) -> Self {
        FfiMassMeasurement { unit: FfiMassUnit::from(m.unit), value: m.value }
    }
}

#[derive(uniffi::Enum, Debug, Clone)]
pub enum FfiMassValue {
    Exact { measurements: Vec<FfiMassMeasurement> },
    Range { min: Vec<FfiMassMeasurement>, max: Vec<FfiMassMeasurement> },
}

impl From<MassValue> for FfiMassValue {
    fn from(v: MassValue) -> Self {
        match v {
            MassValue::Exact(ms) => FfiMassValue::Exact {
                measurements: ms.into_iter().map(FfiMassMeasurement::from).collect(),
            },
            MassValue::Range { min, max } => FfiMassValue::Range {
                min: min.into_iter().map(FfiMassMeasurement::from).collect(),
                max: max.into_iter().map(FfiMassMeasurement::from).collect(),
            },
        }
    }
}

#[derive(uniffi::Enum, Debug, Clone)]
pub enum FfiAttributeValue {
    Numeric(FfiNumericValue),
    Select(FfiSelectValue),
    Mass(FfiMassValue),
}

impl From<AttributeValue> for FfiAttributeValue {
    fn from(v: AttributeValue) -> Self {
        match v {
            AttributeValue::Numeric(v) => FfiAttributeValue::Numeric(v.into()),
            AttributeValue::Select(v) => FfiAttributeValue::Select(v.into()),
            AttributeValue::Mass(v) => FfiAttributeValue::Mass(v.into()),
        }
    }
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiValue {
    pub entry_id: String,
    pub attribute_id: String,
    pub index_float: Option<f64>,
    pub index_string: Option<String>,
    pub plan: Option<FfiAttributeValue>,
    pub actual: Option<FfiAttributeValue>,
}

impl From<Value> for FfiValue {
    fn from(v: Value) -> Self {
        FfiValue {
            entry_id: v.entry_id.to_string(),
            attribute_id: v.attribute_id.to_string(),
            index_float: v.index_float,
            index_string: v.index_string,
            plan: v.plan.map(FfiAttributeValue::from),
            actual: v.actual.map(FfiAttributeValue::from),
        }
    }
}

// --- AttributePair ---

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiNumericAttributePair {
    pub attr_id: String,
    pub entry_id: String,
    pub owner_id: String,
    pub name: String,
    pub config: FfiNumericConfig,
    pub index_float: Option<f64>,
    pub plan: Option<FfiNumericValue>,
    pub actual: Option<FfiNumericValue>,
}

impl From<NumericAttributePair> for FfiNumericAttributePair {
    fn from(p: NumericAttributePair) -> Self {
        FfiNumericAttributePair {
            attr_id: p.attr_id.to_string(),
            entry_id: p.entry_id.to_string(),
            owner_id: p.owner_id.to_string(),
            name: p.name,
            config: p.config.into(),
            index_float: p.index_float,
            plan: p.plan.map(FfiNumericValue::from),
            actual: p.actual.map(FfiNumericValue::from),
        }
    }
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiSelectAttributePair {
    pub attr_id: String,
    pub entry_id: String,
    pub owner_id: String,
    pub name: String,
    pub config: FfiSelectConfig,
    pub index_string: Option<String>,
    pub plan: Option<FfiSelectValue>,
    pub actual: Option<FfiSelectValue>,
}

impl From<SelectAttributePair> for FfiSelectAttributePair {
    fn from(p: SelectAttributePair) -> Self {
        FfiSelectAttributePair {
            attr_id: p.attr_id.to_string(),
            entry_id: p.entry_id.to_string(),
            owner_id: p.owner_id.to_string(),
            name: p.name,
            config: p.config.into(),
            index_string: p.index_string,
            plan: p.plan.map(FfiSelectValue::from),
            actual: p.actual.map(FfiSelectValue::from),
        }
    }
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiMassAttributePair {
    pub attr_id: String,
    pub entry_id: String,
    pub owner_id: String,
    pub name: String,
    pub config: FfiMassConfig,
    pub index_float: Option<f64>,
    pub plan: Option<FfiMassValue>,
    pub actual: Option<FfiMassValue>,
}

impl From<MassAttributePair> for FfiMassAttributePair {
    fn from(p: MassAttributePair) -> Self {
        FfiMassAttributePair {
            attr_id: p.attr_id.to_string(),
            entry_id: p.entry_id.to_string(),
            owner_id: p.owner_id.to_string(),
            name: p.name,
            config: p.config.into(),
            index_float: p.index_float,
            plan: p.plan.map(FfiMassValue::from),
            actual: p.actual.map(FfiMassValue::from),
        }
    }
}

#[derive(uniffi::Enum, Debug, Clone)]
pub enum FfiAttributePair {
    Numeric(FfiNumericAttributePair),
    Select(FfiSelectAttributePair),
    Mass(FfiMassAttributePair),
}

impl From<AttributePair> for FfiAttributePair {
    fn from(p: AttributePair) -> Self {
        match p {
            AttributePair::Numeric(p) => FfiAttributePair::Numeric(p.into()),
            AttributePair::Select(p) => FfiAttributePair::Select(p.into()),
            AttributePair::Mass(p) => FfiAttributePair::Mass(p.into()),
        }
    }
}

// --- EntryJoin ---

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiEntryJoin {
    pub entry: FfiEntry,
    pub activity: Option<FfiActivity>,
    /// Flattened from HashMap<Uuid, AttributePair> via `.attributes()` iterator.
    pub attributes: Vec<FfiAttributePair>,
}

impl From<EntryJoin> for FfiEntryJoin {
    fn from(ej: EntryJoin) -> Self {
        FfiEntryJoin {
            entry: ej.entry.clone().into(),
            activity: ej.activity.clone().map(FfiActivity::from),
            attributes: ej.attributes().map(|a| FfiAttributePair::from(a.clone())).collect(),
        }
    }
}

// --- Queries ---

#[derive(uniffi::Enum, Clone)]
pub enum FfiAnyQuery {
    // Auth
    IsEmailRegistered { email: String },
    FindUserById { actor_id: String },
    FindUserByUsername { username: String },
    AllActorIds,
    // Activity
    FindActivityById { id: String },
    AllActivities,
    // Entry
    AllEntries,
    EntriesRootedInTimeInterval { from: i64, to: i64 },
    FindAncestors { entry_id: String },
    FindEntryById { entry_id: String },
    FindEntryJoinById { entry_id: String },
    FindDescendants { entry_id: String },
    // Attribute
    FindAttributeById { attribute_id: String },
    AllAttributes,
    FindAttributesByOwner { owner_id: String },
    // Value
    FindValueByKey { entry_id: String, attribute_id: String },
    FindValuesForEntry { entry_id: String },
    FindValuesForEntries { entry_ids: Vec<String> },
    FindAttributePairsForEntry { entry_id: String },
}

impl TryFrom<FfiAnyQuery> for AnyQuery {
    type Error = FfiError;

    fn try_from(q: FfiAnyQuery) -> Result<AnyQuery, FfiError> {
        match q {
            // Auth
            FfiAnyQuery::IsEmailRegistered { email } => Ok(AnyQuery::IsEmailRegistered(
                IsEmailRegistered { email: Email::parse(email).map_err(FfiError::from)? },
            )),
            FfiAnyQuery::FindUserById { actor_id } => Ok(AnyQuery::FindUserById(
                FindUserById { actor_id: parse_uuid(&actor_id)? },
            )),
            FfiAnyQuery::FindUserByUsername { username } => Ok(AnyQuery::FindUserByUsername(
                FindUserByUsername { username: Username::parse(username).map_err(FfiError::from)? },
            )),
            FfiAnyQuery::AllActorIds => Ok(AnyQuery::AllActorIds(AllActorIds {})),
            // Activity
            FfiAnyQuery::FindActivityById { id } => Ok(AnyQuery::FindActivityById(
                FindActivityById { id: parse_uuid(&id)? },
            )),
            FfiAnyQuery::AllActivities => Ok(AnyQuery::AllActivities(AllActivities {})),
            // Entry
            FfiAnyQuery::AllEntries => Ok(AnyQuery::AllEntries(AllEntries {})),
            FfiAnyQuery::EntriesRootedInTimeInterval { from, to } => {
                Ok(AnyQuery::EntriesRootedInTimeInterval(EntriesRootedInTimeInterval {
                    from: parse_timestamp_ms(from)?,
                    to: parse_timestamp_ms(to)?,
                }))
            }
            FfiAnyQuery::FindAncestors { entry_id } => Ok(AnyQuery::FindAncestors(
                FindAncestors { entry_id: parse_uuid(&entry_id)? },
            )),
            FfiAnyQuery::FindEntryById { entry_id } => Ok(AnyQuery::FindEntryById(
                FindEntryById { entry_id: parse_uuid(&entry_id)? },
            )),
            FfiAnyQuery::FindEntryJoinById { entry_id } => Ok(AnyQuery::FindEntryJoinById(
                FindEntryJoinById { entry_id: parse_uuid(&entry_id)? },
            )),
            FfiAnyQuery::FindDescendants { entry_id } => Ok(AnyQuery::FindDescendants(
                FindDescendants { entry_id: parse_uuid(&entry_id)? },
            )),
            // Attribute
            FfiAnyQuery::FindAttributeById { attribute_id } => Ok(AnyQuery::FindAttributeById(
                FindAttributeById { attribute_id: parse_uuid(&attribute_id)? },
            )),
            FfiAnyQuery::AllAttributes => Ok(AnyQuery::AllAttributes(AllAttributes {})),
            FfiAnyQuery::FindAttributesByOwner { owner_id } => Ok(AnyQuery::FindAttributesByOwner(
                FindAttributesByOwner { owner_id: parse_uuid(&owner_id)? },
            )),
            // Value
            FfiAnyQuery::FindValueByKey { entry_id, attribute_id } => {
                Ok(AnyQuery::FindValueByKey(FindValueByKey {
                    entry_id: parse_uuid(&entry_id)?,
                    attribute_id: parse_uuid(&attribute_id)?,
                }))
            }
            FfiAnyQuery::FindValuesForEntry { entry_id } => Ok(AnyQuery::FindValuesForEntry(
                FindValuesForEntry { entry_id: parse_uuid(&entry_id)? },
            )),
            FfiAnyQuery::FindValuesForEntries { entry_ids } => {
                let ids = entry_ids
                    .iter()
                    .map(|id| parse_uuid(id))
                    .collect::<Result<Vec<Uuid>, FfiError>>()?;
                Ok(AnyQuery::FindValuesForEntries(FindValuesForEntries { entry_ids: ids }))
            }
            FfiAnyQuery::FindAttributePairsForEntry { entry_id } => {
                Ok(AnyQuery::FindAttributePairsForEntry(FindAttributePairsForEntry {
                    entry_id: parse_uuid(&entry_id)?,
                }))
            }
        }
    }
}

#[derive(uniffi::Enum)]
pub enum FfiAnyQueryResponse {
    // Auth
    IsEmailRegistered(bool),
    FindUserById(Option<FfiUser>),
    FindUserByUsername(Option<FfiUser>),
    AllActorIds(Vec<String>),
    // Activity
    FindActivityById(Option<FfiActivity>),
    AllActivities(Vec<FfiActivity>),
    // Entry
    AllEntries(Vec<FfiEntry>),
    EntriesRootedInTimeInterval(Vec<FfiEntry>),
    FindAncestors(Vec<String>),
    FindEntryById(Option<FfiEntry>),
    FindEntryJoinById(Option<FfiEntryJoin>),
    FindDescendants(Vec<FfiEntry>),
    // Attribute
    FindAttributeById(Option<FfiAttribute>),
    AllAttributes(Vec<FfiAttribute>),
    FindAttributesByOwner(Vec<FfiAttribute>),
    // Value
    FindValueByKey(Option<FfiValue>),
    FindValuesForEntry(Vec<FfiValue>),
    FindValuesForEntries(Vec<FfiValue>),
    FindAttributePairsForEntry(Vec<FfiAttributePair>),
}

impl From<AnyQueryResponse> for FfiAnyQueryResponse {
    fn from(r: AnyQueryResponse) -> Self {
        match r {
            // Auth
            AnyQueryResponse::IsEmailRegistered(v) => FfiAnyQueryResponse::IsEmailRegistered(v),
            AnyQueryResponse::FindUserById(v) => {
                FfiAnyQueryResponse::FindUserById(v.map(FfiUser::from))
            }
            AnyQueryResponse::FindUserByUsername(v) => {
                FfiAnyQueryResponse::FindUserByUsername(v.map(FfiUser::from))
            }
            AnyQueryResponse::AllActorIds(v) => {
                FfiAnyQueryResponse::AllActorIds(v.into_iter().map(|id| id.to_string()).collect())
            }
            // Activity
            AnyQueryResponse::FindActivityById(v) => {
                FfiAnyQueryResponse::FindActivityById(v.map(FfiActivity::from))
            }
            AnyQueryResponse::AllActivities(v) => {
                FfiAnyQueryResponse::AllActivities(v.into_iter().map(FfiActivity::from).collect())
            }
            // Entry
            AnyQueryResponse::AllEntries(v) => {
                FfiAnyQueryResponse::AllEntries(v.into_iter().map(FfiEntry::from).collect())
            }
            AnyQueryResponse::EntriesRootedInTimeInterval(v) => {
                FfiAnyQueryResponse::EntriesRootedInTimeInterval(
                    v.into_iter().map(FfiEntry::from).collect(),
                )
            }
            AnyQueryResponse::FindAncestors(v) => {
                FfiAnyQueryResponse::FindAncestors(v.into_iter().map(|id| id.to_string()).collect())
            }
            AnyQueryResponse::FindEntryById(v) => {
                FfiAnyQueryResponse::FindEntryById(v.map(FfiEntry::from))
            }
            AnyQueryResponse::FindEntryJoinById(v) => {
                FfiAnyQueryResponse::FindEntryJoinById(v.map(FfiEntryJoin::from))
            }
            AnyQueryResponse::FindDescendants(v) => {
                FfiAnyQueryResponse::FindDescendants(v.into_iter().map(FfiEntry::from).collect())
            }
            // Attribute
            AnyQueryResponse::FindAttributeById(v) => {
                FfiAnyQueryResponse::FindAttributeById(v.map(FfiAttribute::from))
            }
            AnyQueryResponse::AllAttributes(v) => {
                FfiAnyQueryResponse::AllAttributes(v.into_iter().map(FfiAttribute::from).collect())
            }
            AnyQueryResponse::FindAttributesByOwner(v) => {
                FfiAnyQueryResponse::FindAttributesByOwner(
                    v.into_iter().map(FfiAttribute::from).collect(),
                )
            }
            // Value
            AnyQueryResponse::FindValueByKey(v) => {
                FfiAnyQueryResponse::FindValueByKey(v.map(FfiValue::from))
            }
            AnyQueryResponse::FindValuesForEntry(v) => {
                FfiAnyQueryResponse::FindValuesForEntry(v.into_iter().map(FfiValue::from).collect())
            }
            AnyQueryResponse::FindValuesForEntries(v) => {
                FfiAnyQueryResponse::FindValuesForEntries(
                    v.into_iter().map(FfiValue::from).collect(),
                )
            }
            AnyQueryResponse::FindAttributePairsForEntry(v) => {
                FfiAnyQueryResponse::FindAttributePairsForEntry(
                    v.into_iter().map(FfiAttributePair::from).collect(),
                )
            }
        }
    }
}

// --- Actions ---

pub(crate) fn ffi_temporal_to_core(t: FfiTemporal) -> Result<Temporal, FfiError> {
    Ok(match t {
        FfiTemporal::None => Temporal::None,
        FfiTemporal::Start { start } => Temporal::Start { start: parse_timestamp_ms(start)? },
        FfiTemporal::End { end } => Temporal::End { end: parse_timestamp_ms(end)? },
        FfiTemporal::Duration { duration } => Temporal::Duration { duration },
        FfiTemporal::StartAndEnd { start, end } => Temporal::StartAndEnd {
            start: parse_timestamp_ms(start)?,
            end: parse_timestamp_ms(end)?,
        },
        FfiTemporal::StartAndDuration { start, duration_ms } => Temporal::StartAndDuration {
            start: parse_timestamp_ms(start)?,
            duration_ms,
        },
        FfiTemporal::DurationAndEnd { duration_ms, end } => Temporal::DurationAndEnd {
            duration_ms,
            end: parse_timestamp_ms(end)?,
        },
    })
}

pub(crate) fn ffi_position_to_core(p: FfiPosition) -> Result<Position, FfiError> {
    let parent_id = parse_uuid(&p.parent_id)?;
    Position::parse(Some(parent_id), Some(p.frac_index))
        .map_err(FfiError::from)?
        .ok_or_else(|| FfiError::Generic("position unexpectedly None after parse".to_string()))
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiCreateActivity {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiMoveEntry {
    pub entry_id: String,
    pub position: Option<FfiPosition>,
    pub temporal: FfiTemporal,
}

#[derive(uniffi::Enum, Debug, Clone)]
pub enum FfiAction {
    CreateActivity(FfiCreateActivity),
    MoveEntry(FfiMoveEntry),
}

use chrono::{DateTime, Utc};
use fractional_index::FractionalIndex;
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
        AllActivities, AllActorIds, AllAttributes, AllEntries, AnyQuery, AnyQueryResponse,
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

// --- Leaf custom_type! declarations ---
//
// Every Rust type that crosses the FFI boundary as a leaf value goes through
// a `custom_type!` declaration here. `remote,` opts in to the foreign-crate
// path (Uuid lives in the `uuid` crate, not gv-ffi). Fallible `try_lift`s
// return `anyhow::Error`, which uniffi surfaces to Swift as a thrown error
// when the consuming function returns `Result`.
//
// Generic types like `DateTime<Utc>` can't be parsed by the macro directly
// (`Custom types must only have one component`) — bind them to a type alias
// first. The alias is just a name; uniffi registers the underlying type so
// any field typed as `DateTime<Utc>` picks up the conversion.

uniffi::custom_type!(Uuid, String, {
    remote,
    lower: |u| u.to_string(),
    try_lift: |s| Uuid::parse_str(&s).map_err(Into::into),
});

type UtcDateTime = DateTime<Utc>;
uniffi::custom_type!(UtcDateTime, i64, {
    remote,
    lower: |dt| dt.timestamp_millis(),
    try_lift: |ms| DateTime::<Utc>::from_timestamp_millis(ms)
        .ok_or_else(|| anyhow::anyhow!("invalid timestamp milliseconds: {ms}")),
});

uniffi::custom_type!(FractionalIndex, String, {
    remote,
    lower: |fi| fi.to_string(),
    try_lift: |s| FractionalIndex::from_string(&s).map_err(Into::into),
});

uniffi::custom_type!(Email, String, {
    remote,
    lower: |e| e.as_str().to_string(),
    try_lift: |s| Email::parse(s).map_err(Into::into),
});

uniffi::custom_type!(Username, String, {
    remote,
    lower: |u| u.as_str().to_string(),
    try_lift: |s| Username::parse(s).map_err(Into::into),
});

uniffi::custom_type!(ActivityName, String, {
    remote,
    lower: |n| n.to_string(),
    try_lift: |s| ActivityName::parse(s).map_err(Into::into),
});

// --- Helpers ---

pub(crate) fn parse_uuid(s: &str) -> Result<Uuid, FfiError> {
    Uuid::parse_str(s).map_err(|e| FfiError::Generic(format!("invalid UUID '{}': {}", s, e)))
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
        FfiAction::CreateScalarActivity(a) => {
            Ok(gv_core::actions::CreateScalarActivity {
                actor_id,
                activity: a.activity,
                template: a.template,
            }
            .into())
        }
        FfiAction::CreateSequenceActivity(a) => {
            Ok(gv_core::actions::CreateSequenceActivity {
                actor_id,
                activity: a.activity,
                template: a.template,
            }
            .into())
        }

        FfiAction::MoveEntry(a) => {
            let entry_id = parse_uuid(&a.entry_id)?;
            Ok(gv_core::actions::MoveEntry {
                actor_id,
                entry_id,
                position: a.position,
                temporal: a.temporal,
            }
            .into())
        }
        FfiAction::CreateEntry(a) => {
            let id = parse_uuid(&a.id)?;
            let activity_id = a.activity_id.as_deref().map(parse_uuid).transpose()?;
            let entry = gv_core::models::entry::Entry {
                id,
                activity_id,
                owner_id: actor_id,
                name: a.name,
                position: a.position,
                is_template: a.is_template,
                display_as_sets: a.display_as_sets,
                is_sequence: a.is_sequence,
                is_complete: a.is_complete,
                temporal: a.temporal,
            };
            Ok(gv_core::actions::CreateEntry { actor_id, entry }.into())
        }
        FfiAction::UpdateEntryCompletion(a) => {
            let entry_id = parse_uuid(&a.entry_id)?;
            Ok(gv_core::actions::UpdateEntryCompletion {
                actor_id,
                entry_id,
                is_complete: a.is_complete,
            }
            .into())
        }
        FfiAction::DeleteEntryRecursive(a) => {
            let entry_id = parse_uuid(&a.entry_id)?;
            Ok(gv_core::actions::DeleteEntryRecursive { actor_id, entry_id }.into())
        }
        FfiAction::UpdateAttributeValue(a) => {
            let entry_id = parse_uuid(&a.entry_id)?;
            let attribute_id = parse_uuid(&a.attribute_id)?;
            Ok(gv_core::actions::UpdateAttributeValue {
                actor_id,
                entry_id,
                attribute_id,
                field: a.field.into(),
                value: a.value.into(),
            }
            .into())
        }
    }
}

// --- User ---

#[uniffi::remote(Record)]
pub struct User {
    pub actor_id: Uuid,
    pub username: Username,
    pub email: Email,
}

// --- Activity ---

#[uniffi::remote(Record)]
pub struct Activity {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub source_activity_id: Option<Uuid>,
    pub name: ActivityName,
    pub description: Option<String>,
}

// --- Entry ---

#[uniffi::remote(Record)]
pub struct Position {
    pub parent_id: Uuid,
    pub frac_index: FractionalIndex,
}

#[uniffi::remote(Enum)]
pub enum Temporal {
    None,
    Start { start: DateTime<Utc> },
    End { end: DateTime<Utc> },
    Duration { duration: u32 },
    StartAndEnd { start: DateTime<Utc>, end: DateTime<Utc> },
    StartAndDuration { start: DateTime<Utc>, duration_ms: u32 },
    DurationAndEnd { duration_ms: u32, end: DateTime<Utc> },
}

#[uniffi::remote(Record)]
pub struct Entry {
    pub id: Uuid,
    pub activity_id: Option<Uuid>,
    pub owner_id: Uuid,
    pub name: Option<String>,
    pub position: Option<Position>,
    pub is_template: bool,
    pub display_as_sets: bool,
    pub is_sequence: bool,
    pub is_complete: bool,
    pub temporal: Temporal,
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
        FfiNumericConfig {
            min: c.min,
            max: c.max,
            integer: c.integer,
            default: c.default,
        }
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
        FfiSelectConfig {
            options: c.options,
            ordered: c.ordered,
            default: c.default,
        }
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

impl From<FfiMassUnit> for MassUnit {
    fn from(u: FfiMassUnit) -> Self {
        match u {
            FfiMassUnit::Gram => MassUnit::Gram,
            FfiMassUnit::Kilogram => MassUnit::Kilogram,
            FfiMassUnit::Pound => MassUnit::Pound,
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

impl From<FfiNumericValue> for NumericValue {
    fn from(v: FfiNumericValue) -> Self {
        match v {
            FfiNumericValue::Exact { value } => NumericValue::Exact(value),
            FfiNumericValue::Range { min, max } => NumericValue::Range { min, max },
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

impl From<FfiSelectValue> for SelectValue {
    fn from(v: FfiSelectValue) -> Self {
        match v {
            FfiSelectValue::Exact { value } => SelectValue::Exact(value),
            FfiSelectValue::Range { min, max } => SelectValue::Range { min, max },
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
        FfiMassMeasurement {
            unit: FfiMassUnit::from(m.unit),
            value: m.value,
        }
    }
}

impl From<FfiMassMeasurement> for MassMeasurement {
    fn from(m: FfiMassMeasurement) -> Self {
        MassMeasurement {
            unit: MassUnit::from(m.unit),
            value: m.value,
        }
    }
}

#[derive(uniffi::Enum, Debug, Clone)]
pub enum FfiMassValue {
    Exact {
        measurements: Vec<FfiMassMeasurement>,
    },
    Range {
        min: Vec<FfiMassMeasurement>,
        max: Vec<FfiMassMeasurement>,
    },
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

impl From<FfiMassValue> for MassValue {
    fn from(v: FfiMassValue) -> Self {
        match v {
            FfiMassValue::Exact { measurements } => MassValue::Exact(
                measurements
                    .into_iter()
                    .map(MassMeasurement::from)
                    .collect(),
            ),
            FfiMassValue::Range { min, max } => MassValue::Range {
                min: min.into_iter().map(MassMeasurement::from).collect(),
                max: max.into_iter().map(MassMeasurement::from).collect(),
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

impl From<FfiAttributeValue> for AttributeValue {
    fn from(v: FfiAttributeValue) -> Self {
        match v {
            FfiAttributeValue::Numeric(v) => AttributeValue::Numeric(v.into()),
            FfiAttributeValue::Select(v) => AttributeValue::Select(v.into()),
            FfiAttributeValue::Mass(v) => AttributeValue::Mass(v.into()),
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
    pub entry: Entry,
    pub activity: Option<Activity>,
    pub attributes: Vec<FfiAttributePair>,
    pub display_name: String,
}

impl From<EntryJoin> for FfiEntryJoin {
    fn from(ej: EntryJoin) -> Self {
        FfiEntryJoin {
            entry: ej.entry,
            activity: ej.activity,
            attributes: ej
                .attributes
                .into_iter()
                .map(FfiAttributePair::from)
                .collect(),
            display_name: ej.display_name,
        }
    }
}

// --- Queries ---

#[derive(uniffi::Enum, Clone)]
pub enum FfiAnyQuery {
    // Auth
    IsEmailRegistered {
        email: String,
    },
    FindUserById {
        actor_id: String,
    },
    FindUserByUsername {
        username: String,
    },
    AllActorIds,
    // Activity
    FindActivityById {
        id: String,
    },
    AllActivities,
    // Entry
    AllEntries,
    EntriesRootedInTimeInterval {
        from: i64,
        to: i64,
    },
    FindAncestors {
        entry_id: String,
    },
    FindEntryById {
        entry_id: String,
    },
    FindEntryJoinById {
        entry_id: String,
    },
    FindDescendants {
        entry_id: String,
    },
    // Attribute
    FindAttributeById {
        attribute_id: String,
    },
    AllAttributes,
    FindAttributesByOwner {
        owner_id: String,
    },
    // Value
    FindValueByKey {
        entry_id: String,
        attribute_id: String,
    },
    FindValuesForEntry {
        entry_id: String,
    },
    FindValuesForEntries {
        entry_ids: Vec<String>,
    },
    FindAttributePairsForEntry {
        entry_id: String,
    },
}

impl TryFrom<FfiAnyQuery> for AnyQuery {
    type Error = FfiError;

    fn try_from(q: FfiAnyQuery) -> Result<AnyQuery, FfiError> {
        match q {
            // Auth
            FfiAnyQuery::IsEmailRegistered { email } => {
                Ok(AnyQuery::IsEmailRegistered(IsEmailRegistered {
                    email: Email::parse(email).map_err(FfiError::from)?,
                }))
            }
            FfiAnyQuery::FindUserById { actor_id } => Ok(AnyQuery::FindUserById(FindUserById {
                actor_id: parse_uuid(&actor_id)?,
            })),
            FfiAnyQuery::FindUserByUsername { username } => {
                Ok(AnyQuery::FindUserByUsername(FindUserByUsername {
                    username: Username::parse(username).map_err(FfiError::from)?,
                }))
            }
            FfiAnyQuery::AllActorIds => Ok(AnyQuery::AllActorIds(AllActorIds {})),
            // Activity
            FfiAnyQuery::FindActivityById { id } => {
                Ok(AnyQuery::FindActivityById(FindActivityById {
                    id: parse_uuid(&id)?,
                }))
            }
            FfiAnyQuery::AllActivities => Ok(AnyQuery::AllActivities(AllActivities {})),
            // Entry
            FfiAnyQuery::AllEntries => Ok(AnyQuery::AllEntries(AllEntries {})),
            FfiAnyQuery::EntriesRootedInTimeInterval { from, to } => Ok(
                AnyQuery::EntriesRootedInTimeInterval(EntriesRootedInTimeInterval {
                    from: parse_timestamp_ms(from)?,
                    to: parse_timestamp_ms(to)?,
                }),
            ),
            FfiAnyQuery::FindAncestors { entry_id } => Ok(AnyQuery::FindAncestors(FindAncestors {
                entry_id: parse_uuid(&entry_id)?,
            })),
            FfiAnyQuery::FindEntryById { entry_id } => Ok(AnyQuery::FindEntryById(FindEntryById {
                entry_id: parse_uuid(&entry_id)?,
            })),
            FfiAnyQuery::FindEntryJoinById { entry_id } => {
                Ok(AnyQuery::FindEntryJoinById(FindEntryJoinById {
                    entry_id: parse_uuid(&entry_id)?,
                }))
            }
            FfiAnyQuery::FindDescendants { entry_id } => {
                Ok(AnyQuery::FindDescendants(FindDescendants {
                    entry_id: parse_uuid(&entry_id)?,
                }))
            }
            // Attribute
            FfiAnyQuery::FindAttributeById { attribute_id } => {
                Ok(AnyQuery::FindAttributeById(FindAttributeById {
                    attribute_id: parse_uuid(&attribute_id)?,
                }))
            }
            FfiAnyQuery::AllAttributes => Ok(AnyQuery::AllAttributes(AllAttributes {})),
            FfiAnyQuery::FindAttributesByOwner { owner_id } => {
                Ok(AnyQuery::FindAttributesByOwner(FindAttributesByOwner {
                    owner_id: parse_uuid(&owner_id)?,
                }))
            }
            // Value
            FfiAnyQuery::FindValueByKey {
                entry_id,
                attribute_id,
            } => Ok(AnyQuery::FindValueByKey(FindValueByKey {
                entry_id: parse_uuid(&entry_id)?,
                attribute_id: parse_uuid(&attribute_id)?,
            })),
            FfiAnyQuery::FindValuesForEntry { entry_id } => {
                Ok(AnyQuery::FindValuesForEntry(FindValuesForEntry {
                    entry_id: parse_uuid(&entry_id)?,
                }))
            }
            FfiAnyQuery::FindValuesForEntries { entry_ids } => {
                let ids = entry_ids
                    .iter()
                    .map(|id| parse_uuid(id))
                    .collect::<Result<Vec<Uuid>, FfiError>>()?;
                Ok(AnyQuery::FindValuesForEntries(FindValuesForEntries {
                    entry_ids: ids,
                }))
            }
            FfiAnyQuery::FindAttributePairsForEntry { entry_id } => Ok(
                AnyQuery::FindAttributePairsForEntry(FindAttributePairsForEntry {
                    entry_id: parse_uuid(&entry_id)?,
                }),
            ),
        }
    }
}

#[derive(uniffi::Enum)]
pub enum FfiAnyQueryResponse {
    // Auth
    IsEmailRegistered(bool),
    FindUserById(Option<User>),
    FindUserByUsername(Option<User>),
    AllActorIds(Vec<String>),
    // Activity
    FindActivityById(Option<Activity>),
    AllActivities(Vec<Activity>),
    // Entry
    AllEntries(Vec<Entry>),
    EntriesRootedInTimeInterval(Vec<Entry>),
    FindAncestors(Vec<String>),
    FindEntryById(Option<Entry>),
    FindEntryJoinById(Option<FfiEntryJoin>),
    FindDescendants(Vec<Entry>),
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
            AnyQueryResponse::FindUserById(v) => FfiAnyQueryResponse::FindUserById(v),
            AnyQueryResponse::FindUserByUsername(v) => FfiAnyQueryResponse::FindUserByUsername(v),
            AnyQueryResponse::AllActorIds(v) => {
                FfiAnyQueryResponse::AllActorIds(v.into_iter().map(|id| id.to_string()).collect())
            }
            // Activity
            AnyQueryResponse::FindActivityById(v) => FfiAnyQueryResponse::FindActivityById(v),
            AnyQueryResponse::AllActivities(v) => FfiAnyQueryResponse::AllActivities(v),
            // Entry
            AnyQueryResponse::AllEntries(v) => FfiAnyQueryResponse::AllEntries(v),
            AnyQueryResponse::EntriesRootedInTimeInterval(v) => {
                FfiAnyQueryResponse::EntriesRootedInTimeInterval(v)
            }
            AnyQueryResponse::FindAncestors(v) => {
                FfiAnyQueryResponse::FindAncestors(v.into_iter().map(|id| id.to_string()).collect())
            }
            AnyQueryResponse::FindEntryById(v) => FfiAnyQueryResponse::FindEntryById(v),
            AnyQueryResponse::FindEntryJoinById(v) => {
                FfiAnyQueryResponse::FindEntryJoinById(v.map(FfiEntryJoin::from))
            }
            AnyQueryResponse::FindDescendants(v) => FfiAnyQueryResponse::FindDescendants(v),
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
            AnyQueryResponse::FindValuesForEntries(v) => FfiAnyQueryResponse::FindValuesForEntries(
                v.into_iter().map(FfiValue::from).collect(),
            ),
            AnyQueryResponse::FindAttributePairsForEntry(v) => {
                FfiAnyQueryResponse::FindAttributePairsForEntry(
                    v.into_iter().map(FfiAttributePair::from).collect(),
                )
            }
        }
    }
}

// --- Actions ---

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiCreateScalarActivity {
    pub activity: Activity,
    pub template: Entry,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiCreateSequenceActivity {
    pub activity: Activity,
    pub template: Vec<Entry>,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiMoveEntry {
    pub entry_id: String,
    pub position: Option<Position>,
    pub temporal: Temporal,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiCreateEntry {
    pub id: String,
    pub activity_id: Option<String>,
    pub name: Option<String>,
    pub position: Option<Position>,
    pub is_template: bool,
    pub display_as_sets: bool,
    pub is_sequence: bool,
    pub is_complete: bool,
    pub temporal: Temporal,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiUpdateEntryCompletion {
    pub entry_id: String,
    pub is_complete: bool,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiDeleteEntryRecursive {
    pub entry_id: String,
}

/// Identifies which side of a Value (plan or actual) is being written.
/// Mirrors `gv_core::actions::ValueField`.
#[derive(uniffi::Enum, Debug, Clone)]
pub enum FfiValueField {
    Plan,
    Actual,
}

impl From<FfiValueField> for gv_core::actions::ValueField {
    fn from(f: FfiValueField) -> Self {
        match f {
            FfiValueField::Plan => gv_core::actions::ValueField::Plan,
            FfiValueField::Actual => gv_core::actions::ValueField::Actual,
        }
    }
}

/// Update an existing entry-attribute Value. The Value row must already exist;
/// today the Swift app only edits attribute pairs returned by
/// `FindAttributePairsForEntry` (which all have a Value row, per
/// attributes-design.md states 2 and 3). Add a `FfiCreateValue` action when
/// the entry-attribute add UI lands.
#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiUpdateAttributeValue {
    pub entry_id: String,
    pub attribute_id: String,
    pub field: FfiValueField,
    pub value: FfiAttributeValue,
}

#[derive(uniffi::Enum, Debug, Clone)]
pub enum FfiAction {
    CreateScalarActivity(FfiCreateScalarActivity),
    CreateSequenceActivity(FfiCreateSequenceActivity),
    MoveEntry(FfiMoveEntry),
    CreateEntry(FfiCreateEntry),
    UpdateEntryCompletion(FfiUpdateEntryCompletion),
    DeleteEntryRecursive(FfiDeleteEntryRecursive),
    UpdateAttributeValue(FfiUpdateAttributeValue),
}

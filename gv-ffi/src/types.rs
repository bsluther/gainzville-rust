use chrono::{DateTime, Utc};
use fractional_index::FractionalIndex;
use gv_core::{
    actions::{
        Action, AttachValue, AttributeChange, CreateActivity, CreateAttribute, CreateEntry,
        CreateEntryFromActivity, CreateUser, CreateValue, DeleteAttributeValue,
        DeleteEntryRecursive, EntryChange, MassChange, MoveEntry, NumericChange, SelectChange,
        UpdateAttribute, UpdateAttributeValue, UpdateEntry, UpdateEntryCompletion, ValueField,
    },
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
        EntriesRootedInTimeInterval, FindActivityById, FindActivityTemplateRoot, FindAncestors,
        FindAttributeById,
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
    Start {
        start: DateTime<Utc>,
    },
    End {
        end: DateTime<Utc>,
    },
    Duration {
        duration: u32,
    },
    StartAndEnd {
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    },
    StartAndDuration {
        start: DateTime<Utc>,
        duration_ms: u32,
    },
    DurationAndEnd {
        duration_ms: u32,
        end: DateTime<Utc>,
    },
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

#[uniffi::remote(Record)]
pub struct NumericConfig {
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub integer: bool,
    pub default: Option<f64>,
}

#[uniffi::remote(Record)]
pub struct SelectConfig {
    pub options: Vec<String>,
    pub ordered: bool,
    pub default: Option<String>,
}

#[uniffi::remote(Enum)]
pub enum MassUnit {
    Gram,
    Kilogram,
    Pound,
}

#[uniffi::remote(Record)]
pub struct MassConfig {
    pub default_units: Vec<MassUnit>,
}

#[uniffi::remote(Enum)]
pub enum AttributeConfig {
    Numeric(NumericConfig),
    Select(SelectConfig),
    Mass(MassConfig),
}

#[uniffi::remote(Record)]
pub struct Attribute {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub config: AttributeConfig,
}

// --- Value ---

#[uniffi::remote(Enum)]
pub enum NumericValue {
    Exact(f64),
    Range { min: f64, max: f64 },
}

#[uniffi::remote(Enum)]
pub enum SelectValue {
    Exact(String),
    Range { min: String, max: String },
}

#[uniffi::remote(Record)]
pub struct MassMeasurement {
    pub unit: MassUnit,
    pub value: f64,
}

#[uniffi::remote(Enum)]
pub enum MassValue {
    Exact(Vec<MassMeasurement>),
    Range {
        min: Vec<MassMeasurement>,
        max: Vec<MassMeasurement>,
    },
}

#[uniffi::remote(Enum)]
pub enum AttributeValue {
    Numeric(NumericValue),
    Select(SelectValue),
    Mass(MassValue),
}

#[uniffi::remote(Record)]
pub struct Value {
    pub entry_id: Uuid,
    pub attribute_id: Uuid,
    pub index_float: Option<f64>,
    pub index_string: Option<String>,
    pub plan: Option<AttributeValue>,
    pub actual: Option<AttributeValue>,
}

// --- AttributePair ---

#[uniffi::remote(Record)]
pub struct NumericAttributePair {
    pub attr_id: Uuid,
    pub entry_id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    pub config: NumericConfig,
    pub index_float: Option<f64>,
    pub plan: Option<NumericValue>,
    pub actual: Option<NumericValue>,
}

#[uniffi::remote(Record)]
pub struct SelectAttributePair {
    pub attr_id: Uuid,
    pub entry_id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    pub config: SelectConfig,
    pub index_string: Option<String>,
    pub plan: Option<SelectValue>,
    pub actual: Option<SelectValue>,
}

#[uniffi::remote(Record)]
pub struct MassAttributePair {
    pub attr_id: Uuid,
    pub entry_id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    pub config: MassConfig,
    pub index_float: Option<f64>,
    pub plan: Option<MassValue>,
    pub actual: Option<MassValue>,
}

#[uniffi::remote(Enum)]
pub enum AttributePair {
    Numeric(NumericAttributePair),
    Select(SelectAttributePair),
    Mass(MassAttributePair),
}

// --- EntryJoin ---

#[uniffi::remote(Record)]
pub struct EntryJoin {
    pub entry: Entry,
    pub activity: Option<Activity>,
    pub attributes: Vec<AttributePair>,
    pub display_name: String,
}

// --- Queries ---

#[uniffi::remote(Record)]
pub struct IsEmailRegistered {
    pub email: Email,
}

#[uniffi::remote(Record)]
pub struct FindUserById {
    pub actor_id: Uuid,
}

#[uniffi::remote(Record)]
pub struct FindUserByUsername {
    pub username: Username,
}

#[uniffi::remote(Record)]
pub struct AllActorIds;

#[uniffi::remote(Record)]
pub struct FindActivityById {
    pub id: Uuid,
}

#[uniffi::remote(Record)]
pub struct FindActivityTemplateRoot {
    pub activity_id: Uuid,
}

#[uniffi::remote(Record)]
pub struct AllActivities;

#[uniffi::remote(Record)]
pub struct AllEntries;

#[uniffi::remote(Record)]
pub struct EntriesRootedInTimeInterval {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

#[uniffi::remote(Record)]
pub struct FindAncestors {
    pub entry_id: Uuid,
}

#[uniffi::remote(Record)]
pub struct FindEntryById {
    pub entry_id: Uuid,
}

#[uniffi::remote(Record)]
pub struct FindEntryJoinById {
    pub entry_id: Uuid,
}

#[uniffi::remote(Record)]
pub struct FindDescendants {
    pub entry_id: Uuid,
}

#[uniffi::remote(Record)]
pub struct FindAttributeById {
    pub attribute_id: Uuid,
}

#[uniffi::remote(Record)]
pub struct AllAttributes;

#[uniffi::remote(Record)]
pub struct FindAttributesByOwner {
    pub owner_id: Uuid,
}

#[uniffi::remote(Record)]
pub struct FindValueByKey {
    pub entry_id: Uuid,
    pub attribute_id: Uuid,
}

#[uniffi::remote(Record)]
pub struct FindValuesForEntry {
    pub entry_id: Uuid,
}

#[uniffi::remote(Record)]
pub struct FindValuesForEntries {
    pub entry_ids: Vec<Uuid>,
}

#[uniffi::remote(Record)]
pub struct FindAttributePairsForEntry {
    pub entry_id: Uuid,
}

#[uniffi::remote(Enum)]
pub enum AnyQuery {
    // Auth
    IsEmailRegistered(IsEmailRegistered),
    FindUserById(FindUserById),
    FindUserByUsername(FindUserByUsername),
    AllActorIds(AllActorIds),
    // Activity
    FindActivityById(FindActivityById),
    AllActivities(AllActivities),
    FindActivityTemplateRoot(FindActivityTemplateRoot),
    // Entry
    AllEntries(AllEntries),
    EntriesRootedInTimeInterval(EntriesRootedInTimeInterval),
    FindAncestors(FindAncestors),
    FindEntryById(FindEntryById),
    FindEntryJoinById(FindEntryJoinById),
    FindDescendants(FindDescendants),
    // Attribute
    FindAttributeById(FindAttributeById),
    AllAttributes(AllAttributes),
    FindAttributesByOwner(FindAttributesByOwner),
    // Value
    FindValueByKey(FindValueByKey),
    FindValuesForEntry(FindValuesForEntry),
    FindValuesForEntries(FindValuesForEntries),
    FindAttributePairsForEntry(FindAttributePairsForEntry),
}

#[uniffi::remote(Enum)]
pub enum AnyQueryResponse {
    // Auth
    IsEmailRegistered(bool),
    FindUserById(Option<User>),
    FindUserByUsername(Option<User>),
    AllActorIds(Vec<Uuid>),
    // Activity
    FindActivityById(Option<Activity>),
    FindActivityTemplateRoot(Option<Entry>),
    AllActivities(Vec<Activity>),
    // Entry
    AllEntries(Vec<Entry>),
    EntriesRootedInTimeInterval(Vec<Entry>),
    FindAncestors(Vec<Uuid>),
    FindEntryById(Option<Entry>),
    FindEntryJoinById(Option<EntryJoin>),
    FindDescendants(Vec<Entry>),
    // Attribute
    FindAttributeById(Option<Attribute>),
    AllAttributes(Vec<Attribute>),
    FindAttributesByOwner(Vec<Attribute>),
    // Value
    FindValueByKey(Option<Value>),
    FindValuesForEntry(Vec<Value>),
    FindValuesForEntries(Vec<Value>),
    FindAttributePairsForEntry(Vec<AttributePair>),
}

// --- Actions ---

#[uniffi::remote(Record)]
pub struct CreateUser {
    pub user: User,
}

#[uniffi::remote(Record)]
pub struct CreateActivity {
    pub actor_id: Uuid,
    pub activity: Activity,
    pub template: Vec<Entry>,
}

#[uniffi::remote(Record)]
pub struct CreateEntry {
    pub actor_id: Uuid,
    pub entry: Entry,
}

#[uniffi::remote(Record)]
pub struct CreateEntryFromActivity {
    pub actor_id: Uuid,
    pub activity_id: Uuid,
    pub position: Option<Position>,
    pub temporal: Temporal,
    pub is_template: bool,
}

#[uniffi::remote(Record)]
pub struct MoveEntry {
    pub actor_id: Uuid,
    pub entry_id: Uuid,
    pub position: Option<Position>,
    pub temporal: Temporal,
}

#[uniffi::remote(Record)]
pub struct DeleteEntryRecursive {
    pub actor_id: Uuid,
    pub entry_id: Uuid,
}

#[uniffi::remote(Record)]
pub struct CreateAttribute {
    pub actor_id: Uuid,
    pub attribute: Attribute,
}

#[uniffi::remote(Record)]
pub struct CreateValue {
    pub actor_id: Uuid,
    pub value: Value,
}

#[uniffi::remote(Record)]
pub struct UpdateEntryCompletion {
    pub actor_id: Uuid,
    pub entry_id: Uuid,
    pub is_complete: bool,
}

#[uniffi::remote(Enum)]
pub enum ValueField {
    Plan,
    Actual,
}

#[uniffi::remote(Record)]
pub struct UpdateAttributeValue {
    pub actor_id: Uuid,
    pub entry_id: Uuid,
    pub attribute_id: Uuid,
    pub field: ValueField,
    pub value: AttributeValue,
}

#[uniffi::remote(Record)]
pub struct AttachValue {
    pub actor_id: Uuid,
    pub entry_id: Uuid,
    pub attribute_id: Uuid,
}

#[uniffi::remote(Record)]
pub struct DeleteAttributeValue {
    pub actor_id: Uuid,
    pub entry_id: Uuid,
    pub attribute_id: Uuid,
}

#[uniffi::remote(Enum)]
pub enum NumericChange {
    SetDefault(Option<f64>),
}

#[uniffi::remote(Enum)]
pub enum SelectChange {
    SetDefault(Option<String>),
}

#[uniffi::remote(Enum)]
pub enum MassChange {
    SetDefaultUnits(Vec<MassUnit>),
}

#[uniffi::remote(Enum)]
pub enum AttributeChange {
    SetName(String),
    SetDescription(Option<String>),
    Numeric(NumericChange),
    Select(SelectChange),
    Mass(MassChange),
}

#[uniffi::remote(Record)]
pub struct UpdateAttribute {
    pub actor_id: Uuid,
    pub attribute_id: Uuid,
    pub change: AttributeChange,
}

#[uniffi::remote(Enum)]
pub enum EntryChange {
    SetIsSequence(bool),
}

#[uniffi::remote(Record)]
pub struct UpdateEntry {
    pub actor_id: Uuid,
    pub entry_id: Uuid,
    pub change: EntryChange,
}

#[uniffi::remote(Enum)]
pub enum Action {
    CreateUser(CreateUser),
    CreateActivity(CreateActivity),
    CreateAttribute(CreateAttribute),
    CreateValue(CreateValue),
    AttachValue(AttachValue),
    DeleteAttributeValue(DeleteAttributeValue),
    CreateEntry(CreateEntry),
    CreateEntryFromActivity(CreateEntryFromActivity),
    DeleteEntryRecursive(DeleteEntryRecursive),
    MoveEntry(MoveEntry),
    UpdateEntryCompletion(UpdateEntryCompletion),
    UpdateAttributeValue(UpdateAttributeValue),
    UpdateAttribute(UpdateAttribute),
    UpdateEntry(UpdateEntry),
}

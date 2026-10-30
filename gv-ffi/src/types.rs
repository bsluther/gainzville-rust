use chrono::{DateTime, Utc};
use fractional_index::FractionalIndex;
use gv_client::{
    live::{LiveBatch, LiveUpdate, LiveValue},
    queries::{ClientQuery, ClientQueryResponse},
};
use gv_core::{
    actions::{
        Action, AttachValue, AttributeChange, ConvertToSets, CreateActivity, CreateAttribute,
        CreateEntry, CreateEntryFromActivity, CreateUser, CreateValue, DeleteAttributeValue,
        DeleteEntryRecursive, DuplicateEntry, EntryChange, LengthChange, MassChange, MoveEntry,
        NumericChange, SelectChange, TextChange, UpdateAttribute, UpdateAttributeValue,
        UpdateEntry, UpdateEntryCompletion, ValueField,
    },
    models::{
        activity::{Activity, ActivityName},
        attribute::{
            Attribute, AttributeConfig, AttributeValue, LengthConfig, LengthMeasurement,
            LengthUnit, LengthValue, MassConfig, MassMeasurement, MassUnit, MassValue,
            MultiselectConfig, NumericConfig, NumericValue, SelectConfig, SelectValue, TextConfig,
            Value,
        },
        attribute_pair::{
            AttributePair, LengthAttributePair, MassAttributePair, MultiselectAttributePair,
            NumericAttributePair, SelectAttributePair, TextAttributePair,
        },
        entry::{Entry, Position, Temporal},
        entry_join::EntryJoin,
        user::User,
    },
    queries::*,
    validation::{Email, Username},
};
use gv_ui::{ChildSlot, queries::*};
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

#[uniffi::remote(Record)]
pub struct MultiselectConfig {
    pub options: Vec<String>,
    pub default: Option<Vec<String>>,
}

#[uniffi::remote(Enum)]
pub enum MassUnit {
    Gram,
    Kilogram,
    Pound,
}

#[uniffi::remote(Record)]
pub struct MassConfig {
    pub default_unit: MassUnit,
}

#[uniffi::remote(Enum)]
pub enum LengthUnit {
    Millimeter,
    Centimeter,
    Meter,
    Kilometer,
    Inch,
    Foot,
    Yard,
    Mile,
}

#[uniffi::remote(Record)]
pub struct LengthConfig {
    pub default_unit: LengthUnit,
}

#[uniffi::remote(Record)]
pub struct TextConfig {
    pub default: Option<String>,
    pub autocomplete: bool,
}

#[uniffi::remote(Enum)]
pub enum AttributeConfig {
    Numeric(NumericConfig),
    Select(SelectConfig),
    Multiselect(MultiselectConfig),
    Mass(MassConfig),
    Length(LengthConfig),
    Text(TextConfig),
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
    Exact(MassMeasurement),
    Range { unit: MassUnit, min: f64, max: f64 },
}

/// Unit conversion for mass values; the logic lives in core
/// (`MassValue::converted_to`). uniffi can't attach methods to remote data
/// enums, so pure helpers cross the boundary as free functions and the Swift
/// side wraps them back into extension methods (`MassValue.converted(to:)`).
#[uniffi::export]
pub fn mass_value_converted_to(value: MassValue, unit: MassUnit) -> MassValue {
    value.converted_to(unit)
}

#[uniffi::remote(Record)]
pub struct LengthMeasurement {
    pub unit: LengthUnit,
    pub value: f64,
}

#[uniffi::remote(Enum)]
pub enum LengthValue {
    Exact(LengthMeasurement),
    Range {
        unit: LengthUnit,
        min: f64,
        max: f64,
    },
}

/// Unit conversion for length values; mirrors `mass_value_converted_to` (logic
/// lives in core, `LengthValue::converted_to`). The Swift side wraps it as
/// `LengthValue.converted(to:)`.
#[uniffi::export]
pub fn length_value_converted_to(value: LengthValue, unit: LengthUnit) -> LengthValue {
    value.converted_to(unit)
}

#[uniffi::remote(Enum)]
pub enum AttributeValue {
    Numeric(NumericValue),
    Select(SelectValue),
    Multiselect(Vec<String>),
    Mass(MassValue),
    Length(LengthValue),
    Text(String),
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
pub struct MultiselectAttributePair {
    pub attr_id: Uuid,
    pub entry_id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    pub config: MultiselectConfig,
    pub index_string: Option<String>,
    pub plan: Option<Vec<String>>,
    pub actual: Option<Vec<String>>,
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

#[uniffi::remote(Record)]
pub struct LengthAttributePair {
    pub attr_id: Uuid,
    pub entry_id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    pub config: LengthConfig,
    pub index_float: Option<f64>,
    pub plan: Option<LengthValue>,
    pub actual: Option<LengthValue>,
}

#[uniffi::remote(Record)]
pub struct TextAttributePair {
    pub attr_id: Uuid,
    pub entry_id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    pub config: TextConfig,
    pub index_string: Option<String>,
    pub plan: Option<String>,
    pub actual: Option<String>,
}

#[uniffi::remote(Enum)]
pub enum AttributePair {
    Numeric(NumericAttributePair),
    Select(SelectAttributePair),
    Multiselect(MultiselectAttributePair),
    Mass(MassAttributePair),
    Length(LengthAttributePair),
    Text(TextAttributePair),
}

// --- EntryJoin ---

#[uniffi::remote(Record)]
pub struct EntryJoin {
    pub entry: Entry,
    pub activity: Option<Activity>,
    pub attributes: Vec<AttributePair>,
    pub display_name: String,
}

// --- Questions and query sets (generated) ---
//
// Generated from the same lists that define the Rust types, via the x-macros the defining crates
// export: `core_questions!`/`ui_questions!` (one remote Record per question) and
// `domain_query_set!`/`client_query_set!` (each set and its response enum as remote Enums). Adding
// a question or changing a set's membership needs no edit here. What stays hand-written: the
// answer types (domain records above, `ChildSlot` below) and the live envelope (`LiveValue`,
// `LiveUpdate`, `LiveBatch`). Question field types are resolved in this module, so they must be
// in scope here (an unknown one is a compile error at the x-macro call).

/// Callback for `core_questions!`/`ui_questions!`: one remote Record per question.
macro_rules! ffi_question_records {
    ($(
        $(#[$meta:meta])*
        $vis:vis struct $name:ident $body:tt => $response:ty;
    )*) => {$(
        #[uniffi::remote(Record)]
        $(#[$meta])*
        $vis struct $name $body
    )*};
}

/// Callback for `domain_query_set!`/`client_query_set!`: the set and its response enum as remote
/// Enums, variants in declaration order (forest members first). The backing groups are erased:
/// Swift sees one flat enum.
macro_rules! ffi_query_set {
    (
        $(#[$meta:meta])*
        $vis:vis enum $set:ident -> $response:ident {
            $(forest { $($f:ident),+ $(,)? })?
            executor $exec:ident { $($q:ident),+ $(,)? }
        }
    ) => {
        #[uniffi::remote(Enum)]
        $(#[$meta])*
        $vis enum $set {
            $($($f($f),)+)?
            $($q($q),)+
        }

        #[uniffi::remote(Enum)]
        $vis enum $response {
            $($($f(<$f as Query>::Response),)+)?
            $($q(<$q as Query>::Response),)+
        }
    };
}

gv_core::core_questions!(ffi_question_records);
gv_ui::ui_questions!(ffi_question_records);
gv_core::domain_query_set!(ffi_query_set);
gv_client::client_query_set!(ffi_query_set);

// --- Live queries (gv-client) ---
//
// The live envelope around `ClientQuery`/`ClientQueryResponse`: what comes back per subscription
// (`LiveValue`), delivered in `LiveBatch`es. Nothing here says how a query is answered; that is
// declared in `gv_client::queries`.

#[uniffi::remote(Enum)]
pub enum ChildSlot {
    Drop {
        id: String,
        position: Position,
        pred_id: Option<Uuid>,
        succ_id: Option<Uuid>,
    },
    Entry {
        entry: Entry,
    },
}

#[uniffi::remote(Enum)]
pub enum LiveValue {
    Answer(ClientQueryResponse),
    Failed { message: String },
}

#[uniffi::remote(Record)]
pub struct LiveUpdate {
    pub subscription_id: u64,
    pub value: LiveValue,
}

#[uniffi::remote(Record)]
pub struct LiveBatch {
    pub version: u64,
    pub updates: Vec<LiveUpdate>,
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
    pub value: Option<AttributeValue>,
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
    SetDefaultUnit(MassUnit),
}

#[uniffi::remote(Enum)]
pub enum LengthChange {
    SetDefaultUnit(LengthUnit),
}

#[uniffi::remote(Enum)]
pub enum TextChange {
    SetDefault(Option<String>),
    SetAutocomplete(bool),
}

#[uniffi::remote(Enum)]
pub enum AttributeChange {
    SetName(String),
    SetDescription(Option<String>),
    Numeric(NumericChange),
    Select(SelectChange),
    Mass(MassChange),
    Length(LengthChange),
    Text(TextChange),
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
    SetDisplayAsSets(bool),
}

#[uniffi::remote(Record)]
pub struct UpdateEntry {
    pub actor_id: Uuid,
    pub entry_id: Uuid,
    pub change: EntryChange,
}

#[uniffi::remote(Record)]
pub struct ConvertToSets {
    pub actor_id: Uuid,
    pub entry_id: Uuid,
    pub sequence_id: Uuid,
}

#[uniffi::remote(Record)]
pub struct DuplicateEntry {
    pub actor_id: Uuid,
    pub entry_id: Uuid,
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
    ConvertToSets(ConvertToSets),
    DuplicateEntry(DuplicateEntry),
}

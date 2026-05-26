use uuid::Uuid;

use crate::models::{
    activity::Activity,
    attribute::{Attribute, AttributeValue, MassUnit, Value},
    entry::{Entry, Position, Temporal},
    user::User,
};

#[derive(Debug, Clone)]
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

impl From<CreateUser> for Action {
    fn from(value: CreateUser) -> Self {
        Action::CreateUser(value)
    }
}

impl From<CreateActivity> for Action {
    fn from(value: CreateActivity) -> Self {
        Action::CreateActivity(value)
    }
}

impl From<CreateEntry> for Action {
    fn from(value: CreateEntry) -> Self {
        Action::CreateEntry(value)
    }
}

impl From<MoveEntry> for Action {
    fn from(value: MoveEntry) -> Self {
        Action::MoveEntry(value)
    }
}

impl From<DeleteEntryRecursive> for Action {
    fn from(value: DeleteEntryRecursive) -> Self {
        Action::DeleteEntryRecursive(value)
    }
}

impl From<CreateAttribute> for Action {
    fn from(value: CreateAttribute) -> Self {
        Action::CreateAttribute(value)
    }
}

impl From<CreateValue> for Action {
    fn from(value: CreateValue) -> Self {
        Action::CreateValue(value)
    }
}

impl From<AttachValue> for Action {
    fn from(value: AttachValue) -> Self {
        Action::AttachValue(value)
    }
}

impl From<DeleteAttributeValue> for Action {
    fn from(value: DeleteAttributeValue) -> Self {
        Action::DeleteAttributeValue(value)
    }
}

#[derive(Debug, Clone)]
pub struct CreateActivity {
    pub actor_id: Uuid,
    pub activity: Activity,
    pub template: Vec<Entry>,
}

impl From<Activity> for CreateActivity {
    fn from(activity: Activity) -> Self {
        CreateActivity {
            actor_id: activity.owner_id,
            activity: activity.clone(),
            template: vec![Entry {
                id: uuid::Uuid::new_v4(),
                owner_id: activity.owner_id,
                activity_id: Some(activity.id),
                name: None,
                position: None,
                is_template: true,
                display_as_sets: false,
                is_sequence: true,
                is_complete: false,
                temporal: Temporal::None,
            }],
        }
    }
}

#[derive(Debug, Clone)]
pub struct CreateUser {
    pub user: User,
}

impl From<User> for CreateUser {
    fn from(user: User) -> Self {
        CreateUser { user: user }
    }
}

#[derive(Debug, Clone)]
pub struct CreateEntry {
    pub actor_id: Uuid,
    pub entry: Entry,
}

impl From<Entry> for CreateEntry {
    fn from(entry: Entry) -> Self {
        CreateEntry {
            actor_id: entry.owner_id,
            entry: entry,
        }
    }
}

/// Instantiate an activity's template into a new subtree. The mutator finds the
/// activity's template root, deep-copies the subtree (entries + values) with
/// fresh ids, and places the instantiated root at `position` with `temporal`.
/// Structure (including `is_sequence`) comes from the template.
///
/// `is_template` sets the kind of the instantiated subtree and must match the
/// parent's kind: `false` materializes into the log; `true` composes the
/// activity into another template (instantiating under a template entry).
#[derive(Debug, Clone)]
pub struct CreateEntryFromActivity {
    pub actor_id: Uuid,
    pub activity_id: Uuid,
    pub position: Option<Position>,
    pub temporal: Temporal,
    pub is_template: bool,
}

impl From<CreateEntryFromActivity> for Action {
    fn from(value: CreateEntryFromActivity) -> Self {
        Action::CreateEntryFromActivity(value)
    }
}

#[derive(Debug, Clone)]
pub struct MoveEntry {
    pub actor_id: Uuid,
    pub entry_id: Uuid,
    pub position: Option<Position>,
    pub temporal: Temporal,
}

#[derive(Debug, Clone)]
pub struct DeleteEntryRecursive {
    pub actor_id: Uuid,
    pub entry_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct CreateAttribute {
    pub actor_id: Uuid,
    pub attribute: Attribute,
}

impl From<Attribute> for CreateAttribute {
    fn from(attribute: Attribute) -> Self {
        CreateAttribute {
            actor_id: attribute.owner_id,
            attribute,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CreateValue {
    pub actor_id: Uuid,
    pub value: Value,
}

/// Attach an attribute to an entry, seeding the value from the attribute's
/// config default (both plan and actual). A no-op if a value for
/// `(entry_id, attribute_id)` already exists. Unlike `CreateValue`, the caller
/// passes only identifiers; the mutator resolves the default in core.
#[derive(Debug, Clone)]
pub struct AttachValue {
    pub actor_id: Uuid,
    pub entry_id: Uuid,
    pub attribute_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct UpdateEntryCompletion {
    pub actor_id: Uuid,
    pub entry_id: Uuid,
    pub is_complete: bool,
}

impl From<UpdateEntryCompletion> for Action {
    fn from(value: UpdateEntryCompletion) -> Self {
        Action::UpdateEntryCompletion(value)
    }
}

#[derive(Debug, Clone)]
pub enum ValueField {
    Plan,
    Actual,
}

#[derive(Debug, Clone)]
pub struct UpdateAttributeValue {
    pub actor_id: Uuid,
    pub entry_id: Uuid,
    pub attribute_id: Uuid,
    pub field: ValueField,
    pub value: AttributeValue,
}

impl From<UpdateAttributeValue> for Action {
    fn from(value: UpdateAttributeValue) -> Self {
        Action::UpdateAttributeValue(value)
    }
}

#[derive(Debug, Clone)]
pub struct DeleteAttributeValue {
    pub actor_id: Uuid,
    pub entry_id: Uuid,
    pub attribute_id: Uuid,
}

/// Update an attribute's config or metadata. The `change` enum captures the
/// user's intent as a single edit; the mutator validates it against the
/// attribute's current type and (for type-specific edits) its config. Mirrors
/// the Numeric/Select/Mass grouping used by `AttributeConfig`/`AttributePair`.
#[derive(Debug, Clone)]
pub struct UpdateAttribute {
    pub actor_id: Uuid,
    pub attribute_id: Uuid,
    pub change: AttributeChange,
}

impl From<UpdateAttribute> for Action {
    fn from(value: UpdateAttribute) -> Self {
        Action::UpdateAttribute(value)
    }
}

#[derive(Debug, Clone)]
pub enum AttributeChange {
    // Common to all attribute types, freely editable.
    SetName(String),
    SetDescription(Option<String>),
    // Type-specific; the mutator rejects a variant whose type doesn't match the
    // attribute's config.
    Numeric(NumericChange),
    Select(SelectChange),
    Mass(MassChange),
}

#[derive(Debug, Clone)]
pub enum NumericChange {
    /// Set (or clear, with `None`) the default value. Must respect the config's
    /// `integer`/`min`/`max` constraints.
    SetDefault(Option<f64>),
    // Future additive edits: RaiseMax, LowerMin, SetInteger.
}

#[derive(Debug, Clone)]
pub enum SelectChange {
    /// Set (or clear, with `None`) the default. A non-`None` default must be one
    /// of the config's existing options.
    SetDefault(Option<String>),
    // Future additive edits: AddOption, RenameOption, SetOrdered.
}

#[derive(Debug, Clone)]
pub enum MassChange {
    /// Replace the default unit set. Not additive-constrained — changing units
    /// doesn't invalidate existing values — so add/remove are both allowed.
    SetDefaultUnits(Vec<MassUnit>),
}

/// Update an entry's structural/metadata fields. Deliberately excludes
/// `position` and `temporal` — those are owned by `MoveEntry`, which enforces
/// their cycle/parent/temporal constraints atomically. Completion has its own
/// action (`UpdateEntryCompletion`) for now.
#[derive(Debug, Clone)]
pub struct UpdateEntry {
    pub actor_id: Uuid,
    pub entry_id: Uuid,
    pub change: EntryChange,
}

impl From<UpdateEntry> for Action {
    fn from(value: UpdateEntry) -> Self {
        Action::UpdateEntry(value)
    }
}

#[derive(Debug, Clone)]
pub enum EntryChange {
    /// Toggle sequence/scalar. Becoming a scalar deletes all descendants (a
    /// scalar cannot contain children).
    SetIsSequence(bool),
    // Future: SetName(Option<String>), SetDisplayAsSets(bool), completion.
}

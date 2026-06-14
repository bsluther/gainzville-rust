use uuid::Uuid;

use crate::models::{
    activity::Activity,
    attribute::{Attribute, AttributeValue, LengthUnit, MassUnit, Value},
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
    ConvertToSets(ConvertToSets),
    DuplicateEntry(DuplicateEntry),
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
/// NOTE: this API cannot express actor A creating an entry for actor B, eg a coach creating an
/// entry for a client. That will need to be corrected, but deferring as the design for
/// collaboration is nascent.
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

/// Convert an entry into a sets sequence: insert an anonymous sequence at the
/// entry's position and reparent the entry under it as the sole member (the
/// first "set"). The sequence takes the entry's start/end — it owns the
/// timeline slot — while the entry keeps only its duration. `sequence_id` is
/// client-supplied (like `CreateEntry`'s client-built entry) so the caller can
/// reference the new sequence before the mutation lands, e.g. to carry UI
/// state across the swap.
#[derive(Debug, Clone)]
pub struct ConvertToSets {
    pub actor_id: Uuid,
    pub entry_id: Uuid,
    pub sequence_id: Uuid,
}

impl From<ConvertToSets> for Action {
    fn from(value: ConvertToSets) -> Self {
        Action::ConvertToSets(value)
    }
}

/// Duplicate an entry's subtree in place: an exact copy (attributes, values,
/// temporal, completion) with fresh entry ids, inserted immediately after the
/// source among its siblings. A forest root duplicates as another root with
/// the same temporal, landing adjacent in the day view.
#[derive(Debug, Clone)]
pub struct DuplicateEntry {
    pub actor_id: Uuid,
    pub entry_id: Uuid,
}

impl From<DuplicateEntry> for Action {
    fn from(value: DuplicateEntry) -> Self {
        Action::DuplicateEntry(value)
    }
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
    /// `None` clears the field (sets `plan`/`actual` to `None`) while leaving
    /// the value attached. `Some` sets it.
    pub value: Option<AttributeValue>,
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
    Length(LengthChange),
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
    /// Replace the default unit. Not additive-constrained — stored values
    /// carry their own unit, so changing the default invalidates nothing.
    SetDefaultUnit(MassUnit),
}

#[derive(Debug, Clone)]
pub enum LengthChange {
    /// Replace the default unit. Not additive-constrained — stored values
    /// carry their own unit, so changing the default invalidates nothing.
    SetDefaultUnit(LengthUnit),
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
    /// scalar cannot contain children). Rejected while `display_as_sets` is
    /// set (break out of sets first).
    SetIsSequence(bool),
    /// Toggle the sets presentation. Setting it requires the sets shape: a
    /// sequence with at least one member, all members instances of one
    /// activity (or all anonymous). Clearing it ("breaking out") is always
    /// legal and, when the sequence has no name, also names it
    /// "<first member's display name> Sets" so the broken-out card stays
    /// recognizable instead of rendering as "Unnamed".
    SetDisplayAsSets(bool),
    // Future: SetName(Option<String>), completion.
}

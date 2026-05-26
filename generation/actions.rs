use chrono::{DateTime, Utc};
use rand::Rng;
use uuid::Uuid;

use crate::{Arbitrary, ArbitraryFrom, GenerationContext, gen_random_text, maybe, pick};
use gv_core::{
    actions::{
        Action, AttachValue, AttributeChange, CreateActivity, CreateAttribute, CreateEntry,
        CreateEntryFromActivity, CreateUser, CreateValue, DeleteAttributeValue, EntryChange,
        MassChange, MoveEntry, NumericChange, SelectChange, UpdateAttribute, UpdateEntry,
        UpdateEntryCompletion,
    },
    models::{
        activity::Activity,
        attribute::{Attribute, AttributeConfig, MassUnit, Value},
        entry::{Entry, Position, Temporal},
        user::User,
    },
    validation::{Email, Username},
};

impl ArbitraryFrom<(&[Uuid], &[Activity], &[Entry], &[Attribute])> for Action {
    fn arbitrary_from<R: Rng, C: GenerationContext>(
        rng: &mut R,
        context: &C,
        (actor_ids, activities, entries, attributes): (
            &[Uuid],
            &[Activity],
            &[Entry],
            &[Attribute],
        ),
    ) -> Self {
        // Actions that are always available: CreateUser, CreateActivity, CreateEntry, CreateAttribute
        // Actions that require non-empty entries: MoveEntry, UpdateEntryCompletion
        // Actions that require non-empty entries and attributes: CreateValue
        // Actions requiring entries + attributes that share at least one owner:
        // CreateValue, AttachValue, DeleteAttributeValue.
        let mut choices: Vec<u8> = vec![0, 1, 2, 3];
        if !entries.is_empty() {
            choices.push(4);
            choices.push(6);
            let owners_overlap = entries
                .iter()
                .any(|e| attributes.iter().any(|a| a.owner_id == e.owner_id));
            if owners_overlap {
                choices.push(5);
                choices.push(7);
                choices.push(8);
            }
        }
        if !attributes.is_empty() {
            choices.push(9);
        }
        if !entries.is_empty() {
            choices.push(10);
        }
        if !activities.is_empty() {
            choices.push(11);
        }
        let choice = pick(&choices, rng).unwrap();
        match choice {
            0 => CreateUser::arbitrary(rng, context).into(),
            1 => CreateActivity::arbitrary_from(rng, context, actor_ids).into(),
            2 => CreateEntry::arbitrary_from(rng, context, (actor_ids, activities, entries)).into(),
            3 => CreateAttribute::arbitrary_from(rng, context, actor_ids).into(),
            4 => MoveEntry::arbitrary_from(rng, context, entries).into(),
            5 => CreateValue::arbitrary_from(rng, context, (entries, attributes)).into(),
            6 => UpdateEntryCompletion::arbitrary_from(rng, context, entries).into(),
            7 => AttachValue::arbitrary_from(rng, context, (entries, attributes)).into(),
            8 => DeleteAttributeValue::arbitrary_from(rng, context, (entries, attributes)).into(),
            9 => UpdateAttribute::arbitrary_from(rng, context, attributes).into(),
            10 => UpdateEntry::arbitrary_from(rng, context, entries).into(),
            11 => CreateEntryFromActivity::arbitrary_from(rng, context, activities).into(),
            _ => unreachable!(),
        }
    }
}

impl Arbitrary for CreateUser {
    fn arbitrary<R: rand::Rng, C: crate::GenerationContext>(rng: &mut R, context: &C) -> Self {
        User {
            actor_id: Uuid::arbitrary(rng, context),
            email: Email::arbitrary(rng, context),
            username: Username::arbitrary(rng, context),
        }
        .into()
    }
}

impl ArbitraryFrom<&[Uuid]> for CreateActivity {
    /// Generate an arbitrary activity owned by one of the provided uuids.
    fn arbitrary_from<R: rand::Rng, C: super::GenerationContext>(
        rng: &mut R,
        context: &C,
        actor_ids: &[Uuid],
    ) -> Self {
        Activity::arbitrary_from(rng, context, actor_ids).into()
    }
}

impl ArbitraryFrom<(&[Uuid], &[Activity], &[Entry])> for CreateEntry {
    fn arbitrary_from<R: Rng, C: GenerationContext>(
        rng: &mut R,
        context: &C,
        (actor_ids, activities, entries): (&[Uuid], &[Activity], &[Entry]),
    ) -> Self {
        Entry::arbitrary_from(rng, context, (actor_ids, activities, entries)).into()
    }
}

/// Provided entries must be non-empty.
impl ArbitraryFrom<&[Entry]> for MoveEntry {
    fn arbitrary_from<R: Rng, C: GenerationContext>(
        rng: &mut R,
        context: &C,
        entries: &[Entry],
    ) -> Self {
        // Choose an entry to move.
        // Perhaps I should return an option so it's safe to call this with no entries.
        // - Another idea is to have a No-Op action.
        let entry = pick(entries, rng).expect("entres must be non-empty");

        MoveEntry {
            // For now, choose the owner of the moving entry as the actor.
            actor_id: entry.owner_id,
            entry_id: entry.id,
            // Some of these position/temporal combinations will be invalid, the mutator should catch
            // it.
            position: Option::<Position>::arbitrary_from(rng, context, entries),
            temporal: Temporal::arbitrary(rng, context),
        }
    }
}

impl ArbitraryFrom<&[Uuid]> for CreateAttribute {
    fn arbitrary_from<R: Rng, C: GenerationContext>(
        rng: &mut R,
        context: &C,
        actor_ids: &[Uuid],
    ) -> Self {
        Attribute::arbitrary_from(rng, context, actor_ids).into()
    }
}

/// Provided entries must be non-empty.
impl ArbitraryFrom<&[Entry]> for UpdateEntryCompletion {
    fn arbitrary_from<R: Rng, C: GenerationContext>(
        rng: &mut R,
        _context: &C,
        entries: &[Entry],
    ) -> Self {
        let entry = pick(entries, rng).expect("entries must be non-empty");
        UpdateEntryCompletion {
            actor_id: entry.owner_id,
            entry_id: entry.id,
            is_complete: rng.random_bool(0.5),
        }
    }
}

impl ArbitraryFrom<(&[Entry], &[Attribute])> for CreateValue {
    fn arbitrary_from<R: Rng, C: GenerationContext>(
        rng: &mut R,
        context: &C,
        (entries, attributes): (&[Entry], &[Attribute]),
    ) -> Self {
        let entry = pick(entries, rng).expect("entries must not be empty");
        let value = Value::arbitrary_from(rng, context, (entries, attributes));
        CreateValue {
            actor_id: entry.owner_id,
            value,
        }
    }
}

/// Pick an (entry, attribute) pair whose owners match, mirroring the constraint
/// the attach/detach mutators enforce. Panics if no owned attribute exists for
/// the picked entry — callers gate on owner overlap before generating.
fn pick_owned_pair<'a, R: Rng>(
    rng: &mut R,
    entries: &'a [Entry],
    attributes: &'a [Attribute],
) -> (&'a Entry, &'a Attribute) {
    let entry = pick(entries, rng).expect("entries must not be empty");
    let owned_attrs: Vec<&Attribute> = attributes
        .iter()
        .filter(|a| a.owner_id == entry.owner_id)
        .collect();
    let attribute =
        pick(&owned_attrs[..], rng).expect("no attribute matches the picked entry's owner");
    (entry, attribute)
}

impl ArbitraryFrom<(&[Entry], &[Attribute])> for AttachValue {
    fn arbitrary_from<R: Rng, C: GenerationContext>(
        rng: &mut R,
        _context: &C,
        (entries, attributes): (&[Entry], &[Attribute]),
    ) -> Self {
        let (entry, attribute) = pick_owned_pair(rng, entries, attributes);
        AttachValue {
            actor_id: entry.owner_id,
            entry_id: entry.id,
            attribute_id: attribute.id,
        }
    }
}

impl ArbitraryFrom<(&[Entry], &[Attribute])> for DeleteAttributeValue {
    fn arbitrary_from<R: Rng, C: GenerationContext>(
        rng: &mut R,
        _context: &C,
        (entries, attributes): (&[Entry], &[Attribute]),
    ) -> Self {
        let (entry, attribute) = pick_owned_pair(rng, entries, attributes);
        DeleteAttributeValue {
            actor_id: entry.owner_id,
            entry_id: entry.id,
            attribute_id: attribute.id,
        }
    }
}

impl ArbitraryFrom<&[Activity]> for CreateEntryFromActivity {
    fn arbitrary_from<R: Rng, C: GenerationContext>(
        rng: &mut R,
        context: &C,
        activities: &[Activity],
    ) -> Self {
        let activity = pick(activities, rng).expect("activities must not be empty");
        // Instantiate into the log at a day root; a Start temporal satisfies the
        // root rule.
        CreateEntryFromActivity {
            actor_id: activity.owner_id,
            activity_id: activity.id,
            position: None,
            temporal: Temporal::Start {
                start: DateTime::<Utc>::arbitrary(rng, context),
            },
            is_template: false,
        }
    }
}

impl ArbitraryFrom<&[Entry]> for UpdateEntry {
    fn arbitrary_from<R: Rng, C: GenerationContext>(
        rng: &mut R,
        _context: &C,
        entries: &[Entry],
    ) -> Self {
        let entry = pick(entries, rng).expect("entries must not be empty");
        UpdateEntry {
            actor_id: entry.owner_id,
            entry_id: entry.id,
            change: EntryChange::SetIsSequence(rng.random_bool(0.5)),
        }
    }
}

impl ArbitraryFrom<&[Attribute]> for UpdateAttribute {
    fn arbitrary_from<R: Rng, C: GenerationContext>(
        rng: &mut R,
        _context: &C,
        attributes: &[Attribute],
    ) -> Self {
        let attribute = pick(attributes, rng).expect("attributes must not be empty");
        // Common edits are always valid; type-specific edits generate values
        // that satisfy the config so the mutator accepts them.
        let change = match rng.random_range(0..3) {
            0 => AttributeChange::SetName(gen_random_text(rng, 1..3)),
            1 => AttributeChange::SetDescription(maybe(rng, 0.5, |rng| gen_random_text(rng, 2..6))),
            _ => match &attribute.config {
                AttributeConfig::Numeric(cfg) => {
                    let default = maybe(rng, 0.7, |rng| {
                        let lo = cfg.min.unwrap_or(0.0);
                        let hi = cfg.max.unwrap_or(lo + 100.0);
                        let v = if hi > lo {
                            rng.random_range(lo..=hi)
                        } else {
                            lo
                        };
                        if cfg.integer { v.round() } else { v }
                    });
                    AttributeChange::Numeric(NumericChange::SetDefault(default))
                }
                AttributeConfig::Select(cfg) => {
                    let default = if cfg.options.is_empty() {
                        None
                    } else {
                        maybe(rng, 0.7, |rng| pick(&cfg.options[..], rng).unwrap().clone())
                    };
                    AttributeChange::Select(SelectChange::SetDefault(default))
                }
                AttributeConfig::Mass(_) => {
                    let all = [MassUnit::Gram, MassUnit::Kilogram, MassUnit::Pound];
                    let units: Vec<MassUnit> = all
                        .iter()
                        .filter(|_| rng.random_bool(0.5))
                        .cloned()
                        .collect();
                    AttributeChange::Mass(MassChange::SetDefaultUnits(units))
                }
            },
        };
        UpdateAttribute {
            actor_id: attribute.owner_id,
            attribute_id: attribute.id,
            change,
        }
    }
}

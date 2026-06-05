use chrono::{DateTime, Utc};
use rand::RngExt;
use rand::seq::IteratorRandom;
use uuid::Uuid;

use crate::{Arbitrary, GenerationContext, arbitrary_actor_id, gen_random_text, maybe, pick};
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

/// Choose an arbitrary action uniformly by delegating to the each action's arbitrary impl. The
/// impls handle an anemic model where a valid action cannot be created by fabricating invalid
/// actions, which the core logic should catch.
/// TODO: add missing actions.
impl Arbitrary for Action {
    fn arbitrary<R: RngExt, C: GenerationContext>(rng: &mut R, context: &C) -> Self {
        let choice = rng.random_range(0..=11);
        match choice {
            0 => CreateUser::arbitrary(rng, context).into(),
            1 => CreateActivity::arbitrary(rng, context).into(),
            2 => CreateEntry::arbitrary(rng, context).into(),
            3 => CreateAttribute::arbitrary(rng, context).into(),
            4 => MoveEntry::arbitrary(rng, context).into(),
            5 => CreateValue::arbitrary(rng, context).into(),
            6 => UpdateEntryCompletion::arbitrary(rng, context).into(),
            7 => AttachValue::arbitrary(rng, context).into(),
            8 => DeleteAttributeValue::arbitrary(rng, context).into(),
            9 => UpdateAttribute::arbitrary(rng, context).into(),
            10 => UpdateEntry::arbitrary(rng, context).into(),
            11 => CreateEntryFromActivity::arbitrary(rng, context).into(),
            _ => unreachable!(),
        }
    }
}

impl Arbitrary for CreateUser {
    fn arbitrary<R: rand::RngExt, C: crate::GenerationContext>(rng: &mut R, context: &C) -> Self {
        User {
            actor_id: Uuid::arbitrary(rng, context),
            email: Email::arbitrary(rng, context),
            username: Username::arbitrary(rng, context),
        }
        .into()
    }
}

impl Arbitrary for CreateActivity {
    fn arbitrary<R: rand::RngExt, C: super::GenerationContext>(rng: &mut R, context: &C) -> Self {
        Activity::arbitrary(rng, context).into()
    }
}

impl Arbitrary for CreateEntry {
    fn arbitrary<R: RngExt, C: GenerationContext>(rng: &mut R, context: &C) -> Self {
        Entry::arbitrary(rng, context).into()
    }
}

impl Arbitrary for MoveEntry {
    fn arbitrary<R: RngExt, C: GenerationContext>(rng: &mut R, context: &C) -> Self {
        // The actor is the owner of the moving entry.
        let (entry_id, actor_id) = arbitrary_entry_target(rng, context);
        MoveEntry {
            actor_id,
            entry_id,
            // Some of these position/temporal combinations will be invalid; the
            // mutator should catch it.
            position: Option::<Position>::arbitrary(rng, context),
            temporal: Temporal::arbitrary(rng, context),
        }
    }
}

impl Arbitrary for CreateAttribute {
    fn arbitrary<R: RngExt, C: GenerationContext>(rng: &mut R, context: &C) -> Self {
        Attribute::arbitrary(rng, context).into()
    }
}

impl Arbitrary for UpdateEntryCompletion {
    fn arbitrary<R: RngExt, C: GenerationContext>(rng: &mut R, context: &C) -> Self {
        let (entry_id, actor_id) = arbitrary_entry_target(rng, context);
        UpdateEntryCompletion {
            actor_id,
            entry_id,
            is_complete: rng.random_bool(0.5),
        }
    }
}

impl Arbitrary for CreateValue {
    fn arbitrary<R: RngExt, C: GenerationContext>(rng: &mut R, context: &C) -> Self {
        let value = Value::arbitrary(rng, context);
        // The actor must own the entry the value attaches to; recover it from the
        // model. A fabricated entry (empty model) has no real owner, so fall back
        // to a fabricated actor_id.
        let actor_id = context
            .model()
            .entry(value.entry_id)
            .map(|e| e.owner_id)
            .unwrap_or_else(|| Uuid::arbitrary(rng, context));
        CreateValue { actor_id, value }
    }
}

/// An `(entry_id, actor_id)` target for entry-scoped actions: a real entry if one
/// exists, otherwise a fabricated one (whose owner is a real actor when the model
/// has any). The actor is the entry's owner, matching the permission the mutators
/// enforce.
fn arbitrary_entry_target<R: RngExt, C: GenerationContext>(
    rng: &mut R,
    context: &C,
) -> (Uuid, Uuid) {
    match context.model().entries().choose(rng) {
        Some(e) => (e.id, e.owner_id),
        None => {
            // Entry::arbitrary already picks a real owner when actors exist.
            let e = Entry::arbitrary(rng, context);
            (e.id, e.owner_id)
        }
    }
}

/// Generate an owner-matched `(actor_id, entry_id, attribute_id)` for the
/// attach/detach actions — `actor_id` is the shared owner the mutators require.
/// Generates as real as possible. The (entries × attributes) input space:
///
/// 1. Neither: fabricate both, under a real actor when one exists.
/// 2. Entries only: keep a real entry, fabricate the attribute under its owner.
/// 3. Attributes only: keep a real attribute, fabricate the entry under its owner.
/// 4. Both, but no owner has both: keep one real side (chosen at random so both
///    symmetric variants occur), fabricate the other under that owner.
/// 5. Both, and some owner has both: a fully real owner-matched pair.
///
/// Fabricated sides are just fresh ids — the owner constraint is carried by
/// `actor_id` and the real side, so there's no entity to materialize.
fn arbitrary_owned_pair<R: RngExt, C: GenerationContext>(
    rng: &mut R,
    context: &C,
) -> (Uuid, Uuid, Uuid) {
    let model = context.model();

    // Case 5: an owner that has both a real entry and a real attribute.
    let eligible: Vec<&Entry> = model
        .entries()
        .filter(|e| model.attributes().any(|a| a.owner_id == e.owner_id))
        .collect();
    if let Some(entry) = pick(&eligible, rng) {
        let attribute = model
            .attributes()
            .filter(|a| a.owner_id == entry.owner_id)
            .choose(rng)
            .expect("eligible entry must have a matching attribute");
        return (entry.owner_id, entry.id, attribute.id);
    }

    // No shared-owner pair exists (cases 1–4). Keep whatever real side we can and
    // fabricate the other under its owner.
    let keep_entry = match (
        model.entries().next().is_some(),
        model.attributes().next().is_some(),
    ) {
        (true, false) => true,                // case 2: only entries
        (false, true) => false,               // case 3: only attributes
        (true, true) => rng.random_bool(0.5), // case 4: disjoint owners — random side
        (false, false) => {
            // Case 1: nothing real — fabricate both under a real actor if any.
            return (
                arbitrary_actor_id(rng, context),
                Uuid::arbitrary(rng, context),
                Uuid::arbitrary(rng, context),
            );
        }
    };

    if keep_entry {
        let entry = model.entries().choose(rng).expect("entries non-empty");
        (entry.owner_id, entry.id, Uuid::arbitrary(rng, context))
    } else {
        let attribute = model
            .attributes()
            .choose(rng)
            .expect("attributes non-empty");
        (
            attribute.owner_id,
            Uuid::arbitrary(rng, context),
            attribute.id,
        )
    }
}

impl Arbitrary for AttachValue {
    fn arbitrary<R: RngExt, C: GenerationContext>(rng: &mut R, context: &C) -> Self {
        let (actor_id, entry_id, attribute_id) = arbitrary_owned_pair(rng, context);
        AttachValue {
            actor_id,
            entry_id,
            attribute_id,
        }
    }
}

impl Arbitrary for DeleteAttributeValue {
    fn arbitrary<R: RngExt, C: GenerationContext>(rng: &mut R, context: &C) -> Self {
        let (actor_id, entry_id, attribute_id) = arbitrary_owned_pair(rng, context);
        DeleteAttributeValue {
            actor_id,
            entry_id,
            attribute_id,
        }
    }
}

impl Arbitrary for CreateEntryFromActivity {
    fn arbitrary<R: RngExt, C: GenerationContext>(rng: &mut R, context: &C) -> Self {
        let (actor_id, activity_id) = context
            .model()
            .activities()
            .choose(rng)
            .map(|a| (a.owner_id, a.id))
            // Fabricate the activity id under a real actor when one exists.
            .unwrap_or_else(|| {
                (
                    arbitrary_actor_id(rng, context),
                    Uuid::arbitrary(rng, context),
                )
            });
        // Instantiate into the log at a day root; a Start temporal satisfies the root rule.
        // TODO: pick an arbitrary position.
        CreateEntryFromActivity {
            actor_id,
            activity_id,
            position: None,
            temporal: Temporal::Start {
                start: DateTime::<Utc>::arbitrary(rng, context),
            },
            is_template: false,
        }
    }
}

impl Arbitrary for UpdateEntry {
    fn arbitrary<R: RngExt, C: GenerationContext>(rng: &mut R, context: &C) -> Self {
        let (entry_id, actor_id) = arbitrary_entry_target(rng, context);
        UpdateEntry {
            actor_id,
            entry_id,
            change: EntryChange::SetIsSequence(rng.random_bool(0.5)),
        }
    }
}

impl Arbitrary for UpdateAttribute {
    fn arbitrary<R: RngExt, C: GenerationContext>(rng: &mut R, context: &C) -> Self {
        let attribute = context
            .model()
            .attributes()
            .choose(rng)
            .cloned()
            .unwrap_or_else(|| Attribute::arbitrary(rng, context));
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

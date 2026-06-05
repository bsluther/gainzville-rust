use anyhow::Result;
use chrono::{DateTime, Utc};
use dbsp::{
    IndexedZSetReader, OrdZSet, OutputHandle, RootCircuit, ZSet, ZSetHandle, ZWeight, utils::Tup2,
};
use generation::{Arbitrary, SimulationContext};
use gv_core::{
    SYSTEM_ACTOR_ID,
    models::{
        activity::Activity,
        actor::{Actor, ActorKind},
        entry::Entry,
    },
    queries::Snapshot,
};
use ivm::types::{Id, IvmEntry};
use rand::SeedableRng;
use rand::rngs::ChaCha8Rng;
use uuid::Uuid;

fn build_circuit(
    circuit: &mut RootCircuit,
) -> Result<(ZSetHandle<IvmEntry>, OutputHandle<OrdZSet<IvmEntry>>)> {
    let (entry_stream, input_handle) = circuit.add_input_zset::<IvmEntry>();

    entry_stream.inspect(|records| {
        println!("{}", records.weighted_count());
    });

    let subset = entry_stream.filter(|e| e.owner_id == Id(SYSTEM_ACTOR_ID));

    Ok((input_handle, subset.output()))
}

/// A snapshot holding just actors and activities — the model state generation
/// needs to draw owner-matched entries from.
fn world(actors: Vec<Actor>, activities: Vec<Activity>) -> Snapshot {
    Snapshot {
        users: vec![],
        actors,
        activities,
        attributes: vec![],
        entries: vec![],
        values: vec![],
    }
}

/// Build a small model (a couple of actors and some activities) and generate
/// arbitrary entries against it. `Activity::arbitrary` draws owners from the
/// model's actors and `Entry::arbitrary` draws from its activities, so we seed
/// the model in two passes via `load_snapshot`.
fn generate_entries(rng: &mut ChaCha8Rng) -> Vec<IvmEntry> {
    let mut context = SimulationContext::default();
    let created_at = "2026-01-01T12:00:00Z".parse::<DateTime<Utc>>().unwrap();

    // Two owners so the SYSTEM filter has a mix to work with.
    let actors = vec![
        Actor {
            actor_id: SYSTEM_ACTOR_ID,
            actor_kind: ActorKind::System,
            created_at,
        },
        Actor {
            actor_id: Uuid::arbitrary(rng, &context),
            actor_kind: ActorKind::User,
            created_at,
        },
    ];
    context.load_snapshot(world(actors.clone(), vec![]));

    let activities = (0..5)
        .map(|_| Activity::arbitrary(rng, &context))
        .collect::<Vec<_>>();
    context.load_snapshot(world(actors, activities));

    (0..10)
        .map(|_| Entry::arbitrary(rng, &context).into())
        .collect()
}

pub fn main() -> Result<()> {
    // Build circuit.
    let (circuit, (input_handle, output_handle)) = RootCircuit::build(build_circuit)?;

    // Generation setup.
    let seed = 1337u64;
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let entries = generate_entries(&mut rng);

    // Update incrementally.
    for entry in entries {
        println!("Inputing {:?}", entry);
        let mut container = vec![Tup2(entry, 1)];
        input_handle.append(&mut container);
        circuit.transaction()?;
        output_handle.consolidate().iter().for_each(|(e, _, w)| {
            println!("id={:?}, owner_id={:?}:{w:+}", e.id, e.owner_id);
        });
        println!();
    }

    Ok(())
}

fn _basic_main() -> Result<()> {
    // Generation setup.
    let seed = 1337u64;
    let mut rng = ChaCha8Rng::seed_from_u64(seed);

    // Build circuit.
    let (circuit, (input_handle, output_handle)) = RootCircuit::build(build_circuit)?;

    let entries = generate_entries(&mut rng);
    let mut input_records = entries
        .into_iter()
        .map(|e| Tup2(e, 1))
        .collect::<Vec<Tup2<IvmEntry, ZWeight>>>();

    input_handle.append(&mut input_records);
    circuit.transaction()?;
    output_handle.consolidate().iter().for_each(|(e, _, w)| {
        println!("id={:?}, owner_id={:?}:{w:+}", e.id, e.owner_id);
    });

    Ok(())
}

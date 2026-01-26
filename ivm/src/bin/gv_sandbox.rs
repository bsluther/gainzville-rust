use anyhow::Result;
use dbsp::{
    IndexedZSetReader, OrdZSet, OutputHandle, RootCircuit, ZSet, ZSetHandle, ZWeight, utils::Tup2,
};
use generation::{Arbitrary, ArbitraryFrom, SimulationContext};
use gv_core::{
    SYSTEM_ACTOR_ID,
    models::{activity::Activity, entry::Entry},
};
use ivm::types::{Id, IvmEntry};
use rand_chacha::{ChaCha8Rng, rand_core::SeedableRng};
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

pub fn main() -> Result<()> {
    // Build circuit.
    let (circuit, (input_handle, output_handle)) = RootCircuit::build(build_circuit)?;

    // Generation setup.
    let seed = 1337u64;
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let context = SimulationContext {};
    let actor_ids = vec![SYSTEM_ACTOR_ID, Uuid::arbitrary(&mut rng, &context)];
    let activities = (0..5)
        .map(|_| Activity::arbitrary_from(&mut rng, &context, &actor_ids))
        .collect::<Vec<_>>();
    let base_entries: Vec<Entry> = Vec::new();
    let entries = (0..10)
        .map(|_| {
            Entry::arbitrary_from(&mut rng, &context, (&actor_ids, &activities, &base_entries))
                .into()
        })
        .collect::<Vec<IvmEntry>>();

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

    // let mut input_records = entries
    //     .map(|e| Tup2(e.into(), 1))
    //     .collect::<Vec<Tup2<IvmEntry, ZWeight>>>();

    // input_handle.append(&mut input_records);
    // println!("{}", output_handle.consolidate().weighted_count());

    Ok(())
}

fn _basic_main() -> Result<()> {
    // Generation setup.
    let seed = 1337u64;
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let context = SimulationContext {};

    // Build circuit.
    let (circuit, (input_handle, output_handle)) = RootCircuit::build(build_circuit)?;

    let actor_ids = vec![SYSTEM_ACTOR_ID, Uuid::arbitrary(&mut rng, &context)];
    let activities = (0..5)
        .map(|_| Activity::arbitrary_from(&mut rng, &context, &actor_ids))
        .collect::<Vec<_>>();
    let base_entries: Vec<Entry> = Vec::new();
    let entries = (0..10).map(|_| {
        Entry::arbitrary_from(&mut rng, &context, (&actor_ids, &activities, &base_entries))
    });
    let mut input_records = entries
        .map(|e| Tup2(e.into(), 1))
        .collect::<Vec<Tup2<IvmEntry, ZWeight>>>();

    input_handle.append(&mut input_records);
    circuit.transaction()?;
    output_handle.consolidate().iter().for_each(|(e, _, w)| {
        println!("id={:?}, owner_id={:?}:{w:+}", e.id, e.owner_id);
    });
    // println!("{}", output_handle.consolidate().weighted_count());

    Ok(())
}

use anyhow::Result;
use dbsp::{
    Circuit, IndexedZSetReader, OrdZSet, RootCircuit, Stream,
    operator::Generator,
    utils::{Tup3, Tup4},
    zset, zset_set,
};

fn main() -> Result<()> {
    const STEPS: usize = 2;

    let (circuit_handle, output_handle) = RootCircuit::build(move |root_circuit| {
        let mut edges_data = ([
            zset_set! {
                Tup3(0_usize, 1_usize, 1_usize),
                Tup3(1, 2, 1),
                Tup3(2, 3, 2),
                Tup3(3, 4, 2)
            },
            zset! { Tup3(1, 2, 1) => -1 },
        ] as [_; STEPS])
            .into_iter();

        let edges = root_circuit.add_source(Generator::new(move || edges_data.next().unwrap()));

        // Create a base stream with all paths of length 1.
        let len_1 = edges.map(|Tup3(from, to, weight)| Tup4(*from, *to, *weight, 1));

        let closure = root_circuit.recursive(
            |child_circuit, len_n_minus_1: Stream<_, OrdZSet<Tup4<usize, usize, usize, usize>>>| {
                // Import the `edges` and `len_1` stream from the parent circuit through the
                // `delta0` operator.
                let edges = edges.delta0(child_circuit);
                let len_1 = len_1.delta0(child_circuit);

                // Perform an iterative step (n-1 to n) through joining the paths of length n-1
                // with the edges.
                let len_n = len_n_minus_1
                    .map_index(|Tup4(start, end, cum_weight, hopcnt)| {
                        (*end, Tup4(*start, *end, *cum_weight, *hopcnt))
                    })
                    .join(
                        &edges
                            .map_index(|Tup3(from, to, weight)| (*from, Tup3(*from, *to, *weight))),
                        |_end_from,
                         Tup4(start, _end, cum_weight, hopcnt),
                         Tup3(_from, to, weight)| {
                            Tup4(*start, *to, cum_weight + weight, hopcnt + 1)
                        },
                    )
                    .plus(&len_1);

                Ok(len_n)
            },
        )?;

        let mut expected_outputs = ([
            // We expect the full transitive closure in the first step.
            zset! {
                Tup4(0, 1, 1, 1) => 1,
                Tup4(0, 2, 2, 2) => 1,
                Tup4(0, 3, 4, 3) => 1,
                Tup4(0, 4, 6, 4) => 1,
                Tup4(1, 2, 1, 1) => 1,
                Tup4(1, 3, 3, 2) => 1,
                Tup4(1, 4, 5, 3) => 1,
                Tup4(2, 3, 2, 1) => 1,
                Tup4(2, 4, 4, 2) => 1,
                Tup4(3, 4, 2, 1) => 1,
            },
            // These paths are removed in the second step.
            zset! {
                Tup4(0, 2, 2, 2) => -1,
                Tup4(0, 3, 4, 3) => -1,
                Tup4(0, 4, 6, 4) => -1,
                Tup4(1, 2, 1, 1) => -1,
                Tup4(1, 3, 3, 2) => -1,
                Tup4(1, 4, 5, 3) => -1,
            },
        ] as [_; STEPS])
            .into_iter();

        closure.inspect(move |output| assert_eq!(*output, expected_outputs.next().unwrap()));

        Ok(closure.output())
    })?;

    for i in 0..STEPS {
        let iteration = i + 1;
        println!("Iteration {} starts...", iteration);
        circuit_handle.transaction()?;
        output_handle.consolidate().iter().for_each(
            |(Tup4(start, end, cum_weight, hopcnt), _, z_weight)| {
                println!(
                    "{start} -> {end} (cum weight: {cum_weight}, hops: {hopcnt}) => {z_weight}"
                );
            },
        );
        println!("Iteration {} finished.", iteration);
    }

    Ok(())
}

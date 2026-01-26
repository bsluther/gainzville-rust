use std::u32;

use anyhow::Result;
use chrono::{Datelike, NaiveDate};
use csv::Reader;
use dbsp::{
    IndexedZSetReader, OrdIndexedZSet, OutputHandle, RootCircuit, ZSetHandle, ZWeight,
    operator::time_series::{RelOffset, RelRange},
    utils::{Tup2, Tup3},
};
use rkyv::{Archive, Serialize};
use size_of::SizeOf;

// SizeOf is the only one I know is required so far.
#[derive(
    Clone,
    Default,
    Debug,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    SizeOf,
    Archive,
    Serialize,
    rkyv::Deserialize,
    serde::Deserialize,
    feldera_macros::IsNone,
)]
#[archive_attr(derive(Ord, Eq, PartialEq, PartialOrd))]
struct Record {
    location: String,
    date: NaiveDate,
    daily_vaccinations: Option<u64>,
}

#[derive(
    Clone,
    Default,
    Debug,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    SizeOf,
    Archive,
    Serialize,
    rkyv::Deserialize,
    serde::Deserialize,
    feldera_macros::IsNone,
)]
#[archive_attr(derive(Ord, Eq, PartialEq, PartialOrd))]
struct VaxMonthly {
    count: u64,
    year: i32,
    month: u8,
}

fn build_circuit(
    circuit: &mut RootCircuit,
) -> Result<(
    ZSetHandle<Record>,
    OutputHandle<OrdIndexedZSet<String, VaxMonthly>>,
)> {
    let (vax_stream, vax_handle) = circuit.add_input_zset::<Record>();
    let (pop_stream, _pop_handle) = circuit.add_input_indexed_zset::<String, u64>();

    let subset = vax_stream.filter(|r| {
        r.location == "England"
            || r.location == "Northern Ireland"
            || r.location == "Scotland"
            || r.location == "Wales"
    });

    let monthly_totals = subset
        .map_index(|r| {
            (
                Tup3(r.location.clone(), r.date.year(), r.date.month() as u8),
                r.daily_vaccinations.unwrap_or(0),
            )
        })
        .aggregate_linear(|v| *v as ZWeight);

    let moving_averages = monthly_totals
        .map_index(|(Tup3(l, y, m), v)| (*y as u32 * 12 + (*m as u32 - 1), Tup2(l.clone(), *v)))
        .partitioned_rolling_average(
            |Tup2(l, v)| (l.clone(), *v),
            RelRange::new(RelOffset::Before(2), RelOffset::Before(0)),
        )
        .map_index(|(l, Tup2(date, avg))| {
            (
                Tup3(l.clone(), (date / 12) as i32, (date % 12 + 1) as u8),
                avg.unwrap(),
            )
        });

    let most_vax = monthly_totals
        .map_index(|(Tup3(l, y, m), sum)| {
            (
                l.clone(),
                VaxMonthly {
                    count: *sum as u64,
                    year: *y,
                    month: *m,
                },
            )
        })
        .topk_desc(3);

    let running_monthly_totals = monthly_totals
        .map_index(|(Tup3(l, y, m), v)| (*y as u32 * 12 + (*m as u32 - 1), Tup2(l.clone(), *v)))
        .partitioned_rolling_aggregate_linear(
            |Tup2(l, v)| (l.clone(), *v),
            |vaxxed| *vaxxed,
            |total| total,
            RelRange::new(RelOffset::Before(u32::MAX), RelOffset::Before(0)),
        );

    let _vax_rates = running_monthly_totals
        .map_index(|(l, Tup2(date, total))| {
            (
                l.clone(),
                Tup3((date / 12) as i32, (date % 12 + 1) as u8, total.unwrap()),
            )
        })
        .join_index(&pop_stream, |l, Tup3(y, m, total), pop| {
            Some((Tup3(l.clone(), *y, *m), Tup2(*total, *pop)))
        });

    let _joined = monthly_totals.join_index(&moving_averages, |Tup3(l, y, m), cur, avg| {
        Some((Tup3(l.clone(), *y, *m), Tup2(*cur, *avg)))
    });

    Ok((vax_handle, most_vax.output()))
}

fn main() -> Result<()> {
    // Build circuit.
    let (circuit, (input_handle, output_handle)) = RootCircuit::build(build_circuit)?;

    // Feed data into circuit.
    let path = format!("{}/vaccinations.csv", env!("CARGO_MANIFEST_DIR"));
    let mut reader = Reader::from_path(path)?;
    let mut input_records = reader.deserialize();
    loop {
        let mut batch = Vec::new();
        while batch.len() < 500 {
            let Some(record) = input_records.next() else {
                break;
            };
            batch.push(Tup2(record?, 1));
        }
        if batch.is_empty() {
            break;
        }
        println!("Input {} records:", batch.len());
        input_handle.append(&mut batch);

        circuit.transaction()?;

        output_handle
            .consolidate()
            .iter()
            .for_each(|(l, VaxMonthly { count, year, month }, w)| {
                println!("   {l:16} {year}-{month:02} {count:10}: {w:+}")
            });
        println!();
    }

    Ok(())
}

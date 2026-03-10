use chrono::{Duration, Local, NaiveDate};
use dioxus::prelude::*;
use std::collections::HashMap;

use crate::components::{FrequencyHeatmap, Quantizer};

#[component]
pub fn Viz() -> Element {
    let today: NaiveDate = Local::now().date_naive();

    // Demo data: 9 weeks of pseudo-random training minutes
    let raw: &[(i64, f64)] = &[
        (0, 0.0),
        (1, 45.0),
        (2, 60.0),
        (3, 0.0),
        (4, 90.0),
        (5, 30.0),
        (6, 0.0),
        (7, 75.0),
        (8, 0.0),
        (9, 45.0),
        (10, 0.0),
        (11, 120.0),
        (12, 60.0),
        (13, 0.0),
        (14, 30.0),
        (15, 90.0),
        (16, 0.0),
        (17, 60.0),
        (18, 45.0),
        (19, 0.0),
        (20, 0.0),
        (21, 90.0),
        (22, 45.0),
        (23, 0.0),
        (24, 120.0),
        (25, 60.0),
        (26, 30.0),
        (27, 0.0),
        (28, 75.0),
        (29, 45.0),
        (30, 0.0),
        (31, 90.0),
        (32, 0.0),
        (33, 60.0),
        (34, 30.0),
        (35, 0.0),
        (36, 45.0),
        (37, 120.0),
        (38, 60.0),
        (39, 0.0),
        (40, 0.0),
        (41, 90.0),
        (42, 30.0),
        (43, 60.0),
        (44, 0.0),
        (45, 45.0),
        (46, 75.0),
        (47, 0.0),
        (48, 60.0),
        (49, 90.0),
        (50, 0.0),
        (51, 120.0),
        (52, 45.0),
        (53, 0.0),
        (54, 30.0),
        (55, 60.0),
        (56, 0.0),
        (57, 90.0),
        (58, 45.0),
        (59, 0.0),
        (60, 75.0),
        (61, 0.0),
        (62, 60.0),
    ];

    // Quantize: 0 → 0, 1-30 → 1, 31-60 → 2, 61-90 → 3, 91+ → 4
    let quantizer = Quantizer::from_thresholds(vec![1.0, 31.0, 61.0, 91.0]);

    let data: HashMap<NaiveDate, usize> = raw
        .iter()
        .filter(|(_, v)| *v > 0.0)
        .map(|(days_ago, v)| {
            let date = today - Duration::days(*days_ago);
            (date, quantizer.quantize(*v))
        })
        .collect();

    // Color palette: dark purple → light lavender (index 0 = lowest activity)
    let colors = vec![
        "rgb(51, 45, 70)".to_string(),    // 1: low
        "rgb(85, 75, 120)".to_string(),   // 2: medium-low
        "rgb(120, 105, 165)".to_string(), // 3: medium-high
        "rgb(155, 135, 210)".to_string(), // 4: high
    ];

    let data_signal: Signal<HashMap<NaiveDate, usize>> = use_signal(move || data);
    let colors_signal: Signal<Vec<String>> = use_signal(move || colors);

    rsx! {
        div { class: "p-8",
            h2 { class: "text-lg font-semibold mb-6", "Training Frequency" }
            FrequencyHeatmap {
                data: data_signal,
                colors: colors_signal,
                num_columns: 9,
                end_date: today,
                tile_gap: 6,
            }
        }
    }
}

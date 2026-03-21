use chrono::Local;
use dioxus::prelude::*;

use crate::components::FrequencyHeatmap;
use crate::views::dev::{heatmap_colors, stub_heatmap_data};

#[component]
pub fn Viz() -> Element {
    let today = Local::now().date_naive();

    let data_signal = use_signal(move || stub_heatmap_data(today));
    let colors_signal = use_signal(heatmap_colors);

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

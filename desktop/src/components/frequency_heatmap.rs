use chrono::{Datelike, Duration, NaiveDate};
use dioxus::prelude::*;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Quantizer
// ---------------------------------------------------------------------------

/// Maps continuous values to discrete intensity levels.
///
/// `N` thresholds produce `N + 1` output levels (0 through N).
/// A value below the first threshold → level 0;
/// at or above the last threshold → level N.
pub struct Quantizer {
    pub thresholds: Vec<f64>,
}

impl Quantizer {
    /// Create `num_levels - 1` evenly-spaced thresholds between `min` and `max`,
    /// yielding `num_levels` output levels total.
    pub fn linear(min: f64, max: f64, num_levels: usize) -> Self {
        assert!(num_levels >= 2, "num_levels must be at least 2");
        let n = num_levels - 1;
        let step = (max - min) / n as f64;
        let thresholds = (0..n).map(|i| min + i as f64 * step).collect();
        Self { thresholds }
    }

    /// Create a quantizer from explicit threshold values.
    pub fn from_thresholds(thresholds: Vec<f64>) -> Self {
        Self { thresholds }
    }

    /// Returns the intensity level (0..=thresholds.len()) for a given value.
    pub fn quantize(&self, value: f64) -> usize {
        for (i, &t) in self.thresholds.iter().enumerate() {
            if value < t {
                return i;
            }
        }
        self.thresholds.len()
    }
}

// ---------------------------------------------------------------------------
// FrequencyGrid — low-level tile renderer
// ---------------------------------------------------------------------------

const EMPTY_TILE_COLOR: &str = "rgb(30, 30, 32)";

#[derive(Props, Clone, PartialEq)]
pub struct FrequencyGridProps {
    /// cells[col][row]; None = empty tile
    pub cells: Vec<Vec<Option<usize>>>,
    /// colors[n] = CSS color string for intensity n
    pub colors: Vec<String>,
    /// Tile size in pixels
    #[props(default = 14)]
    pub tile_size: u32,
    /// Gap between tiles in pixels
    #[props(default = 2)]
    pub tile_gap: u32,
}

#[component]
pub fn FrequencyGrid(props: FrequencyGridProps) -> Element {
    let tile_size = props.tile_size;
    let tile_gap = props.tile_gap;
    let colors = props.colors.clone();

    rsx! {
        div {
            class: "flex flex-row shrink-0",
            style: "gap: {tile_gap}px;",
            for col in props.cells.iter() {
                div {
                    class: "flex flex-col",
                    style: "gap: {tile_gap}px;",
                    for cell in col.iter() {
                        {
                            let bg = match cell {
                                None => EMPTY_TILE_COLOR.to_string(),
                                Some(idx) => colors
                                    .get(*idx)
                                    .cloned()
                                    .unwrap_or_else(|| EMPTY_TILE_COLOR.to_string()),
                            };
                            rsx! {
                                div {
                                    class: "rounded-[3px]",
                                    style: "width: {tile_size}px; height: {tile_size}px; background-color: {bg};",
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// FrequencyHeatmap — date-aware, label-aware wrapper
// ---------------------------------------------------------------------------

/// Returns the most recent Monday on or before `date`.
fn week_start(date: NaiveDate) -> NaiveDate {
    let days_from_monday = date.weekday().num_days_from_monday();
    date - Duration::days(days_from_monday as i64)
}

#[derive(Props, Clone, PartialEq)]
pub struct FrequencyHeatmapProps {
    /// Map of date → pre-quantized intensity index
    pub data: ReadSignal<HashMap<NaiveDate, usize>>,
    /// Color palette; colors[n] = CSS color for intensity n
    pub colors: ReadSignal<Vec<String>>,
    /// Number of week columns to display
    pub num_columns: usize,
    /// Last date shown (inclusive)
    pub end_date: NaiveDate,
    /// Tile size in pixels (tiles are square)
    #[props(default = 14)]
    pub tile_size: u32,
    /// Gap between tiles in pixels
    #[props(default = 2)]
    pub tile_gap: u32,
}

#[component]
pub fn FrequencyHeatmap(props: FrequencyHeatmapProps) -> Element {
    let data = props.data;
    let colors = props.colors;
    let num_columns = props.num_columns;
    let end_date = props.end_date;

    // The grid ends at the end of the week containing end_date.
    let last_week_start = week_start(end_date);
    // The grid starts num_columns weeks before the last week.
    let first_week_start = last_week_start - Duration::weeks((num_columns - 1) as i64);

    // Build cells[col][row] where col=0 is oldest, row=0=Monday, row=6=Sunday.
    let data_ref = data.read();
    let colors_ref = colors.read();

    let mut cells: Vec<Vec<Option<usize>>> = Vec::with_capacity(num_columns);
    let mut month_labels: Vec<Option<String>> = Vec::with_capacity(num_columns);

    let mut prev_month: Option<u32> = None;

    for col in 0..num_columns {
        let col_week_start = first_week_start + Duration::weeks(col as i64);

        // Month label: emit month name if this week starts a new month.
        let month_num = col_week_start.month();
        let label = if prev_month != Some(month_num) {
            prev_month = Some(month_num);
            Some(month_abbrev(month_num))
        } else {
            None
        };
        month_labels.push(label);

        // 7 rows: Mon(0) through Sun(6)
        let col_cells: Vec<Option<usize>> = (0..7)
            .map(|row| {
                let day = col_week_start + Duration::days(row as i64);
                if day > end_date {
                    None
                } else {
                    Some(*data_ref.get(&day).unwrap_or(&0))
                }
            })
            .collect();
        cells.push(col_cells);
    }

    let tile_size = props.tile_size;
    let tile_gap = props.tile_gap;
    let col_width = tile_size + tile_gap; // stride per column

    // Day labels: Mon, (empty), Wed, (empty), Fri, (empty), Sun
    let day_labels = ["Mon", "", "Wed", "", "Fri", "", "Sun"];
    let label_col_width = 32u32; // px reserved for day label column

    rsx! {
        div { class: "inline-flex flex-col select-none",
            // Month label row
            div {
                class: "flex flex-row mb-1",
                style: "padding-left: {label_col_width}px;",
                for (i, label) in month_labels.iter().enumerate() {
                    div {
                        key: "{i}",
                        class: "whitespace-nowrap overflow-visible text-[11px] text-[rgb(160,160,170)]",
                        style: "width: {col_width}px;",
                        {label.as_deref().unwrap_or("")}
                    }
                }
            }

            // Main row: day labels + grid
            div {
                class: "flex flex-row items-start",

                // Day label column
                div {
                    class: "flex flex-col shrink-0",
                    style: "gap: {tile_gap}px; width: {label_col_width}px;",
                    for label in day_labels.iter() {
                        div {
                            class: "text-right pr-1 text-[10px] text-[rgb(160,160,170)]",
                            style: "height: {tile_size}px; line-height: {tile_size}px;",
                            {*label}
                        }
                    }
                }

                // Grid
                FrequencyGrid {
                    cells,
                    colors: colors_ref.clone(),
                    tile_size,
                    tile_gap,
                }
            }
        }
    }
}

fn month_abbrev(month: u32) -> String {
    match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "???",
    }
    .to_string()
}

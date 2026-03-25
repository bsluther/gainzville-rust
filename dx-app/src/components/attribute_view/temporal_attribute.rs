use chrono::{DateTime, Local, Utc};
use dioxus::prelude::*;
use dioxus_free_icons::icons::io_icons::IoChevronDown;
use dioxus_free_icons::Icon;
use gv_core::models::entry::Temporal;

use crate::components::AttributeRow;

#[component]
pub fn TemporalAttribute(temporal: Temporal) -> Element {
    let mut expanded = use_signal(|| false);

    let start = temporal.start();
    let end = temporal.end();
    let duration = temporal.duration();
    let summary = temporal_summary(&temporal);

    rsx! {
        document::Link { rel: "stylesheet", href: asset!("./style.css") }
        div { class: "temporal-attribute flex flex-col gap-2",
            div { class: "temporal-header", onclick: move |_| expanded.toggle(),
                div { class: "flex flex-row items-center gap-1",
                    span { class: "text-sm text-gv-neutral-500 select-none", "Time" }
                    Icon {
                        width: 14,
                        height: 14,
                        fill: "var(--gv-neutral-500)",
                        class: if expanded() { "rotate-180" } else { "" },
                        icon: IoChevronDown,
                    }
                }
                span { class: "temporal-summary", "{summary}" }
            }
            if expanded() {
                div { class: "temporal-expanded flex flex-col gap-2",
                    AttributeRow { label: "Start".to_string(),
                        if let Some(start) = start {
                            span { class: "attribute-pill", "{format_date(&start)}" }
                            span { class: "attribute-pill", "{format_time(&start)}" }
                        } else {
                            span { class: "attribute-pill attribute-pill--empty" }
                        }
                    }
                    AttributeRow { label: "End".to_string(),
                        if let Some(end) = end {
                            span { class: "attribute-pill", "{format_date(&end)}" }
                            span { class: "attribute-pill", "{format_time(&end)}" }
                        } else {
                            span { class: "attribute-pill attribute-pill--empty" }
                        }
                    }
                    AttributeRow { label: "Duration".to_string(),
                        if let Some(duration_ms) = duration {
                            span { class: "attribute-pill", "{format_duration(duration_ms)}" }
                        } else {
                            span { class: "attribute-pill attribute-pill--empty" }
                        }
                    }
                }
            }
        }
    }
}

fn format_time(dt: &DateTime<Utc>) -> String {
    let local = dt.with_timezone(&Local);
    local.format("%-I:%M %p").to_string()
}

fn format_date(dt: &DateTime<Utc>) -> String {
    let local = dt.with_timezone(&Local);
    local.format("%b %-d").to_string()
}

fn format_duration(ms: u32) -> String {
    let d = ms / 86_400_000;
    let h = (ms % 86_400_000) / 3_600_000;
    let m = (ms % 3_600_000) / 60_000;
    let s = (ms % 60_000) / 1_000;

    let mut parts = Vec::new();
    if d > 0 {
        parts.push(format!("{}d", d));
    }
    if h > 0 {
        parts.push(format!("{}h", h));
    }
    if m > 0 {
        parts.push(format!("{}m", m));
    }
    if s > 0 || parts.is_empty() {
        parts.push(format!("{}s", s));
    }
    parts.join(" ")
}

/// Formats a start–end range for the collapsed summary.
/// - < 60s: just the start time
/// - same local day: "9:53 AM - 10:23 AM"
/// - spans multiple days: "June 4 - June 8"
fn format_temporal_range(start: &DateTime<Utc>, end: &DateTime<Utc>, duration_ms: i64) -> String {
    if duration_ms < 60_000 {
        return format_time(start);
    }
    let local_start = start.with_timezone(&Local);
    let local_end = end.with_timezone(&Local);
    if local_start.date_naive() == local_end.date_naive() {
        format!("{} - {}", format_time(start), format_time(end))
    } else {
        format!(
            "{} - {}",
            local_start.format("%b %-d"),
            local_end.format("%b %-d")
        )
    }
}

fn temporal_summary(temporal: &Temporal) -> String {
    match temporal {
        Temporal::None => String::new(),
        Temporal::Start { start } => format_time(start),
        Temporal::End { end } => format_time(end),
        Temporal::Duration { duration } => format_duration(*duration),
        Temporal::StartAndEnd { start, end } => {
            let duration_ms = (*end - *start).num_milliseconds();
            format_temporal_range(start, end, duration_ms)
        }
        Temporal::StartAndDuration { start, duration_ms } => {
            let end = *start + chrono::Duration::milliseconds(*duration_ms as i64);
            format_temporal_range(start, &end, *duration_ms as i64)
        }
        Temporal::DurationAndEnd { duration_ms, end } => {
            let start = *end - chrono::Duration::milliseconds(*duration_ms as i64);
            format_temporal_range(&start, end, *duration_ms as i64)
        }
    }
}

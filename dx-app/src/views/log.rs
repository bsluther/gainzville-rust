use chrono;
use dioxus::prelude::*;
use gv_core::forest::Forest;
use gv_client::client::SqliteClient;

use crate::{
    components::{EntryView, LogDatePicker},
    hooks::use_stream::use_stream,
};

#[component]
pub fn Log() -> Element {
    let entries_opt = use_stream(move || consume_context::<SqliteClient>().stream_entries());

    let forest = use_memo(move || Forest::from(entries_opt.read().cloned().unwrap_or_default()));
    use_context_provider(|| forest);

    let mut log_date = use_signal(|| chrono::Local::now().date_naive());

    rsx! {
        div { class: "flex flex-col w-full items-center gap-4",
            LogDatePicker {
                selected_date: log_date(),
                on_date_change: move |d| log_date.set(d),
            }

            div { class: "flex flex-1 flex-row justify-center h-full",
                ul { class: "flex flex-col gap-[var(--entry-list-gap)] w-96",
                    for entry in forest.read().roots() {
                        EntryView { key: "{entry.id}", id: entry.id }
                    }
                }

            }
        }
    }
}

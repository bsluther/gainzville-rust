use dioxus::prelude::*;
use gv_core::{forest, models::entry::Entry};
use gv_sqlite::client::SqliteClient;
use uuid::Uuid;

use crate::views::activity_sandbox::use_stream;
const ENTRY_CSS: Asset = asset!("/assets/styling/entry.css");

#[component]
pub fn EntryNode(id: Uuid) -> Element {
    let entries_signal = consume_context::<Signal<Vec<Entry>>>();
    let entries = entries_signal();
    let entry = use_stream(move || consume_context::<SqliteClient>().stream_entry_view_by_id(id));

    let Some(entry) = entry() else {
        return rsx! {
            div { "Loading..." }
        };
    };

    let children = forest::children_of(id, &entries);

    rsx! {
        document::Link { rel: "stylesheet", href: ENTRY_CSS }
        div { id: "entry", class: if entry.is_sequence() { "sequence" } else { "scalar" },
            div { class: "header", "{entry.display_name()}" }

            if entry.is_sequence() && !children.is_empty() {
                div { class: "entry-list",
                    for child in children {
                        EntryNode { id: child.id }
                    }
                }
            }
        }
    }
}

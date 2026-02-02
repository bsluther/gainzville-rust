use dioxus::prelude::*;
use gv_core::{
    actions::{Action, DeleteEntryRecursive},
    forest,
    models::entry::Entry,
    SYSTEM_ACTOR_ID,
};
use gv_sqlite::client::SqliteClient;
use uuid::Uuid;

use crate::hooks::use_stream::use_stream;

const ENTRY_CSS: Asset = asset!("/assets/styling/entry.css");

#[component]
pub fn EntryNode(id: ReadSignal<Uuid>) -> Element {
    let forest = consume_context::<Memo<Vec<Entry>>>();

    let entry_view =
        use_stream(move || consume_context::<SqliteClient>().stream_entry_view_by_id(id()));

    let children = use_memo(move || {
        forest::children_of(id(), &forest())
            .into_iter()
            .cloned()
            .collect::<Vec<_>>()
    });

    let Some(entry_view) = entry_view() else {
        return rsx! {};
    };

    rsx! {
        document::Link { rel: "stylesheet", href: ENTRY_CSS }
        div {
            id: "entry",
            class: if entry_view.is_sequence() { "sequence" } else { "scalar" },
            div { class: "header flex flex-row justify-between pr-4",
                "{entry_view.display_name()}"
                button {
                    onclick: move |_e| async move {
                        let delete_recursive_action = DeleteEntryRecursive {
                            actor_id: SYSTEM_ACTOR_ID,
                            entry_id: entry_view.entry.id,
                        };
                        let client = consume_context::<SqliteClient>();
                        if let Err(e) = client.run_action(delete_recursive_action.into()).await {
                            debug!("Error running delete_entry_recursive action: {e}");
                        }
                    },
                    class: "radius-2 text-red-700",
                    "D"
                }
            }

            if !children().is_empty() {
                div { class: "entry-list",
                    for child in children() {
                        EntryNode { key: "{child.id}", id: child.id }
                    }
                }
            }
        }
    }
}

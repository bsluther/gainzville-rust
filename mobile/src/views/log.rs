use dioxus::prelude::*;
use generation::{Arbitrary, SimulationContext};
use gv_core::{actions::CreateEntry, models::entry::Entry, SYSTEM_ACTOR_ID};
use gv_sqlite::client::SqliteClient;

use crate::{components::EntryView, views::activity_sandbox::use_stream};

#[component]
pub fn Log() -> Element {
    let client = consume_context::<SqliteClient>();
    // YOU ARE HERE
    // The stream read fails because we're trying to decode to an EntryRow, but UUID's are stored
    // as string. Need a principled approach - *SqliteRow for all types?
    let entries = use_stream(move || client.stream_entries());
    rsx! {
        div { "log" }
        ul {
            if let Some(entries) = entries() {
                for entry in entries {
                    EntryView { is_sequence: entry.is_sequence }
                }
            }
        }
        button {
            class: "bg-blue-500 ",
            onclick: async |_e| {
                let mut rng = rand::rng();
                let context = SimulationContext {};
                let mut entry = Entry::arbitrary(&mut rng, &context);
                entry.activity_id = None;
                entry.position = None;
                entry.owner_id = SYSTEM_ACTOR_ID;
                let create_entry: CreateEntry = entry.into();
                let client = consume_context::<SqliteClient>();
                if let Err(e) = client.run_action(create_entry.into()).await {
                    println!("Create Entry failed: {:?}", e);
                }
            },
            "Create Entry"
        }
    }
}

use dioxus::prelude::*;
use generation::{Arbitrary, ArbitraryFrom, SimulationContext};
use gv_core::{actions::CreateEntry, forest, models::entry::Entry, SYSTEM_ACTOR_ID};
use gv_sqlite::client::SqliteClient;

use crate::{components::EntryNode, views::activity_sandbox::use_stream};

#[component]
pub fn Log() -> Element {
    let entries_opt = use_stream(move || consume_context::<SqliteClient>().stream_entries());
    let activities = use_stream(move || consume_context::<SqliteClient>().stream_activities());

    // Create unwrapped signal for children (avoids PartialEq requirement of use_memo)
    let mut entries = use_signal(Vec::<Entry>::new);
    use_effect(move || {
        if let Some(e) = entries_opt() {
            entries.set(e);
        }
    });

    // Provide the unwrapped Signal<Vec<Entry>> to children
    use_context_provider(|| entries);

    // Handle loading state
    if entries_opt().is_none() {
        return rsx! {
            div { class: "p-4", "Loading entries..." }
        };
    }

    rsx! {
        div { class: "flex flex-col w-full",

            div { class: "bg-[var(--gray-1200)] flex flex-1 flex-row justify-center h-full",
                ul { class: "entry-list",
                    for entry in forest::roots(&entries()) {
                        EntryNode { id: entry.id }
                    }
                }
            
            }
            div { class: "flex flex-row justify-center",
                button {
                    class: "border-1 rounded-sm px-2 border-gray-500 ",
                    onclick: move |_e| async move {
                        if let (Some(activities), Some(entries)) = (activities(), entries_opt()) {
                            let mut rng = rand::rng();
                            let context = SimulationContext {};

                            let actor_ids = vec![SYSTEM_ACTOR_ID];
                            let entry = Entry::arbitrary_from(
                                &mut rng,
                                &context,
                                (&actor_ids, &activities, &entries),
                            );

                            let create_entry: CreateEntry = entry.into();
                            let client = consume_context::<SqliteClient>();
                            if let Err(e) = client.run_action(create_entry.into()).await {
                                println!("Create Entry failed: {:?}", e);
                            }

                        }
                    },
                    "Create ArbitraryFrom Entry"
                }
            }
            div { class: "flex flex-row justify-center",
                button {
                    class: "border-1 rounded-sm px-2 border-gray-500 ",
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
                    "Create Arbitrary Entry"
                }
            }
        }
    }
}

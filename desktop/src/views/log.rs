use dioxus::prelude::*;
use generation::{Arbitrary, ArbitraryFrom, SimulationContext};
use gv_core::{actions::CreateEntry, forest, models::entry::Entry, SYSTEM_ACTOR_ID};
use gv_sqlite::client::SqliteClient;

use crate::{components::EntryNode, hooks::use_stream::use_stream};

#[component]
pub fn Log() -> Element {
    let entries_opt = use_stream(move || consume_context::<SqliteClient>().stream_entries());
    let activities = use_stream(move || consume_context::<SqliteClient>().stream_activities());
    let entries = use_memo(move || entries_opt.read().cloned().unwrap_or(Vec::new()));

    use_context_provider(|| entries);

    let roots = use_memo(move || {
        forest::roots(&entries.read())
            .into_iter()
            .cloned()
            .collect::<Vec<Entry>>()
    });

    rsx! {
        div { class: "flex flex-col w-full",

            div { class: "bg-[var(--gray-1200)] flex flex-1 flex-row justify-center h-full",
                ul { class: "entry-list",
                    for entry in roots() {
                        EntryNode { key: "{entry.id}", id: entry.id }
                    }
                }
            
            }
            div { class: "flex flex-row justify-center",
                button {
                    class: "border-1 rounded-sm px-2 border-gray-500 ",
                    onclick: move |_e| async move {
                        if let (Some(activities), entries) = (activities(), entries()) {
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

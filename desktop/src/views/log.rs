use chrono;
use dioxus::prelude::*;
use generation::{Arbitrary, ArbitraryFrom, SimulationContext};
use gv_core::{
    actions::{CreateEntry, CreateValue},
    forest::Forest,
    models::entry::Entry,
    reader::Reader,
    SYSTEM_ACTOR_ID,
};
use gv_sqlite::{client::SqliteClient, reader::SqliteReader};

use crate::{
    components::{EntryView, LogDatePicker},
    hooks::use_stream::use_stream,
};

#[component]
pub fn Log() -> Element {
    let entries_opt = use_stream(move || consume_context::<SqliteClient>().stream_entries());
    let activities = use_stream(move || consume_context::<SqliteClient>().stream_activities());
    let entries = use_memo(move || entries_opt.read().cloned().unwrap_or(Vec::new()));

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
                ul { class: "entry-list",
                    for entry in forest.read().roots() {
                        EntryView { key: "{entry.id}", id: entry.id }
                    }
                }
            
            }
            div { class: "flex w-72 flex-col items-center justify-center gap-4",
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
                button {
                    class: "border-1 rounded-sm px-2 border-gray-500 ",
                    onclick: move |_e| async move {
                        let mut rng = rand::rng();
                        let context = SimulationContext {};
                        let client = consume_context::<SqliteClient>();

                        let mut conn = client.pool.acquire().await.unwrap();
                        let attrs = SqliteReader::all_attributes(&mut conn).await.unwrap();

                        let create_value = CreateValue::arbitrary_from(
                            &mut rng,
                            &context,
                            (&entries(), &attrs),
                        );
                        if let Err(e) = client.run_action(create_value.into()).await {
                            println!("Create Value failed: {:?}", e);
                        }
                    },
                    "Create ArbitraryFrom Value"
                }
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

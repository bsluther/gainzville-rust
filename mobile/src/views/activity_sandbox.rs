use dioxus::prelude::*;
use gv_core::{
    actions::CreateActivity,
    models::activity::{Activity, ActivityName},
    SYSTEM_ACTOR_ID,
};
use gv_sqlite::client::SqliteClient;
use uuid::Uuid;

use crate::hooks::use_stream::use_stream;

#[component]
pub fn ActivitySandbox() -> Element {
    let client = consume_context::<SqliteClient>();
    let activities = use_stream(move || client.stream_activities());

    rsx! {
        CreateActivityComponent {}
        div { class: "p-4",
            h3 { "Activities ({activities.read().as_ref().map_or(0, |a| a.len())}):" }
            ul {
                if let Some(activities) = activities() {
                    for activity in activities.iter() {
                        li { "{activity.name.to_string()}" }
                    }
                }
            }
        }
    }
}

#[component]
pub fn CreateActivityComponent() -> Element {
    let mut name_signal = use_signal(|| "".to_string());

    rsx! {
        div { class: "flex gap-2",
            p { "Activity name" }
            input {
                class: "bg-gray-300 text-black",
                value: name_signal,
                oninput: move |e| name_signal.set(e.value()),
            }
        }
        button {
            class: "bg-gray-400 text-gray-800 px-2 rounded-sm",
            onclick: move |_| async move {
                let activity = Activity {
                    id: Uuid::new_v4(),
                    owner_id: SYSTEM_ACTOR_ID,
                    source_activity_id: None,
                    name: ActivityName::parse(name_signal()).expect("invalid activity name"),
                    description: None,
                };
                let create_activity: CreateActivity = activity.into();
                let _ = consume_context::<SqliteClient>()
                    .run_action(create_activity.into())
                    .await;
                println!("creating activity with name: {}", name_signal());
            },
            "Save activity"
        }
    }
}

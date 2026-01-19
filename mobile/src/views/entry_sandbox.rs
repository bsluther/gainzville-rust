use std::time::Duration;

use crate::components::Entry;
use dioxus::{html::form::action, prelude::*};
use futures_util::StreamExt;
use gv_core::{
    actions::CreateActivity,
    models::activity::{Activity, ActivityName},
    SYSTEM_ACTOR_ID,
};
use gv_sqlite::client::Client;
use uuid::Uuid;

/// Custom hook that creates a signal updated by a fake stream
fn use_fake_stream() -> Signal<String> {
    let mut signal = use_signal(|| "Initial data".to_string());

    use_resource(move || async move {
        let mut fake_stream =
            futures_util::stream::iter(["Chunk 1", "Chunk 2", "Chunk 3", "Final chunk"]);

        while let Some(chunk) = fake_stream.next().await {
            signal.set(chunk.to_string());
            tokio::time::sleep(Duration::from_millis(1000)).await;
        }
    });

    signal
}

/// Custom hook that streams activities from the database
fn use_activities_stream() -> Signal<Vec<Activity>> {
    let mut signal = use_signal(Vec::new);
    let client = use_context::<Client>();

    use_resource(move || {
        let client = client.clone();
        async move {
            let stream = client.stream_activities();
            tokio::pin!(stream);

            while let Some(result) = stream.next().await {
                match result {
                    Ok(activities) => signal.set(activities),
                    Err(e) => eprintln!("Error fetching activities: {e}"),
                }
            }
        }
    });

    signal
}

/// The Home page component that will be rendered when the current route is `[Route::Home]`
#[component]
pub fn EntrySandbox() -> Element {
    let stream_data = use_fake_stream();
    let activities = use_activities_stream();

    rsx! {
        div { class: "entry-list",
            Entry { is_sequence: true }
            Entry { is_sequence: false }
        }
        div { {stream_data} }
        CreateActivityComponent {}
        div { class: "p-4",
            h3 { "Activities ({activities.read().len()}):" }
            ul {
                for activity in activities.read().iter() {
                    li { "{activity.name.to_string()}" }
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
                let _ = consume_context::<Client>().run_action(create_activity.into()).await;
                println!("creating activity with name: {}", name_signal());
            },
            "Save activity"
        }
    }
}

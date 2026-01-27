use crate::components::EntryView;
use dioxus::prelude::*;
use futures_util::{Stream, StreamExt};
use gv_core::{
    actions::CreateActivity,
    error::Result,
    models::activity::{Activity, ActivityName},
    SYSTEM_ACTOR_ID,
};
use gv_sqlite::client::SqliteClient;
use uuid::Uuid;

/// Custom hook that streams activities from the database
// fn use_activities_stream() -> Signal<Vec<Activity>> {
//     let mut signal = use_signal(Vec::new);
//     let client = use_context::<Client>();

//     use_resource(move || {
//         let client = client.clone();
//         async move {
//             let stream = client.stream_activities();
//             tokio::pin!(stream);

//             while let Some(result) = stream.next().await {
//                 match result {
//                     Ok(activities) => signal.set(activities),
//                     Err(e) => eprintln!("Error fetching activities: {e}"),
//                 }
//             }
//         }
//     });

//     signal
// }

/// Create a signal that reads from a stream. The stream must return a result, which will be mapped
/// into an option. The stream returns None before the first item is pulled from the stream or if
/// there is an error.
pub fn use_stream<T, S, F>(stream_fn: F) -> Signal<Option<T>>
where
    T: 'static + Clone,
    S: 'static + Stream<Item = Result<T>>,
    F: Fn() -> S + 'static,
{
    let mut signal = use_signal(|| None);

    use_resource(move || {
        let stream = stream_fn();
        tracing::debug!("Test");
        async move {
            tokio::pin!(stream);

            while let Some(result) = stream.next().await {
                match result {
                    Ok(result) => signal.set(Some(result)),
                    Err(e) => eprintln!("Error reading stream in use_stream: {e}"),
                }
            }
        }
    });
    signal
}

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

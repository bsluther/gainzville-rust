use crate::{
    components::{Command, CommandPalette},
    Route,
};
use dioxus::document::eval;
use dioxus::prelude::*;
use generation::{Arbitrary, ArbitraryFrom, SimulationContext};
use gv_core::{
    actions::{CreateEntry, CreateValue},
    models::entry::Entry,
    queries::{AllActivities, AllAttributes, AllEntries},
    query_executor::QueryExecutor,
    SYSTEM_ACTOR_ID,
};
use gv_client::{client::SqliteClient, sqlite_executor::SqliteQueryExecutor};

fn all_commands() -> Vec<Command> {
    vec![
        Command {
            id: "log",
            label: "Go to Log",
            shortcut: Some("g l"),
        },
        Command {
            id: "sandbox",
            label: "Go to Sandbox",
            shortcut: None,
        },
        Command {
            id: "dev_arbitrary_from_entry",
            label: "Dev: Create ArbitraryFrom Entry",
            shortcut: None,
        },
        Command {
            id: "dev_arbitrary_from_value",
            label: "Dev: Create ArbitraryFrom Value",
            shortcut: None,
        },
        Command {
            id: "dev_arbitrary_entry",
            label: "Dev: Create Arbitrary Entry",
            shortcut: None,
        },
    ]
}


#[component]
pub fn Navbar() -> Element {
    let mut palette_open = use_signal(|| false);
    let nav = use_navigator();

    let log_focus = move || {
        spawn(async move {
            let result = eval(
                r#"
                const el = document.activeElement;
                return el ? `${el.tagName}#${el.id}.${el.className}` : 'null';
            "#,
            )
            .await;
            if let Ok(value) = result {
                tracing::debug!("log_focus: {:?}", value);
            }
        });
    };

    // Focus stays on the unmounted command palette when it closes, so we use this function to force
    // the focus back to the app_container so that keyboard controls work.
    let focus_app_container = move || {
        spawn(async move {
            let result = eval(
                r#"
                const el = document.getElementById("app-container");
                el.tabIndex = 0;
                el.focus();
                "#,
            )
            .await;
            match result {
                Ok(_) => tracing::debug!("Successfully focused app_container in eval"),
                Err(e) => tracing::debug!("Failed to focus app_container in eval, {}", e),
            };
        });
        log_focus();
    };

    // Global keyboard handler to open command palette with cmd+p.
    // Closing the palette and all other keyboard handling is handled by the CommandPalette.
    let handle_global_keydown = move |evt: KeyboardEvent| {
        log_focus();
        if evt.modifiers().meta() && evt.key() == Key::Character("p".to_string()) {
            evt.prevent_default();
            if !palette_open() {
                palette_open.set(true);
            }
        }
    };

    let handle_select = move |id: String| match id.as_str() {
        "log" => {
            nav.push(Route::Log {});
        }
        "sandbox" => {
            nav.push(Route::ActivitySandbox {});
        }
        "dev_arbitrary_from_entry" => {
            spawn(async move {
                let client = consume_context::<SqliteClient>();
                let mut conn = client.pool.acquire().await.unwrap();
                let activities = SqliteQueryExecutor::new(&mut *conn).execute(AllActivities {}).await.unwrap();
                let entries = SqliteQueryExecutor::new(&mut *conn).execute(AllEntries {}).await.unwrap();
                drop(conn);
                let mut rng = rand::rng();
                let context = SimulationContext::default();
                let actor_ids = vec![SYSTEM_ACTOR_ID];
                let entry =
                    Entry::arbitrary_from(&mut rng, &context, (&actor_ids, &activities, &entries));
                let create_entry: CreateEntry = entry.into();
                if let Err(e) = client.run_action(create_entry.into()).await {
                    tracing::error!("Create Entry failed: {:?}", e);
                }
            });
        }
        "dev_arbitrary_from_value" => {
            spawn(async move {
                let client = consume_context::<SqliteClient>();
                let mut conn = client.pool.acquire().await.unwrap();
                let entries = SqliteQueryExecutor::new(&mut *conn).execute(AllEntries {}).await.unwrap();
                let attrs = SqliteQueryExecutor::new(&mut *conn).execute(AllAttributes {}).await.unwrap();
                drop(conn);
                let mut rng = rand::rng();
                let context = SimulationContext::default();
                let create_value =
                    CreateValue::arbitrary_from(&mut rng, &context, (&entries, &attrs));
                if let Err(e) = client.run_action(create_value.into()).await {
                    tracing::error!("Create Value failed: {:?}", e);
                }
            });
        }
        "dev_arbitrary_entry" => {
            spawn(async move {
                let client = consume_context::<SqliteClient>();
                let mut rng = rand::rng();
                let context = SimulationContext::default();
                let mut entry = Entry::arbitrary(&mut rng, &context);
                entry.activity_id = None;
                entry.position = None;
                entry.owner_id = SYSTEM_ACTOR_ID;
                let create_entry: CreateEntry = entry.into();
                if let Err(e) = client.run_action(create_entry.into()).await {
                    tracing::error!("Create Entry failed: {:?}", e);
                }
            });
        }
        _ => {}
    };

    rsx! {
        document::Link { rel: "stylesheet", href: asset!("./navbar.css") }

        // App container to catch keyboard input at any time.
        div {
            id: "app-container",
            class: "p-4 h-screen w-screen flex flex-col",
            tabindex: 0,
            onkeydown: handle_global_keydown,
            onmounted: move |evt| {
                spawn(async move {
                    _ = evt.set_focus(true).await;
                });
            },
            if palette_open() {
                CommandPalette {
                    commands: all_commands(),
                    on_close: move |_| {
                        tracing::debug!("Command palette: on_close called");
                        palette_open.set(false);
                        focus_app_container();
                    },
                    on_select: handle_select,
                }
            }

            div { id: "navbar",
                Link { to: Route::Log {}, "Log" }
                Link { to: Route::ActivitySandbox {}, "Activity Sandbox" }
                Link { to: Route::Viz {}, "Viz" }
                Link { to: Route::LibraryActivitiesIndex {}, "Library" }
            }

            // The `Outlet` component is used to render the next component inside the layout.
            div { class: "flex flex-1", Outlet::<Route> {} }
        }
    }
}

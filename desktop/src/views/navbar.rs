use crate::{components::CommandPalette, Route};
use dioxus::document::eval;
use dioxus::prelude::*;

const NAVBAR_CSS: Asset = asset!("/assets/styling/navbar.css");

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
        "home" => {
            nav.push(Route::Home {});
        }
        "log" => {
            nav.push(Route::Log {});
        }
        "sandbox" => {
            nav.push(Route::ActivitySandbox {});
        }
        _ => {}
    };

    rsx! {
        document::Link { rel: "stylesheet", href: NAVBAR_CSS }

        // App container to catch keyboard input at any time.
        div {
            id: "app-container",
            class: "p-4 h-screen w-screen",
            tabindex: 0,
            onkeydown: handle_global_keydown,
            onmounted: move |evt| {
                spawn(async move {
                    _ = evt.set_focus(true).await;
                });
            },
            if palette_open() {
                CommandPalette {
                    on_close: move |_| {
                        tracing::debug!("Command palette: on_close called");
                        palette_open.set(false);
                        focus_app_container();
                    },
                    on_select: handle_select,
                }
            }

            div { id: "navbar",
                Link { to: Route::Home {}, "Home" }
                Link { to: Route::Blog { id: 1 }, "Blog" }
                Link { to: Route::Log {}, "Log" }
                Link { to: Route::ActivitySandbox {}, "Activity Sandbox" }
            }

            // The `Outlet` component is used to render the next component inside the layout.
            div { class: "flex flex-1", Outlet::<Route> {} }
        }
    }
}

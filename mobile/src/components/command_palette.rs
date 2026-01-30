use dioxus::prelude::*;

const PALETTE_CSS: Asset = asset!("/assets/styling/command_palette.css");

// This is AI generated, mostly a placeholder to get opening/closing via cmd-p working correctly.

#[derive(Clone, PartialEq)]
pub struct Command {
    pub id: &'static str,
    pub label: &'static str,
    pub shortcut: Option<&'static str>,
}

fn all_commands() -> Vec<Command> {
    vec![
        Command {
            id: "home",
            label: "Go to Home",
            shortcut: Some("g h"),
        },
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
    ]
}

#[component]
pub fn CommandPalette(on_close: EventHandler<()>, on_select: EventHandler<String>) -> Element {
    let mut query = use_signal(String::new);
    let mut selected_index = use_signal(|| 0usize);

    let commands = all_commands();
    let filtered: Vec<_> = commands
        .iter()
        .filter(|cmd| {
            query().is_empty() || cmd.label.to_lowercase().contains(&query().to_lowercase())
        })
        .collect();

    // Reset selection when query changes
    use_effect(move || {
        selected_index.set(0);
    });

    let handle_keydown = move |evt: KeyboardEvent| {
        let filtered_len = {
            let q = query();
            all_commands()
                .iter()
                .filter(|cmd| q.is_empty() || cmd.label.to_lowercase().contains(&q.to_lowercase()))
                .count()
        };

        match evt.key() {
            Key::Tab => {
                // Focus trap - prevent Tab from leaving the palette
                evt.prevent_default();
            }
            Key::Escape => on_close.call(()),
            Key::ArrowDown => {
                evt.prevent_default();
                if filtered_len > 0 {
                    selected_index.set((selected_index() + 1) % filtered_len);
                }
            }
            Key::ArrowUp => {
                evt.prevent_default();
                if filtered_len > 0 {
                    selected_index.set(selected_index().checked_sub(1).unwrap_or(filtered_len - 1));
                }
            }
            Key::Enter => {
                evt.prevent_default();
                let q = query();
                let filtered: Vec<_> = all_commands()
                    .into_iter()
                    .filter(|cmd| {
                        q.is_empty() || cmd.label.to_lowercase().contains(&q.to_lowercase())
                    })
                    .collect();
                if let Some(cmd) = filtered.get(selected_index()) {
                    on_select.call(cmd.id.to_string());
                    on_close.call(());
                }
            }
            _ => {}
        }
    };

    rsx! {
        document::Link { rel: "stylesheet", href: PALETTE_CSS }

        div {
            class: "palette-overlay",
            tabindex: -1,
            onclick: move |_| on_close.call(()),
            onkeydown: handle_keydown,
            div { class: "palette", onclick: move |e| e.stop_propagation(),
                input {
                    class: "palette-input",
                    placeholder: "Type a command...",
                    value: "{query}",
                    oninput: move |e| query.set(e.value()),
                    onkeydown: handle_keydown,
                    onmounted: move |evt| async move {
                        debug!("cmd palette input onmounted called");
                        _ = evt.set_focus(true).await;
                    },
                }
                div { class: "palette-results",
                    for (i , cmd) in filtered.iter().enumerate() {
                        div {
                            class: if i == selected_index() { "palette-item selected" } else { "palette-item" },
                            onclick: {
                                let id = cmd.id.to_string();
                                move |_| {
                                    on_select.call(id.clone());
                                    on_close.call(());
                                }
                            },
                            span { class: "palette-label", "{cmd.label}" }
                            if let Some(shortcut) = cmd.shortcut {
                                span { class: "palette-shortcut", "{shortcut}" }
                            }
                        }
                    }
                    if filtered.is_empty() {
                        div { class: "palette-empty", "No commands found" }
                    }
                }
            }
        }
    }
}

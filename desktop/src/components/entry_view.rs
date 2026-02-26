use dioxus::prelude::*;
use dioxus_free_icons::{icons::io_icons::IoCheckbox, Icon};
use dioxus_primitives::context_menu::{ContextMenuContent, ContextMenuItem, ContextMenuTrigger};
use gv_core::{actions::DeleteEntryRecursive, forest::Forest, SYSTEM_ACTOR_ID};
use gv_sqlite::client::SqliteClient;
use uuid::Uuid;

#[component]
fn CheckboxIcon() -> Element {
    rsx!(
        Icon {
            width: 30,
            height: 30,
            fill: "black",
            icon: IoCheckbox,
        }
    )
}

use crate::{components::context_menu::ContextMenu, hooks::use_stream::use_stream};

const ENTRY_CSS: Asset = asset!("/assets/styling/entry.css");

#[component]
pub fn EntryView(id: ReadSignal<Uuid>) -> Element {
    let forest = consume_context::<Memo<Forest>>();

    let entry_join =
        use_stream(move || consume_context::<SqliteClient>().stream_entry_join_by_id(id()));

    let child_ids = use_memo(move || {
        forest()
            .children(id())
            .into_iter()
            .map(|e| e.id)
            .collect::<Vec<Uuid>>()
    });

    let Some(entry_join) = entry_join() else {
        return rsx! {};
    };

    let handle_delete_recursive = move |_e| async move {
        let delete_recursive_action = DeleteEntryRecursive {
            actor_id: SYSTEM_ACTOR_ID,
            entry_id: entry_join.entry.id,
        };
        let client = consume_context::<SqliteClient>();
        if let Err(e) = client.run_action(delete_recursive_action.into()).await {
            debug!("Error running delete_entry_recursive action: {e}");
        }
    };

    rsx! {
        document::Link { rel: "stylesheet", href: ENTRY_CSS }

        EntryContextMenu { id,
            Container { is_sequence: entry_join.is_sequence(),
                Header {
                    display_name: entry_join.display_name(),
                    on_delete_recursive: handle_delete_recursive,
                }
                if !child_ids().is_empty() {
                    ChildEntries { entry_ids: child_ids() }
                }
            }
        }
    }
}

#[component]
fn Container(is_sequence: ReadSignal<bool>, children: Element) -> Element {
    rsx! {
        div { class: if is_sequence() { "entry sequence" } else { "entry scalar" }, {children} }
    }
}

#[component]
fn Header(
    display_name: ReadSignal<String>,
    on_delete_recursive: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div { class: "header flex flex-row justify-between pr-4",
            "{display_name()}"
            button { onclick: on_delete_recursive, class: "radius-2 text-red-700", "D" }
            CheckboxIcon {}
        }
    }
}

#[component]
fn ChildEntries(entry_ids: Vec<Uuid>) -> Element {
    rsx! {
        div { class: "entry-list",
            for child_id in entry_ids {
                EntryView { key: "{child_id}", id: child_id }
            }
        }
    }
}

#[component]
fn EntryContextMenu(id: ReadSignal<Uuid>, children: Element) -> Element {
    rsx! {
        ContextMenu {
            ContextMenuTrigger { children }
            ContextMenuContent { class: "context-menu-content",
                ContextMenuItem {
                    class: "context-menu-item",
                    value: "delete".to_string(),
                    index: 0usize,
                    disabled: false,
                    on_select: move |_e| async move {
                        let delete_recursive_action = DeleteEntryRecursive {
                            actor_id: SYSTEM_ACTOR_ID,
                            entry_id: *id.read(),
                        };
                        let client = consume_context::<SqliteClient>();
                        if let Err(e) = client.run_action(delete_recursive_action.into()).await {
                            debug!("Error running delete_entry_recursive action: {e}");
                        }
                    },
                    "Delete"
                }
            }
        }
    }
}

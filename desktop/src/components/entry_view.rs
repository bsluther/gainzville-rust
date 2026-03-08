use dioxus::prelude::*;
use dioxus_free_icons::icons::io_icons::IoEllipsisVertical;
use dioxus_free_icons::Icon;
use dioxus_primitives::context_menu::{ContextMenuContent, ContextMenuItem, ContextMenuTrigger};
use gv_core::{
    actions::{DeleteEntryRecursive, UpdateEntryCompletion},
    forest::Forest,
    models::{attribute_pair::AttributePair, entry::Temporal},
    SYSTEM_ACTOR_ID,
};
use gv_sqlite::client::SqliteClient;
use uuid::Uuid;

use crate::{
    components::{context_menu::ContextMenu, AttributeView, TemporalAttribute},
    hooks::use_stream::use_stream,
};

// Code style:
// - Prefer pure components (UI is a function of props) for smaller/lower-level UI componets.
//   E.g. access data through side-effects (hooks) at the higher-level orchestration level and wire
//   to
// - Access data at the top of a component; use props or context to pass to subcomponents.

#[component]
pub fn EntryView(id: ReadSignal<Uuid>) -> Element {
    let mut expanded = use_signal(|| false);
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

    let entry_id = entry_join.entry.id;
    let is_complete = entry_join.entry.is_complete;

    let handle_delete_recursive = move |_e| async move {
        let client = consume_context::<SqliteClient>();
        if let Err(e) = client
            .run_action(
                DeleteEntryRecursive {
                    actor_id: SYSTEM_ACTOR_ID,
                    entry_id,
                }
                .into(),
            )
            .await
        {
            debug!("Error running delete_entry_recursive action: {e}");
        }
    };

    let handle_toggle_complete = move |_| async move {
        let client = consume_context::<SqliteClient>();
        if let Err(e) = client
            .run_action(
                UpdateEntryCompletion {
                    actor_id: SYSTEM_ACTOR_ID,
                    entry_id,
                    is_complete: !is_complete,
                }
                .into(),
            )
            .await
        {
            debug!("Error running update_entry_completion action: {e}");
        }
    };

    let toggle_expanded = move |_| {
        expanded.toggle();
    };

    rsx! {

        Container { is_sequence: entry_join.is_sequence(),
            Header {
                entry_id: id(),
                display_name: entry_join.display_name(),
                on_delete_recursive: handle_delete_recursive,
                expanded: expanded(),
                on_toggle_expand: toggle_expanded,
                is_sequence: entry_join.is_sequence(),
                is_complete,
                on_toggle_complete: handle_toggle_complete,
            }
            if expanded() {
                div { class: "entry-body",
                    Attributes {
                        attr_pairs: entry_join.attributes().cloned().collect::<Vec<_>>(),
                        temporal: entry_join.clone().entry.temporal,
                    }
                    if !child_ids().is_empty() {
                        ChildEntries { entry_ids: child_ids() }
                    }
                    if !entry_join.is_sequence() {
                        FooterScalar {
                            is_complete,
                            on_toggle: handle_toggle_complete,
                        }
                    }
                }
            }
        }

    }
}

#[component]
fn Container(is_sequence: ReadSignal<bool>, children: Element) -> Element {
    rsx! {
        div {
            class: "entry-view",
            "data-entry-kind": if is_sequence() { "sequence" } else { "scalar" },
            {children}
        }
    }
}

#[component]
fn Header(
    entry_id: Uuid,
    is_sequence: bool,
    is_complete: bool,
    display_name: ReadSignal<String>,
    on_delete_recursive: EventHandler<MouseEvent>,
    on_toggle_expand: EventHandler<MouseEvent>,
    on_toggle_complete: EventHandler<MouseEvent>,
    expanded: bool,
) -> Element {
    rsx! {
        // Outer row containing two clickable elements.
        div { class: "entry-header flex flex-row justify-between items-center",
            // Title/summary; clicking here expands/contracts the entry.
            div {
                class: "flex flex-row grow items-center pr-4 gap-4 select-none",
                onclick: move |e| on_toggle_expand(e),
                // Title
                div { "{display_name()}" }
                // Summary text
                div {}
            }
            // Checkbox controlling `Entry.is_complete` or menu trigger.
            if expanded || is_sequence {
                div { class: "pr-1",
                    EntryContextMenu { id: entry_id }
                }
            } else {
                div { class: "pr-2",
                    FillCheckbox {
                        checked: is_complete,
                        on_toggle: move |e| on_toggle_complete(e),
                    }
                }
            }
        }
    }
}

#[component]
fn FooterScalar(is_complete: bool, on_toggle: EventHandler<MouseEvent>) -> Element {
    rsx! {
        div { class: "entry-footer flex flex-row grow justify-end",
            FillCheckbox { checked: is_complete, on_toggle: move |e| on_toggle(e) }
        }
    }
}

#[component]
fn Attributes(attr_pairs: Vec<AttributePair>, temporal: Temporal) -> Element {
    rsx! {
        div { class: "attribute-list",
            TemporalAttribute { temporal: temporal.clone() }
            for pair in attr_pairs {
                AttributeView { pair }
            }
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
fn FillCheckbox(checked: bool, on_toggle: EventHandler<MouseEvent>) -> Element {
    rsx! {
        div {
            class: "entry-checkbox",
            "data-state": if checked { "plan" } else { "actual" },
            onclick: on_toggle,
            span { class: "entry-checkbox-check" }
        }
    }
}

#[component]
fn EntryContextMenu(id: ReadSignal<Uuid>, children: Element) -> Element {
    rsx! {
        ContextMenu {
            ContextMenuTrigger {
                Icon {
                    width: 20,
                    height: 20,
                    fill: "var(--gv-neutral-500)",
                    class: "red-200",
                    icon: IoEllipsisVertical,
                }
            }
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

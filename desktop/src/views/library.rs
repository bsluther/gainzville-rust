use dioxus::prelude::*;
use dioxus_free_icons::icons::io_icons::IoAddOutline;
use dioxus_free_icons::Icon;
use gv_sqlite::client::SqliteClient;
use uuid::Uuid;

use crate::{
    Route,
    components::{
        button::Button,
        input::Input,
        sheet::{Sheet, SheetContent, SheetSide},
    },
    hooks::use_stream::use_stream,
};

/// Redirect component for `/library` → `/library/activities`.
#[component]
pub fn Library() -> Element {
    let nav = use_navigator();
    use_effect(move || {
        nav.replace(Route::LibraryActivitiesIndex {});
    });
    rsx! {}
}

#[component]
pub fn LibraryActivities() -> Element {
    let client = consume_context::<SqliteClient>();
    let activities = use_stream(move || client.stream_activities());
    let nav = use_navigator();
    let route = use_route::<Route>();
    let is_detail = matches!(route, Route::LibraryActivityDetail { .. });
    let selected_id: Option<Uuid> = match route {
        Route::LibraryActivityDetail { id } => Some(id),
        _ => None,
    };

    rsx! {
        div { class: "library-layout",
            div { class: "library-browser",
                nav { class: "library-tab-bar",
                    Link { to: Route::LibraryActivitiesIndex {}, class: "library-tab-active", "Activities" }
                    Link { to: Route::LibraryAttributesIndex {}, "Attributes" }
                }
                div { class: "library-list",
                    if let Some(activities) = activities() {
                        for activity in activities.iter() {
                            div {
                                key: "{activity.id}",
                                class: "library-item",
                                "data-selected": selected_id == Some(activity.id),
                                onclick: {
                                    let id = activity.id;
                                    move |_| { nav.push(Route::LibraryActivityDetail { id }); }
                                },
                                "{activity.name.to_string()}"
                            }
                        }
                    }
                }
                div { class: "library-controls",
                    Input { placeholder: "Search..." }
                    Button {
                        Icon { icon: IoAddOutline, width: 16, height: 16 }
                    }
                }
            }
            div { class: "library-profile",
                Outlet::<Route> {}
            }
            Sheet {
                open: is_detail,
                on_open_change: move |open: bool| {
                    if !open {
                        nav.push(Route::LibraryActivitiesIndex {});
                    }
                },
                SheetContent { side: SheetSide::Bottom,
                    div { class: "library-profile-stub", "Profile view" }
                }
            }
        }
    }
}

#[component]
pub fn LibraryAttributes() -> Element {
    let client = consume_context::<SqliteClient>();
    let attributes = use_stream(move || client.stream_attributes());
    let nav = use_navigator();
    let route = use_route::<Route>();
    let is_detail = matches!(route, Route::LibraryAttributeDetail { .. });
    let selected_id: Option<Uuid> = match route {
        Route::LibraryAttributeDetail { id } => Some(id),
        _ => None,
    };

    rsx! {
        div { class: "library-layout",
            div { class: "library-browser",
                nav { class: "library-tab-bar",
                    Link { to: Route::LibraryActivitiesIndex {}, "Activities" }
                    Link { to: Route::LibraryAttributesIndex {}, class: "library-tab-active", "Attributes" }
                }
                div { class: "library-list",
                    if let Some(attributes) = attributes() {
                        for attribute in attributes.iter() {
                            div {
                                key: "{attribute.id}",
                                class: "library-item",
                                "data-selected": selected_id == Some(attribute.id),
                                onclick: {
                                    let id = attribute.id;
                                    move |_| { nav.push(Route::LibraryAttributeDetail { id }); }
                                },
                                "{attribute.name}"
                            }
                        }
                    }
                }
                div { class: "library-controls",
                    Input { placeholder: "Search..." }
                    Button {
                        Icon { icon: IoAddOutline, width: 16, height: 16 }
                    }
                }
            }
            div { class: "library-profile",
                Outlet::<Route> {}
            }
            Sheet {
                open: is_detail,
                on_open_change: move |open: bool| {
                    if !open {
                        nav.push(Route::LibraryAttributesIndex {});
                    }
                },
                SheetContent { side: SheetSide::Bottom,
                    div { class: "library-profile-stub", "Profile view" }
                }
            }
        }
    }
}

#[component]
pub fn LibraryActivitiesIndex() -> Element {
    rsx! {
        div { class: "library-profile-empty", "Select an activity" }
    }
}

#[component]
pub fn LibraryActivityDetail(id: Uuid) -> Element {
    rsx! {
        div { class: "library-profile-content", "Activity {id}" }
    }
}

#[component]
pub fn LibraryAttributesIndex() -> Element {
    rsx! {
        div { class: "library-profile-empty", "Select an attribute" }
    }
}

#[component]
pub fn LibraryAttributeDetail(id: Uuid) -> Element {
    rsx! {
        div { class: "library-profile-content", "Attribute {id}" }
    }
}

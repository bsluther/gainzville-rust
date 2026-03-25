use chrono::Local;
use dioxus::prelude::*;
use dioxus_free_icons::icons::io_icons::IoAddOutline;
use dioxus_free_icons::icons::ld_icons::LdSquarePen;
use dioxus_free_icons::Icon;
use gv_core::models::{activity::Activity, attribute::Attribute};
use gv_sqlite::client::SqliteClient;
use uuid::Uuid;

use crate::components::FrequencyHeatmap;
use crate::views::dev::{heatmap_colors, stub_heatmap_data};

use crate::{
    components::{
        button::Button,
        input::Input,
        sheet::{Sheet, SheetContent, SheetSide},
    },
    hooks::use_stream::use_stream,
    Route,
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
    let is_detail = matches!(route, Route::LibraryActivityProfile { .. });
    let selected_id: Option<Uuid> = match route {
        Route::LibraryActivityProfile { id } => Some(id),
        _ => None,
    };

    rsx! {
        document::Link { rel: "stylesheet", href: asset!("./library.css") }
        div { class: "library-layout",
            div { class: "library-browser",
                nav { class: "library-tab-bar",
                    Link {
                        to: Route::LibraryActivitiesIndex {},
                        class: "library-tab-active",
                        "Activities"
                    }
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
                                    move |_| {
                                        nav.push(Route::LibraryActivityProfile {
                                            id,
                                        });
                                    }
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
            div { class: "library-profile", Outlet::<Route> {} }
            Sheet {
                open: is_detail,
                on_open_change: move |open: bool| {
                    if !open {
                        nav.push(Route::LibraryActivitiesIndex {});
                    }
                },
                SheetContent { side: SheetSide::Bottom,
                    div { class: "library-sheet-content",
                        if let Some(activity) = activities()
                            .and_then(|list| { list.into_iter().find(|a| Some(a.id) == selected_id) })
                        {
                            ActivityProfile { activity }
                        }
                    }
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
    let is_detail = matches!(route, Route::LibraryAttributeProfile { .. });
    let selected_id: Option<Uuid> = match route {
        Route::LibraryAttributeProfile { id } => Some(id),
        _ => None,
    };

    rsx! {
        document::Link { rel: "stylesheet", href: asset!("./library.css") }
        div { class: "library-layout",
            div { class: "library-browser",
                nav { class: "library-tab-bar",
                    Link { to: Route::LibraryActivitiesIndex {}, "Activities" }
                    Link {
                        to: Route::LibraryAttributesIndex {},
                        class: "library-tab-active",
                        "Attributes"
                    }
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
                                    move |_| {
                                        nav.push(Route::LibraryAttributeProfile {
                                            id,
                                        });
                                    }
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
            div { class: "library-profile", Outlet::<Route> {} }
            Sheet {
                open: is_detail,
                on_open_change: move |open: bool| {
                    if !open {
                        nav.push(Route::LibraryAttributesIndex {});
                    }
                },
                SheetContent { side: SheetSide::Bottom,
                    div { class: "library-sheet-content",
                        if let Some(attribute) = attributes()
                            .and_then(|list| { list.into_iter().find(|a| Some(a.id) == selected_id) })
                        {
                            AttributeProfile { attribute }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ProfileSectionHeader(
    title: String,
    #[props(default)] on_edit: Option<EventHandler<MouseEvent>>,
) -> Element {
    rsx! {
        div { class: "profile-section-header",
            span { class: "profile-section-label", "{title}" }
            if let Some(on_edit) = on_edit {
                button {
                    class: "profile-section-edit",
                    onclick: move |e| on_edit.call(e),
                    Icon { icon: LdSquarePen, width: 14, height: 14 }
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
pub fn LibraryActivityProfile(id: Uuid) -> Element {
    let client = consume_context::<SqliteClient>();
    let activities = use_stream(move || client.stream_activities());
    let activity = activities().and_then(|list| list.into_iter().find(|a| a.id == id));
    rsx! {
        if let Some(activity) = activity {
            ActivityProfile { activity }
        }
    }
}

#[component]
fn ActivityProfile(activity: Activity) -> Element {
    let today = Local::now().date_naive();
    let data_signal = use_signal(move || stub_heatmap_data(today));
    let colors_signal = use_signal(heatmap_colors);

    rsx! {
        div { class: "library-profile-content",
            div {
                ProfileSectionHeader { title: "Name", on_edit: move |_| {} }
                div { class: "profile-section-body", "{activity.name.to_string()}" }
            }
            div {
                ProfileSectionHeader { title: "Description", on_edit: move |_| {} }
                div { class: "profile-section-body", "Description for Activity {activity.id}" }
            }
            div {
                ProfileSectionHeader { title: "Recent" }
                FrequencyHeatmap {
                    data: data_signal,
                    colors: colors_signal,
                    num_columns: 9,
                    end_date: today,
                    tile_gap: 6,
                }
            }
            div {
                ProfileSectionHeader { title: "Categories" }
                div { class: "profile-section-body", "Categories for Activity {activity.id}" }
            }
            div {
                ProfileSectionHeader { title: "Sub-Categories" }
                div { class: "profile-section-body", "Sub-categories for Activity {activity.id}" }
            }
            div {
                ProfileSectionHeader { title: "Attributes" }
                div { class: "profile-section-body", "Attributes for Activity {activity.id}" }
            }
        }
    }
}

#[component]
pub fn LibraryAttributesIndex() -> Element {
    rsx! {
        div { class: "library-profile-empty", "Select an attribute" }
    }
}

#[component]
pub fn LibraryAttributeProfile(id: Uuid) -> Element {
    let client = consume_context::<SqliteClient>();
    let attributes = use_stream(move || client.stream_attributes());
    let attribute = attributes().and_then(|list| list.into_iter().find(|a| a.id == id));
    rsx! {
        if let Some(attribute) = attribute {
            AttributeProfile { attribute }
        }
    }
}

#[component]
fn AttributeProfile(attribute: Attribute) -> Element {
    rsx! {
        div { class: "library-profile-content",
            div {
                ProfileSectionHeader { title: "Name", on_edit: move |_| {} }
                div { class: "profile-section-body", "{attribute.name}" }
            }
            div {
                ProfileSectionHeader { title: "Description", on_edit: move |_| {} }
                div { class: "profile-section-body", "Description for Attribute {attribute.id}" }
            }
            div {
                ProfileSectionHeader { title: "Configuration" }
                div { class: "profile-section-body", "Configuration for Attribute {attribute.id}" }
            }
        }
    }
}

use dioxus::prelude::*;
use dioxus_free_icons::icons::io_icons::IoRemove;
use dioxus_free_icons::Icon;
use gv_core::{
    SYSTEM_ACTOR_ID,
    actions::{UpdateAttributeValue, ValueField},
    models::attribute::{AttributeValue, MassMeasurement, MassUnit, MassValue, NumericValue, SelectValue},
    models::attribute_pair::{AttributePair, MassAttributePair, NumericAttributePair, SelectAttributePair},
};
use gv_sqlite::client::SqliteClient;

#[component]
pub fn AttributeRow(label: String, children: Element) -> Element {
    rsx! {
        div { class: "attribute-row",
            span { class: "attribute-label", "{label}" }
            div { class: "flex flex-row gap-1 items-center", {children} }
        }
    }
}

#[component]
pub fn AttributeView(pair: AttributePair) -> Element {
    match pair {
        AttributePair::Numeric(p) => rsx! {
            NumericView { pair: p }
        },
        AttributePair::Select(p) => rsx! {
            SelectView { pair: p }
        },
        AttributePair::Mass(p) => rsx! {
            MassView { pair: p }
        },
    }
}

#[component]
fn NumericView(pair: NumericAttributePair) -> Element {
    let integer = pair.config.integer;
    let attr_id = pair.attr_id;
    let entry_id = pair.entry_id;

    let formatted = match &pair.actual {
        Some(NumericValue::Exact(n)) => format_numeric(*n, integer),
        _ => String::new(),
    };

    let mut input_text = use_signal(|| formatted.clone());

    // Clobber shadow state when a DB update arrives.
    let mut synced_db = use_signal(|| formatted.clone());
    if formatted != *synced_db.peek() {
        synced_db.set(formatted.clone());
        input_text.set(formatted.clone());
    }

    let commit = move || async move {
        let raw = input_text.peek().clone();
        if let Ok(n) = raw.trim().parse::<f64>() {
            let client = consume_context::<SqliteClient>();
            let _ = client
                .run_action(
                    UpdateAttributeValue {
                        actor_id: SYSTEM_ACTOR_ID,
                        entry_id,
                        attribute_id: attr_id,
                        field: ValueField::Actual,
                        value: AttributeValue::Numeric(NumericValue::Exact(n)),
                    }
                    .into(),
                )
                .await;
        }
    };

    rsx! {
        AttributeRow { label: pair.name.clone(),
            input {
                class: "attribute-pill",
                r#type: "text",
                value: "{input_text}",
                oninput: move |e| input_text.set(e.value()),
                onblur: move |_| async move { commit().await },
                onkeydown: move |e: KeyboardEvent| async move {
                    if e.key() == Key::Enter {
                        commit().await;
                    }
                },
            }
        }
    }
}

#[component]
fn SelectView(pair: SelectAttributePair) -> Element {
    let attr_id = pair.attr_id;
    let entry_id = pair.entry_id;
    let options = pair.config.options.clone();
    let mut open = use_signal(|| false);

    let current = match &pair.actual {
        Some(SelectValue::Exact(s)) => s.clone(),
        _ => String::new(),
    };

    rsx! {
        AttributeRow { label: pair.name.clone(),
            div { class: "select-wrapper",
                // Backdrop: captures outside clicks to close the popover.
                if open() {
                    div {
                        class: "select-backdrop",
                        onclick: move |_| open.set(false),
                    }
                }
                button {
                    class: "attribute-pill select-trigger",
                    onclick: move |_| open.set(!open()),
                    if current.is_empty() { "\u{00a0}" } else { "{current}" }
                }
                if open() {
                    div { class: "select-popover",
                        for option in options {
                            div {
                                class: "select-option",
                                onclick: {
                                    let option = option.clone();
                                    move |_| {
                                        let option = option.clone();
                                        open.set(false);
                                        async move {
                                            let client = consume_context::<SqliteClient>();
                                            let _ = client
                                                .run_action(
                                                    UpdateAttributeValue {
                                                        actor_id: SYSTEM_ACTOR_ID,
                                                        entry_id,
                                                        attribute_id: attr_id,
                                                        field: ValueField::Actual,
                                                        value: AttributeValue::Select(
                                                            SelectValue::Exact(option),
                                                        ),
                                                    }
                                                    .into(),
                                                )
                                                .await;
                                        }
                                    }
                                },
                                "{option}"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn MassView(pair: MassAttributePair) -> Element {
    let name = pair.name.clone();
    let pills = match pair.actual {
        Some(MassValue::Exact(measurements)) => rsx! {
            span { class: "attribute-pill", "{format_mass(&measurements)}" }
        },
        Some(MassValue::Range { min, max }) => rsx! {
            span { class: "attribute-pill", "{format_mass(&min)}" }
            Icon {
                width: 12,
                height: 12,
                fill: "var(--gv-neutral-500)",
                icon: IoRemove,
            }
            span { class: "attribute-pill", "{format_mass(&max)}" }
        },
        None => rsx! {
            span { class: "attribute-pill" }
        },
    };
    rsx! {
        AttributeRow { label: name, {pills} }
    }
}

fn format_numeric(v: f64, integer: bool) -> String {
    if integer {
        format!("{}", v as i64)
    } else {
        format!("{}", v)
    }
}

fn format_mass_unit(unit: &MassUnit) -> &'static str {
    match unit {
        MassUnit::Gram => "g",
        MassUnit::Kilogram => "kg",
        MassUnit::Pound => "lb",
    }
}

fn format_mass(measurements: &[MassMeasurement]) -> String {
    measurements
        .iter()
        .map(|m| format!("{} {}", m.value, format_mass_unit(&m.unit)))
        .collect::<Vec<_>>()
        .join(" ")
}

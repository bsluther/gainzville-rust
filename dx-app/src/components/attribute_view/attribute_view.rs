use dioxus::prelude::*;
use dioxus_free_icons::icons::io_icons::IoCog;
use dioxus_free_icons::Icon;
use gv_core::{
    actions::{UpdateAttributeValue, ValueField},
    models::attribute::{
        AttributeValue, MassMeasurement, MassUnit, MassValue, NumericValue, SelectValue,
    },
    models::attribute_pair::{
        AttributePair, MassAttributePair, NumericAttributePair, SelectAttributePair,
    },
    SYSTEM_ACTOR_ID,
};
use gv_sqlite::client::SqliteClient;

use crate::components::{PlatformMenu, PlatformMenuItem, PlatformPopover};

#[component]
pub fn AttributeRow(label: String, children: Element) -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: asset!("./style.css") }
        div { class: "attribute-row",
            span { class: "text-sm text-gv-neutral-500 select-none", "{label}" }
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

    let current = match &pair.actual {
        Some(SelectValue::Exact(s)) => s.clone(),
        _ => String::new(),
    };

    rsx! {
        AttributeRow { label: pair.name.clone(),
            PlatformMenu {
                trigger: rsx! {
                    div { class: "attribute-pill select-trigger",
                        if current.is_empty() {
                            "\u{00a0}"
                        } else {
                            "{current}"
                        }
                    }
                },
                content: rsx! {
                    for (i , option) in options.iter().enumerate() {
                        PlatformMenuItem::<String> {
                            // class: "bg-green-500",
                            value: option.clone(),
                            index: i,
                            on_select: move |selected: String| {
                                spawn(async move {
                                    let client = consume_context::<SqliteClient>();
                                    let _ = client
                                        .run_action(
                                            UpdateAttributeValue {
                                                actor_id: SYSTEM_ACTOR_ID,
                                                entry_id,
                                                attribute_id: attr_id,
                                                field: ValueField::Actual,
                                                value: AttributeValue::Select(SelectValue::Exact(selected)),
                                            }
                                                .into(),
                                        )
                                        .await;
                                });
                            },
                            "{option}"
                        }
                    }
                },
            }
        }
    }
}

fn mass_input_strings(units: &[MassUnit], actual: &Option<MassValue>) -> Vec<String> {
    units
        .iter()
        .map(|unit| match actual {
            Some(MassValue::Exact(ms)) => ms
                .iter()
                .find(|m| &m.unit == unit)
                .map(|m| m.value.to_string())
                .unwrap_or_default(),
            _ => String::new(),
        })
        .collect()
}

#[component]
fn MassView(pair: MassAttributePair) -> Element {
    let attr_id = pair.attr_id;
    let entry_id = pair.entry_id;
    // let units = pair.config.default_units.clone();
    let units = pair.defined_units();
    let all_units = vec![MassUnit::Gram, MassUnit::Kilogram, MassUnit::Pound];
    let configured_units = units.clone();

    let db_strs = mass_input_strings(&units, &pair.actual);
    let mut inputs = use_signal(|| db_strs.clone());
    let mut synced_db = use_signal(|| db_strs.clone());
    if db_strs != *synced_db.peek() {
        synced_db.set(db_strs.clone());
        inputs.set(db_strs.clone());
    }

    // Store units in a signal so commit closure is Copy.
    let units_sig = use_signal(|| units.clone());
    let mut show_picker = use_signal(|| false);

    let commit = move || async move {
        let strs = inputs.peek().clone();
        let units = units_sig.peek().clone();
        let measurements: Vec<MassMeasurement> = units
            .iter()
            .enumerate()
            .filter_map(|(i, unit)| {
                strs.get(i)
                    .and_then(|s| s.trim().parse::<f64>().ok())
                    .map(|v| MassMeasurement {
                        unit: unit.clone(),
                        value: v,
                    })
            })
            .collect();
        if measurements.is_empty() {
            return;
        }
        let client = consume_context::<SqliteClient>();
        let _ = client
            .run_action(
                UpdateAttributeValue {
                    actor_id: SYSTEM_ACTOR_ID,
                    entry_id,
                    attribute_id: attr_id,
                    field: ValueField::Actual,
                    value: AttributeValue::Mass(MassValue::Exact(measurements)),
                }
                .into(),
            )
            .await;
    };

    rsx! {
        AttributeRow { label: pair.name.clone(),
            for (i , unit) in units.iter().enumerate() {
                input {
                    class: "attribute-pill mass-input",
                    r#type: "text",
                    value: "{inputs()[i]}",
                    oninput: move |e| {
                        let mut v = inputs();
                        if let Some(s) = v.get_mut(i) {
                            *s = e.value();
                        }
                        inputs.set(v);
                    },
                    onblur: move |_| async move { commit().await },
                    onkeydown: move |e: KeyboardEvent| async move {
                        if e.key() == Key::Enter {
                            commit().await;
                        }
                    },
                }
                span { class: "mass-unit-label", "{format_mass_unit(unit)}" }
            }
            PlatformPopover {
                open: show_picker(),
                on_open_change: move |v| show_picker.set(v),
                trigger: rsx! {
                    div { class: "mass-cog-btn",
                        Icon {
                            width: 14,
                            height: 14,
                            fill: "var(--gv-neutral-500)",
                            icon: IoCog,
                        }
                    }
                },
                content: rsx! {
                    for unit in all_units {
                        div { class: "unit-picker-row",
                            span { class: "unit-picker-label", "{format_mass_unit(&unit)}" }
                            input {
                                r#type: "checkbox",
                                checked: configured_units.contains(&unit),
                                disabled: true,
                            }
                        }
                    }
                },
            }
        }
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

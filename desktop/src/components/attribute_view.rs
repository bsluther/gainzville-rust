use dioxus::prelude::*;
use dioxus_free_icons::icons::io_icons::IoRemove;
use dioxus_free_icons::Icon;
use gv_core::models::attribute::{MassMeasurement, MassUnit, MassValue, NumericValue, SelectValue};
use gv_core::models::attribute_pair::{
    AttributePair, MassAttributePair, NumericAttributePair, SelectAttributePair,
};

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
    let name = pair.name.clone();
    let integer = pair.config.integer;
    let pills = match pair.actual {
        Some(NumericValue::Exact(v)) => rsx! {
            span { class: "attribute-pill", "{format_numeric(v, integer)}" }
        },
        Some(NumericValue::Range { min, max }) => rsx! {
            span { class: "attribute-pill", "{format_numeric(min, integer)}" }
            Icon {
                width: 12,
                height: 12,
                fill: "var(--gv-neutral-500)",
                icon: IoRemove,
            }
            span { class: "attribute-pill", "{format_numeric(max, integer)}" }
        },
        None => rsx! {
            span { class: "attribute-pill" }
        },
    };
    rsx! {
        AttributeRow { label: name, {pills} }
    }
}

#[component]
fn SelectView(pair: SelectAttributePair) -> Element {
    let name = pair.name.clone();
    let pills = match pair.actual {
        Some(SelectValue::Exact(s)) => rsx! {
            span { class: "attribute-pill", "{s}" }
        },
        Some(SelectValue::Range { min, max }) => rsx! {
            span { class: "attribute-pill", "{min}" }
            Icon {
                width: 12,
                height: 12,
                fill: "var(--gv-neutral-500)",
                icon: IoRemove,
            }
            span { class: "attribute-pill", "{max}" }
        },
        None => rsx! {
            span { class: "attribute-pill" }
        },
    };
    rsx! {
        AttributeRow { label: name, {pills} }
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

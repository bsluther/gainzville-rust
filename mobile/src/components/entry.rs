use dioxus::prelude::*;

const ENTRY_CSS: Asset = asset!("/assets/styling/entry.css");

#[component]
pub fn Entry(is_sequence: bool) -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: ENTRY_CSS }
        div { id: "entry", class: if is_sequence { "sequence" } else { "scalar" },
            div { class: "header", "test" }
        }
    }
}

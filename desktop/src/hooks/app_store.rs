use dioxus::prelude::*;
use uuid::Uuid;

#[allow(unused)]
#[derive(Store)]
struct AppStore {
    selection: Selection,
}

#[allow(unused)]
enum AppSelection {
    None,
    Entry(Uuid),
    Activity(Uuid),
}

#[allow(unused)]
struct Selection {
    id: Uuid,
    selection_type: SelectionType,
}

#[allow(unused)]
enum SelectionType {
    Entry,
    Activity,
}

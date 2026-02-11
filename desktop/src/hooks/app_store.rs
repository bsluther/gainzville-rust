use dioxus::prelude::*;
use uuid::Uuid;

#[derive(Store)]
struct AppStore {
    selection: Selection,
}

enum AppSelection {
    None,
    Entry(Uuid),
    Activity(Uuid),
}

struct Selection {
    id: Uuid,
    selection_type: SelectionType,
}

enum SelectionType {
    Entry,
    Activity,
}

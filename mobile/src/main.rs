use std::fs;

use dioxus::prelude::*;

use gv_sqlite::sandbox::SqliteClient;
use views::{Blog, EntrySandbox, Home, Navbar};

mod components;
mod views;

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
enum Route {
    #[layout(Navbar)]
        #[route("/")]
        Home {},
        // The route attribute can include dynamic parameters that implement [`std::str::FromStr`] and [`std::fmt::Display`] with the `:` syntax.
        // In this case, id will match any integer like `/blog/123` or `/blog/-456`.
        #[route("/blog/:id")]
        Blog { id: i32 },

        #[route("/sandbox/entry")]
        EntrySandbox {}
}

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/styling/main.css");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");
const TOKENS_CSS: Asset = asset!("/assets/styling/tokens.css");
const LOG_CSS: Asset = asset!("/assets/styling/log.css");

#[tokio::main]
async fn main() {
    // On iOS we only have permission to access certain directories in the filesystem.
    // ProjectDirs is a utility for working with standard locations on a given plaform.
    // The iOS directory structure is similar to that for macOS, so this works to find a place
    // where we can read/write to a sqlite database file.
    let proj_dirs = directories::ProjectDirs::from("com", "villa", "gainzville")
        .expect("failed to generate project_dirs");
    let data_dir = proj_dirs.data_dir();
    fs::create_dir_all(data_dir).expect("failed to create data dir");

    let db_file_path = data_dir.join("gv.db");
    // Create the database file if it doesn't exist.
    let db_url = format!("sqlite:{}?mode=rwc", db_file_path.display());

    // let client = Client::init(&db_url).await.expect("failed to init client");
    let client = SqliteClient::init(&db_url)
        .await
        .expect("failed to init client");

    dioxus::LaunchBuilder::new()
        .with_context(client)
        .launch(App);
}
#[component]
fn App() -> Element {
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        document::Link { rel: "stylesheet", href: TOKENS_CSS }
        document::Link { rel: "stylesheet", href: LOG_CSS }

        Router::<Route> {}
    }
}

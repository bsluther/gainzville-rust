use std::fs;

use dioxus::prelude::*;

use gv_core::{actions::CreateAttribute, std_lib};
use gv_sqlite::client::SqliteClient;
use tracing::Level;
use views::{ActivitySandbox, Log, Navbar};

mod components;
mod hooks;
mod views;

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
enum Route {
    #[layout(Navbar)]
        #[route("/")]
        Log {},
        #[route("/sandbox/entry")]
        ActivitySandbox {},
}

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/styling/main.css");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");
const TOKENS_CSS: Asset = asset!("/assets/styling/tokens.css");
const LOG_CSS: Asset = asset!("/assets/styling/log.css");
const ENTRY_CSS: Asset = asset!("/assets/styling/entry.css");
const ATTRIBUTE_CSS: Asset = asset!("/assets/styling/attribute.css");
const DX_COMPONENTS_CSS: Asset = asset!("/assets/dx-components-theme.css");

fn main() {
    let _ = dotenvy::dotenv();

    // On iOS we only have permission to access certain directories in the filesystem.
    // ProjectDirs is a utility for working with standard locations on a given plaform.
    // The iOS directory structure is similar to that for macOS, so this works to find a place
    // where we can read/write to a sqlite database file.
    let proj_dirs = directories::ProjectDirs::from("com", "villa", "gainzville")
        .expect("failed to generate project_dirs");
    let data_dir = proj_dirs.data_dir();
    fs::create_dir_all(data_dir).expect("failed to create data dir");

    let db_file_path = data_dir.join("gv.db");

    // To wipe the existing database, uncomment the next line.
    // let _ = fs::remove_file(&db_file_path);

    // Create the database file if it doesn't exist.
    let db_url = format!("sqlite:{}?mode=rwc", db_file_path.display());

    // Create a short-lived Tokio runtime for initialization
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to build tokio runtime");

    let client = rt
        .block_on(async { SqliteClient::init(&db_url).await })
        .expect("failed to init client");

    // Populate database with attributes from the "standard library".
    // for attr in std_lib::StandardLibrary::attributes() {
    //     let action: CreateAttribute = attr.into();
    //     rt.block_on(async { client.run_action(action.into()).await })
    //         .expect("failed to create std_lib attributes")
    // }

    dioxus::logger::init(Level::DEBUG).expect("failed to init logger");

    dioxus::LaunchBuilder::new()
        .with_context(client)
        .with_cfg(desktop! {
            dioxus::desktop::Config::new()
                .with_window(
                    dioxus::desktop::WindowBuilder::new()
                        .with_title("Gainzville")
                        .with_inner_size(dioxus::desktop::LogicalSize::new(1200.0, 800.0))
                )
        })
        .launch(App);
}
#[component]
fn App() -> Element {
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        document::Link { rel: "stylesheet", href: DX_COMPONENTS_CSS }
        document::Link { rel: "stylesheet", href: TOKENS_CSS }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Link { rel: "stylesheet", href: LOG_CSS }
        document::Link { rel: "stylesheet", href: ENTRY_CSS }
        document::Link { rel: "stylesheet", href: ATTRIBUTE_CSS }

        Router::<Route> {}
    }
}

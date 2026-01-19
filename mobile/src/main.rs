use std::fs;

use dioxus::prelude::*;

use gv_core::{models::activity::ActivityName, SYSTEM_ACTOR_ID};
use gv_sqlite::client::Client;
use views::{Blog, EntrySandbox, Home, Navbar};

/// Define a components module that contains all shared components for our app.
mod components;
/// Define a views module that contains the UI for all Layouts and Routes for our app.
mod views;

/// The Route enum is used to define the structure of internal routes in our app. All route enums need to derive
/// the [`Routable`] trait, which provides the necessary methods for the router to work.
/// 
/// Each variant represents a different URL pattern that can be matched by the router. If that pattern is matched,
/// the components for that route will be rendered.
#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
enum Route {
    // The layout attribute defines a wrapper for all routes under the layout. Layouts are great for wrapping
    // many routes with a common UI like a navbar.
    #[layout(Navbar)]
        // The route attribute defines the URL pattern that a specific route matches. If that pattern matches the URL,
        // the component for that route will be rendered. The component name that is rendered defaults to the variant name.
        #[route("/")]
        Home {},
        // The route attribute can include dynamic parameters that implement [`std::str::FromStr`] and [`std::fmt::Display`] with the `:` syntax.
        // In this case, id will match any integer like `/blog/123` or `/blog/-456`.
        #[route("/blog/:id")]
        // Fields of the route variant will be passed to the component as props. In this case, the blog component must accept
        // an `id` prop of type `i32`.
        Blog { id: i32 },
        #[route("/sandbox/entry")]
        EntrySandbox {}
}

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/styling/main.css");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");
const TOKENS_CSS: Asset = asset!("/assets/styling/tokens.css");
const LOG_CSS: Asset = asset!("/assets/styling/log.css");
// fn main() {
//     // The `launch` function is the main entry point for a dioxus app. It takes a component and renders it with the platform feature
//     // you have enabled
//     dioxus::launch(App);
// }
#[tokio::main]
async fn main() {
    let proj_dirs = directories::ProjectDirs::from("com", "villa", "gainzville")
        .expect("failed to generate project_dirs");
    let data_dir = proj_dirs.data_dir();
    fs::create_dir_all(data_dir).expect("failed to create data dir");

    let db_file_path = data_dir.join("gv.db");
    let db_url = format!("sqlite:{}?mode=rwc", db_file_path.display());

    let client = Client::init(&db_url).await.expect("failed to init client");

    dioxus::LaunchBuilder::new()
        .with_context(client)
        .launch(App);
}
#[component]
fn App() -> Element {
    // let db_test_result = use_resource(|| async {
    //     // On iOS we only have permission to access certain directories in the filesystem.
    //     // ProjectDirs is a utility for working with standard locations on a given plaform.
    //     // The iOS directory structure is similar to that for macOS, so this works to find a place
    //     // where we can read/write to a sqlite database file.
    //     let proj_dirs = directories::ProjectDirs::from("com", "villa", "gainzville")
    //         .expect("failed to generate project_dirs");
    //     let data_dir = proj_dirs.data_dir();

    //     // Try to create the directory
    //     if let Err(e) = fs::create_dir_all(data_dir) {
    //         return format!("Failed to create dir: {e}");
    //     }

    //     // Build SQLite connection string with create mode
    //     let db_file_path = data_dir.join("gv.db");
    //     let db_url = format!("sqlite:{}?mode=rwc", db_file_path.display());

    //     // Try to initialize gv_sqlite
    //     // let controller = match gv_sqlite::init(&db_url).await {
    //     //     Ok(c) => c,
    //     //     Err(e) => return format!("SQLite init failed: {e}\nPath: {}", db_file_path.display()),
    //     // };
    //     let client = match gv_sqlite::client::Client::init(&db_url).await {
    //         Ok(c) => c,
    //         Err(e) => return format!("SQLite init failed: {e}\nPath: {}", db_file_path.display()),
    //     };

    //     // Run migrations
    //     // if let Err(e) = gv_sqlite::run_migrations(&controller).await {
    //     //     return format!("Migration failed: {e}\nPath: {}", db_file_path.display());
    //     // }
    //     let create_pushups_activity: gv_core::actions::CreateActivity =
    //         gv_core::models::activity::Activity {
    //             id: uuid::Uuid::new_v4(),
    //             owner_id: SYSTEM_ACTOR_ID,
    //             name: ActivityName::parse("Push Ups".to_string()).unwrap(),
    //             description: Some("Lie prone and push yourself up.".to_string()),
    //             source_activity_id: None,
    //         }
    //         .into();
    //     if let Err(e) = client
    //         .controller
    //         .run_action(create_pushups_activity.into())
    //         .await
    //     {
    //         return format!("Action failed: {e}");
    //     }

    //     format!(
    //         "SQLite ready!\nPath: {}\nMigrations: applied\nAction ran successfully.",
    //         db_file_path.display()
    //     )
    // });

    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        document::Link { rel: "stylesheet", href: TOKENS_CSS }
        document::Link { rel: "stylesheet", href: LOG_CSS }

        // Display database test result
        div {
            class: "p-5 text-gray-900 bg-gray-100 m-2.5 rounded-lg font-mono whitespace-pre-wrap",
            h3 { "SQLite Test:" }
                // match &*db_test_result.read() {
        //     Some(result) => rsx! {
        //         p { "{result}" }
        //     },
        //     None => rsx! {
        //         p { "Initializing SQLite..." }
        //     },
        // }
        }

        Router::<Route> {}
    }
}

//! gv-import: the import tool surface.
//!
//! `mcp` serves the import tools over stdio MCP (the agent-facing surface —
//! docs/import-design.md); `init` seeds an import workspace; `snapshot`
//! copies the DB aside; `check` diffs candidate artifacts against the
//! gold set.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::Arc;

use chrono::NaiveDate;
use gv_client::client::SqliteClient;
use gv_core::actions::CreateAttribute;
use gv_core::models::activity::{Activity, ActivityName};
use gv_core::models::attribute::{Attribute, AttributeConfig};
use gv_core::queries::{AllActivities, AllAttributes, FindValuesForEntries};
use gv_import::document::DayDocument;
use gv_import::import::{ImportConfig, ImportReport, Importer};
use gv_import::registry::Registry;
use gv_import::workspace::{Workspace, check_against_gold};
use rmcp::{
    ErrorData as McpError, Json, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
    transport::stdio,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Parser)]
#[command(name = "gv-import", about = "LLM import tools for Gainzville")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Serve the import tools as a stdio MCP server.
    Mcp {
        /// Path to the SQLite database file (a scratch copy of the app DB).
        #[arg(long)]
        db: PathBuf,
        /// Import workspace directory (artifacts, aliases, questions, snapshots).
        #[arg(long)]
        workspace: PathBuf,
        /// IANA timezone the log's wall-clock times are anchored in.
        #[arg(long, default_value = "America/Denver")]
        timezone: String,
        /// Registered source name — part of every imported entry's identity;
        /// keep it stable across runs of one source.
        #[arg(long, default_value = "obsidian-training-log")]
        source: String,
        /// Skip the pre-run DB snapshot (on by default).
        #[arg(long)]
        no_snapshot: bool,
    },
    /// Create/seed an import workspace.
    Init {
        #[arg(long)]
        workspace: PathBuf,
    },
    /// Snapshot the DB (with -wal/-shm) into the workspace.
    Snapshot {
        #[arg(long)]
        db: PathBuf,
        #[arg(long)]
        workspace: PathBuf,
    },
    /// Diff candidate artifacts against gold-set artifacts (volatile fields
    /// ignored). Exits non-zero on any mismatch.
    Check {
        #[arg(long)]
        approved: PathBuf,
        #[arg(long)]
        candidate: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // stdout belongs to the MCP protocol; all logging goes to stderr.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_max_level(tracing::Level::INFO)
        .init();

    match Cli::parse().command {
        Command::Init { workspace } => {
            Workspace::new(&workspace).init()?;
            eprintln!("workspace ready at {}", workspace.display());
            Ok(())
        }
        Command::Snapshot { db, workspace } => {
            let dir = snapshot(&Workspace::new(&workspace), &db)?;
            eprintln!("snapshot at {}", dir.display());
            Ok(())
        }
        Command::Check {
            approved,
            candidate,
        } => {
            let mismatches = check_against_gold(&approved, &candidate)?;
            if mismatches.is_empty() {
                eprintln!("check clean");
                Ok(())
            } else {
                for m in &mismatches {
                    eprintln!("{m}");
                }
                anyhow::bail!("{} mismatch(es)", mismatches.len());
            }
        }
        Command::Mcp {
            db,
            workspace,
            timezone,
            source,
            no_snapshot,
        } => {
            let workspace = Workspace::new(&workspace);
            workspace.init()?;
            if !no_snapshot {
                let dir = snapshot(&workspace, &db)?;
                tracing::info!("pre-run snapshot at {}", dir.display());
            }
            let tz: chrono_tz::Tz = timezone
                .parse()
                .map_err(|e| anyhow::anyhow!("bad timezone '{timezone}': {e}"))?;
            let client = SqliteClient::init(&format!("sqlite://{}", db.display()))
                .await
                .context("opening database")?;
            let importer = Importer {
                client,
                config: ImportConfig {
                    actor_id: gv_core::constants::DEFAULT_USER_ID,
                    timezone: tz,
                },
            };
            let server = GvImportServer::new(importer, workspace, source);
            tracing::info!("gv-import MCP server on stdio");
            let service = server.serve(stdio()).await?;
            service.waiting().await?;
            Ok(())
        }
    }
}

fn snapshot(workspace: &Workspace, db: &std::path::Path) -> Result<PathBuf> {
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%3fZ").to_string();
    workspace.snapshot_db(db, &stamp)
}

// ---------- MCP server ----------

#[derive(Clone)]
struct GvImportServer {
    importer: Arc<Importer>,
    workspace: Arc<Workspace>,
    source: String,
    /// Serializes mutating tool calls: agents batch parallel tool calls, and
    /// the exists-then-create sequences and questions-file read-modify-write
    /// are not safe to interleave.
    write_lock: Arc<tokio::sync::Mutex<()>>,
    tool_router: ToolRouter<Self>,
}

fn internal(e: impl std::fmt::Display) -> McpError {
    McpError::internal_error(e.to_string(), None)
}

fn invalid(e: impl std::fmt::Display) -> McpError {
    McpError::invalid_params(e.to_string(), None)
}

fn domain_error(e: gv_core::error::DomainError) -> McpError {
    match e {
        gv_core::error::DomainError::Rejected(_) => invalid(e),
        _ => internal(e),
    }
}

impl GvImportServer {
    fn new(importer: Importer, workspace: Workspace, source: String) -> Self {
        GvImportServer {
            importer: Arc::new(importer),
            workspace: Arc::new(workspace),
            source,
            write_lock: Arc::new(tokio::sync::Mutex::new(())),
            tool_router: Self::tool_router(),
        }
    }

    /// Fresh registry per call: DB reads are cheap at this scale, and
    /// re-reading aliases.toml means edits made in the resolution session
    /// apply live.
    async fn registry(&self) -> Result<Registry, McpError> {
        let activities = self
            .importer
            .client
            .run_query(AllActivities {})
            .await
            .map_err(internal)?;
        let attributes = self
            .importer
            .client
            .run_query(AllAttributes {})
            .await
            .map_err(internal)?;
        let aliases = self.workspace.load_aliases().map_err(internal)?;
        Ok(Registry::new(activities, attributes, aliases))
    }
}

// ---------- Tool parameter / result shapes ----------

#[derive(Deserialize, schemars::JsonSchema)]
struct GetDayRequest {
    /// The day to inspect, ISO 8601 (e.g. "2026-02-24").
    date: NaiveDate,
}

#[derive(Serialize, schemars::JsonSchema)]
struct DayView {
    /// Root entries of the day, with their subtrees.
    entries: Vec<DayEntryView>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct DayEntryView {
    id: Uuid,
    /// Entry name, or its activity's name.
    label: String,
    is_sequence: bool,
    /// Local wall-clock times, human-rendered.
    #[serde(skip_serializing_if = "Option::is_none")]
    start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    end: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_min: Option<i64>,
    /// Attribute name → stored value (actual), core serde encoding.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    values: std::collections::BTreeMap<String, serde_json::Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    children: Vec<DayEntryView>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ImportDayRequest {
    /// The day-document to apply. See the import skill/glossary for the
    /// authoring rules; dates come from filenames, never from log text.
    document: DayDocument,
    /// Rehearse without writing (default false).
    #[serde(default)]
    dry_run: bool,
}

#[derive(Serialize, schemars::JsonSchema)]
struct ImportDayResult {
    report: ImportReport,
    /// Where the applied document was persisted (non-dry runs).
    #[serde(skip_serializing_if = "Option::is_none")]
    artifact: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct CreateActivityRequest {
    name: String,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct CreateAttributeRequest {
    name: String,
    #[serde(default)]
    description: Option<String>,
    /// Externally-tagged core AttributeConfig JSON. Examples:
    /// {"Text": {"default": null, "autocomplete": true}} ·
    /// {"Select": {"options": ["flash","sent"], "ordered": false, "default": null}} ·
    /// {"Multiselect": {"options": ["crimpy"], "default": null}} ·
    /// {"Numeric": {"min": 0, "max": 10, "integer": false, "default": null}} ·
    /// {"Mass": {"default_unit": "Pound"}} · {"Length": {"default_unit": "Millimeter"}}
    // Pin the schema to an object so MCP clients that stringify untyped params
    // pass a JSON object, not a string (serde_json::Value's default schema is
    // untyped). Runtime type stays Value; from_value handles the object.
    #[schemars(with = "std::collections::BTreeMap<String, serde_json::Value>")]
    config: serde_json::Value,
}

#[derive(Serialize, schemars::JsonSchema)]
struct CreatedSchema {
    id: Uuid,
    name: String,
}

#[derive(Serialize, schemars::JsonSchema)]
struct RegistryView {
    activities: Vec<RegistryActivity>,
    attributes: Vec<RegistryAttribute>,
    /// Surface form → canonical name (from the workspace aliases.toml).
    activity_aliases: std::collections::BTreeMap<String, String>,
    attribute_aliases: std::collections::BTreeMap<String, String>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct RegistryActivity {
    id: Uuid,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct RegistryAttribute {
    id: Uuid,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    /// Full core config (externally tagged) — options, bounds, units.
    config: serde_json::Value,
}

// ---------- Tools ----------

#[tool_router]
impl GvImportServer {
    #[tool(
        description = "The import registry: all activities and attributes (with configs) plus the alias tables. Day-documents may only reference these names; propose anything new via create_activity/create_attribute after resolving with the user."
    )]
    async fn registry_snapshot(&self) -> Result<Json<RegistryView>, McpError> {
        let registry = self.registry().await?;
        let activities = registry
            .activities
            .iter()
            .map(|a| RegistryActivity {
                id: a.id,
                name: a.name.to_string(),
                description: a.description.clone(),
            })
            .collect();
        let attributes = registry
            .attributes
            .iter()
            .map(|a| {
                Ok(RegistryAttribute {
                    id: a.id,
                    name: a.name.clone(),
                    description: a.description.clone(),
                    config: serde_json::to_value(&a.config).map_err(internal)?,
                })
            })
            .collect::<Result<_, McpError>>()?;
        Ok(Json(RegistryView {
            activities,
            attributes,
            activity_aliases: registry.aliases.activities.clone(),
            attribute_aliases: registry.aliases.attributes.clone(),
        }))
    }

    #[tool(
        description = "Existing entries for a day (local time), with values — check before importing, and for dedup questions."
    )]
    async fn get_day(
        &self,
        Parameters(req): Parameters<GetDayRequest>,
    ) -> Result<Json<DayView>, McpError> {
        let registry = self.registry().await?;
        let roots = self
            .importer
            .day_entries(req.date)
            .await
            .map_err(internal)?;

        let mut views = Vec::with_capacity(roots.len());
        for (root, descendants) in roots {
            let mut ids: Vec<Uuid> = vec![root.id];
            ids.extend(descendants.iter().map(|e| e.id));
            let values = self
                .importer
                .client
                .run_query(FindValuesForEntries { entry_ids: ids })
                .await
                .map_err(internal)?;
            views.push(
                day_view(
                    &root,
                    &descendants,
                    &values,
                    &registry,
                    self.importer.config.timezone,
                )
                .map_err(internal)?,
            );
        }
        Ok(Json(DayView { entries: views }))
    }

    #[tool(
        description = "Apply (or dry-run) a day-document: resolves names via the registry, mints deterministic ids (re-runs skip what already exists), validates values, creates entries/values, persists the artifact, and appends the document's questions to the workspace questions file. Check report.ok and per-entry statuses."
    )]
    async fn import_day(
        &self,
        Parameters(req): Parameters<ImportDayRequest>,
    ) -> Result<Json<ImportDayResult>, McpError> {
        let _write_guard = self.write_lock.lock().await;
        let mut doc = req.document;
        gv_import::workspace::validate_source_file(&doc.source_file).map_err(invalid)?;
        if doc.source != self.source {
            return Err(invalid(format!(
                "document source '{}' does not match this server's source '{}'",
                doc.source, self.source
            )));
        }
        let registry = self.registry().await?;
        let report = self
            .importer
            .import_day(&registry, &doc, req.dry_run)
            .await
            .map_err(internal)?;

        let mut artifact = None;
        if !req.dry_run {
            doc.applied = Some(report.applied());
            let path = self.workspace.write_artifact(&doc).map_err(internal)?;
            self.workspace
                .append_questions(&doc.source_file, &doc.questions)
                .map_err(internal)?;
            artifact = Some(path.display().to_string());
        }
        Ok(Json(ImportDayResult { report, artifact }))
    }

    #[tool(
        description = "Create a new activity (with a minimal template). Only after the name has been resolved with the user in the schema workflow — never invent activities mid-import."
    )]
    async fn create_activity(
        &self,
        Parameters(req): Parameters<CreateActivityRequest>,
    ) -> Result<Json<CreatedSchema>, McpError> {
        let _write_guard = self.write_lock.lock().await;
        let name = ActivityName::parse(req.name.clone()).map_err(invalid)?;
        let activity = Activity {
            id: Uuid::new_v4(),
            owner_id: self.importer.config.actor_id,
            name,
            description: req.description,
            source_activity_id: None,
        };
        let action = activity.into_create_activity(Uuid::new_v4());
        self.importer
            .client
            .run_action(action.into())
            .await
            .map_err(domain_error)?;
        Ok(Json(CreatedSchema {
            id: activity.id,
            name: req.name,
        }))
    }

    #[tool(
        description = "Create a new attribute. Only after the name/config has been resolved with the user in the schema workflow. Configs are additive-only, so include the full expected option set up front."
    )]
    async fn create_attribute(
        &self,
        Parameters(req): Parameters<CreateAttributeRequest>,
    ) -> Result<Json<CreatedSchema>, McpError> {
        let _write_guard = self.write_lock.lock().await;
        let config: AttributeConfig = serde_json::from_value(req.config)
            .map_err(|e| invalid(format!("bad config: {e}")))?;
        let attribute = Attribute {
            id: Uuid::new_v4(),
            owner_id: self.importer.config.actor_id,
            name: req.name.clone(),
            description: req.description,
            config,
        };
        self.importer
            .client
            .run_action(
                CreateAttribute {
                    actor_id: self.importer.config.actor_id,
                    attribute: attribute.clone(),
                }
                .into(),
            )
            .await
            .map_err(domain_error)?;
        Ok(Json(CreatedSchema {
            id: attribute.id,
            name: req.name,
        }))
    }
}

#[tool_handler]
impl ServerHandler for GvImportServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.instructions = Some(
            "Import tools for Gainzville training data. Workflow: registry_snapshot to see \
             what exists; get_day to inspect a day's current state; import_day (dry_run \
             first when unsure) to apply a day-document. create_activity/create_attribute \
             only for schema the user has approved. Never invent registry names — unknown \
             shorthand becomes a question + skip, not a guess."
                .to_string(),
        );
        info
    }
}

// ---------- get_day rendering ----------

fn day_view(
    entry: &gv_core::models::entry::Entry,
    descendants: &[gv_core::models::entry::Entry],
    values: &[gv_core::models::attribute::Value],
    registry: &Registry,
    tz: chrono_tz::Tz,
) -> Result<DayEntryView> {
    let label = entry
        .name
        .clone()
        .or_else(|| {
            entry
                .activity_id
                .and_then(|id| registry.activity_by_id(id))
                .map(|a| a.name.to_string())
        })
        .unwrap_or_else(|| "(anonymous)".to_string());

    let local =
        |t: chrono::DateTime<chrono::Utc>| t.with_timezone(&tz).format("%-I:%M%P").to_string();

    let mut entry_values = std::collections::BTreeMap::new();
    for value in values.iter().filter(|v| v.entry_id == entry.id) {
        if let Some(actual) = &value.actual {
            let name = registry
                .attribute_by_id(value.attribute_id)
                .map(|a| a.name.clone())
                .unwrap_or_else(|| value.attribute_id.to_string());
            entry_values.insert(name, serde_json::to_value(actual)?);
        }
    }

    let mut children: Vec<&gv_core::models::entry::Entry> = descendants
        .iter()
        .filter(|e| e.parent_id() == Some(entry.id))
        .collect();
    children.sort_by(|a, b| a.frac_index().cmp(&b.frac_index()));

    Ok(DayEntryView {
        id: entry.id,
        label,
        is_sequence: entry.is_sequence,
        start: entry.temporal.start().map(local),
        end: entry.temporal.end().map(local),
        duration_min: entry.temporal.duration().map(|ms| (ms as i64) / 60_000),
        values: entry_values,
        children: children
            .into_iter()
            .map(|c| day_view(c, descendants, values, registry, tz))
            .collect::<Result<_>>()?,
    })
}

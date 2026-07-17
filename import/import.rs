//! The `import_day` handler: day-document in, actions out, report back.
//!
//! Two phases. **Plan** (pure, synchronous): resolve names against the
//! registry, mint deterministic ids, parse temporals, convert values, check
//! the shape rules — every problem becomes a recorded error on its node, so
//! one bad line never sinks the file. **Execute** (async): existence checks
//! against the DB (idempotent re-runs collide-and-skip by id), fractional
//! indices from document order, and `run_action` per create. `dry_run` stops
//! after the existence checks.
//!
//! The model interprets, the handler administers (docs/import-design.md).

use crate::document::{Applied, DayDocument, DocEntry, SCHEMA_VERSION};
use crate::ident;
use crate::registry::Registry;
use crate::temporal::build_temporal;
use crate::values;
use anyhow::{Context, bail};
use chrono::{NaiveDate, Utc};
use chrono_tz::Tz;
use fractional_index::FractionalIndex;
use gv_client::client::SqliteClient;
use gv_core::actions::{CreateEntry, CreateValue, EntryChange, UpdateEntry};
use gv_core::error::DomainError;
use gv_core::models::attribute::{AttributeValue, Value};
use gv_core::models::entry::{Entry, Position, Temporal};
use gv_core::queries::{EntriesRootedInTimeInterval, FindDescendants, FindEntryById, FindValueByKey};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ImportConfig {
    /// Actor and owner of everything imported (the app's default user).
    pub actor_id: Uuid,
    /// Timezone the log's wall-clock times are anchored in.
    pub timezone: Tz,
}

pub struct Importer {
    pub client: SqliteClient,
    pub config: ImportConfig,
}

// ---------- Report ----------

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EntryStatus {
    /// Dry run: would be created.
    Planned,
    Created,
    /// Already present under its deterministic id — left untouched.
    Existed,
    /// This node had plan errors or was rejected on apply.
    Failed,
    /// An ancestor failed, so this node was never attempted.
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValueStatus {
    Planned,
    Created,
    /// A value for (entry, attribute) already exists — never overwritten, so
    /// re-runs can't clobber edits made in the app since import.
    Existed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ValueOutcome {
    pub attribute: String,
    pub status: ValueStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EntryOutcome {
    /// Tree-path of the entry within the document (also its identity key).
    pub path: String,
    pub id: Uuid,
    pub status: EntryStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<ValueOutcome>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ImportReport {
    pub dry_run: bool,
    pub source_file: String,
    pub date: NaiveDate,
    pub entries: Vec<EntryOutcome>,
    /// Document-level problems (bad schema version, day-membership warnings).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// Ids of all entries confirmed present (created now or previously) —
    /// what gets written back into the artifact's `applied.entry_ids`.
    pub entry_ids: Vec<Uuid>,
    pub ok: bool,
}

impl ImportReport {
    /// The `applied` block to write back into the artifact after a non-dry
    /// apply.
    pub fn applied(&self) -> Applied {
        Applied {
            imported_at: Utc::now(),
            entry_ids: self.entry_ids.clone(),
        }
    }
}

// ---------- Plan ----------

struct PlannedValue {
    attribute_id: Uuid,
    attribute_name: String,
    value: AttributeValue,
}

struct PlanNode {
    path: String,
    id: Uuid,
    parent: Option<usize>,
    activity_id: Option<Uuid>,
    name: Option<String>,
    is_sequence: bool,
    display_as_sets: bool,
    temporal: Temporal,
    values: Vec<PlannedValue>,
    value_errors: Vec<ValueOutcome>,
    errors: Vec<String>,
}

/// The identity label an entry contributes to its tree-path component:
/// **registry-canonical** activity name where one resolves (so alias/casing
/// wobble between extraction runs can't re-key ids), else the surface form,
/// else the entry name — all sanitized (trim/lowercase/defuse '/' and '#').
fn canonical_label(registry: &Registry, entry: &DocEntry) -> String {
    let surface = match (&entry.activity, &entry.name) {
        (Some(activity), _) => registry
            .resolve_activity(activity)
            .map(|a| a.name.to_string())
            .unwrap_or_else(|_| activity.clone()),
        (None, Some(name)) => name.clone(),
        (None, None) => "anonymous".to_string(),
    };
    ident::sanitize_label(&surface)
}

/// DFS pre-order plan: parents always precede children, so the execute phase
/// can run as a flat sequential pass.
fn plan(registry: &Registry, doc: &DayDocument, tz: Tz) -> Vec<PlanNode> {
    let mut nodes: Vec<PlanNode> = Vec::new();
    // (entry, parent index in `nodes`, tree-path). Reversed pushes keep
    // document order under pop().
    let mut stack: Vec<(&DocEntry, Option<usize>, String)> = Vec::new();

    let sibling_components = |siblings: &[DocEntry]| {
        let labels: Vec<String> = siblings
            .iter()
            .map(|e| canonical_label(registry, e))
            .collect();
        ident::components(&labels)
    };

    for (entry, component) in doc.entries.iter().zip(sibling_components(&doc.entries)).rev() {
        stack.push((entry, None, component));
    }

    while let Some((entry, parent, path)) = stack.pop() {
        let node_index = nodes.len();
        nodes.push(plan_node(registry, doc, tz, entry, parent, &path));

        for (child, component) in entry.children.iter().zip(sibling_components(&entry.children)).rev()
        {
            stack.push((child, Some(node_index), format!("{path}/{component}")));
        }
    }
    nodes
}

fn plan_node(
    registry: &Registry,
    doc: &DayDocument,
    tz: Tz,
    entry: &DocEntry,
    parent: Option<usize>,
    path: &str,
) -> PlanNode {
    let mut errors = Vec::new();

    let activity_id = match &entry.activity {
        Some(name) => match registry.resolve_activity(name) {
            Ok(a) => Some(a.id),
            Err(e) => {
                errors.push(e.to_string());
                None
            }
        },
        None => None,
    };

    let is_sequence = entry.sequence.unwrap_or(!entry.children.is_empty());
    if !entry.children.is_empty() && entry.sequence == Some(false) {
        errors.push("entry has children but sequence: false".to_string());
    }

    // A `display_as_sets` sequence is the "sets" shape: an exercise (Bench Press)
    // whose children are its sets — same activity on the node and every child. A
    // climbing session (activity Climbing) holding Boulder children is NOT sets,
    // so requiring the node's own activity to match the children's is what
    // separates the two. The importer only sets the flag; the sets UI reads it.
    let display_as_sets = is_sequence
        && activity_id.is_some()
        && !entry.children.is_empty()
        && entry.children.iter().all(|child| {
            child
                .activity
                .as_ref()
                .and_then(|name| registry.resolve_activity(name).ok())
                .map(|a| a.id)
                == activity_id
        });

    if entry.name.as_deref().is_some_and(|n| n.trim().is_empty()) {
        errors.push("entry name is empty".to_string());
    }

    // Anonymous naming rules (docs/model.md): anonymous scalars need a name
    // to render as; anonymous sequences render as their members and take none.
    if entry.activity.is_none() {
        match (is_sequence, &entry.name) {
            (false, None) => errors.push("anonymous scalar entry must have a name".to_string()),
            (true, Some(_)) => errors.push("anonymous sequence entry may not have a name".to_string()),
            _ => {}
        }
    }

    let temporal = match build_temporal(
        doc.date,
        tz,
        entry.start.as_deref(),
        entry.end.as_deref(),
        entry.duration.as_deref(),
    ) {
        Ok(t) => t,
        Err(e) => {
            errors.push(e.to_string());
            Temporal::None
        }
    };
    if parent.is_none() && temporal.start().is_none() && temporal.end().is_none() {
        errors.push("root entry must have a start or end time".to_string());
    }

    // A stated duration that contradicts the start-end span signals a
    // misparse (or an end on the wrong side of midnight) - surface it rather
    // than silently discarding the duration.
    if let (Some(_), Some(_), Some(stated)) = (&entry.start, &entry.end, &entry.duration)
        && let Ok(stated_ms) = crate::temporal::parse_duration_ms(stated)
        && let Some(span_ms) = temporal.infer_duration_ms()
        && (span_ms - stated_ms as i64).abs() > 60_000
    {
        errors.push(format!(
            "stated duration '{stated}' contradicts the start-end span ({} min)",
            span_ms / 60_000
        ));
    }

    let mut values = Vec::new();
    let mut value_errors = Vec::new();
    for (attr_name, raw) in &entry.attributes {
        match registry
            .resolve_attribute(attr_name)
            .map_err(|e| e.to_string())
            .and_then(|attribute| {
                values::convert(attribute, raw)
                    .map(|v| (attribute, v))
                    .map_err(|e| e.to_string())
            }) {
            Ok((attribute, value)) => values.push(PlannedValue {
                attribute_id: attribute.id,
                attribute_name: attribute.name.clone(),
                value,
            }),
            Err(detail) => value_errors.push(ValueOutcome {
                attribute: attr_name.clone(),
                status: ValueStatus::Failed,
                detail: Some(detail),
            }),
        }
    }

    PlanNode {
        path: path.to_string(),
        id: ident::entry_id(&doc.source, &doc.source_file, doc.date, path),
        parent,
        activity_id,
        name: entry.name.clone(),
        is_sequence,
        display_as_sets,
        temporal,
        values,
        value_errors,
        errors,
    }
}

// ---------- Execute ----------

impl Importer {
    /// Apply (or, with `dry_run`, rehearse) a day-document. Per-node problems
    /// land in the report; only infrastructure failures (database errors,
    /// invariant violations) return `Err`.
    pub async fn import_day(
        &self,
        registry: &Registry,
        doc: &DayDocument,
        dry_run: bool,
    ) -> anyhow::Result<ImportReport> {
        if doc.schema_version != SCHEMA_VERSION {
            bail!(
                "document schema_version {} unsupported (loader speaks {})",
                doc.schema_version,
                SCHEMA_VERSION
            );
        }

        let nodes = plan(registry, doc, self.config.timezone);

        let mut outcomes: Vec<EntryOutcome> = Vec::with_capacity(nodes.len());
        // Last fractional index handed out per parent this run.
        let mut last_index: HashMap<Uuid, FractionalIndex> = HashMap::new();
        let mut entry_ids = Vec::new();

        for node in &nodes {
            let parent_outcome = node.parent.map(|i| &outcomes[i]);
            let parent_failed = parent_outcome
                .is_some_and(|p| matches!(p.status, EntryStatus::Failed | EntryStatus::Blocked));

            if parent_failed {
                outcomes.push(EntryOutcome {
                    path: node.path.clone(),
                    id: node.id,
                    status: EntryStatus::Blocked,
                    detail: Some("ancestor failed".to_string()),
                    // Keep per-value diagnostics so a layered problem doesn't
                    // cost an extra fix-and-rerun round trip.
                    values: node.value_errors.clone(),
                });
                continue;
            }
            if !node.errors.is_empty() {
                outcomes.push(EntryOutcome {
                    path: node.path.clone(),
                    id: node.id,
                    status: EntryStatus::Failed,
                    detail: Some(node.errors.join("; ")),
                    values: node.value_errors.clone(),
                });
                continue;
            }

            let existing = self
                .client
                .run_query(FindEntryById { entry_id: node.id })
                .await
                .context("FindEntryById")?;

            let (mut status, mut detail) = match (&existing, dry_run) {
                (Some(_), _) => (EntryStatus::Existed, None),
                (None, true) => (EntryStatus::Planned, None),
                (None, false) => (EntryStatus::Created, None),
            };

            // A previously-imported scalar that the re-extraction now gives
            // children (e.g. after a question was answered) must become a
            // sequence, or core's placement guard rejects every child forever.
            if let Some(existing) = &existing {
                if node.is_sequence && !existing.is_sequence {
                    if dry_run {
                        detail = Some("would be promoted to a sequence".to_string());
                    } else {
                        match self.promote_to_sequence(existing).await? {
                            Ok(()) => detail = Some("promoted to a sequence".to_string()),
                            Err(reason) => {
                                status = EntryStatus::Failed;
                                detail = Some(format!("cannot promote to a sequence: {reason}"));
                            }
                        }
                    }
                } else if !node.is_sequence && existing.is_sequence {
                    // Never demote — SetIsSequence(false) deep-deletes children.
                    detail =
                        Some("exists as a sequence; document says scalar - left as-is".to_string());
                }
            }

            if existing.is_none() && !dry_run {
                let position = match node.parent {
                    None => None,
                    Some(i) => {
                        let parent_id = outcomes[i].id;
                        Some(Position {
                            parent_id,
                            frac_index: self.next_index(&mut last_index, parent_id).await?,
                        })
                    }
                };
                let entry = Entry {
                    id: node.id,
                    activity_id: node.activity_id,
                    owner_id: self.config.actor_id,
                    name: node.name.clone(),
                    position,
                    is_template: false,
                    // Core rejects display_as_sets on a fresh entry (the shape
                    // isn't there yet). Flipped on in a post-pass once members
                    // exist — see below.
                    display_as_sets: false,
                    is_sequence: node.is_sequence,
                    // Historical imports are completed actuals; sequences
                    // derive their completion from members.
                    is_complete: !node.is_sequence,
                    temporal: node.temporal.clone(),
                };
                let action = CreateEntry {
                    actor_id: self.config.actor_id,
                    entry,
                };
                match self.client.run_action(action.into()).await {
                    Ok(_) => {
                        // Mid-insertion order is not preserved on re-runs:
                        // new children of an existing parent append at the end.
                        if let Some(i) = node.parent
                            && matches!(outcomes[i].status, EntryStatus::Existed)
                        {
                            detail = Some(
                                "appended after existing children (document order not preserved for mid-insertions)"
                                    .to_string(),
                            );
                        }
                    }
                    Err(DomainError::Rejected(reason)) => {
                        status = EntryStatus::Failed;
                        detail = Some(reason.to_string());
                    }
                    Err(e) => return Err(e).context("create entry"),
                }
            }

            let mut value_outcomes = node.value_errors.clone();
            if !matches!(status, EntryStatus::Failed) {
                for planned in &node.values {
                    value_outcomes.push(self.apply_value(node.id, planned, dry_run, status).await?);
                }
            }
            // Only entries confirmed present — Planned ids on dry runs are
            // phantoms and must not leak into applied provenance.
            if matches!(status, EntryStatus::Created | EntryStatus::Existed) {
                entry_ids.push(node.id);
            }

            outcomes.push(EntryOutcome {
                path: node.path.clone(),
                id: node.id,
                status,
                detail,
                values: value_outcomes,
            });
        }

        // Sets post-pass: an exercise sequence (Bench Press with set children)
        // gets display_as_sets, but core rejects the flag on a fresh entry and
        // requires the members to exist first — so flip it on only now that the
        // whole subtree is created. Idempotent: SetDisplayAsSets no-ops when the
        // flag already matches, so re-runs cost nothing.
        let mut warnings = Vec::new();
        if !dry_run {
            for (node, outcome) in nodes.iter().zip(&outcomes) {
                if !node.display_as_sets
                    || !matches!(outcome.status, EntryStatus::Created | EntryStatus::Existed)
                {
                    continue;
                }
                let update = UpdateEntry {
                    actor_id: self.config.actor_id,
                    entry_id: node.id,
                    change: EntryChange::SetDisplayAsSets(true),
                };
                match self.client.run_action(update.into()).await {
                    Ok(_) => {}
                    Err(DomainError::Rejected(reason)) => {
                        warnings.push(format!(
                            "{}: could not mark as sets: {reason}",
                            node.path
                        ));
                    }
                    Err(e) => return Err(e).context("set display_as_sets"),
                }
            }
        }

        for (node, outcome) in nodes.iter().zip(&outcomes) {
            if matches!(outcome.status, EntryStatus::Failed | EntryStatus::Blocked) {
                warnings.push(format!("{}: {}", outcome.path, outcome.status_line()));
            } else if node.parent.is_none() && node.temporal.start().is_none() {
                // Allowed by the model, but the day queries filter on start
                // time, so an end-only root is invisible to get_day.
                warnings.push(format!(
                    "{}: end-only root - not visible to day views",
                    outcome.path
                ));
            }
            // Backstop for extraction typos ("7:30am-9:30pm" where one
            // meridiem is wrong): a span this long is almost never real.
            if node.temporal.start().is_some()
                && node.temporal.end().is_some()
                && let Some(span_ms) = node.temporal.infer_duration_ms()
                && span_ms > 12 * 3_600_000
            {
                warnings.push(format!(
                    "{}: spans {:.1}h - probably a timestamp typo, verify with the user",
                    outcome.path,
                    span_ms as f64 / 3_600_000.0
                ));
            }
            for value in &outcome.values {
                if matches!(value.status, ValueStatus::Failed) {
                    warnings.push(format!(
                        "{}: value '{}' failed: {}",
                        outcome.path,
                        value.attribute,
                        value.detail.as_deref().unwrap_or("")
                    ));
                }
            }
        }

        let ok = outcomes.iter().all(|o| {
            !matches!(o.status, EntryStatus::Failed | EntryStatus::Blocked)
                && o.values.iter().all(|v| !matches!(v.status, ValueStatus::Failed))
        });

        Ok(ImportReport {
            dry_run,
            source_file: doc.source_file.clone(),
            date: doc.date,
            entries: outcomes,
            warnings,
            entry_ids,
            ok,
        })
    }

    /// Promote an existing scalar entry to a sequence. One action: the
    /// conversion itself sheds scalar completion atomically in core. Inner
    /// `Err` is a rejection the caller reports; outer `Err` is
    /// infrastructure failure.
    async fn promote_to_sequence(
        &self,
        existing: &Entry,
    ) -> anyhow::Result<Result<(), String>> {
        let update = UpdateEntry {
            actor_id: self.config.actor_id,
            entry_id: existing.id,
            change: EntryChange::SetIsSequence(true),
        };
        match self.client.run_action(update.into()).await {
            Ok(_) => Ok(Ok(())),
            Err(DomainError::Rejected(reason)) => Ok(Err(reason.to_string())),
            Err(e) => Err(e).context("promote to sequence"),
        }
    }

    /// Next append position under `parent_id`: after the last index handed
    /// out this run, else after the parent's last existing child, else first.
    async fn next_index(
        &self,
        last_index: &mut HashMap<Uuid, FractionalIndex>,
        parent_id: Uuid,
    ) -> anyhow::Result<FractionalIndex> {
        if let Some(last) = last_index.get(&parent_id) {
            let next = FractionalIndex::new_after(last);
            last_index.insert(parent_id, next.clone());
            return Ok(next);
        }
        let descendants = self
            .client
            .run_query(FindDescendants { entry_id: parent_id })
            .await
            .context("FindDescendants")?;
        let last_existing = descendants
            .iter()
            .filter(|e| e.parent_id() == Some(parent_id))
            .filter_map(|e| e.frac_index())
            .max()
            .cloned();
        let next = match last_existing {
            Some(last) => FractionalIndex::new_after(&last),
            None => FractionalIndex::default(),
        };
        last_index.insert(parent_id, next.clone());
        Ok(next)
    }

    async fn apply_value(
        &self,
        entry_id: Uuid,
        planned: &PlannedValue,
        dry_run: bool,
        entry_status: EntryStatus,
    ) -> anyhow::Result<ValueOutcome> {
        let attribute = planned.attribute_name.clone();

        // Only check for an existing value where the entry itself might have
        // one (fresh entries can't).
        let exists = match entry_status {
            EntryStatus::Existed => self
                .client
                .run_query(FindValueByKey {
                    entry_id,
                    attribute_id: planned.attribute_id,
                })
                .await
                .context("FindValueByKey")?
                .is_some(),
            _ => false,
        };
        if exists {
            return Ok(ValueOutcome {
                attribute,
                status: ValueStatus::Existed,
                detail: None,
            });
        }
        if dry_run {
            return Ok(ValueOutcome {
                attribute,
                status: ValueStatus::Planned,
                detail: None,
            });
        }

        let action = CreateValue {
            actor_id: self.config.actor_id,
            value: Value {
                entry_id,
                attribute_id: planned.attribute_id,
                index_float: None,
                index_string: None,
                // Historical logs are actuals; plan stays empty.
                plan: None,
                actual: Some(planned.value.clone()),
            },
        };
        match self.client.run_action(action.into()).await {
            Ok(_) => Ok(ValueOutcome {
                attribute,
                status: ValueStatus::Created,
                detail: None,
            }),
            Err(DomainError::Rejected(reason)) => Ok(ValueOutcome {
                attribute,
                status: ValueStatus::Failed,
                detail: Some(reason.to_string()),
            }),
            Err(e) => Err(e).context("create value"),
        }
    }

    /// The day's existing state, for the agent's dedup/inspection: root
    /// entries whose canonical instant falls on `date` (local), with their
    /// subtree entries.
    pub async fn day_entries(&self, date: NaiveDate) -> anyhow::Result<Vec<(Entry, Vec<Entry>)>> {
        use chrono::TimeZone;
        let tz = self.config.timezone;
        let from = tz
            .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
            .single()
            .context("day start ambiguous in timezone")?
            .with_timezone(&Utc);
        // Next local midnight (not from+24h): correct on DST transition days.
        let to = tz
            .from_local_datetime(
                &date
                    .succ_opt()
                    .context("date overflow")?
                    .and_hms_opt(0, 0, 0)
                    .unwrap(),
            )
            .single()
            .context("day end ambiguous in timezone")?
            .with_timezone(&Utc);
        // The query returns the whole day forest (roots in-interval *and* their
        // descendants); keep only the roots and rebuild each subtree below.
        let roots: Vec<Entry> = self
            .client
            .run_query(EntriesRootedInTimeInterval { from, to })
            .await
            .context("EntriesRootedInTimeInterval")?
            .into_iter()
            .filter(|e| e.parent_id().is_none())
            .collect();
        let mut out = Vec::with_capacity(roots.len());
        for root in roots {
            let descendants = self
                .client
                .run_query(FindDescendants { entry_id: root.id })
                .await
                .context("FindDescendants")?
                .into_iter()
                .filter(|e| e.id != root.id)
                .collect();
            out.push((root, descendants));
        }
        Ok(out)
    }
}

impl EntryOutcome {
    fn status_line(&self) -> String {
        match &self.detail {
            Some(d) => format!("{:?}: {d}", self.status),
            None => format!("{:?}", self.status),
        }
    }
}

//! The import workspace: the on-disk home of everything the import produces
//! besides DB rows — persisted day-document artifacts (audit / diff / re-run
//! surface), the human-approved alias table, the questions file, and pre-run
//! DB snapshots (rollback). Layout:
//!
//! ```text
//! <workspace>/
//!   aliases.toml         # surface form → canonical name, human-approved
//!   glossary.md          # shorthand conventions (read by the extraction prompt)
//!   questions.md         # unresolved items, appended by loader + extractor
//!   artifacts/<source>/<source_file>.json
//!   snapshots/<stamp>/   # db + -wal + -shm copies
//! ```

use crate::document::DayDocument;
use crate::registry::Aliases;
use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Component, Path, PathBuf};

/// Validate a document's `source_file` before it is used as a filesystem
/// path (artifact location) or an identity key (UUIDv5 input): it must be a
/// plain relative path with normal components only. This both blocks path
/// escape from the artifacts dir and rejects spelling wobble ("./x.md" vs
/// "x.md") that would silently re-key every entry id in the file.
pub fn validate_source_file(source_file: &str) -> Result<()> {
    if source_file.is_empty() {
        bail!("source_file is empty");
    }
    let path = Path::new(source_file);
    if path.is_absolute() {
        bail!("source_file must be vault-relative, got absolute path '{source_file}'");
    }
    for component in path.components() {
        if !matches!(component, Component::Normal(_)) {
            bail!(
                "source_file must be a plain relative path (no '.', '..', or prefixes): '{source_file}'"
            );
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct Workspace {
    pub root: PathBuf,
}

impl Workspace {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Workspace { root: root.into() }
    }

    /// Create the directory skeleton and seed files if absent. Idempotent.
    pub fn init(&self) -> Result<()> {
        fs::create_dir_all(self.root.join("artifacts"))?;
        fs::create_dir_all(self.root.join("snapshots"))?;
        for (name, seed) in [
            ("aliases.toml", "# Surface form -> canonical registry name. Additive-only.\n[activities]\n\n[attributes]\n"),
            ("glossary.md", "# Import Glossary\n\nShorthand conventions, resolved with the user. Read by the extraction prompt.\n"),
            ("questions.md", "# Import Questions\n\nUnresolved items. Answered questions move into the glossary or aliases.\n"),
        ] {
            let path = self.root.join(name);
            if !path.exists() {
                fs::write(&path, seed).with_context(|| format!("seeding {name}"))?;
            }
        }
        Ok(())
    }

    pub fn load_aliases(&self) -> Result<Aliases> {
        let path = self.root.join("aliases.toml");
        if !path.exists() {
            return Ok(Aliases::default());
        }
        let text = fs::read_to_string(&path).context("reading aliases.toml")?;
        toml::from_str(&text).context("parsing aliases.toml")
    }

    /// Where a document's artifact lives: mirrors the vault-relative source
    /// path under `artifacts/<source>/`.
    pub fn artifact_path(&self, source: &str, source_file: &str) -> PathBuf {
        self.root
            .join("artifacts")
            .join(source)
            .join(format!("{source_file}.json"))
    }

    /// Persist a day-document artifact (pretty JSON, trailing newline for
    /// clean diffs).
    pub fn write_artifact(&self, doc: &DayDocument) -> Result<PathBuf> {
        validate_source_file(&doc.source_file)?;
        let path = self.artifact_path(&doc.source, &doc.source_file);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut json = serde_json::to_string_pretty(doc)?;
        json.push('\n');
        fs::write(&path, json).with_context(|| format!("writing {}", path.display()))?;
        Ok(path)
    }

    pub fn read_artifact(&self, path: &Path) -> Result<DayDocument> {
        let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    /// Append questions to the workspace questions file, tagged with their
    /// source file.
    pub fn append_questions(&self, source_file: &str, questions: &[String]) -> Result<()> {
        if questions.is_empty() {
            return Ok(());
        }
        let path = self.root.join("questions.md");
        let mut text = fs::read_to_string(&path).unwrap_or_default();
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        for q in questions {
            // Idempotent re-runs: dedup on the file+question content, not the
            // whole line, so a checked-off "- [x]" (or edited) entry still
            // counts as present.
            let key = format!("`{source_file}`: {q}");
            if !text.contains(&key) {
                text.push_str(&format!("- [ ] {key}\n"));
            }
        }
        fs::write(&path, text).context("writing questions.md")
    }

    /// Copy the SQLite database (with -wal/-shm sidecars) into a timestamped
    /// snapshot directory — the whole-run rollback point. Call before any
    /// mutating run, with the app closed.
    pub fn snapshot_db(&self, db_path: &Path, stamp: &str) -> Result<PathBuf> {
        let dir = self.root.join("snapshots").join(stamp);
        fs::create_dir_all(&dir)?;
        let file_name = db_path
            .file_name()
            .context("db path has no file name")?
            .to_string_lossy()
            .to_string();
        fs::copy(db_path, dir.join(&file_name))
            .with_context(|| format!("copying {}", db_path.display()))?;
        for suffix in ["-wal", "-shm"] {
            let sidecar = db_path.with_file_name(format!("{file_name}{suffix}"));
            if sidecar.exists() {
                fs::copy(&sidecar, dir.join(format!("{file_name}{suffix}")))?;
            }
        }
        Ok(dir)
    }
}

/// Compare two artifact directories for the gold-set regression check:
/// candidate documents must match approved ones after stripping volatile
/// fields. Returns human-readable mismatch lines (empty = clean).
pub fn check_against_gold(approved_dir: &Path, candidate_dir: &Path) -> Result<Vec<String>> {
    let mut mismatches = Vec::new();
    let approved = collect_artifacts(approved_dir)?;
    if approved.is_empty() {
        mismatches.push(format!("no artifacts under {}", approved_dir.display()));
        return Ok(mismatches);
    }
    for approved_path in approved {
        let relative = approved_path.strip_prefix(approved_dir).unwrap();
        let candidate_path = candidate_dir.join(relative);
        if !candidate_path.exists() {
            mismatches.push(format!("missing candidate: {}", relative.display()));
            continue;
        }
        let approved_doc: DayDocument =
            serde_json::from_str(&fs::read_to_string(&approved_path)?)
                .with_context(|| format!("parsing {}", approved_path.display()))?;
        let candidate_doc: DayDocument =
            serde_json::from_str(&fs::read_to_string(&candidate_path)?)
                .with_context(|| format!("parsing {}", candidate_path.display()))?;
        let a = serde_json::to_value(approved_doc.normalized_for_check())?;
        let b = serde_json::to_value(candidate_doc.normalized_for_check())?;
        if a != b {
            mismatches.push(format!("differs: {}", relative.display()));
        }
    }
    // A candidate with no approved counterpart is a drift signal too (e.g. a
    // prompt change renamed the source_file) - never pass it silently.
    let approved_set: std::collections::BTreeSet<PathBuf> = collect_artifacts(approved_dir)?
        .into_iter()
        .map(|p| p.strip_prefix(approved_dir).unwrap().to_path_buf())
        .collect();
    for candidate_path in collect_artifacts(candidate_dir)? {
        let relative = candidate_path.strip_prefix(candidate_dir).unwrap();
        if !approved_set.contains(relative) {
            mismatches.push(format!("unexpected candidate: {}", relative.display()));
        }
    }
    Ok(mismatches)
}

fn collect_artifacts(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if !dir.exists() {
        return Ok(files);
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for child in fs::read_dir(&d)? {
            let path = child?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "json") {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Applied, SCHEMA_VERSION};

    fn temp_workspace(tag: &str) -> Workspace {
        let root = std::env::temp_dir()
            .join("gv-import-tests")
            .join(format!("{tag}-{}", uuid::Uuid::new_v4()));
        let ws = Workspace::new(root);
        ws.init().unwrap();
        ws
    }

    fn doc(source_file: &str) -> DayDocument {
        DayDocument {
            schema_version: SCHEMA_VERSION,
            source: "test-source".to_string(),
            source_file: source_file.to_string(),
            content_hash: None,
            date: chrono::NaiveDate::from_ymd_opt(2026, 2, 24).unwrap(),
            model: Some("test-model".to_string()),
            prompt_version: Some("1".to_string()),
            entries: vec![],
            skips: vec![],
            questions: vec![],
            applied: None,
        }
    }

    #[test]
    fn source_file_validation() {
        assert!(validate_source_file("2026-02/2026-02-24.md").is_ok());
        assert!(validate_source_file("").is_err());
        assert!(validate_source_file("/etc/passwd").is_err());
        assert!(validate_source_file("../escape.md").is_err());
        assert!(validate_source_file("a/../b.md").is_err());
        // Spelling wobble that would re-key ids is rejected, not normalized.
        assert!(validate_source_file("./2026-02/x.md").is_err());
    }

    #[test]
    fn artifact_roundtrip_and_traversal_rejection() {
        let ws = temp_workspace("artifact");
        let mut d = doc("2026-02/2026-02-24.md");
        d.applied = Some(Applied {
            imported_at: chrono::Utc::now(),
            entry_ids: vec![uuid::Uuid::new_v4()],
        });
        let path = ws.write_artifact(&d).unwrap();
        assert!(path.starts_with(&ws.root));
        let read = ws.read_artifact(&path).unwrap();
        assert_eq!(read.source_file, d.source_file);
        assert_eq!(read.applied.unwrap().entry_ids, d.applied.unwrap().entry_ids);

        assert!(ws.write_artifact(&doc("../escape.md")).is_err());
        assert!(ws.write_artifact(&doc("/abs/path.md")).is_err());
    }

    #[test]
    fn questions_dedup_survives_checkoff() {
        let ws = temp_workspace("questions");
        let questions = vec!["what does '20x 13:13:13' mean".to_string()];
        ws.append_questions("a.md", &questions).unwrap();
        ws.append_questions("a.md", &questions).unwrap();
        let text = fs::read_to_string(ws.root.join("questions.md")).unwrap();
        assert_eq!(text.matches("13:13:13").count(), 1, "{text}");

        // The user checks it off in place; a re-apply must not re-append.
        let checked = text.replace("- [ ]", "- [x]");
        fs::write(ws.root.join("questions.md"), checked).unwrap();
        ws.append_questions("a.md", &questions).unwrap();
        let text = fs::read_to_string(ws.root.join("questions.md")).unwrap();
        assert_eq!(text.matches("13:13:13").count(), 1, "{text}");
    }

    #[test]
    fn gold_check_flags_diffs_and_extras_but_not_volatile_fields() {
        let approved_ws = temp_workspace("gold-approved");
        let candidate_ws = temp_workspace("gold-candidate");
        let approved_dir = approved_ws.root.join("artifacts");
        let candidate_dir = candidate_ws.root.join("artifacts");

        // Identical content, differing volatile fields: clean.
        let mut gold = doc("2026-02/a.md");
        gold.applied = Some(Applied {
            imported_at: chrono::Utc::now(),
            entry_ids: vec![],
        });
        approved_ws.write_artifact(&gold).unwrap();
        let mut candidate = doc("2026-02/a.md");
        candidate.model = Some("newer-model".to_string());
        candidate.prompt_version = Some("2".to_string());
        candidate_ws.write_artifact(&candidate).unwrap();
        assert!(check_against_gold(&approved_dir, &candidate_dir).unwrap().is_empty());

        // Content drift: flagged.
        let mut drifted = doc("2026-02/a.md");
        drifted.questions = vec!["new question".to_string()];
        candidate_ws.write_artifact(&drifted).unwrap();
        let mismatches = check_against_gold(&approved_dir, &candidate_dir).unwrap();
        assert!(mismatches.iter().any(|m| m.starts_with("differs:")), "{mismatches:?}");

        // Extra candidate with no approved counterpart: flagged.
        candidate_ws.write_artifact(&doc("2026-02/extra.md")).unwrap();
        let mismatches = check_against_gold(&approved_dir, &candidate_dir).unwrap();
        assert!(
            mismatches.iter().any(|m| m.starts_with("unexpected candidate:")),
            "{mismatches:?}"
        );

        // Missing candidate: flagged.
        approved_ws.write_artifact(&doc("2026-02/only-gold.md")).unwrap();
        let mismatches = check_against_gold(&approved_dir, &candidate_dir).unwrap();
        assert!(mismatches.iter().any(|m| m.starts_with("missing candidate:")), "{mismatches:?}");
    }
}

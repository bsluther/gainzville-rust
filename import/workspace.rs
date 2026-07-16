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
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

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
        for q in questions {
            let line = format!("- [ ] `{source_file}`: {q}\n");
            // Idempotent re-runs: the same question from the same file lands once.
            if !text.contains(&line) {
                text.push_str(&line);
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

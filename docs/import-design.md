# LLM Import Design

## Summary

Import unstructured markdown training logs into Gainzville with an LLM doing the interpretation and
the existing Action/Query interface doing all writes. Near term: the owner's multi-year Obsidian
vaults, run locally against a copy of the app DB. Long term: the same components back a user-facing
import feature (other sources: Strava, other vaults).

The architecture is three layers, from durable to disposable:

1. **`gv-import` crate** — import-shaped handler functions over `gv-client`. The durable asset:
   a future in-app importer calls these same handlers in-process.
2. **Day-document** — a denormalized, name-based description of one day's log. The single pivot
   format: it is the MCP tool parameter, the persisted per-file artifact, and the loader input.
3. **stdio MCP server** (thin `rmcp` wrapper over the handlers) + a Claude Code **skill** carrying
   the extraction conventions. Disposable glue; either can be replaced without touching 1 or 2.

Division of labor: **the model interprets, the handler administers.** The model never sees UUIDs,
fractional indices, plan/actual mechanics, or validation rules — it produces day-documents in the
source's own vocabulary. The handler resolves names, mints ids, derives positions, validates, and
rejects with errors legible enough for the model (or a human) to act on.

## Day-document

One document per source file. Illustrative shape (exact serde DTOs live in `gv-import`, not core):

```yaml
schema_version: 1
source: obsidian-training-log        # registered source name; later: strava, work-vault, …
source_file: 2026-02/2026-02-24.md   # vault-relative
content_hash: sha256:…               # of the source file at extraction time
imported_at: 2026-07-15T22:41:00Z    # written by the loader on apply
model: claude-…                      # extraction provenance
prompt_version: 3
date: 2026-02-24                     # derived from the filename, never by the model
entries:
  - activity: Climbing               # name-based; must resolve via registry
    start: "3:03pm"
    attributes: { Location: Stone Age }
    children:
      - activity: Autobelay
        attributes: { YDS: "5.10", Outcome: repeat }
        note: "Chill."
      # …
skips:                               # every unconsumed source line is accounted for
  - lines: 31-38
    reason: health check-in, out of scope
questions:                           # appended to the workspace questions file by the loader
  - "line 12: '20x 13:13:13' — unknown notation"
applied:                             # written back by the loader after a successful apply
  entry_ids: [ … ]
```

Handler responsibilities on `import_day(document)`:

- Resolve activity/attribute names → UUIDs via the registry (exact + alias table). An unknown name
  is a rejection, never a silent create — new schema goes through the resolution workflow below.
- Mint **deterministic UUIDv5** entry ids keyed on `(source, source_file, date, tree-path)`,
  where tree-path components are `<label>#<occurrence-among-same-label-siblings>` — stable under
  file edits (not line numbers). Labels are **registry-canonical** (aliases resolved, trimmed,
  lowercased, `/`/`#` defused), so alias or casing wobble between extraction runs cannot re-key
  ids; `source_file` is validated (plain relative path only) for the same reason. Re-applying a
  document (or a re-extraction of the same file) re-derives the same ids, so already-imported
  entries are detected and skipped rather than duplicated. Re-run-after-answering-questions is
  the routine primitive. A previously-imported scalar that a re-extraction gives children is
  promoted to a sequence in place (never demoted — that would delete children); values are never
  overwritten on re-runs, so app-side edits survive.
- Derive `Position` fractional indices from array order (via `Forest` helpers); set
  `is_sequence` on anything with children; enforce parent-shape rules.
- Historical imports are **actuals** with `is_complete = true`; attribute values validate against
  configs with importer-side normalization (2-decimal rounding, select-option matching).
- `dry_run` mode: full resolution + validation, no writes — the diff/check surface.

## Workflow

Two-phase, schema-first, with a questions loop. A pass never guesses and never blocks on a human.

| Phase | What happens | Output |
|---|---|---|
| 0. Vocabulary sweep | Cheap scan of the whole corpus: session types, exercise names, shorthand tokens, value formats, with counts + example lines | candidate registry, `questions.md` |
| 1. Resolution session | Interactive (Claude Code + user): canonicalization ("fingerboard" vs "hangboard"), attribute typing/configs (option sets generous up front — configs are additive-only), shorthand semantics ("does bare `Att` mean the previous problem?"). **Every answer lands in an artifact**, not just chat | registry applied to GV (`CreateActivity`/`CreateAttribute`), `aliases.yaml`, `glossary.md` |
| 2. Load pass | Per file, frozen registry: extract → day-document → `import_day`. Unknowns → `questions.md` + explicit skip records | applied day-documents (with ids written back) |
| 3+. Rounds | Resolve new questions, re-run only the affected files (deterministic ids make this safe) | — |

Sets mapping (e.g. `8x 20 / 8x 20 / 5x 40` dumbbell blocks → per-set child entries +
`ConvertToSets`) is decided in the resolution session, not fixed here.

### Review

- **Gated start**: for the first ~20 files, extraction writes day-documents only; the user reviews
  them before the loader applies. Tunable threshold.
- **Steady state**: live-load — the agent calls `import_day` directly — but every applied
  day-document is persisted to the import workspace regardless, so audit, diff, and re-run exist
  without a gate. Review by sampling + the questions file.
- **Regression set**: the gate-approved documents *are* the gold set — copied once to a `gold/`
  directory when the gate ends (live artifacts are overwritten by re-runs). `gv-import check`
  diffs re-extracted candidates against it (volatile provenance fields ignored; drift, missing,
  and unexpected files all fail); run before any prompt change is trusted for a bulk re-run.
- **Rollback**: runs target a **scratch copy** of the app DB (promoted manually when satisfied);
  the loader snapshots the SQLite file (db + wal + shm) before each run. The Swift app stays closed
  during any run against its real DB (its reactive cache only refreshes on in-process broadcasts).

### Provenance

Lives in the artifacts, not in core: `source` + `source_file` + `content_hash` + `imported_at` +
model/prompt versions on each day-document, and applied entry ids written back after load.
Attribution is bidirectional — grep the workspace for an entry UUID seen in the app, or open a
file's document to see what it produced. A core-level provenance field (`ActionCause` sketched in
`sync.md`) stays deferred; deterministic ids make it unnecessary for dedup.

## Tool surface (MCP)

Small and import-shaped — thin wrappers over the handlers, not Action/Query passthrough:

| Tool | Backing |
|---|---|
| `registry` | `AllActivities` + `AllAttributes` + alias table, compact |
| `get_day(date)` | `EntriesRootedInTimeInterval` + values, for inspection/dedup |
| `import_day(document, dry_run)` | the handler above |
| `create_activity` / `create_attribute` | resolution-session schema writes (registry-mediated) |

Existing queries suffice; the known gaps (name search, entries-by-activity) are deliberately not
filled — at personal-vault scale the registry rides in the prompt, and dedup is per-day.

## Core changes

Exactly one for v1: **`create_entry` gains parent checks** — parent must exist and be a sequence
(`Rejected(Precondition)`), matching the guards already present in `move_entry` and
`create_entry_from_activity`. This is an invariant hole for any non-UI caller, not an importer
accommodation.

Explicitly *not* changed: no serde derives on `Action`/`AnyQuery`/models (day-document DTOs live in
`gv-import`, externally tagged per the `arbitrary_precision` constraint); no provenance column; no
new queries. `sync.md` gets a note that a multi-year import implies a bulk/snapshot upload path
once the mutation log ships (today `run_action` writes no log, so imports have no sync
side-effects).

## Corpus notes (extraction-prompt inputs)

From the sample vault (~1,000–1,500 daily files projected, < 1 MB total — cost is a non-factor):

- Stable four-year skeleton: Woke / weight (`731am weight wbo 194.4`) / session header with time +
  location / exercise blocks / `Finished` / Bed. Wake/weight/bed are dense, trivial scalars.
- Climbing blocks are the hard part and are **anaphora-heavy**: bare `Att`/`Sent`/`Wrk` refer to
  the previous problem; "the one from 2 problems ago". Interpretation with running state, not
  line parsing.
- Shorthand vocabulary for the glossary: `f` (flash), `att`, `wrk`, `rpt`, `OS`, `rp`, `d`, `wbo`,
  `CP`, `EWE`, `NxM` (reps × weight), hang notation (`20s 15mm open hand`), RPE with uncertainty
  (`8-9 rpe?`), uncertain grades (`V6? Att`). Unknowns exist (`20x 13:13:13`) → questions file.
- Out-of-scope content is interleaved (health check-ins, people, `#fight`, `#gainzville/ux`) →
  explicit skip records so omissions are visible.
- Dates come from filenames with normalization (en-dash variants observed: `2026–02-27.md`).

## Model / harness

Claude-class models for the import runs; cost is deferred as a concern. The MCP boundary keeps the
door open to local models later (Gemma 4 shipped native function calling 2026-03; the same stdio
server plugs into LM Studio / goose / mcphost unchanged) — local models are weakest at exactly the
agentic long-chain work and strongest at single-shot constrained extraction, which the day-document
shape happens to favor. The eventual in-app importer is a raw API tool-use loop calling the
handlers in-process — no MCP hop, no Claude Code dependency.

## Deferred / future work

- Other sources: remaining two vaults (same registry/format, new extraction prompt), Strava.
- In-app import feature (raw API loop over the same handlers; server-side once HTTP exists).
- Local-model extraction path (grammar-constrained day-documents).
- `ActionCause`-style provenance in core; bulk sync-upload path (noted in `sync.md`).
- Review-gate tuning; embedding-based alias matching (exact + fuzzy suffices at this vocabulary
  size).

## Decision log

| # | Decision | Rationale |
|---|---|---|
| D1 | One denormalized day-document = tool param = artifact = loader input | One handler (`import_day`) serves agentic and batch shapes; the shape question becomes a dial, not a fork |
| D2 | Schema-first two-phase with frozen registry + questions loop | Kills entity drift (prior-art consensus); passes never guess or block — unknowns become questions + skips |
| D3 | Deterministic UUIDv5 entry ids (source, file, date, tree-path, occurrence) | Re-runs collide instead of duplicating; no core provenance field needed; survives rollback and vault edits |
| D4 | Review = gated artifacts for first ~20 files, then live-load with artifacts persisted | Full audit trail without per-file ceremony at ~1,000 files; gate threshold tunable |
| D5 | Claude-class models now; MCP boundary preserves local-model option | Corpus anaphora rewards frontier interpretation; cost immaterial at < 1 MB corpus |
| D6 | Day-document DTOs in `gv-import`; no serde on core Action/Query | Avoids the `arbitrary_precision` minefield and a 1–3 day derive project; import vocabulary ≠ core vocabulary |
| D7 | Scratch DB + pre-run snapshots | Whole-run rollback for free; promotion to the real DB is a manual copy |
| D8 | Fix `create_entry` parent checks in core | Genuine invariant hole (guards exist in `move_entry`/`create_entry_from_activity`); any non-UI caller can corrupt the forest |
| D9 | Provenance on artifacts (source name, file, hash, load date, model/prompt) + ids written back | Bidirectional file↔data attribution with zero core changes |
| D10 | Gate-approved documents double as the regression gold set | Zero extra labeling; `check` guards prompt changes before bulk re-runs |
| D11 | Import-shaped tools, not Action/Query passthrough | Model never handles ids/indices/validation; hardened boundary against LLM-typical errors |
| D12 | Tree-path labels are registry-canonical and sanitized; `source_file` strictly validated | Review finding: surface-form labels let alias/casing/path wobble re-key every UUIDv5 and duplicate the archive; canonicalization makes id stability hold under prompt iteration |
| D13 | Existed entries: promote scalar→sequence when a re-extraction brings children; never demote; never overwrite values | The answer-questions-and-re-run loop must converge — without promotion, core's placement guard rejects the children forever; without no-overwrite, re-runs clobber app-side edits |

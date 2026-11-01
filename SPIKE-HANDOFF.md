# Spike handoff: remove the query macros

You're picking up work on `spike/gvkit-query-hooks`. Read all of this before changing anything.

## What this branch is

It's a **spike**: an experiment to evaluate an architecture, not code headed for `main` as-is. It
implements a vertical slice of the lean in RFD 0003, "The core–Swift boundary". The RFD lives on a
different branch; read it with:

```
git fetch origin rfd-0003-core-swift-boundary
git show origin/rfd-0003-core-swift-boundary:docs/rfd/0003-core-swift-boundary/0003-core-swift-boundary.md
```

In that RFD, read the section "Latest / lean (2026-10-29)": first principles, what's different from
today, the shape, the queries section ("a question with an answer, grouped by who can answer it"),
and the spike section. The review comments `comment-2026-10-30-*.md` in the same directory are useful
background.

The architecture in one paragraph: Swift asks domain questions and states intents through **GvKit**,
a thin Swift SDK in `swift-app/Gainzville/GvKit/`, using a `@GvQuery` hook and a read-once `read`.
Rust decides how each question is answered: from the in-memory forest, or from SQLite. Swift can't
see that choice. Rust pushes one consistent, versioned batch of changed answers per data change
(`client/live.rs`). Queries are grouped into **flat sets by who can answer them**:

- `DomainQuery`: used by mutators and the server; must be answerable by SQLite and Postgres.
- `ClientQuery`: the only set Swift sees, answered from the forest or SQLite.

A question can belong to several sets as a plain variant. **No set ever wraps another set.**

The spike is verified: Rust tests pass, both Xcode targets build, and the owner's manual runtime
checks passed. The owner is now **reviewing the code by reading it** to get familiar with the
approach. That's the reason for this task.

## The task: remove the macros and keep the architecture

The last round introduced a set of `macro_rules!` macros to generate the query plumbing. The owner
found that the macros obscure what's going on and make the rest of the spike hard to review.
Introducing them during the spike was premature; the macro may come back later, after the design is
settled. **Replace the generated code with plain, hand-written Rust.** Repetition is fine here,
because readability for review is the goal.

The macros to remove (run `grep -rn "macro_rules!" core client gv-ui gv-ffi`):

| Macro | File |
|---|---|
| `define_questions!` | `core/src/queries.rs` |
| `define_query_set!` | `core/src/queries.rs` |
| `core_questions!` | `core/src/queries.rs` |
| `domain_query_set!` | `core/src/queries.rs` |
| `client_query_set!` | `client/queries.rs` |
| `ui_questions!` | `gv-ui/src/queries.rs` |
| `ffi_question_records!` | `gv-ffi/src/types.rs` |
| `ffi_query_set!` | `gv-ffi/src/types.rs` |

Plus the x-macro callback invocations that use them.

**Keep, written out by hand:**

- **Each question struct and its `Query` impl.** The small pre-spike `define_query!` (struct, derives,
  `Query` impl, nothing else) predates the spike and the owner knows it. Restoring that one is fine
  (see `git show 0b5c270^:core/src/queries.rs`), but nothing more macro-like.
- **The set enums `DomainQuery` and `ClientQuery`,** with their response enums, `From` impls, the
  executor bounds (`DomainQueryExecutor`), and the dispatch `match`es.
- **The backing decision,** as a plain `match` in one obvious function, such as `is_forest_backed` or
  `answer`. Keep it easy to find, because "Rust picks the backing" is a core idea of the spike.
- **The `gv-ffi` UniFFI declarations** (`#[uniffi::remote(...)]` and so on) for the questions, the
  sets and the responses, written explicitly.
- **Everything else unchanged in behavior:**
  - the unsealed `Query` trait, with questions placed by crate (`gv-ui` for forest answers that
    return UI types);
  - `LiveStore` and the versioned batches;
  - GvKit and the migrated Swift.

  Don't redesign anything else. If something only made sense because of the macro, simplify it and
  say so in your report.

A good way to start is to expand the macros (`cargo expand -p gv-core queries` or similar, if
`cargo-expand` is installed) and use the output as a draft. Then tidy it into code a person would
write: grouped, commented where helpful, and formatted with rustfmt.

Update the "query sets" part of `docs/actions_and_queries.md` on this branch to match. Don't edit
the RFD.

## Owner comments in the code

Comments prefixed `BSL:` are the owner's review notes (`client/live.rs`, near `LiveValue` and
`LiveBatch`). **Leave them in place.** If one points at something clearly right to fix that's within
this task, such as wording in a doc comment, you may fix it and add a one-line `Claude:` reply under
the comment saying what changed. Otherwise leave it for discussion.

## Verify

- `cargo test -p gv-core -p gv-ui -p gv-client -p gv-ffi -p generation` and
  `cargo test -p gv-sql --no-default-features --features sqlite`.
- `scripts/regen-bindings.sh` from the repo root. The Postgres container must be running, because
  building `gv-ffi` needs it (the `gainzville-postgres` Docker container). Because this task only
  changes how code is written, the generated `gv_ffi.swift` should be unchanged or nearly so. Note
  any differences.
- From `swift-app/`, build both targets as `swift-app/SWIFT-APP.md` describes (`xcodebuild -target
  Gainzville` and `-target 'Gainzville macOS'`). Don't launch the app.

## Git

- **If git says the branch is checked out in another worktree**
  (`.claude/worktrees/bridge-cse_012FY6oJCbh9WKVpwqVrnpqF`, left over from an earlier build session),
  ask the owner before removing it. Alternatively, work on a detached `HEAD` at
  `origin/spike/gvkit-query-hooks` and push with `git push origin HEAD:spike/gvkit-query-hooks`.
- **Commit in logical steps,** and end each message with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- **Push to `spike/gvkit-query-hooks`.** No PR.
- **Delete this handoff file in your final commit.**

## Report back

Write a short summary for the owner:

- **What changed,** with before and after line counts for the query plumbing in each crate.
- **The cost of adding one client question now,** as the list of files touched.
- **What was verified.**
- **Anything that got simpler or harder without the macro.**

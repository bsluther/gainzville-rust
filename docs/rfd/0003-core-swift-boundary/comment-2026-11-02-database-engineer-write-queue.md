# RFD 0003 comment: database-engineer, single writer and write failures

- Date: 2026-11-02
- Lens: what established database patterns apply here, given that GV isn't a general database?
- Read: RFD 0003 at 52bce8b; `client/client.rs`; `core/src/error.rs`; `gv-ffi/src/{core,types}.rs`; `GvKit/GvWrites.swift`

## Verdict
Worth committing to one writer behind a queue. It is a modest change that gives writes a single owner, supports commit sequence numbers, and creates a place to handle failure.

## Concerns
- Re: "Writes are intents": the RFD says what a write expresses, not who executes it. Today writers compete through a 20-connection pool, and `run_action` blocks the main thread through `block_on`, for up to the 5 s busy timeout.
- Re: failures: `GvClient.run` logs a failed write and drops it, so the user can believe a set was saved when it wasn't. `FfiError::Generic` also flattens `DomainError`, so Swift can't tell `Rejected` from `Database`.
- Re: atomicity: each action is its own transaction. A crash partway through an LLM import leaves part of a log.

## Missing
- A UI surface for failures, such as toasts or a notifications section. A rejected action and a storage failure probably deserve different treatment.
- The acknowledgement rule. A queue held in memory loses anything not yet committed, so "saved" can only mean committed.

## Recommendation
Route every local writer (UI, import, later sync) through one writer connection, and read through a small read-only pool. The writer assigns sequence numbers, and it can later group several actions under one flush. Add a multi-action transaction. Carry typed errors across the FFI, and give GvKit one failure channel that the app displays.

Start with callers waiting for commit, which keeps today's semantics. Returning before commit is a separate decision, because it needs a pending state in the UI.

## Can expand on
- Waiting for commit versus returning early, including `fullfsync` cost on the main thread.
- A failure-channel design for GvKit.
- Group commit.

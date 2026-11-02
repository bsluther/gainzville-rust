# RFD 0003 comment: database-engineer, commit sequence numbers

- Date: 2026-11-02
- Lens: what established database patterns apply here, given that GV isn't a general database?
- Read: RFD 0003 at 52bce8b; `docs/sync.md`; `docs/gvkit-migration/{plan,live-review}.md`; `client/{live,client,queries}.rs`; GvKit

## Verdict
The boundary is a single-node database with materialized views and a replica in Swift. It lacks a commit position, and a commit sequence number looks like the largest architectural gain available.

## Concerns
- Re: principle 3, one version of the data: `Version` counts builds, not commits, so answers can't be ordered against each other. `swap_in` says a build "can't tell which is newer". `read_during`, the straddle check, the reconcile build and the `subscribe` retry all work around this.
- Re: writes: `run_action` holds exact deltas, then broadcasts `()`. The read path recovers what changed by re-reading.
- Re: read-your-writes: no token exists to wait on, so `sync()` counts builds instead.

## Missing
- The log is durable, versioned data. Stored delta shapes need a place in RFD 2's data versioning.
- A retention policy, once sync defines what can be pruned.
- A rule that every write goes through the log. See the write-queue comment.

## Recommendation
Extend the RFD with the `mutation_log` from `sync.md`, written inside the write transaction. Its local sequence is the version. Each build and first value reads `max(seq)` in the same read transaction as its answer, and `swap_in` keeps whichever value has the higher sequence. That should remove the reconcile protocol, and it provides read-your-writes by waiting for a sequence, plus the sync outbox and an audit trail. The cost is one row per write and a stored format to maintain.

## Can expand on
- A revised `subscribe` and `swap_in` sketch.
- How local sequences relate to server sequences during rebase.
- Why pinning a WAL snapshot is the weaker alternative.



There will be 20+ database tables. Does each table have it's own change log? Or do you group all the
changes together into a single log per client?
- Electric does one log per table (shape).
- I could join each individual table stream into a single client stream in DBSP, keyed on
(seq_num, op_offset, table_name).
    - This sorta breaks electric compatability. Could stream join on the client, perhaps.

### Rebasing
To support offline writes:
When a client comes back online and receives sync updates from the server, the client reverses all
local writes which have not yet been rejected or synced (see below). The client applies sync changes
from the server, then re-applies the local writes on top of the new, up-to-date state.
- Note that it should be possible to switch back to offline mode even if rebasing is interrupted, eg
if we don't catch all the way up, because of the seq_num assigned to each message/change.
    - If the client has mutation mx7 which is acknowdged by the server as seq_241, then the client
    knows it will see a sync change with seq_241 because the change is in that client's permission
    scope. Is there any situation where this wouldn't be the case? Yes, some other client revokes
    a permission. So in that case, you can't rely on seeing an acknowledged seq_num in the change
    stream as a means to confirm you can discard it. Coupled with unreliable message ordering, this
    is why you want something like **durable streams**.

When the client sends a mutation to the server, the server acknowledges with the global seq_num
assigned to that mutation. Now the client tracks which seq_num it has synced to, once that seq_num
is greater than or equal to the acknoledged mutation, the local write is thrown away as we have now
received the state via sync.
- The server needs to send the seq_num with each message

### Global sequence number
We need a global incrementing sequence number. Might make sense to write this into each table,
so we don't have a hotpath on a single global row. It's immutable, so safe to duplicate.

Various approaches
- Use (PG LSN + op_offset), must read changes from WAL (non-deterinistic?), seq_num only lives in
sync logs.
- Manually maintain global seq_num + delta_offset, store in all rows. Why store in rows? We know
when that row was updated, like an updated_at column. Not sure but it could be useful.


### Electric Inspiration

#### Persist per-client change logs
Persist change logs to disk for each client (or actor, more likely - same for any device). When a
client needs to catch up, they send they're actor_id and the last offset they saw. Catch them up
basd on the persisted log.

The change log can be compacted by removing old offsets, if a client's offset is out of date, force
them to start over from scratch.

- Could as a simple first pass use a single global change log where changes are indexed by
(seq_num, client_id).

#### Store a snapshot as the log head
Store a snapshot of the state at the beginning of each log which is just the query result at that
point in time, that way you don't have to start from 0 whenever you create a stream. You just query
the current state, save the snapshot as the "head" of the log, and then start writing changes into
the log as the database changes.

#### Postgres LSN as global sequence number
Offsets are globally unique and ordered: offset = (tx_offset, op_offset) where
- tx_offset: Postgres's Log Sequence Number (LSN), a 64-bit value representing a byte position
in the current WAL stream. Consistent across a single transaction?
- op_offset: operation index within a transaction. This is computed by electric.
- So an offset looks something like (23987237_0) for transaction 23987237, operation 0.


### Activities and Libraries

Strategy: duplicate-on-add.
- When user A adds an Activity to their Library, a copy of that Activity, owned by user A, is added
to user A's library.
- Why? What happens if the Activity creator changes the activity? More importanatly, what if they
delete it?

For scalar entries, the relationship between an Activity and an Entry is qualitative: changing the
Activity does not create a data-level conflict with previously created Entries.

For sequence entries, this is more complicated: a change to the Activity template would change the
recorded data. Thus we can maintain a pointer from a sequenty Entry to a changed Activity, but the
structure of the Entry may diverge. There are two types of "equality":
- Structural equality: the entry sub-tree is identical.
- Qualitative equality: the referenced Activity is the same.

### Attributes
Changes to Attributes must be backwards-compatible.
Ok: [a, b, c] -> [a, b, c, d]
Not ok: [a, b, c] -> [a, b]
Why: we need to be able to interpret old Values.


### Read-path sync
The plan and hope is to use Electric. They are moving to durable-streams protocol which already has
a client written in Rust. The next major Electric release will support durable-streams, which means
I can use it in Rust.
- This almost surely introduces non-determinism into the system.

#### Could I build a good-enough deterministic read-path sync module?
I was thinking this problem is too hard after doing similar research for supporting live queries in
the client. It seems parsing SQL into DBSP is very complicated, at least enough so that I don't want
to spend weeks seeing if I can get it to work.

But it occurs to me this morning that read-path sync is different: we consume the Postgres
replication stream and have relatively few queries which I could write as DBSP circuits or
differential dataflows. Not needing to parse SQL takes a big burder off, and having a small number
of fixed queries makes that seem reasonable.

The next potentiall big problem seems like transactions. Not really sure how bad that could be. But
it sounds more like a fun project to work at that level, writing circuits, feeding data in from PG's
replication streaming, constructing client messages. And this approach seems like I could,
potentially, get determinisim out of it.
- Postgres may introduce non-determinism via the replication stream. One way around thus: consume
the deltas I apply rather than the replication stream. 

### Other
Media (images, video): how is that sent/stored/synced?
- One idea: if the blob data is separate and the main db only contains metadata, then the blob data is just a
cache that tracks the metadata. Sync the metadata, cache the blobs.

### Daydreaming
To deal with running out of storage space on a client, change logs could include an aggregation that
counts the number of rows synced to a client, which we can use to estimate total storage used. If
that number gets close to some limit, add a filter to the change stream which will remove some old
stuff, and include the filter clause in the synced data. Now the client knows what it's missing and
can request it if needed.

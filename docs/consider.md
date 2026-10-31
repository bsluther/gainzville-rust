# Consider
- Use dates as units of log virtualization: log entries for a date are loaded into memory completely or not at all.
- Can the FFI/Swift boundary be cleaned up? Can more logic move into the Rust core?
- Is query subscription the right FFI API boundary? It forces more logic into the client. At present, I have a hybrid of query subscriptions + forest operations.
- Should entry (forest) and activity (library) data be virtualized? Obvious paritions are by log day and category DAG roots.

### Treat log days as sequences
Treat days as sequences: they can contain entries without a canonical instant and children have fractional indices.

Should I relax the constraint that entry roots must have a definite date *and* time to just a definite date?

The user interface is centered on displaying entries for a day or an entry. But there is no "day container",
we just fetch the entries with a canonical instant on that day. 
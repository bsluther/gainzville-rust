Collection of reference material relevant to the analysis feature.

[CMU 15-445 (Fall 2025): #13 - Query Execution Part 1 (CMU Intro to Database Systems)](https://youtu.be/E-UUd6cB57w?si=D9A58tgp1SXoluhe)
- We're building a constrained *query engine*, emphasis on constrained, but the database world has
much to say about that topic. I think of query optimization when I think of query engines, but
that's only part of what a query engine does. Query execution is the process of executing a query
plan against the data; we can view each segment as a naive query plan (filter -> reduce or project).

[Malloy docs: ungrouped aggregates](https://docs.malloydata.dev/documentation/language/aggregates#ungrouped-aggregates)
- Good example of grouping by multiple dimensions (i.e. multiple group_by's).
- [Detailed docs for ungrouped aggregates](https://docs.malloydata.dev/documentation/language/ungrouped-aggregates)
- Malloy introduces `all(expr)`, `all(aggregate_expr, grouping_dimension, ...)` and
`exclude(aggregate_expr, ungroup_expr)` to ignore the grouping in the current query.

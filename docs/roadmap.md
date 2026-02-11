


1. Build mutation log on client, transaction log on server.
- Blocked by 7.
2. Undo/redo via log data.
- Blocked by 1.
3. Reify Reader queries.
4. Basic permissions (auth token, read/write grant).
5. Parse markdown file via LLM.
- One approach: just send everything the LLM needs in one big request. But this is brittle, one
thing goes wrong and you have to start over. And it relies on the LLM generating UUIDs and the like.
- Blocked by 3, 4, and probably 8.
6. Numeric Attribute/Value.
7. Add client IDs (HLC) to mutations, tx_id to transaction.
8. Build MCP server.
9. Extract non-determinism to a parameter (UUID generation, time).
10. Attribute/Values.
11. Categories.


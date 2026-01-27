
Granting and revoking permissions is modeled in the same Action system as the rest of the app.

Reads (reader methods) all need to be authorized.

Grants are stored explicitly in a table.

A grant must be accepted by the grantee before it is applied.

Entry's which are granted to but not owned by a user do *not* appear in that user's Log, rather they
appear in the a separate "Other users logs" section. The same applies to Activitys, Attributes, and
Values - they do not appear in the user's Log/Library, but rather in separate sections of the app.

### Grant Scopes
Entry
Activity
Attribute
Category

### Grant Types
Read
Write (implies Read)
Root-Write + Read-Your-Writes: write to root-level only and automatically get write permission for
any Entry you write.
- Speculative. Alice wants her coach Bob to be able to add entries to her root, and wants Bob to be
able to read/write the Entries he created (or children, eg if she adds an extra exercise to a
session Bob created). But she doesn't want Bob to see what time she woke up or her meditation - Bob
should only see what he created.

### Grantees
Actor (user, LLM)
Group (training partners, team)
- Could model a Group as an Actor.
Public (everybody)
- 

### Entries

Granting permission to an Entry grants permission to:
- All child entries.
- The Entry's Activity (if it exists).
    - What about it's Categories?
- Any Attribute and Value used by that Entry.

Every Entry tree must only contain Entries owned by the same actor.

### Activities

Granting permission to an Entry grants permission to:
- That Activity's template and any Attributes/Values used 
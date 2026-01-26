
Granting and revoking permissions is modeled in the same Action system as the rest of the app.

Reads (reader methods) all need to be authorized.

Grants are stored explicitly in a table.

### Grant Scopes
Entry
Activity


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
- Activity Template? Would simplify the rule: it's just like they got access to the Activity.
- Looks like we have a permissions graph - any constraints beyond a general graph? It could be
acyclic since it's directed - two reasons for one user to have permission to read Entry A. So maybe
it's a DAG?

Every Entry tree must only contain Entries owned by the same actor.
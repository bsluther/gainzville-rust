---
name: reconcile-docs
description: Gradually reconcile GV's repo docs (todo lists, roadmap, design-doc open questions) and Linear issues, one item per session. Picks a single unreconciled item, presents it self-contained, and offers reconciliation options for the user to choose from. Use when the user asks to reconcile docs or issues, chip away at doc/Linear cleanup, or invokes /reconcile-docs (optionally with a scope like a file, topic, todo id, or issue id).
---

# Reconcile docs

GV deliberately allows unreconciled todos, priorities, docs, and issues. New work goes into
dated files (`docs/todos/YYYY-MM-DD.md`) or Linear without first being reconciled with older
material. This skill is the other half: a gradual reconciliation the user does in spare
moments, **one item per session**, often from a phone.

The ideas in these documents and issues have value that careless processing destroys. Be
conservative: the user decides; you surface, propose, and apply exactly what was chosen.

## Sources

**Repo:**

- `todo.md`
- `docs/todos/*.md`
- `docs/features.md`
- The open-questions / deferred / future sections of `docs/*.md` and `swift-app/SWIFT-APP.md`
- Open RFDs in `docs/rfd/` (state `ideation` or `discussion`). The process, header, and states
  are in `docs/rfd/rfd.md`; follow it whenever an option creates or changes an RFD.

**Linear:** open issues (Backlog, Todo, In Progress) in the **Gainzville** team (key `GV`).
Use the Linear MCP tools.

- The **Gainzville (Legacy)** team (`OLDGV`) is an archive and out of scope. Don't pick from it,
  edit it, or propose moving issues out of it.
- If no Linear tools are available in the session, say so in one line and work from the repo
  only.

If the user gave a scope (a file, topic, todo id, or issue id like `GV-48`), stay within it.

## 1. Pick one item

Good candidates, roughly in this order:

1. **Overlap.** The same idea in two or more places: doc ↔ doc, doc ↔ issue, issue ↔ issue.
   Wording may differ.
2. **Conflict.** Two places that disagree, e.g. an issue vs a newer design decision in a doc.
3. **Possibly stale.** An open issue or todo that may already be done, or superseded by a
   later decision. Check the code and `git log` before claiming that; say what you checked.
4. **Misplaced.** Detailed work (bugs, polish, small features) sitting in a doc that would fit
   better as an issue, or design reasoning buried in an issue that belongs in a doc.
   For example, the properties the system should hold are a design and system concern, so
   they're listed in `docs/properties.md`. The matching issue tracks the work of increasing
   property testing and links to the doc. It doesn't spell out "implement property A".
5. **Open question without an RFD.** A big open design question, one that reaches from what
   the user can do down to how data is modeled, living in an issue, a dated todo, or scattered
   across docs. This includes existing design docs that are really RFDs already: a question,
   options, and a lean, with no decision. `docs/convex-evaluation.md` is an example.

There's no record of past picks. Vary the area (repo vs Linear, core vs swift, different docs)
rather than always starting at the top of a list. Pick something answerable in a few minutes.
If an item turns out to be a deep design question, it can still be the item: offer "move to an
RFD", "turn into a dated todo", or "leave as an issue" instead of trying to settle the design.

## 2. Present it self-contained

The user is often on a phone with no computer. Picture the reader as someone who knows
the project and how it has evolved and has forgotten many details. They don't know what is in a
given file or at a given line number.

The presentation answers two questions in order. First, what does the item say, and is that
all of it? Second, where do things stand now?

1. **Source and proposal first, before any judgment about status.** Say where the item lives
   in words (the old `todo.md` list; GV-12, *title*), not by `path:line`. Then say what it
   proposes, quoting the text verbatim when it's short.
2. **Say whether that's the whole item.** The reader needs to know they're seeing everything.
   If a one-line title or todo is all there is, say so explicitly. If there's more, such as
   sub-bullets, a description, or comments, give the gist of what the extra material adds.
3. **Current state, briefly.** Describe what exists now as behavior or concepts, not code.
   Name a type or file only when it's the thing being decided on. Say in a few words what
   you checked, and how the locations relate (overlap, conflict, stale, misplaced).

Example of the right register:

> There's a Linear issue, **GV-7** "Add `clock` and `rng` to core to allow for determinism",
> from April. That title is the whole issue: no description, no comments. There's also a
> matching one-line todo in the old `todo.md` list: "Use seeded rng for determinism in
> application code (e.g. for generating Uuid's)." That line is the entire todo too.
>
> Your June work on making the clock and random IDs injectable, so deterministic simulations
> can control them, appears to cover both.

Keep it about that long. Don't walk through how the code works or argue for why an idea is
good. If the user asks for more, add one layer at a time.

## 3. Offer reconciliations

Use AskUserQuestion with up to four options. The user can always type a custom answer.
Every question includes these two options:

- **Need more context**: change nothing yet. Re-present the item one layer deeper, then ask
  again.
- **Skip**: change nothing.

That leaves room for one or two concrete proposals. Each one says exactly what changes and
where.

Typical shapes:

- **Mark done.** Remove the item from repo lists, or move the issue to Done, citing the
  evidence (commit or code location).
- **Merge.** Keep it in one canonical place and replace the others with a link. For two
  issues, mark one as Duplicate of the other.
- **Move to Linear.** Create a GV issue (see *Writing issues* below). Replace the doc text with
  the issue id and link, or remove it if the whole item moved.
- **Move to a doc.** Copy the design reasoning from an issue into the doc verbatim, and comment
  on the issue with the link.
- **Move to an RFD.** Create `docs/rfd/NNNN-slug/NNNN-slug.md` (next number) with the header from
  `docs/rfd/rfd.md`. Copy each source in verbatim under its own section with a one-line origin
  note, the newest as the latest/lean and older material as background. The user tidies it up
  later. Then delete the copied text from the repo sources, with no link left behind. Reduce
  the Linear issue's description to a pointer to the RFD (the issue history keeps the old
  text), or create a `Discovery` issue if there isn't one.
- **Convert a doc into an RFD.** For a design doc that is really an open question. `git mv` it to
  `docs/rfd/NNNN-slug/NNNN-slug.md`, add the header, and leave the body as is. Update inbound links,
  including the `CLAUDE.md` docs table, and point or create the Linear issue as above.
- **Cross-link.** Leave both and link each to the other, for when the overlap is real but the
  angles differ.
- **Cancel.** Move an issue to Canceled with a comment saying why.
- **Turn into a dated todo.** For things that need real thought, not cleanup.
- **Skip.** Change nothing. No record is kept; the item may come up again.

Rules for the options:

- Prefer moving and linking over rewriting. When text moves between docs, move it
  **verbatim**.
- Never paraphrase the user's ideas into "cleaner" wording unless that is the chosen option.
- Repo text is removed only if it's preserved elsewhere (another doc, an issue) or verified
  done. Git history keeps the removed text.
- **Linear is the home for work items.** When a `todo.md` item is covered by a GV issue
  (already, or after Move to Linear), remove the line from `todo.md` rather than leaving a link
  behind. Keeping both is split brain. Inline TODO comments in code are judged case by case,
  since a code TODO that duplicates an issue can be healthy.
- **Never delete Linear issues or comments.** Use Done, Canceled, or Duplicate plus a comment,
  so the text survives.
- New issues use the existing GV labels (`Bug`, `Feature`, `Improvement`, `Discovery`, `Idea`,
  `core`, `swift`, `Attributes`, `simulation`). Don't create labels without asking.

**Writing issues.** An issue describes the work as it stands now, following ordinary
issue-tracker practice. It is not a record of where the text came from.

- Title and description state the problem or idea as it is today: what to do, why, and any
  current-state context someone picking it up needs.
- Keep the user's own wording wherever it still applies. Quote it or use it directly, rather
  than rephrasing it. Leave out parts that are done, superseded, or obsolete.
- Provenance is one short line at the end at most, e.g. "From `todo.md` (reconciled
  2026-10-03)". Don't quote an old item in full just to preserve it, because git history
  already keeps it.

**Cross-reference conventions:**

- Docs refer to issues by id with a link: `[GV-48](https://linear.app/gainzville/issue/GV-48)`.
- Issues refer to repo files with GitHub links:
  `https://github.com/bsluther/gainzville-rust/blob/main/<path>`.

## 4. Apply and record

1. Make exactly the chosen change, nothing adjacent. Show a brief summary of what changed
   (`path:line`, issue ids).
2. **Record the decision where the change lives.** There is no separate log.
   - Repo changes: the commit message (below).
   - Linear changes: a comment on each issue touched, saying what was decided and why, and
     linking to anything it now points to.
3. **Commit and push repo changes.** This is the default, so don't ask first; the user chose it
   to avoid PR overhead. Skip this step if only Linear changed.
   - Stage only the files this item touched, by path. Never `git add -A` or `git add .`: the
     working tree may hold the user's unrelated work.
   - Commit message: the subject is `Reconcile docs: <short item title>`. The body gives the
     decision (the chosen option or the user's custom answer, in their words), the sources
     (paths, issue ids), and any Linear issues created or changed.
   - Push straight to `main`, or to the branch the user is working on locally. Don't open a pull
     request. In a cloud session that started on a generated branch (e.g. `claude/...`), push
     the commit to `main`: `git pull --rebase origin main`, then `git push origin HEAD:main`.
   - If the rebase conflicts or the push is rejected, stop and report it. Don't force-push, and
     don't resolve conflicts in the user's docs without asking.
4. End by asking whether they want another item. Don't start one unprompted.

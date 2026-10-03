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

There's no record of past picks. Vary the area (repo vs Linear, core vs swift, different docs)
rather than always starting at the top of a list. Pick something answerable in a few minutes.
If an item turns out to be a deep design question, it can still be the item: offer "turn into
a dated todo" or "leave as an issue" instead of trying to settle the design.

## 2. Present it self-contained

The user may have no context at all: a free moment, cold start, small screen. Write the item
so it can be understood without opening anything:

- **What it is**, in one or two sentences.
- **The source text, quoted verbatim**, with `path:line` for repo locations and the issue id +
  title for Linear.
- **Context needed to judge it**: what the referenced code or concept is, today's state
  (verified, with what you checked), and how the locations relate (overlap, conflict, stale,
  misplaced).
- Re-ground any coined term or doc name inline. Don't assume earlier sessions.

Keep it short. This is a quick decision, not an essay.

## 3. Offer reconciliations

Use AskUserQuestion with 2–4 concrete options. The user can always type a custom answer.
Each option says exactly what changes and where. **Always include "Skip"** (change nothing),
unless the options are already at four and one of them is clearly a no-op.

Typical shapes:

- **Mark done.** Remove the item from repo lists, or move the issue to Done, citing the
  evidence (commit or code location).
- **Merge.** Keep it in one canonical place and replace the others with a link. For two
  issues, mark one as Duplicate of the other.
- **Move to Linear.** Create a GV issue whose description is the doc text verbatim, plus a link
  back to the doc. Replace the doc text with the issue id and link.
- **Move to a doc.** Copy the design reasoning from an issue into the doc verbatim, and comment
  on the issue with the link.
- **Cross-link.** Leave both and link each to the other, for when the overlap is real but the
  angles differ.
- **Cancel.** Move an issue to Canceled with a comment saying why.
- **Turn into a dated todo.** For things that need real thought, not cleanup.
- **Skip.** Change nothing. No record is kept; the item may come up again.

Rules for the options:

- Prefer moving and linking over rewriting. When text moves, move it **verbatim**.
- Never paraphrase the user's ideas into "cleaner" wording unless that is the chosen option.
- Repo text is removed only if it's preserved elsewhere (another doc, an issue) or verified
  done. Git history keeps the removed text.
- **Never delete Linear issues or comments.** Use Done, Canceled, or Duplicate plus a comment,
  so the text survives.
- New issues use the existing GV labels (`Bug`, `Feature`, `Improvement`, `Discovery`, `Idea`,
  `core`, `swift`, `Attributes`). Don't create labels without asking.

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

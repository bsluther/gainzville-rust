---
name: reconcile-docs
description: Gradually reconcile GV's todo lists, roadmap, and design-doc open questions, one item per session. Picks a single unreconciled item, presents it self-contained, and offers reconciliation options for the user to choose from. Use when the user asks to reconcile docs, chip away at doc cleanup, or invokes /reconcile-docs (optionally with a scope like a file, topic, or todo id).
---

# Reconcile docs

GV deliberately allows unreconciled todos, priorities, and docs. New work goes into dated files
(`docs/todos/YYYY-MM-DD.md`) without first being reconciled with older material. This skill is
the other half: a gradual reconciliation the user does in spare moments, **one item per
session**.

The ideas in these documents have value that careless processing destroys. Be conservative:
the user decides; you surface, propose, and apply exactly what was chosen.

## 1. Pick one item

**Sources:**

- `todo.md`
- `docs/todos/*.md`
- `docs/features.md`
- The open-questions / deferred / future sections of `docs/*.md` and `swift-app/SWIFT-APP.md`

If the user gave a scope (a file, topic, or todo id), stay within it.

Read the log entries in `docs/reconciliation-log/`, one file per reconciled item (see the
format below). Skip items already logged, unless an entry says "revisit" and the condition has
passed.

Good candidates, roughly in this order:

1. **Overlap.** The same idea in two or more places, possibly worded differently.
2. **Conflict.** Two places that disagree, e.g. a design doc vs a newer dated todo.
3. **Possibly stale.** An item that may already be done, or superseded by a later decision.
   Check the code and `git log` before claiming that; say what you checked.
4. **Homeless.** An item in a dated todo whose idea belongs in a design doc, or vice versa.

Vary the area across sessions rather than always starting at the top of `todo.md`. Pick
something answerable in a few minutes. If an item turns out to be a deep design question, it
can still be the item: offer "turn into a dated todo" as an option instead of trying to settle
the design.

## 2. Present it self-contained

The user may have no context at all: a free moment, cold start. Write the item so it can be
understood without opening any file:

- **What it is**, in one or two sentences.
- **The source text, quoted verbatim**, with `path:line` for every location it appears.
- **Context needed to judge it**: what the referenced code or concept is, today's state
  (verified, with what you checked), and how the locations relate (overlap, conflict, stale,
  homeless).
- Re-ground any coined term or doc name inline. Don't assume earlier sessions.

Keep it short. This is a quick decision, not an essay.

## 3. Offer reconciliations

Use AskUserQuestion with 2–4 concrete options. The user can always type a custom answer.
Each option says exactly what changes and where. Typical shapes:

- **Mark done.** Remove from the list(s), citing the evidence (commit or code location).
- **Merge.** Keep it in one canonical place and replace the others with a link.
- **Move.** Relocate verbatim to the doc where it belongs.
- **Cross-link.** Leave both, linked to each other, for when the overlap is real but the angles
  differ.
- **Turn into a dated todo.** For things that need real thought, not cleanup.
- **Skip for now.** Logged as revisit, so it isn't immediately re-picked.

Rules for the options:

- Prefer moving and linking over rewriting. When text moves, move it **verbatim**.
- Never paraphrase the user's ideas into "cleaner" wording unless that is the chosen option.
- Deletion only removes text that is preserved elsewhere, or verified done. Removed text is
  always copied verbatim into the log entry.

## 4. Apply, log, commit

1. Make exactly the chosen edit, nothing adjacent. Show a brief summary of what changed
   (`path:line`).
2. Write a log entry as a **new file**: `docs/reconciliation-log/YYYY-MM-DD-<slug>.md`, where
   `<slug>` is a short kebab-case name for the item. One file per item keeps parallel sessions
   from conflicting.
3. Commit and push. This is the default, so don't ask first; the user chose it to avoid PR
   overhead.
   - Stage only the files this item touched, by path. Never `git add -A` or `git add .`: the
     working tree may hold the user's unrelated work.
   - Commit message: `Reconcile docs: <short item title>`.
   - Push straight to `main`, or to the branch the user is working on locally. Don't open a pull
     request. In a cloud session that started on a generated branch (e.g. `claude/...`), push
     the commit to `main`: `git pull --rebase origin main`, then `git push origin HEAD:main`.
   - If the rebase conflicts or the push is rejected, stop and report it. Don't force-push, and
     don't resolve conflicts in the user's docs without asking.
4. End by asking whether they want another item. Don't start one unprompted.

### Log entry format

```markdown
# <short item title>

- **Date:** YYYY-MM-DD
- **Sources:** `path:line`, `path:line`
- **Decision:** <chosen option, or the user's custom answer, in their words>
- **Changes:** <what was edited, where>
- **Removed text (verbatim):** <only if anything was removed>
- **Revisit:** <condition, only for skips>
```

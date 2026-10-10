# Requests for Discussion (RFDs)

An RFD captures a big open question: a design, feature, or modeling question that isn't settled
yet. It's a label for the question and a place to organize the reasoning and related work.
Adapted from [Oxide's RFD process](https://rfd.shared.oxide.computer/rfd/0001) for a single user
working with agents, so there's no branch, PR, or review tooling. Like Oxide's, RFDs are timely
rather than polished.

## Files

- One directory per RFD: `docs/rfd/NNNN-slug/`, numbered sequentially from `0001`. "RFD 1" is
  the short handle in docs, issues, and conversation.
- The RFD itself is `NNNN-slug/NNNN-slug.md`. The leading digits sort it above everything else
  in the directory, and the name stays distinct in editor tabs and search results.
- Everything else sits flat beside it, prefixed by kind so it groups when sorted:

  ```
  docs/rfd/0002-dogfooding/
    0002-dogfooding.md                       the RFD
    comment-2026-10-12-skeptic.md            perspective comments (see below)
    source-2026-10-10-transcript-1.md        source material
  ```

- **Sources** are raw input: transcripts, notes, pasted threads. Fix obvious transcription
  errors only; don't rewrite. Date each one. The RFD summarizes and links them, so the RFD
  stays readable and the original stays available when a summary drops something.
- Each RFD starts with a header:

  ```markdown
  # RFD NNNN: Title

  - State: ideation | discussion | decided | abandoned
  - Linear: [GV-NN](https://linear.app/gainzville/issue/GV-NN)
  ```

## States

- **ideation**: the question is captured, not actively being worked.
- **discussion**: actively being worked on.
- **decided**: settled. Add a short decision section at the top; the rest stays as the record of
  how the decision was reached.
- **abandoned**: deliberately dropped.

## Comments

A comment is one perspective's take on an RFD, solicited from an agent (or a person). Each comment
is its own file, `comment-YYYY-MM-DD-<persona>.md`. Commenters don't edit the RFD, and comments
aren't edited after they're written.

**Short by design.** The commenter does the full analysis (reads the RFD, its sources, and the
relevant docs and code) but presents only the summary of its perspective. Aim for under ~300
words. Depth is available on request rather than up front, so the comment lists what it could
expand on.

Shape:

```markdown
# RFD NNNN comment: <persona>

- Date: YYYY-MM-DD
- Lens: <one line: what this perspective cares about>
- Read: <RFD as of date/commit; sources, docs, and code consulted>

## Verdict
One or two sentences.

## Concerns
- Re: <RFD section>: the concern, and why it matters from this lens.

## Missing
What the RFD doesn't address that this lens would expect.

## Recommendation
What this perspective would do, and why.

## Can expand on
- Topics with deeper analysis behind them, available if asked.
```

**Folding back.** After reading comments, the RFD's author adds a new dated latest/lean section
saying what changed and links the comments that changed it. Responses to a comment go in the
RFD, not the comment.

### Persona palette

Reusable perspectives. Pick the ones that fit the RFD, and define bespoke ones for a topic or on
the fly (give a bespoke persona a one-line lens in its comment's header).

- **scope-minimizer**: the smallest version that delivers the goal. What can be cut, deferred,
  or done by hand?
- **skeptic**: argues against the current lean, or against doing this now at all.
- **future-self**: me in a year, having forgotten the code. Is it reversible, legible, and
  recoverable?
- **data-safety**: what can lose or corrupt data, and how would I notice and recover?
- **athlete**: does it fit how training is actually done and logged, mid-session and after?
- **coach**: someone planning for and reviewing another person's training.
- **sync**: offline-first and multi-device implications: merges, ordering, conflicts, HLC.
- **architecture**: fit with the existing model, crate boundaries, docs, and decided RFDs.
- **platform**: iOS/Swift/Xcode/TestFlight constraints and costs.
- **testability**: can it be property-tested or covered by deterministic simulation?
- **prior-art**: how other training apps and tools handle it, and what they learned.

## Linear

Each RFD has one Linear issue (label `Discovery`) that references the RFD and closes when it's
decided or abandoned. The reasoning lives in the RFD, not the issue. Work that follows from the
decision gets its own issues, which reference the RFD.

## After a decision

The RFD stays in place, marked `decided`. Design docs (`docs/*.md`) describe how the system works
now and link back to the RFD for the reasoning.

## For agents

- RFDs in `ideation` or `discussion` are context, not rules. Don't treat a lean as a decision.
- `decided` RFDs are binding. If a task conflicts with one, stop and raise it rather than
  working around it.
- `abandoned` RFDs are history only.

## Adoption

New open questions become RFDs. Existing open questions in other docs and issues move over
gradually (e.g. via reconcile-docs), not all at once.

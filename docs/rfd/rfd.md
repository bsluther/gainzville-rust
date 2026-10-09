# Requests for Discussion (RFDs)

An RFD captures a big open question: a design, feature, or modeling question that isn't settled
yet. It's a label for the question and a place to organize the reasoning and related work.
Adapted from [Oxide's RFD process](https://rfd.shared.oxide.computer/rfd/0001) for a single user
working with agents, so there's no branch, PR, or review tooling. Like Oxide's, RFDs are timely
rather than polished.

## Files

- One file per RFD: `docs/rfd/NNNN-slug.md`, numbered sequentially from `0001`. "RFD 1" is
  the short handle in docs, issues, and conversation.
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

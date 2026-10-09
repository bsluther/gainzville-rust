# RFD 0001: Plan and actual

- State: discussion
- Linear: [GV-63](https://linear.app/gainzville/issue/GV-63)

## Latest / lean (2026-10-02)

Originally the "planning" section of the 2026-10-02 dated todo (`docs/todos/2026-10-02.md`).

Consider moving the plan/actual split from the attribute level to the entry level. Consider
"actual is completion of plan X" as a hint rather than a definitive statement.

The main problem is what "completing" a plan does:

- Mutate the plan in place. Lossy, unless there's history.
- Link to a separate completed entry. This means maintaining the link and keeping both a plan
  copy and an actual copy. If a user checks and unchecks "complete" repeatedly, is there a
  separate plan/actual for each? What does unchecking mean?

The motivation is a diff between a training plan and what someone actually did. It's still
unclear how to achieve that, so one approach is to simplify as much as possible: an entry is
plan or actual via a boolean field, changing it mutates the entry in place, and plans aren't
retained. Add features to support plan retention later rather than anticipating them.

## Background (2026-06-02)

Originally the description of
[GV-63](https://linear.app/gainzville/issue/GV-63) "Consider moving plan/actual split from
attribute to entry level".

Currently plan vs actual/complete is represented in two ways

* `Entry.is_complete` signals whether that entry is a planned or completed.
* `Value` contains both the planned and actual values for an attribute + entry.

Use cases: what plan/actual/complete is meant to achieve.

* **Planning.** Users can assemble a workout they plan to complete (eg a sequence of entries in their log) and differentiate the plan from what they have actually completed.
* **Execution.** Users can follow a workout plan, complete exercises as they go by checking them off.
* **Diffing.** A user or coach can differentiate what was planned vs what actually occurred.

Requirements

* Diffing indicates we need to (1) retain both original plan and the completed event and (2) link the plan to the completed event.
  * A consequence of (1) is that when the user edits the original plan, eg to record the real number of reps they performed, they need to be editing something other than the plan. This is complicated by the fact that we have some original planned value and some final current / actual value, in between there could be any number intermediate steps. Which is "the plan"? This indicates planning should be an explicit user action - do not try to figure out which point in the timeline is the plan and which is the actual, instead have the user or their coach explicitly say "this is the plan value". Similar for actual value.
* Execution and planning require that we can convert a plan into an actual event in an ergonomic way. User clicks the checkbox, we record the values. That does **not** require we retain the plan values - all we need is a checkbox.

Design

* The plan/actual split in attribute currently implemented is meant to support diffing.
* The problem I see with this approach is primarily about the model, not the behavior. *Any* attribute value contains this split. The idea of an attribute value contains the idea of planning and time. Concretely we can see the design creaking when
  * A default value (which is modeled as a template entry with values attached) needs to ignore the plan side and "pick" the actual as the place to store a value.
* The obvious alternative is to split plan/actual at the entry level. This means more illegal states become possible.

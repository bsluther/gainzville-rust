### Type

General: Boolean, Int, Float, Text
GV: Activity, Attribute
- Activity exposes `Activity.name`, later categories functions (`Activity.ancestors`).
- Attributes are plan/actual and can be range values, so we need to project/fold down.
    - Can choose `RangeFold` once at the `View` level (or not at all and we just default to `Average`)
    or override per attribute (deferred).
    - Default all attribute values to `actual` and expose a `Attribute.plan` to get at the plan value.
- Attributes have type-specific behavior like ordering, members, domain, etc.

### BaseSource

### Dimension

Anything of type `row -> value` at the current grain.

A Dimension encompasses both the stored and computed "columns" of the source.
Example: `(instant, reps, rpe, training_load = reps * rpe)`.

Malloy Docs: https://docs.malloydata.dev/documentation/language/fields#dimensions

In Malloy, a Dimension is a kind of Field. What is the GV analogue of Field?

Question: are Dimensions defined on a `Source`?

### Measure

Anything of type `set-of-rows -> value`; not a `Dimension` because you can't evaluate it on a single row.

A `Measure` applied at a grouping produces a `Dimension` in the new grain.

`Measures`s live in the `Catalog` (should this just be `Source`?) as recipes; they're outputs live
in the result as `Dimension`s.



### OPEN QUESTIONS

Is the `Catalog` just the `BaseSource`?
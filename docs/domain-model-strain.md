# Domain model strain log

Places where importing real training logs (see [import design](./import-design.md)) pushed on
the domain model and forced a workaround. Each entry records the strain, the **interim** encoding
we're using, and a sketch of a **future** first-class fix. This is a backlog of model gaps, not a
spec — revisit when the modeling cost of the workaround outweighs the fix.

## 1. Entries with no definite time

**Strain.** Some logged entries have no timestamp at all — e.g. loose mobility jotted with no
session header or time (`3x hamstring` / `2x hooklying`). The model requires every **root** entry
to carry a start or end (`import_day` rejects a timeless root), so there's nowhere to hang them.

**Interim.** Give the entry a *fabricated* complete timestamp (a neutral placeholder) and flag it
with the **`Tags`** multiselect attribute option **`Unknown time`**, so the fabrication is visible
downstream rather than masquerading as a real time.

**Future.** First-class time precision: store a real timestamp plus a precision marker
(`day` / `time`), so a day-precision entry renders as "sometime on 2025-01-15" instead of a
fake wall-clock time. Could live as a field on the temporal, or a precision enum alongside it.

## 2. Sub-minute and rest durations (fingerboard / interval work)

**Strain.** Fingerboard repeaters carry three sub-minute time quantities per protocol: the hang
time (e.g. 7s), rest between reps (e.g. 3s), and rest between sets (e.g. 3 min). The model has:
(a) a temporal **duration** that the `get_day` view renders in **whole minutes**, so a 7-second
set shows as `0 min`; and (b) **no duration attribute type**, so per-rep/per-set rests have
nowhere structured to go.

**Interim.** Hang time → the set entry's temporal `duration` (stored fine as ms; just renders
coarsely). Rests → numeric attributes **`Rest Between Sets (mins)`** and **`Rest Between Reps
(secs)`** (plain numbers with the unit baked into the name).

**Future.** A real **Duration attribute type** (value + unit, sub-second capable) so rests are
typed durations, not bare numbers; and finer-grained duration rendering (`s`/`m`/`h`) in views.

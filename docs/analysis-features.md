# Analysis & Visualization — Motivating Features

The analyses a user should be able to build over their training data. This is **motivation, not
design**: it describes *what* users want, in domain terms, and is the bar any analysis design should
support. It does not commit to how anything is implemented.

## Counting
- After a session, see the total count of boulder problems attempted.
- Total count; count by outcome; sends by grade; outcome by grade.
  - Counts are over **entries** (each logged set, each problem). Reps within a set are a *value* on
    that entry, not extra entries — so `count` is number of entries, while "total reps" sums the
    reps value.

## Frequency / consistency
- Sets per day, sessions per week, workouts per month.

## Aggregation
- Count of sends by grade.
- Longest run (max distance ran).
- Heaviest single lift per period (max load by period).

## Nested aggregation
- Climb outcome by grade — e.g. `{ v4: { attempt: 2, send: 7 }, v6: { attempt: 7, send: 3 } }`.
- Average climbs per session (count per session, then average across sessions).

## Filtering and re-grouping on computed values
- Sessions with ≥ 10 climbs, counted per week (summarize into sessions, keep those meeting the
  threshold, then count per week).
- Weeks where weekly volume exceeded X (filter on a computed total).

## Per-session analysis
- Group and summarize by training session: climbs per session, volume per session.
- Sessions are marked **explicitly** by the user — e.g. a "Session Type" = Strength / Climbing
  attribute on the entry that contains the session's work — and analysis groups by that.
  Inferring sessions automatically from timing is a possible future need, not a current requirement.

## Arithmetic measures (derived metrics)
- Tonnage: `reps × weight`.
- Send rate: `sends / attempts`.
- Estimated 1RM (Epley): `weight × (1 + reps / 30)`.
- Pace: `distance / duration`.
- Training load: `RPE × duration`.
- Training density: `work / time`.

## Sequential / time-ordered
- Moving averages (7-day bodyweight, smoothed pace).
- Acute:Chronic Workload Ratio (7-day load vs. 28-day load).
- Streaks: consecutive weeks hitting a target.
- Deltas between consecutive sessions.
- PR detection: is this a new best vs. everything prior?
- Time-since-last: days since this muscle group was trained.

## Extremes with context
- PR (max weight) and the date it happened.
- Hardest send this season, and which route / when.
- These return not just the extreme value but the **context** of the entry that produced it (date,
  route, …).

## Statistics
- Histograms: run distances, set weights, session lengths.
- Percentiles: p90 session length.
- Median: median run length.
- Training monotony (Foster): mean load ÷ standard deviation of load.
- Grouping continuous values into named bands: HR zones, RPE bands, grade tiers.

## Cross-source correlation
- Scatter: bodyweight vs. estimated 1RM over the same period.
- Sleep vs. performance.
- Fingerboard tonnage vs. max send over time.
- I.e. relating two different measurements aligned over a shared period or grouping.

## Taxonomy / categorization
- Volume per muscle group.
- Volume per push / pull / legs.
- These rollups rely on **categorizing activities** — an activity "is a" Push / Chest / Pectoralis
  Major / Triceps, etc. (e.g. Bench Press). The user maintains these categories as data; analysis
  rolls up by them. One entry can belong to several categories, so per-category totals can exceed
  the overall total — that's expected.

## Absence analysis
- Rest-day count, training gaps, longest layoff.
- Missed planned workouts.
- These depend on accounting for periods that have **no** data, which ordinary summaries omit.

## Goals & forecasting
- Goal progress: e.g. 1000 miles this year, or send ten 5.12s this summer — showing **how close**,
  not just whether it's achieved.
- Trend / projection over time.

## Cross-cutting assumptions
Domain realities these features share:
- Calendar groupings (per day / week / month) depend on the user's **timezone** and a chosen
  **week start**.
- Analysis is over **actual** logged values by default; **planned** values support goals,
  forecasting, and "missed planned" comparisons.
- Some attributes (numeric, mass/length, ordered selects) can hold a **range** rather than a single
  value, so analyses over them need a defined way to reduce a range to one value.
- Most groupings are over a single value per entry, but **categorization is multi-valued** (an
  activity can be many categories at once) — see Taxonomy above.

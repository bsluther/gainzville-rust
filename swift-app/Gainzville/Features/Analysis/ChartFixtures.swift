import Foundation

// Hand-written ChartModels for the gallery — the fixtures the components are
// prototyped against, standing in for `render_model` output until the Rust
// side exists. Two per kind: a canonical, high-traffic usage, and one that
// stretches the contract (the edge cases the design settled: empty categories,
// nil-vs-zero, step lines, sparse gaps, the Other bundle, empty cells).

struct ChartFixture: Identifiable {
    let id: String          // short name shown nowhere; models carry titles
    let note: String        // what this fixture exercises — shown under the card
    let content: Content

    enum Content {
        case bigValue(BigValueModel)
        case bar(BarModel)
        case line(LineModel)
        case pie(PieModel)
        case table(TableModel)
    }
}

enum ChartFixtures {

    static let sections: [(title: String, fixtures: [ChartFixture])] = [
        ("Big value", [bigValueCanonical, bigValueStretch, bigValueEmpty]),
        ("Bar", [barCanonical, barStretch, barEmpty]),
        ("Line", [lineCanonical, lineStretch, lineLong, lineEmpty]),
        ("Pie", [pieCanonical, pieStretch, pieEmpty]),
        ("Table", [tableCanonical, tableStretch, tableEmpty]),
    ]

    // MARK: Big value

    static let bigValueCanonical = ChartFixture(
        id: "big-count",
        note: "Canonical: a plain count.",
        content: .bigValue(BigValueModel(
            title: nil, label: "climbs this month", value: 47, format: .count)))

    static let bigValueStretch = ChartFixture(
        id: "big-quantity",
        note: "Stretch: a large quantity — grouping separator and unit from the format descriptor.",
        content: .bigValue(BigValueModel(
            title: nil, label: "total volume this month", value: 12540,
            format: .quantity(unit: "kg", precision: 0))))

    // MARK: Bar

    private static let grades = ["V0", "V1", "V2", "V3", "V4", "V5", "V6", "V7", "V8"]
        .map(CategorySlot.init)

    static let barCanonical = ChartFixture(
        id: "bar-simple",
        note: "Canonical: one series. V0 is nil (never attempted) — the full domain keeps its slot empty instead of dropping it. Legend hidden at one series.",
        content: .bar(BarModel(
            title: "Climbs by grade",
            xLabel: "Grade",
            domain: grades,
            yLabel: "Climbs",
            yFormat: .count,
            series: [BarSeries(meta: SeriesMeta(label: "Climbs", colorSlot: 0),
                               values: [nil, 4, 9, 14, 18, 12, 7, 3, 1])],
            stacking: .stacked)))

    static let barStretch = ChartFixture(
        id: "bar-stacked",
        note: "Stretch: stacked series with a legend. V0 is nil in both series; V8 attempts is a measured 0 (present, empty bar) — nil and zero are different data.",
        content: .bar(BarModel(
            title: "Outcome by grade",
            xLabel: "Grade",
            domain: grades,
            yLabel: "Climbs",
            yFormat: .count,
            series: [
                BarSeries(meta: SeriesMeta(label: "Send", colorSlot: 0),
                          values: [nil, 4, 8, 11, 12, 6, 3, 1, 1]),
                BarSeries(meta: SeriesMeta(label: "Attempt", colorSlot: 1),
                          values: [nil, 1, 2, 3, 6, 6, 4, 2, 0]),
            ],
            stacking: .stacked)))

    // MARK: Line

    static let lineCanonical = ChartFixture(
        id: "line-sparse",
        note: "Canonical: one sparse series — the 8-day logging gap stays visible as horizontal distance; markers keep isolated points honest. Zero-baseline off (a 77–79 kg series from 0 is a flat line).",
        content: .line(LineModel(
            title: "Bodyweight",
            grain: .day,
            yLabel: "Bodyweight",
            yFormat: .quantity(unit: "kg", precision: 1),
            yIncludesZero: false,
            series: [LineSeries(
                meta: SeriesMeta(label: "Bodyweight", colorSlot: 0),
                step: false,
                points: [
                    point(6, 1, 78.4), point(6, 2, 78.1), point(6, 3, 78.6),
                    point(6, 5, 78.2), point(6, 6, 77.9),
                    // 8-day gap
                    point(6, 14, 78.9), point(6, 15, 78.5), point(6, 17, 78.2),
                    point(6, 20, 77.8), point(6, 21, 77.6), point(6, 24, 77.9),
                    point(6, 27, 77.4),
                ])])))

    static let lineStretch = ChartFixture(
        id: "line-step",
        note: "Stretch: two series sharing one axis — daily top set plus its running max as a step line (no markers on the step series).",
        content: .line({
            let topSet = [
                point(5, 4, 102.5), point(5, 8, 105), point(5, 11, 100),
                point(5, 15, 107.5), point(5, 18, 105), point(5, 22, 110),
                point(5, 25, 107.5), point(5, 29, 112.5), point(6, 1, 110),
                point(6, 5, 115), point(6, 8, 112.5), point(6, 12, 117.5),
                point(6, 15, 115), point(6, 19, 120), point(6, 22, 117.5),
                point(6, 26, 122.5),
            ]
            return LineModel(
                title: "Back squat — top set & PR",
                grain: .week,
                yLabel: "Load",
                yFormat: .quantity(unit: "kg", precision: 0),
                yIncludesZero: false,
                series: [
                    LineSeries(meta: SeriesMeta(label: "Top set", colorSlot: 0),
                               step: false, points: topSet),
                    LineSeries(meta: SeriesMeta(label: "PR so far", colorSlot: 2),
                               step: true, points: runningMax(topSet)),
                ])
        }()))

    static let lineLong = ChartFixture(
        id: "line-long",
        note: "Stretch: three years of weekly data — spans past the week-grain window (26 weeks), so the chart scrolls horizontally and opens at the most recent data.",
        content: .line(LineModel(
            title: "Back squat — top set, 3 years",
            grain: .week,
            yLabel: "Load",
            yFormat: .quantity(unit: "kg", precision: 0),
            yIncludesZero: false,
            series: [LineSeries(
                meta: SeriesMeta(label: "Top set", colorSlot: 0),
                step: false,
                points: (0..<156).map { week in
                    // deterministic long-term shape: slow gains + training waves
                    let value = 90 + Double(week) * 0.28 + 7 * sin(Double(week) / 6)
                    return TimePoint(
                        instant: day(6, 27).addingTimeInterval(Double(week - 155) * 7 * 86_400),
                        value: (value * 2).rounded() / 2)   // to the nearest 0.5 kg plate-ish
                })])))

    // MARK: Pie

    static let pieCanonical = ChartFixture(
        id: "pie-split",
        note: "Canonical: three slices, value-descending, no cap in play.",
        content: .pie(PieModel(
            title: "Volume by split",
            slices: [
                PieSlice(label: "Legs", value: 9800, colorSlot: 0),
                PieSlice(label: "Push", value: 8400, colorSlot: 1),
                PieSlice(label: "Pull", value: 7200, colorSlot: 2),
            ],
            other: nil,
            format: .quantity(unit: "kg", precision: 0),
            isDonut: false)))

    static let pieStretch = ChartFixture(
        id: "pie-other",
        note: "Stretch: donut with the slice cap bound — the Other bundle (3 grades, 8 climbs) exceeds the smallest kept slice but still renders last, in the neutral token.",
        content: .pie(PieModel(
            title: "Climbs by grade",
            slices: [
                PieSlice(label: "V4", value: 18, colorSlot: 0),
                PieSlice(label: "V3", value: 14, colorSlot: 1),
                PieSlice(label: "V5", value: 12, colorSlot: 2),
                PieSlice(label: "V2", value: 9, colorSlot: 3),
                PieSlice(label: "V6", value: 7, colorSlot: 4),
                PieSlice(label: "V1", value: 4, colorSlot: 5),
            ],
            other: OtherSlice(value: 8, bundledCount: 3),
            format: .count,
            isDonut: true)))

    // MARK: Table

    static let tableCanonical = ChartFixture(
        id: "table-simple",
        note: "Canonical: the grounding view for a grouped result — text left, numbers right.",
        content: .table(TableModel(
            title: "Outcome by grade",
            columns: [
                TableColumn(label: "Grade", format: .count),
                TableColumn(label: "Climbs", format: .count),
                TableColumn(label: "Sends", format: .count),
            ],
            rows: [
                [.text("V2"), .number(9), .number(8)],
                [.text("V3"), .number(14), .number(11)],
                [.text("V4"), .number(18), .number(12)],
                [.text("V5"), .number(12), .number(6)],
                [.text("V6"), .number(7), .number(3)],
            ])))

    static let tableStretch = ChartFixture(
        id: "table-wide",
        note: "Stretch: five columns with dates, empty cells, a long label, and mixed formats.",
        content: .table(TableModel(
            title: "Recent sets",
            columns: [
                TableColumn(label: "Date", format: .count),
                TableColumn(label: "Activity", format: .count),
                TableColumn(label: "Load", format: .quantity(unit: "kg", precision: 1)),
                TableColumn(label: "Reps", format: .count),
                TableColumn(label: "RPE", format: .number(precision: 1)),
            ],
            rows: [
                [.instant(day(6, 26)), .text("Back Squat"), .number(122.5), .number(3), .number(8.5)],
                [.instant(day(6, 26)), .text("Bulgarian Split Squat (rear foot elevated)"), .number(24), .number(10), .empty],
                [.instant(day(6, 24)), .text("Bench Press"), .number(85), .number(5), .number(7)],
                [.instant(day(6, 24)), .text("Bouldering"), .empty, .empty, .empty],
                [.instant(day(6, 22)), .text("Deadlift"), .number(150), .number(2), .number(9)],
            ])))

    // MARK: Empty results
    //
    // The third fixture per kind: a COMPLETE model whose data came back empty
    // (zero rows matched — "last week" with nothing logged). Exercises the
    // totality convention's promise: a designed empty state, never a blank
    // plot or an error.

    static let bigValueEmpty = ChartFixture(
        id: "big-empty",
        note: "Empty: max over zero rows has no value — nil, not 0 (a 0 kg PR would be a lie).",
        content: .bigValue(BigValueModel(
            title: nil, label: "heaviest set this week", value: nil,
            format: .quantity(unit: "kg", precision: 1))))

    static let barEmpty = ChartFixture(
        id: "bar-empty",
        note: "Empty: full domain and series present, every value nil.",
        content: .bar(BarModel(
            title: "Climbs by grade — this week",
            xLabel: "Grade",
            domain: grades,
            yLabel: "Climbs",
            yFormat: .count,
            series: [BarSeries(meta: SeriesMeta(label: "Climbs", colorSlot: 0),
                               values: Array(repeating: nil, count: grades.count))],
            stacking: .stacked)))

    static let lineEmpty = ChartFixture(
        id: "line-empty",
        note: "Empty: the series exists, its points don't.",
        content: .line(LineModel(
            title: "Bodyweight — this week",
            grain: .day,
            yLabel: "Bodyweight",
            yFormat: .quantity(unit: "kg", precision: 1),
            yIncludesZero: false,
            series: [LineSeries(meta: SeriesMeta(label: "Bodyweight", colorSlot: 0),
                                step: false, points: [])])))

    static let pieEmpty = ChartFixture(
        id: "pie-empty",
        note: "Empty: no slices, no Other.",
        content: .pie(PieModel(
            title: "Volume by split — this week",
            slices: [], other: nil,
            format: .quantity(unit: "kg", precision: 0),
            isDonut: false)))

    static let tableEmpty = ChartFixture(
        id: "table-empty",
        note: "Empty: headers stay (the shape is still information), zero rows.",
        content: .table(TableModel(
            title: "Outcome by grade — this week",
            columns: [
                TableColumn(label: "Grade", format: .count),
                TableColumn(label: "Climbs", format: .count),
                TableColumn(label: "Sends", format: .count),
            ],
            rows: [])))

    // MARK: Helpers

    private static func day(_ month: Int, _ day: Int) -> Date {
        Calendar.current.date(from: DateComponents(year: 2026, month: month, day: day))!
    }

    private static func point(_ month: Int, _ dayOfMonth: Int, _ value: Double) -> TimePoint {
        TimePoint(instant: day(month, dayOfMonth), value: value)
    }

    /// The step-line companion of a series: its cumulative max at each instant.
    private static func runningMax(_ points: [TimePoint]) -> [TimePoint] {
        var best = -Double.infinity
        return points.map { p in
            best = max(best, p.value)
            return TimePoint(instant: p.instant, value: best)
        }
    }
}

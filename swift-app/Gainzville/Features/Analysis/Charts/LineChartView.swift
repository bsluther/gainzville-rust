import SwiftUI
import Charts

/// Line chart over time. Series are already unified by the model (a measure
/// split by a dimension and several measures arrive identically), and all
/// share the one y-axis/format — there is deliberately no dual axis.
///
/// Scrubbing: `chartXSelection` owns the gesture (drag on iOS, hover on
/// macOS); we snap the continuous date to the nearest data instant and render
/// a rule + bubble. The bubble is built entirely from the model — points,
/// format, grain, series meta — no extra tooltip payload needed.
struct LineChartView: View {
    let model: LineModel

    @State private var selection: Date?

    private var isMultiSeries: Bool { model.series.count > 1 }

    var body: some View {
        VStack(alignment: .leading, spacing: GvSpacing.md) {
            ChartTitle(model.title)
            if model.isEmpty {
                ChartEmptyState()
            } else {
                chart
                if isMultiSeries {
                    ChartLegend(entries: model.series.map {
                        ($0.meta.label, Color.gvChartSeries(slot: $0.meta.colorSlot))
                    })
                }
            }
        }
    }

    /// When the data spans more than the grain's visible window, the chart
    /// scrolls horizontally, opening at the most recent data (Health-style).
    /// Note Swift Charts keeps the y-scale fixed to the FULL domain while
    /// scrolling — correct for long series, no rescale-per-window.
    private var scrollWindow: TimeInterval? {
        guard let window = model.grain.visibleWindow,
              let range = model.instantRange,
              range.upperBound.timeIntervalSince(range.lowerBound) > window
        else { return nil }
        return window
    }

    @ViewBuilder
    private var chart: some View {
        if let window = scrollWindow, let range = model.instantRange {
            baseChart
                .chartScrollableAxes(.horizontal)
                .chartXVisibleDomain(length: Int(window))
                .chartScrollPosition(initialX: range.upperBound.addingTimeInterval(-window))
        } else {
            baseChart
        }
    }

    private var baseChart: some View {
        Chart {
            ForEach(Array(model.series.enumerated()), id: \.offset) { _, series in
                ForEach(Array(series.points.enumerated()), id: \.offset) { _, point in
                    LineMark(
                        x: .value("Date", point.instant),
                        y: .value(model.yLabel ?? "Value", point.value),
                        series: .value("Series", series.meta.label)
                    )
                    .foregroundStyle(by: .value("Series", series.meta.label))
                    .interpolationMethod(series.step ? .stepEnd : .linear)
                    .lineStyle(StrokeStyle(lineWidth: 2))

                    // Markers on regular series keep sparse data honest: an
                    // isolated measurement between gaps still shows as a dot.
                    // Step series (running max) stay clean lines.
                    if !series.step {
                        PointMark(
                            x: .value("Date", point.instant),
                            y: .value(model.yLabel ?? "Value", point.value)
                        )
                        .foregroundStyle(by: .value("Series", series.meta.label))
                        .symbolSize(24)
                    }
                }
            }

            if let snapped = selection.flatMap(snappedInstant(for:)) {
                RuleMark(x: .value("Selected", snapped))
                    .foregroundStyle(Color.gvNeutral700)
                    .lineStyle(StrokeStyle(lineWidth: 1))
                    .annotation(
                        position: .top, spacing: 0,
                        overflowResolution: .init(x: .fit(to: .chart), y: .fit(to: .chart))
                    ) {
                        selectionBubble(at: snapped)
                    }
            }
        }
        .chartXSelection(value: $selection)
        .chartYScale(domain: .automatic(includesZero: model.yIncludesZero))
        .chartForegroundStyleScale(
            domain: model.series.map(\.meta.label),
            range: model.series.map { Color.gvChartSeries(slot: $0.meta.colorSlot) }
        )
        .chartLegend(.hidden)
        .chartXAxis {
            AxisMarks { value in
                AxisValueLabel {
                    if let date = value.as(Date.self) {
                        // "Jun 14" alone means this year; other years stack
                        // beneath so multi-year series stay unambiguous.
                        VStack(spacing: 0) {
                            Text(date, format: model.grain.axisFormat)
                            if model.grain != .year && !date.isInCurrentYear {
                                Text(date, format: .dateTime.year())
                            }
                        }
                        .font(.gvCaption)
                        .foregroundStyle(Color.gvChartAxisLabel)
                    }
                }
            }
        }
        .chartYAxis {
            AxisMarks { value in
                AxisGridLine().foregroundStyle(Color.gvChartGrid)
                AxisValueLabel {
                    if let number = value.as(Double.self) {
                        Text(model.yFormat.string(from: number))
                            .font(.gvCaption)
                            .foregroundStyle(Color.gvChartAxisLabel)
                    }
                }
            }
        }
    }

    // MARK: Selection

    /// The selection binding delivers a continuous date; snap to the nearest
    /// instant that actually has data (across all series).
    private func snappedInstant(for date: Date) -> Date? {
        model.series
            .flatMap { $0.points.map(\.instant) }
            .min { abs($0.timeIntervalSince(date)) < abs($1.timeIntervalSince(date)) }
    }

    /// Series values at the snapped instant. A series with no point exactly
    /// there is skipped (spike simplification — fine while series share
    /// instants or scrub independently).
    private func values(at instant: Date) -> [(meta: SeriesMeta, value: Double)] {
        model.series.compactMap { series in
            series.points.first { $0.instant == instant }
                .map { (series.meta, $0.value) }
        }
    }

    private func selectionBubble(at instant: Date) -> some View {
        VStack(alignment: .leading, spacing: GvSpacing.sm) {
            // Same year rule as the axis, inline ("Jun 12, 2024").
            Text(instant, format: instant.isInCurrentYear || model.grain == .year
                ? model.grain.axisFormat
                : model.grain.axisFormat.year())
                .font(.gvCaption)
                .foregroundStyle(Color.gvTextSecondary)
            ForEach(Array(values(at: instant).enumerated()), id: \.offset) { _, entry in
                HStack(spacing: GvSpacing.sm) {
                    RoundedRectangle(cornerRadius: 2)
                        .fill(Color.gvChartSeries(slot: entry.meta.colorSlot))
                        .frame(width: 8, height: 8)
                    if isMultiSeries {
                        Text(entry.meta.label)
                            .font(.gvCaption)
                            .foregroundStyle(Color.gvTextSecondary)
                    }
                    Text(model.yFormat.string(from: entry.value))
                        .font(.gvCaption.monospacedDigit())
                        .foregroundStyle(Color.gvTextBright)
                }
            }
        }
        .padding(GvSpacing.md)
        .background(
            RoundedRectangle(cornerRadius: 8)
                .fill(Color.gvNeutral900)
                .stroke(Color.gvNeutral800, lineWidth: 1)
        )
    }
}

private extension Date {
    /// Axis/tooltip year rule: a bare "Jun 14" means the current year.
    var isInCurrentYear: Bool {
        Calendar.current.isDate(self, equalTo: .now, toGranularity: .year)
    }
}

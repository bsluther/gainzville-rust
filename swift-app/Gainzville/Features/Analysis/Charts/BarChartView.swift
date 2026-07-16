import SwiftUI
import Charts

/// Bar chart over a discrete domain. One component covers plain, stacked, and
/// grouped bars (model decision D5): a plain bar is one series with the legend
/// hidden; multi-series marks stack by default and group when
/// `stacking == .grouped`.
struct BarChartView: View {
    let model: BarModel

    private var isMultiSeries: Bool { model.series.count > 1 }

    var body: some View {
        VStack(alignment: .leading, spacing: GvSpacing.md) {
            ChartTitle(model.title)
            if model.isEmpty {
                ChartEmptyState()
            } else {
                chart
            }
        }
    }

    private var chart: some View {
        Group {
            Chart {
                ForEach(Array(model.series.enumerated()), id: \.offset) { _, series in
                    ForEach(Array(series.values.enumerated()), id: \.offset) { index, value in
                        // nil = no data: no mark at all. The category still
                        // occupies an x slot because the scale domain below is
                        // the full domain, not just categories present in data.
                        if let value {
                            mark(category: model.domain[index].label, value: value, series: series)
                        }
                    }
                }
            }
            .chartXScale(domain: model.domain.map(\.label))
            .chartForegroundStyleScale(
                domain: model.series.map(\.meta.label),
                range: model.series.map { Color.gvChartSeries(slot: $0.meta.colorSlot) }
            )
            .chartLegend(.hidden)
            .chartXAxis {
                AxisMarks { _ in
                    AxisValueLabel()
                        .font(.gvCaption)
                        .foregroundStyle(Color.gvChartAxisLabel)
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
            if isMultiSeries {
                ChartLegend(entries: model.series.map {
                    ($0.meta.label, Color.gvChartSeries(slot: $0.meta.colorSlot))
                })
            }
        }
    }

    @ChartContentBuilder
    private func mark(category: String, value: Double, series: BarSeries) -> some ChartContent {
        let bar = BarMark(
            x: .value(model.xLabel ?? "Category", category),
            y: .value(model.yLabel ?? "Value", value)
        )
        .foregroundStyle(by: .value("Series", series.meta.label))
        .cornerRadius(2)

        if isMultiSeries && model.stacking == .grouped {
            bar.position(by: .value("Series", series.meta.label))
        } else {
            bar  // multiple marks on one x stack automatically
        }
    }
}

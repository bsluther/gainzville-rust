import SwiftUI
import Charts

/// Parts-of-whole. Slices arrive value-descending from the model; the Other
/// bundle (if the slice cap bound) always renders last, in the neutral token,
/// so it never reads as a real category.
struct PieChartView: View {
    let model: PieModel

    var body: some View {
        VStack(alignment: .leading, spacing: GvSpacing.md) {
            ChartTitle(model.title)
            if model.isEmpty {
                ChartEmptyState()
            } else {
                Chart {
                    ForEach(Array(model.slices.enumerated()), id: \.offset) { _, slice in
                        sector(value: slice.value)
                            .foregroundStyle(Color.gvChartSeries(slot: slice.colorSlot))
                    }
                    if let other = model.other {
                        sector(value: other.value)
                            .foregroundStyle(Color.gvChartOther)
                    }
                }
                ChartLegend(entries: legendEntries)
            }
        }
    }

    private func sector(value: Double) -> some ChartContent {
        SectorMark(
            angle: .value("Value", value),
            innerRadius: model.isDonut ? .ratio(0.62) : .ratio(0),
            angularInset: 1.5   // the surface gap between adjacent fills
        )
        .cornerRadius(2)
    }

    private var legendEntries: [(label: String, color: Color)] {
        var entries = model.slices.map { slice in
            ("\(slice.label)  \(model.format.string(from: slice.value))",
             Color.gvChartSeries(slot: slice.colorSlot))
        }
        if let other = model.other {
            entries.append(("Other (\(other.bundledCount))  \(model.format.string(from: other.value))",
                            Color.gvChartOther))
        }
        return entries
    }
}

import SwiftUI

/// The universal fallback: a thin rendering of the result table itself, legal
/// for every result shape ("what am I actually charting?"). Deliberately
/// minimal — no sorting, no interaction; the view scrolls if the card doesn't
/// fit. Named to avoid colliding with SwiftUI's `Table`.
struct TableChartView: View {
    let model: TableModel

    var body: some View {
        VStack(alignment: .leading, spacing: GvSpacing.md) {
            ChartTitle(model.title)
            Grid(alignment: .leading, horizontalSpacing: GvSpacing.xl, verticalSpacing: GvSpacing.md) {
                GridRow {
                    ForEach(Array(model.columns.enumerated()), id: \.offset) { index, column in
                        Text(column.label)
                            .font(.gvCaption)
                            .foregroundStyle(Color.gvTextSecondary)
                            .gridColumnAlignment(alignment(forColumn: index))
                    }
                }
                Divider().gridCellUnsizedAxes(.horizontal)
                if model.rows.isEmpty {
                    // Empty result: keep the header (the shape is still information),
                    // say why there's nothing under it.
                    Text("No rows")
                        .font(.gvCaption)
                        .foregroundStyle(Color.gvTextSecondary)
                        .padding(.top, GvSpacing.sm)
                }
                ForEach(Array(model.rows.enumerated()), id: \.offset) { _, row in
                    GridRow {
                        ForEach(Array(row.enumerated()), id: \.offset) { index, cell in
                            cellView(cell, format: model.columns[index].format)
                        }
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func cellView(_ cell: TableCell, format: ChartFormat) -> some View {
        switch cell {
        case .number(let value):
            Text(format.string(from: value))
                .font(.gvBody.monospacedDigit())
                .foregroundStyle(Color.gvTextPrimary)
        case .text(let string):
            Text(string)
                .font(.gvBody)
                .foregroundStyle(Color.gvTextPrimary)
        case .instant(let date):
            Text(date, format: .dateTime.month(.abbreviated).day().year())
                .font(.gvBody)
                .foregroundStyle(Color.gvTextPrimary)
        case .empty:
            Text("–")
                .font(.gvBody)
                .foregroundStyle(Color.gvTextSecondary)
        }
    }

    /// Numbers right-align, everything else left-aligns. The column's kind is
    /// read off its first non-empty cell (the model has no per-column type).
    private func alignment(forColumn index: Int) -> HorizontalAlignment {
        for row in model.rows {
            switch row[index] {
            case .number: return .trailing
            case .empty: continue
            default: return .leading
            }
        }
        return .leading
    }
}

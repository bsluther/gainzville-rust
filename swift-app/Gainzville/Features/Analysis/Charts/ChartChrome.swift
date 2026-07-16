import SwiftUI

// Shared chrome for chart components: title header and the hand-rolled legend.
// Swift Charts' built-in legend is hard to theme, so components hide it and
// render this one — which also keeps the rule that text wears text tokens
// (identity is carried by the swatch, never by coloring the label).

struct ChartTitle: View {
    let title: String?

    init(_ title: String?) { self.title = title }

    var body: some View {
        if let title {
            Text(title)
                .font(.gvHeadline)
                .foregroundStyle(Color.gvTextBright)
        }
    }
}

/// The designed "no data" state for an empty result — shown in place of the
/// plot area (the card and title stay, so the chart still says what it is).
struct ChartEmptyState: View {
    var body: some View {
        VStack(spacing: GvSpacing.sm) {
            Image(systemName: "chart.bar")
                .font(.title3)
                .foregroundStyle(Color.gvNeutral600)
            Text("No data")
                .font(.gvCaption)
                .foregroundStyle(Color.gvTextSecondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

struct ChartLegend: View {
    let entries: [(label: String, color: Color)]

    var body: some View {
        // Adaptive grid so long legends (a capped pie: 6 slices + Other) wrap
        // instead of compressing on one row.
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 110), alignment: .leading)],
                  alignment: .leading, spacing: GvSpacing.sm) {
            ForEach(Array(entries.enumerated()), id: \.offset) { _, entry in
                HStack(spacing: GvSpacing.sm) {
                    RoundedRectangle(cornerRadius: 2)
                        .fill(entry.color)
                        .frame(width: 10, height: 10)
                    Text(entry.label)
                        .font(.gvCaption)
                        .foregroundStyle(Color.gvTextSecondary)
                        .lineLimit(1)
                }
            }
        }
    }
}

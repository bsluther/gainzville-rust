import SwiftUI

/// Fixture gallery: every chart component rendered against its two fixtures
/// (canonical + stretch), grouped by kind. This is the prototyping surface —
/// it exercises the ChartModel contract visually before the Rust side exists.
struct ChartGalleryView: View {
    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: GvSpacing.xl) {
                ForEach(ChartFixtures.sections, id: \.title) { section in
                    Text(section.title)
                        .font(.gvTitle)
                        .foregroundStyle(Color.gvTextBright)
                    ForEach(section.fixtures) { fixture in
                        FixtureCard(fixture: fixture)
                    }
                }
            }
            .frame(maxWidth: GvSpacing.contentWidthMax)
            .frame(maxWidth: .infinity)   // center the capped column on wide windows
            .padding(GvSpacing.xl)
        }
        .background(Color.gvBackground)
    }
}

private struct FixtureCard: View {
    let fixture: ChartFixture

    var body: some View {
        VStack(alignment: .leading, spacing: GvSpacing.lg) {
            content
            Text(fixture.note)
                .font(.gvFootnote)
                .foregroundStyle(Color.gvTextSecondary)
        }
        .padding(GvSpacing.lg)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: GvSpacing.entryCornerRadius)
                .fill(Color.gvSurface)
                .stroke(Color.gvNeutral850, lineWidth: 1)
        )
    }

    @ViewBuilder
    private var content: some View {
        switch fixture.content {
        case .bigValue(let model):
            BigValueView(model: model)
        case .bar(let model):
            BarChartView(model: model).frame(height: 240)
        case .line(let model):
            LineChartView(model: model).frame(height: 240)
        case .pie(let model):
            PieChartView(model: model).frame(height: 280)
        case .table(let model):
            ScrollView(.horizontal) { TableChartView(model: model) }
        }
    }
}

#Preview {
    ChartGalleryView()
}

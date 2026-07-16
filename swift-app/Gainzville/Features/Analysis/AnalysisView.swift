import SwiftUI

/// Entry point for the analysis feature (the "Analysis" tab). Today it hosts
/// the component gallery; saved charts, the builder, and per-activity defaults
/// land here as the feature grows (see docs/analysis/README.md).
struct AnalysisView: View {
    var body: some View {
        ChartGalleryView()
            .navigationTitle("Analysis")
    }
}

#Preview {
    NavigationStack { AnalysisView() }
}

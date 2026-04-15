import SwiftUI

/// Placeholder for the workout log. Entry creation and display are Stage 5.
struct LogView: View {
    var body: some View {
        ContentUnavailableView(
            "Log",
            systemImage: "list.bullet.rectangle",
            description: Text("Entry logging coming soon.")
        )
        .navigationTitle("Log")
    }
}

#Preview {
    NavigationStack {
        LogView()
    }
}

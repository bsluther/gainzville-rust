import SwiftUI

/// Library root — browse Activities and Attributes.
/// Currently shows placeholder content; Stage 4 wires in real data and detail navigation.
struct LibraryView: View {
    @State private var selectedTab: LibraryTab = .activities

    var body: some View {
        VStack(spacing: 0) {
            Picker("Library section", selection: $selectedTab) {
                ForEach(LibraryTab.allCases) { tab in
                    Text(tab.title).tag(tab)
                }
            }
            .pickerStyle(.segmented)
            .padding()

            ContentUnavailableView(
                selectedTab.title,
                systemImage: selectedTab.icon,
                description: Text("Coming soon.")
            )
        }
        .navigationTitle("Library")
    }
}

// MARK: - Tab model

enum LibraryTab: String, CaseIterable, Identifiable {
    case activities
    case attributes

    var id: String { rawValue }

    var title: String {
        switch self {
        case .activities: return "Activities"
        case .attributes: return "Attributes"
        }
    }

    var icon: String {
        switch self {
        case .activities: return "figure.run"
        case .attributes: return "list.bullet"
        }
    }
}

#Preview {
    NavigationStack {
        LibraryView()
    }
}

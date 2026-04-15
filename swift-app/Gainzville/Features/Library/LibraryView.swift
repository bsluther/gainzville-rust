import SwiftUI

/// Navigation destination types within the Library section.
enum LibraryDestination: Hashable {
    case activity(FfiActivity)
    case attribute(FfiAttribute)
}

/// Library root — browse Activities and Attributes.
struct LibraryView: View {
    @EnvironmentObject var activitiesVM: ActivitiesViewModel
    @EnvironmentObject var attributesVM: AttributesViewModel
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

            switch selectedTab {
            case .activities:
                ActivitiesListView(activities: activitiesVM.activities)
            case .attributes:
                AttributesListView(attributes: attributesVM.attributes)
            }
        }
        .navigationTitle("Library")
        .navigationDestination(for: LibraryDestination.self) { destination in
            switch destination {
            case .activity(let activity):
                ActivityDetailView(activity: activity)
            case .attribute(let attribute):
                AttributeDetailView(attribute: attribute)
            }
        }
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
            .environmentObject(ActivitiesViewModel())
            .environmentObject(AttributesViewModel())
    }
}

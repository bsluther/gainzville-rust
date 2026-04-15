import SwiftUI

/// Top-level navigation container. iOS uses a TabView; macOS uses a NavigationSplitView sidebar.
struct AppNavigation: View {
    var body: some View {
        #if os(macOS)
        MacNavigation()
        #else
        IOSNavigation()
        #endif
    }
}

// MARK: - iOS / iPadOS

/// Tab-based navigation for iOS and iPadOS.
///
/// On iPhone (compact width) this renders as a bottom tab bar with the iOS 26
/// liquid glass appearance. On iPad, the system automatically adapts the tab
/// bar to a sidebar when `.tabViewStyle(.sidebarAdaptable)` is used.
struct IOSNavigation: View {
    var body: some View {
        TabView {
            Tab("Log", systemImage: "list.bullet.rectangle") {
                NavigationStack {
                    LogView()
                }
            }
            Tab("Library", systemImage: "books.vertical") {
                NavigationStack {
                    LibraryView()
                }
            }
        }
        .tabViewStyle(.sidebarAdaptable)
    }
}

// MARK: - macOS

/// Sidebar + detail navigation for macOS.
struct MacNavigation: View {
    @State private var selection: AppSection? = .log

    var body: some View {
        NavigationSplitView {
            List(selection: $selection) {
                ForEach(AppSection.allCases) { section in
                    Label(section.title, systemImage: section.icon)
                        .tag(section)
                }
            }
            .navigationSplitViewColumnWidth(min: 160, ideal: 200)
            .navigationTitle("Gainzville")
        } detail: {
            switch selection {
            case .log:
                LogView()
            case .library:
                LibraryView()
            case .none:
                ContentUnavailableView("Select a section", systemImage: "sidebar.left")
            }
        }
    }
}

// MARK: - Section model

enum AppSection: String, CaseIterable, Identifiable, Hashable {
    case log
    case library

    var id: String { rawValue }

    var title: String {
        switch self {
        case .log:     return "Log"
        case .library: return "Library"
        }
    }

    var icon: String {
        switch self {
        case .log:     return "list.bullet.rectangle"
        case .library: return "books.vertical"
        }
    }
}

#Preview("iOS") {
    IOSNavigation()
}

#Preview("macOS") {
    MacNavigation()
}

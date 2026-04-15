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
            Tab("Log", systemImage: AppSection.log.icon) {
                NavigationStack {
                    LogView()
                }
            }
            Tab("Library", systemImage: AppSection.library.icon) {
                NavigationStack {
                    LibraryView()
                }
            }
            Tab("Settings", systemImage: AppSection.settings.icon) {
                NavigationStack {
                    SettingsView()
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
                ForEach(AppSection.mainSections) { section in
                    Label(section.title, systemImage: section.icon)
                        .tag(section)
                }
                Divider()
                ForEach(AppSection.utilitySections) { section in
                    Label(section.title, systemImage: section.icon)
                        .tag(section)
                }
            }
            .navigationSplitViewColumnWidth(min: 160, ideal: 200)
            .navigationTitle("Gainzville")
        } detail: {
            NavigationStack {
                switch selection {
                case .log:
                    LogView()
                case .library:
                    LibraryView()
                case .settings:
                    SettingsView()
                case .none:
                    ContentUnavailableView("Select a section", systemImage: "sidebar.left")
                }
            }
        }
    }
}

// MARK: - Section model

enum AppSection: String, CaseIterable, Identifiable, Hashable {
    case log
    case library
    case settings

    var id: String { rawValue }

    var title: String {
        switch self {
        case .log:      return "Log"
        case .library:  return "Library"
        case .settings: return "Settings"
        }
    }

    var icon: String {
        switch self {
        case .log:      return "list.bullet.rectangle"
        case .library:  return "books.vertical"
        case .settings: return "gear"
        }
    }

    /// Primary navigation sections shown above the divider in the macOS sidebar.
    static let mainSections: [AppSection] = [.log, .library]
    /// Utility sections shown below the divider.
    static let utilitySections: [AppSection] = [.settings]
}

#Preview("iOS") {
    IOSNavigation()
}

#Preview("macOS") {
    MacNavigation()
}

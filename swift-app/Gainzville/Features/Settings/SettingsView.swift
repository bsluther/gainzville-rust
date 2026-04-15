import SwiftUI

struct SettingsView: View {
    @AppStorage("gv.isAuthenticated") private var isAuthenticated = false

    var body: some View {
        List {
            Section {
                Button(role: .destructive) {
                    isAuthenticated = false
                } label: {
                    Label("Sign Out", systemImage: "rectangle.portrait.and.arrow.right")
                }
            }
        }
        .navigationTitle("Settings")
    }
}

#Preview {
    NavigationStack {
        SettingsView()
    }
}

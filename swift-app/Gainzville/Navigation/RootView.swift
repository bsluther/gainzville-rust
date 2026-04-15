import SwiftUI

/// Auth gate. Shows LoginView until the user signs in, then AppNavigation.
struct RootView: View {
    @AppStorage("gv.isAuthenticated") private var isAuthenticated = false

    var body: some View {
        if isAuthenticated {
            AppNavigation()
        } else {
            LoginView()
        }
    }
}

#Preview {
    RootView()
}

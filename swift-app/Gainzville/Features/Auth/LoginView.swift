import SwiftUI

/// Placeholder login screen. Sign In sets the auth flag without real authentication.
/// Replace with OAuth / email flow when auth is implemented in core.
struct LoginView: View {
    @AppStorage("gv.isAuthenticated") private var isAuthenticated = false

    var body: some View {
        VStack(spacing: 32) {
            Spacer()

            VStack(spacing: 8) {
                Text("Gainzville")
                    .font(.largeTitle.bold())
                Text("Track what you do.")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }

            Spacer()

            Button("Sign In") {
                isAuthenticated = true
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)

            Spacer()
                .frame(height: 32)
        }
        .padding()
    }
}

#Preview {
    LoginView()
}

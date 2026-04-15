import SwiftUI

/// Placeholder login screen. Sign In sets the auth flag without real authentication.
/// Replace with OAuth / email flow when auth is implemented in core.
struct LoginView: View {
    @AppStorage("gv.isAuthenticated") private var isAuthenticated = false

    var body: some View {
        ZStack {
            Color.gvBackground.ignoresSafeArea()

            VStack(spacing: GvSpacing.xl) {
                Spacer()

                VStack(spacing: GvSpacing.sm) {
                    Text("Gainzville")
                        .font(.gvLargeTitle.bold())
                        .foregroundStyle(Color.gvTextPrimary)
                    Text("Track what you do.")
                        .font(.gvCallout)
                        .foregroundStyle(Color.gvTextSecondary)
                }

                Spacer()

                Button("Sign In") {
                    isAuthenticated = true
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.large)

                Spacer()
                    .frame(height: GvSpacing.xl)
            }
            .padding(GvSpacing.xl)
        }
    }
}

#Preview {
    LoginView()
}

import SwiftUI

/// A labeled section used in detail/profile views.
///
/// Shows a small-caps header with an optional icon action button,
/// followed by a generic content body.
struct GvDetailSection<Content: View>: View {
    let title: String
    var actionIcon: String? = nil
    var onAction: (() -> Void)? = nil
    @ViewBuilder var content: () -> Content

    var body: some View {
        VStack(alignment: .leading, spacing: GvSpacing.md) {
            HStack {
                Text(title)
                    .font(.gvCaption)
                    .foregroundStyle(Color.gvTextSecondary)
                    .textCase(.uppercase)
                    .tracking(0.5)
                Spacer()
                if let icon = actionIcon, let onAction {
                    Button(action: onAction) {
                        Image(systemName: icon)
                            .foregroundStyle(Color.gvTextSecondary)
                    }
                    .buttonStyle(.plain)
                }
            }
            content()
        }
    }
}

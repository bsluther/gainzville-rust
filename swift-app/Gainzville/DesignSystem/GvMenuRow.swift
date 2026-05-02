import SwiftUI

struct GvMenuRow: View {
    let label: String
    var icon: String? = nil
    var isDestructive: Bool = false
    var action: () -> Void = {}
    @Environment(\.dismiss) private var dismiss

    init(_ label: String, icon: String? = nil, isDestructive: Bool = false, action: @escaping () -> Void = {}) {
        self.label = label
        self.icon = icon
        self.isDestructive = isDestructive
        self.action = action
    }

    var body: some View {
        Button {
            dismiss()
            action()
        } label: {
            HStack(spacing: GvSpacing.lg) {
                if let icon {
                    Image(systemName: icon)
                        .frame(width: 20)
                }
                Text(label)
                    .font(.gvBody)
                Spacer()
            }
            .foregroundStyle(isDestructive ? Color.red : Color.gvTextPrimary)
            .padding(.horizontal, GvSpacing.lg)
            .padding(.vertical, GvSpacing.lg)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

struct GvMenuDivider: View {
    var body: some View {
        Rectangle()
            .fill(Color.gvNeutral800)
            .frame(height: 0.5)
    }
}

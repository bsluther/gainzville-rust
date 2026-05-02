import SwiftUI

struct GvCheckbox: View {
    let checked: Bool
    var size: CGFloat = 20
    var onToggle: () -> Void = {}

    var body: some View {
        Button(action: onToggle) {
            Image(systemName: checked ? "checkmark.square" : "square")
                .resizable()
                .scaledToFit()
                .frame(width: size, height: size)
                .foregroundStyle(Color.gvNeutral400)
                .frame(width: 44, height: 44)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

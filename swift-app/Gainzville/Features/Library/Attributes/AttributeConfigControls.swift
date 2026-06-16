import SwiftUI

/// Label on the left, a value control on the right — the shared config-row
/// rhythm used by both the create form and the attribute detail card. The label
/// tone defaults to the detail card's brighter primary; the create form passes
/// the dimmer secondary (`labelColor`).
struct ConfigRow<Control: View>: View {
    var labelColor: Color = .gvTextPrimary
    let label: String
    @ViewBuilder var control: () -> Control

    var body: some View {
        HStack {
            Text(label)
                .font(.gvBody)
                .foregroundStyle(labelColor)
            Spacer()
            control()
        }
    }
}

/// Modal single-select string picker (sheet on iOS, popover on macOS) with
/// centered rows and a trailing checkmark. Shared by the create form's type and
/// default pickers and the detail card's default/unit pickers. `onPick` reports
/// nil only when `includeNone` is set and the "None" row is chosen.
struct OptionPickerList: View {
    let title: String
    let options: [String]
    let selection: String?
    var includeNone: Bool = false
    let onPick: (String?) -> Void

    var body: some View {
        #if os(iOS)
        NavigationStack { list.navigationTitle(title).navigationBarTitleDisplayMode(.inline) }
            .presentationDetents([.medium, .large])
        #else
        list.padding(GvSpacing.md).frame(minWidth: 220)
        #endif
    }

    private var list: some View {
        ScrollView {
            VStack(spacing: 0) {
                if includeNone {
                    row(label: "None", value: nil, isSelected: selection == nil)
                }
                ForEach(options, id: \.self) { option in
                    row(label: option, value: option, isSelected: option == selection)
                }
            }
        }
    }

    private func row(label: String, value: String?, isSelected: Bool) -> some View {
        Button { onPick(value) } label: {
            HStack {
                Spacer()
                Text(label).font(.gvBody).foregroundStyle(Color.gvTextPrimary)
                Spacer()
            }
            .overlay(alignment: .trailing) {
                if isSelected {
                    Image(systemName: "checkmark").foregroundStyle(Color.gvLoggedBlue)
                }
            }
            .padding(.horizontal, GvSpacing.lg)
            .padding(.vertical, GvSpacing.lg)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

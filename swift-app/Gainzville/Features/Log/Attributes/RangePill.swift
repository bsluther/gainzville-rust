import SwiftUI

/// Which end of a range an input edits. Shared by the numeric, select, and
/// mass editors for focus state, presentation routing, and submit-advance.
enum RangeEndpoint: Hashable {
    case min, max
}

/// The composite range pill: min and max inputs separated by a hyphen, with
/// the shared pill border around the whole composite (inner inputs render
/// borderless). Used in exact mode too (`isRange: false`) so the min input
/// keeps a stable view identity when the mode flips — a structural change
/// there would drop keyboard focus mid-toggle.
///
/// An unset endpoint renders as the bare space its input occupies — no
/// placeholder text (per the range-editing design sketch).
struct RangePill<MinInput: View, MaxInput: View>: View {
    let isRange: Bool
    @ViewBuilder var minInput: MinInput
    @ViewBuilder var maxInput: MaxInput

    var body: some View {
        HStack(spacing: GvSpacing.sm) {
            minInput
            if isRange {
                Text("–")
                    .font(.attrField)
                    .foregroundStyle(Color.entryTextSecondary)
                maxInput
            }
        }
        .gvAttributePill()
        .fixedSize(horizontal: true, vertical: false)
    }
}

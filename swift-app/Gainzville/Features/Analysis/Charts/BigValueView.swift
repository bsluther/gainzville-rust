import SwiftUI

/// A single headline number ("18 days", "47"). Not a plot — the value is the
/// whole message, so it renders large in text tokens with its label beneath.
struct BigValueView: View {
    let model: BigValueModel

    var body: some View {
        VStack(alignment: .leading, spacing: GvSpacing.sm) {
            ChartTitle(model.title)
            if let value = model.value {
                Text(model.format.string(from: value))
                    .font(.system(size: 44, weight: .semibold, design: .rounded))
                    .foregroundStyle(Color.gvTextBright)
            } else {
                // Empty result: the aggregation has no value (max over zero rows).
                Text("—")
                    .font(.system(size: 44, weight: .semibold, design: .rounded))
                    .foregroundStyle(Color.gvNeutral600)
            }
            Text(model.label)
                .font(.gvCaption)
                .foregroundStyle(Color.gvTextSecondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

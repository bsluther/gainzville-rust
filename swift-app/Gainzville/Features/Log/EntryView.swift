import SwiftUI

struct EntryView: View {
    let entry: FfiEntry
    @EnvironmentObject var forestVM: ForestViewModel
    @EnvironmentObject var activitiesVM: ActivitiesViewModel
    @State private var isExpanded = false

    private var displayName: String {
        if let activityId = entry.activityId,
           let act = activitiesVM.activities.first(where: { $0.id == activityId }) {
            return act.name
        }
        return "Entry"
    }

    var body: some View {
        VStack(spacing: 0) {
            EntryHeader(
                displayName: displayName,
                isSequence: entry.isSequence,
                isComplete: entry.isComplete,
                isExpanded: isExpanded,
                onToggle: { isExpanded.toggle() }
            )
            if isExpanded {
                EntryBody(
                    entry: entry,
                    children: forestVM.children(of: entry.id)
                )
            }
        }
        .entryContainerStyle(isSequence: entry.isSequence)
    }
}

// MARK: - Container styling

private extension View {
    func entryContainerStyle(isSequence: Bool) -> some View {
        let radius: CGFloat = isSequence ? 12 : 8
        return self
            .background(isSequence ? Color.clear : Color.gvSurface)
            .clipShape(RoundedRectangle(cornerRadius: radius))
            .overlay(
                RoundedRectangle(cornerRadius: radius)
                    .stroke(isSequence ? Color.gvDivider : Color.clear, lineWidth: 1)
            )
    }
}

// MARK: - Header

private struct EntryHeader: View {
    let displayName: String
    let isSequence: Bool
    let isComplete: Bool
    let isExpanded: Bool
    let onToggle: () -> Void

    var body: some View {
        HStack(spacing: 0) {
            // Left: tappable expand/collapse zone. Placed first so the right-side
            // control is a sibling button — tapping it won't fire this toggle.
            Button(action: onToggle) {
                Text(displayName)
                    .font(.gvBody)
                    .foregroundStyle(Color.gvTextPrimary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.vertical, GvSpacing.lg)
                    .padding(.leading, GvSpacing.lg)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)

            // Right: sequences always show menu. Scalars show checkbox when collapsed,
            // menu when expanded (checkbox moves to footer when open).
            if isSequence || isExpanded {
                Image(systemName: "ellipsis")
                    .rotationEffect(.degrees(90))
                    .foregroundStyle(Color.gvTextSecondary)
                    .padding(.horizontal, GvSpacing.lg)
                    .padding(.vertical, GvSpacing.lg)
            } else {
                FillCheckbox(checked: isComplete)
                    .padding(.horizontal, GvSpacing.lg)
                    .padding(.vertical, GvSpacing.lg)
            }
        }
    }
}

// MARK: - Body (shown when expanded)

private struct EntryBody: View {
    let entry: FfiEntry
    let children: [FfiEntry]

    var body: some View {
        VStack(alignment: .leading, spacing: GvSpacing.lg) {
            TemporalAttribute(temporal: entry.temporal)
            AttributesSection()
            if entry.isSequence {
                ChildrenSection(children: children)
            }
            EntryFooter(isSequence: entry.isSequence, isComplete: entry.isComplete)
        }
        .padding(.horizontal, GvSpacing.lg)
        .padding(.vertical, GvSpacing.lg)
    }
}

// MARK: - Children

private struct ChildrenSection: View {
    let children: [FfiEntry]

    var body: some View {
        if children.isEmpty {
            EmptyView()
        } else {
            VStack(spacing: GvSpacing.sm) {
                DropTarget()
                ForEach(children, id: \.id) { child in
                    EntryView(entry: child)
                    DropTarget()
                }
            }
        }
    }
}

// MARK: - Drop target (DnD placeholder)

/// Zero-height structural hook. Will become a visible drop indicator when
/// drag-and-drop is implemented.
private struct DropTarget: View {
    var body: some View {
        Color.clear.frame(height: 4)
    }
}

// MARK: - Footer

/// For scalars: checkbox on the right (replaces the header checkbox when expanded).
/// For sequences: placeholder row for future action buttons (add entry, etc.).
private struct EntryFooter: View {
    let isSequence: Bool
    let isComplete: Bool

    var body: some View {
        if isSequence {
            EmptyView()
        } else {
            HStack {
                Spacer()
                if !isSequence {
                    FillCheckbox(checked: isComplete)
                }
            }
        }
    }
}

// MARK: - Fill checkbox

private struct FillCheckbox: View {
    let checked: Bool
    var onToggle: () -> Void = {}

    var body: some View {
        Button(action: onToggle) {
            ZStack {
                RoundedRectangle(cornerRadius: 4)
                    .stroke(Color.gvLoggedBlue, lineWidth: 1.5)
                    .frame(width: 20, height: 20)
                if checked {
                    RoundedRectangle(cornerRadius: 2)
                        .fill(Color.gvLoggedBlue)
                        .frame(width: 12, height: 12)
                }
            }
        }
        .buttonStyle(.plain)
    }
}

// MARK: - Placeholder stubs

private struct AttributesSection: View {
    var body: some View { EmptyView() }
}

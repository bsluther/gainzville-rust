import SwiftUI
#if os(macOS)
import AppKit
#endif

extension View {
    /// Caps content to a readable column width on macOS, where the window can be
    /// very wide; full width on iOS. Shared by the log and the library detail
    /// views so both clamp identically. `alignment` positions the capped column
    /// within the window — `.top` (default) centers it, `.topLeading` pins it left.
    func gvReadableWidth(alignment: Alignment = .top) -> some View {
        #if os(macOS)
        self
            .frame(maxWidth: GvSpacing.contentWidthMax)
            .frame(maxWidth: .infinity, alignment: alignment)
        #else
        self
            .frame(maxWidth: .infinity)
        #endif
    }

    /// iOS sheet chrome: a gvBackground fill with a hairline border drawn over
    /// it. A plain opaque fill (content `.background` or `presentationBackground(Color)`)
    /// hides the sheet's native lighter rim, so we draw our own border, matching
    /// the entry-menu sheet (see EntryView). No-op on macOS (popover has its own
    /// border).
    func gvSheetChrome(cornerRadius: CGFloat = 36) -> some View {
        #if os(iOS)
        self
            // Border drawn ON TOP of content so an opaque header band can't
            // occlude it — drawing it behind (in presentationBackground) made it
            // show only around the picker, where the content is transparent.
            .overlay(
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .strokeBorder(.white.opacity(0.12), lineWidth: 0.5)
                    .allowsHitTesting(false)
            )
            .presentationCornerRadius(cornerRadius)
            .presentationBackground(Color.gvBackground)
        #else
        self
        #endif
    }

    /// Presents content as a sheet on iOS and a popover on macOS.
    /// Content may apply `.presentationDetents` — it's a no-op on macOS.
    func platformPopover<Content: View>(
        isPresented: Binding<Bool>,
        @ViewBuilder content: @escaping () -> Content
    ) -> some View {
        #if os(iOS)
        sheet(isPresented: isPresented, content: content)
        #else
        popover(isPresented: isPresented) {
            content()
                .onAppear {
                    // AppKit-backed controls grab first responder on appear,
                    // causing a focus ring. Resign it immediately.
                    DispatchQueue.main.async {
                        NSApp.keyWindow?.makeFirstResponder(nil)
                    }
                }
        }
        #endif
    }
}

extension View {
    /// Scales the view's rendering by `scale` *and* reserves the scaled size in
    /// layout — unlike `.scaleEffect` alone, which keeps the original footprint
    /// and overflows (clips) inside a size-to-fit container. Use to enlarge a
    /// fixed-size control (e.g. the graphical NSDatePicker, which ignores `font`
    /// and `controlSize`) inside a popover without clipping.
    func gvScaled(_ scale: CGFloat) -> some View {
        ScaledLayout(scale: scale) {
            scaleEffect(scale)
        }
    }
}

/// Reports `scale`× its child's ideal size to the parent while placing the child
/// at its natural size — pair with `.scaleEffect(scale)` on that child so the
/// scaled rendering fills the reserved space. See `View.gvScaled`.
private struct ScaledLayout: Layout {
    var scale: CGFloat

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let size = subviews.first?.sizeThatFits(.unspecified) ?? .zero
        return CGSize(width: size.width * scale, height: size.height * scale)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        subviews.first?.place(at: CGPoint(x: bounds.midX, y: bounds.midY), anchor: .center, proposal: .unspecified)
    }
}

#if os(macOS)
/// macOS calendar/clock picker backed by NSDatePicker with a transparent
/// background.
///
/// SwiftUI's graphical DatePicker draws its own rounded rect, so using it
/// inside any container (popover, sheet, custom panel) produces a double-box.
/// `CalendarPickerNSView` sets `backgroundColor = .clear` and `isBezeled = false`
/// on the underlying NSDatePicker to avoid that.
///
/// The graphical NSDatePicker is a fixed-size control (it ignores `font` and
/// `controlSize` for grid sizing), so we enlarge it with `gvScaled`, which
/// scales the rendering and reserves the scaled space in layout.
struct CalendarPickerMacOS: View {
    @Binding var selection: Date
    var components: DatePickerComponents = .date
    var scale: CGFloat = 1.3

    var body: some View {
        CalendarPickerNSView(selection: $selection, components: components)
            .gvScaled(scale)
    }
}

private struct CalendarPickerNSView: NSViewRepresentable {
    @Binding var selection: Date
    var components: DatePickerComponents

    func makeNSView(context: Context) -> NSDatePicker {
        let picker = NSDatePicker()
        picker.datePickerStyle = .clockAndCalendar
        picker.datePickerElements = nsElements
        picker.backgroundColor = .clear
        picker.isBezeled = false
        picker.dateValue = selection
        picker.target = context.coordinator
        picker.action = #selector(Coordinator.dateChanged(_:))
        return picker
    }

    func updateNSView(_ picker: NSDatePicker, context: Context) {
        if picker.dateValue != selection { picker.dateValue = selection }
        picker.datePickerElements = nsElements
    }

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    private var nsElements: NSDatePicker.ElementFlags {
        components == .hourAndMinute ? .hourMinuteSecond : .yearMonthDay
    }

    class Coordinator: NSObject {
        var parent: CalendarPickerNSView
        init(_ parent: CalendarPickerNSView) { self.parent = parent }
        @objc func dateChanged(_ sender: NSDatePicker) { parent.selection = sender.dateValue }
    }
}
#endif

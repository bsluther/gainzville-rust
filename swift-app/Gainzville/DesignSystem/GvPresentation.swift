import SwiftUI
#if os(macOS)
import AppKit
#endif

extension View {
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

#if os(macOS)
/// macOS calendar picker backed by NSDatePicker with a transparent background.
///
/// SwiftUI's graphical DatePicker draws its own rounded rect, so using it
/// inside any container (popover, sheet, custom panel) produces a double-box.
/// This representable sets `backgroundColor = .clear` and `isBezeled = false`
/// on the underlying NSDatePicker to avoid that.
struct CalendarPickerMacOS: NSViewRepresentable {
    @Binding var selection: Date
    var components: DatePickerComponents = .date

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
        var parent: CalendarPickerMacOS
        init(_ parent: CalendarPickerMacOS) { self.parent = parent }
        @objc func dateChanged(_ sender: NSDatePicker) { parent.selection = sender.dateValue }
    }
}
#endif

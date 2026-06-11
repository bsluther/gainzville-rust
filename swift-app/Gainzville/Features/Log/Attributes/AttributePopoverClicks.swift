#if os(macOS)
import AppKit

// Transient popovers (the macOS attribute action bars) consume the mouse-down
// that dismisses them: the event enters the app but never reaches the view
// hierarchy, so a click on a text field while a popover is open closes the
// popover and silently drops the click. This helper remembers the app's last
// mouse-down — a local monitor sees every event before the popover's own
// monitor reacts — and lets dismissal handlers ask, via AppKit's own
// hit-testing, whether that consumed click landed on a text-input view. The
// handler can then complete the click's intent (focus the field) instead of
// tearing the editing session down; which editor's popover ends up open is
// resolved downstream by the normal focus handover.
@MainActor
enum AttributePopoverClicks {
    private static var monitor: Any?
    private static var lastMouseDown: (event: NSEvent, at: Date)?

    /// Idempotent; call from any view that hosts a dismissal handler.
    static func install() {
        guard monitor == nil else { return }
        monitor = NSEvent.addLocalMonitorForEvents(matching: .leftMouseDown) { event in
            lastMouseDown = (event, Date())
            return event
        }
    }

    /// The text-input view the just-consumed click landed on, or nil when the
    /// dismissal wasn't caused by a recent click on one (plain click-away,
    /// Esc, programmatic close). The focused field's clicks hit the window's
    /// field editor (an NSTextView whose delegate is the field), so both
    /// classes count as text input.
    static func consumedTextFieldHit() -> NSView? {
        guard let (event, at) = lastMouseDown,
              Date().timeIntervalSince(at) < 0.25,
              let window = event.window,
              let content = window.contentView else { return nil }
        // hitTest expects the point in the receiver's superview's coordinates;
        // locationInWindow is in window base coordinates (`from: nil`).
        let point = content.superview?.convert(event.locationInWindow, from: nil)
            ?? event.locationInWindow
        var view = content.hitTest(point)
        while let v = view {
            if v is NSTextField || v is NSTextView { return v }
            view = v.superview
        }
        return nil
    }

    /// Whether `view` already has keyboard focus — directly, or through the
    /// window's shared field editor (first responder is an NSTextView whose
    /// delegate is the field).
    static func isFirstResponder(_ view: NSView) -> Bool {
        guard let fr = view.window?.firstResponder else { return false }
        if fr === view { return true }
        if let editor = fr as? NSTextView, (editor.delegate as? NSView) === view { return true }
        return false
    }
}
#endif

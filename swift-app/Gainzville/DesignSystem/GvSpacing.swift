import CoreFoundation

/// GV spacing scale — a 4pt base grid mirroring the Dioxus design system.
///
/// Dioxus equivalents (1rem = 16px, 1 CSS px ≈ 1 SwiftUI pt):
///   sm = --spacing-sm (0.25rem = 4px)
///   md = --spacing-md (0.50rem = 8px)
///   lg = --spacing-lg (0.75rem = 12px)
///   xl = --spacing-xl (1.50rem = 24px)
enum GvSpacing {
    static let sm: CGFloat =  4
    static let md: CGFloat =  8
    static let lg: CGFloat = 12
    static let xl: CGFloat = 24

    /// Minimum height for attribute pill controls (date, time, duration pickers).
    /// Taller content can overflow; all pills share this baseline.
    static let minAttributeHeight: CGFloat = 32
}

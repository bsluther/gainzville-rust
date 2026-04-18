import SwiftUI

// MARK: - Raw neutral scale

/// The full 18-step GV neutral scale, mirroring the Dioxus design system.
/// Dark end (1200) is the darkest; light end (50) is the lightest.
/// Use semantic tokens (below) in views — reach for raw values only when
/// building new semantic tokens or one-off data visualisation work.
extension Color {
    // Neutral scale
    static let gvNeutral50   = Color(rgb: 250, 250, 250)
    static let gvNeutral100  = Color(rgb: 245, 245, 245)
    static let gvNeutral200  = Color(rgb: 229, 229, 229)
    static let gvNeutral300  = Color(rgb: 212, 212, 212)
    static let gvNeutral400  = Color(rgb: 163, 163, 163)
    static let gvNeutral500  = Color(rgb: 115, 115, 115)
    static let gvNeutral550  = Color(rgb:  96,  96,  96)
    static let gvNeutral600  = Color(rgb:  82,  82,  82)
    static let gvNeutral650  = Color(rgb:  72,  72,  72)
    static let gvNeutral700  = Color(rgb:  64,  64,  64)
    static let gvNeutral750  = Color(rgb:  56,  55,  55)
    static let gvNeutral800  = Color(rgb:  38,  38,  38)
    static let gvNeutral850  = Color(rgb:  30,  30,  32)
    static let gvNeutral900  = Color(rgb:  23,  23,  23)
    static let gvNeutral950  = Color(rgb:  19,  19,  19)
    static let gvNeutral1000 = Color(rgb:  17,  17,  17)
    static let gvNeutral1100 = Color(rgb:  13,  13,  13)
    static let gvNeutral1200 = Color(rgb:  10,  10,  10)

    // Accent
    static let gvLoggedBlue  = Color(rgb: 110, 121, 144)
}

// MARK: - Semantic adaptive tokens
//
// Light/dark adaptive semantic colors are defined as named color sets in
// Assets.xcassets and auto-generated into Color extensions by Xcode
// (ASSETCATALOG_COMPILER_GENERATE_SWIFT_ASSET_SYMBOL_EXTENSIONS = YES).
// Use them directly — no manual declaration needed here.
//
// Available tokens and their light → dark mapping:
//   Color.gvBackground    neutral50  (250,250,250) → neutral1100 (13,13,13)
//   Color.gvSurface       neutral100 (245,245,245) → neutral950  (19,19,19)
//   Color.gvDivider       neutral200 (229,229,229) → neutral800  (38,38,38)
//   Color.gvTextPrimary   neutral1100 (13,13,13)   → neutral50  (250,250,250)
//   Color.gvTextSecondary neutral600  (82,82,82)   → neutral400 (163,163,163)

// MARK: - Semantic aliases

extension Color {
    /// Text color for attribute field values (dates, times, durations, quantities).
    /// Defined here rather than Assets.xcassets so it stays with its semantic peers.
    static var gvAttributeField: Color { .gvTextSecondary }
}

// MARK: - Private initialiser

private extension Color {
    /// Convenience init from 8-bit sRGB components (0–255).
    init(rgb r: Int, _ g: Int, _ b: Int) {
        self.init(
            red:   Double(r) / 255,
            green: Double(g) / 255,
            blue:  Double(b) / 255
        )
    }
}

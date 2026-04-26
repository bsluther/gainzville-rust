import SwiftUI

// MARK: - Raw neutral scale
//
// The 18-step GV neutral scale is defined as adaptive color sets in
// Assets.xcassets and auto-generated into Color extensions by Xcode
// (ASSETCATALOG_COMPILER_GENERATE_SWIFT_ASSET_SYMBOL_EXTENSIONS = YES).
// Light end (50) is the lightest; dark end (1200) is the darkest.
// Use semantic aliases (below) in views.
//
// Available: Color.gvNeutral50 … gvNeutral1200, Color.gvLoggedBlue

// MARK: - Semantic adaptive tokens
//
// Adaptive semantic colors are defined in Assets.xcassets and auto-generated.
// Available: Color.gvBackground, .gvSurface, .gvDivider, .gvTextPrimary, .gvTextSecondary

// MARK: - Semantic aliases

extension Color {
    /// Text color for attribute field values (dates, times, durations, quantities).
    /// Defined here rather than Assets.xcassets so it stays with its semantic peers.
    static var gvAttributeField: Color { .gvTextSecondary }
}

// MARK: - Entry tokens

extension Color {
    static var entryScalarBackground: Color   { .gvSurface }
    static var entrySequenceBackground: Color { .clear }
    static var entryScalarBorder: Color       { .clear }
    static var entrySequenceBorder: Color     { .gvDivider }
    static var entryTitle: Color              { .gvTextPrimary }
    static var attrLabel: Color               { .gvTextSecondary }
}


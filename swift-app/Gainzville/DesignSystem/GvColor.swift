import SwiftUI
#if canImport(UIKit)
import UIKit
#elseif canImport(AppKit)
import AppKit
#endif

// MARK: - Raw neutral scale
//
// The 18-step GV neutral scale is defined as adaptive color sets in
// Assets.xcassets and auto-generated into Color extensions by Xcode
// (ASSETCATALOG_COMPILER_GENERATE_SWIFT_ASSET_SYMBOL_EXTENSIONS = YES).
// Light end (50) is the lightest; dark end (1200) is the darkest.
// Use semantic aliases (below) in views.
//
// Available: Color.gvNeutral50 … gvNeutral1200, Color.gvLoggedBlue

// MARK: - Semantic aliases

extension Color {
    static var gvBackground: Color    { .gvNeutral1100 }
    static var gvSurface: Color       { .gvNeutral1000 }
    static var gvDivider: Color       { .gvNeutral200 }
    static var gvTextPrimary: Color   { .gvNeutral400 }
    static var gvTextSecondary: Color { .gvNeutral500 }

    // The platform's app/window background. Used as the at-rest fill for
    // surfaces that need to match what the OS paints behind the app — e.g.
    // sequence entries, whose container is otherwise clear and would let
    // the LogView's drop-target tint bleed through.
    static var gvAppBackground: Color {
        #if canImport(UIKit)
        Color(UIColor.systemBackground)
        #elseif canImport(AppKit)
        Color(NSColor.windowBackgroundColor)
        #else
        .gvBackground
        #endif
    }
}

// MARK: - Action tokens

extension Color {
    static var gvPrimaryAction: Color { .gvLoggedBlue }
}

// MARK: - Entry tokens

extension Color {
    static var entryScalarBackground: Color   { .gvNeutral950 }
    static var entrySequenceBackground: Color { .gvAppBackground }
    static var entryScalarBorder: Color       { .gvNeutral900 }
    static var entrySequenceBorder: Color     { .gvNeutral800 }
    static var entryTextPrimary: Color            { .gvNeutral400 }
    static var entryTextSecondary: Color          { .gvNeutral500 }
}


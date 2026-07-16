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

// MARK: - Semantic aliases

extension Color {
    static var gvBackground: Color    { .gvNeutral1100 }
    static var gvSurface: Color       { .gvNeutral1000 }
    static var gvDivider: Color       { .gvNeutral200 }
    static var gvTextBright: Color    { .gvNeutral300 }
    static var gvTextPrimary: Color   { .gvNeutral400 }
    static var gvTextSecondary: Color { .gvNeutral500 }

    static var gvAppBackground: Color { .gvBackground }
}

// MARK: - Action tokens

extension Color {
    static var gvPrimaryAction: Color { .gvLoggedBlue }
}

// MARK: - Entry tokens

extension Color {
    static var entryScalarBackground: Color   { .gvNeutral900 }
    static var entrySequenceBackground: Color { .gvAppBackground }
    static var entryScalarBorder: Color       { .gvNeutral850 }
    static var entrySequenceBorder: Color     { .gvNeutral800 }
    static var entryTextPrimary: Color            { .gvNeutral350 }
    static var entryTextSecondary: Color          { .gvNeutral500 }
}

// MARK: - Chart tokens
//
// Categorical series palette for analysis charts. The slot ORDER is load-bearing:
// it was chosen to maximize adjacent-pair colorblind separation and validated
// against gvSurface (#111111) — reorder and the guarantee is gone. Series are
// assigned slots in fixed order, never cycled; a 9th series folds into "Other".
// (See docs/analysis/chart-model.md, decision D3.)

extension Color {
    static let gvChartSeries: [Color] = [
        .gvAzure500, .gvTeal500, .gvAmber500, .gvGreen600,
        .gvViolet400, .gvRed400, .gvMagenta500, .gvOrange500,
    ]

    /// Palette lookup for a model's `colorSlot`. `render_model` guarantees slots
    /// stay in range; the clamp is a belt for hand-written fixtures.
    static func gvChartSeries(slot: Int) -> Color {
        gvChartSeries[max(0, min(slot, gvChartSeries.count - 1))]
    }

    /// The pie "Other" bundle — deliberately neutral so it never reads as a series.
    static var gvChartOther: Color { .gvNeutral600 }

    static var gvChartGrid: Color      { .gvNeutral900 }
    static var gvChartAxisLabel: Color { .gvTextSecondary }
}


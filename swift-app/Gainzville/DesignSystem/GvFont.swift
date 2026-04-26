import SwiftUI

/// GV typography tokens — system font aliases.
///
/// These are placeholders using Dynamic Type styles. Replace with custom
/// typeface decisions when the typography system is defined.
extension Font {
    static let gvLargeTitle = Font.system(.largeTitle)
    static let gvTitle      = Font.system(.title2)
    static let gvHeadline   = Font.system(.headline)
    static let gvBody       = Font.system(.body)
    static let gvCallout    = Font.system(.callout)
    static let gvCaption    = Font.system(.caption)
    static let gvFootnote   = Font.system(.footnote)
}

// MARK: - Entry / attribute tokens

extension Font {
    static let attrLabel = Font(UIFontMetrics(forTextStyle: .callout).scaledFont(for: .systemFont(ofSize: 14)))
    static let attrField = Font.system(.body)
}

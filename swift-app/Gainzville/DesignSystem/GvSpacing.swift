import CoreFoundation

enum GvSpacing {
    static let sm: CGFloat =  4
    static let md: CGFloat =  8
    static let lg: CGFloat = 12
    static let xl: CGFloat = 24

    // MARK: - Entry tokens
    static let entrySpacing: CGFloat              = lg  // inner padding & VStack gap
    static let entryScalarBorderWidth: CGFloat    =  1
    static let entrySequenceBorderWidth: CGFloat  = 1.5
    static let entryCornerRadius: CGFloat         = 10
    static let entryScalarCornerRadius: CGFloat   =  8
    static let entrySequenceCornerRadius: CGFloat = 12
    
    // MARK: - Attribute tokens
    static let minAttributeHeight: CGFloat = 32

    // MARK: - FAB tokens
    static let fabSize: CGFloat    = 56
    static let fabPadding: CGFloat = xl
}

import Foundation

// MARK: - ChartModel (docs/analysis/chart-model.md)
//
// The fully-resolved, per-kind input a chart component consumes. In the final
// architecture these types are produced Rust-side by `render_model(ResultTable,
// result_schema, ChartSpec)` and cross FFI as uniffi-generated mirrors; these
// hand-written versions exist so the components can be prototyped against
// fixtures first. Key conventions they encode (the "totality" rules):
//
//  - A model is always completely renderable: defaults are merged upstream, so
//    components never see partial or invalid input. Empty data ⇒ empty series,
//    and the component renders an empty state.
//  - Discrete axes carry the FULL category domain (empty categories included)
//    with series values as dense arrays aligned by index — no string matching.
//  - Colors are palette SLOTS (indices into Color.gvChartSeries), resolved by
//    the theme; models never contain colors.
//  - Numbers are raw Doubles plus a ChartFormat descriptor; Swift renders them
//    locale-correctly. Unit conversion already happened (the ChartSpec picks
//    the display unit; core converts).

// MARK: Shared vocabulary

/// How a raw value renders. `percent` expects a fraction (0.42 → "42%").
/// `quantity` values arrive already converted to the display unit.
enum ChartFormat {
    case count
    case number(precision: Int)
    case percent(precision: Int)
    case quantity(unit: String, precision: Int)

    func string(from value: Double) -> String {
        switch self {
        case .count:
            return value.formatted(.number.precision(.fractionLength(0)))
        case .number(let precision):
            return value.formatted(.number.precision(.fractionLength(precision)))
        case .percent(let precision):
            return value.formatted(.percent.precision(.fractionLength(precision)))
        case .quantity(let unit, let precision):
            return "\(value.formatted(.number.precision(.fractionLength(precision)))) \(unit)"
        }
    }
}

/// The time bucket a line chart's x-axis was grouped by; drives tick formatting
/// and the horizontal-scroll window.
enum TimeGrain {
    case day, week, month, year

    var axisFormat: Date.FormatStyle {
        switch self {
        case .day, .week: return .dateTime.month(.abbreviated).day()
        case .month:      return .dateTime.month(.abbreviated)
        case .year:       return .dateTime.year()
        }
    }

    /// How much time fits on screen at once. Data spanning more than this
    /// scrolls horizontally (display heuristic — a ChartSpec override can
    /// arrive later as an additive field). Year grain never scrolls.
    var visibleWindow: TimeInterval? {
        let day = 86_400.0
        switch self {
        case .day:   return 90 * day
        case .week:  return 26 * 7 * day
        case .month: return 730 * day    // ~24 months
        case .year:  return nil
        }
    }
}

/// One slot of a discrete axis's full domain — present even when it has no data,
/// so empty categories render as empty slots instead of vanishing.
struct CategorySlot {
    let label: String
}

/// Series identity: display label + palette slot (index into Color.gvChartSeries).
struct SeriesMeta {
    let label: String
    let colorSlot: Int
}

// MARK: Big value

/// `value` is optional because an empty result genuinely has no value for
/// non-count aggregations (max over zero rows) — substituting 0 would lie.
/// nil renders as the designed empty state ("—").
struct BigValueModel {
    let title: String?
    let label: String        // what the number is: "longest gap between training days"
    let value: Double?
    let format: ChartFormat
}

// MARK: Bar

/// One kind covers plain, stacked, and grouped bars: a plain bar is just
/// `series.count == 1` (legend hidden, `stacking` ignored). Decision D5.
struct BarModel {
    let title: String?
    let xLabel: String?
    let domain: [CategorySlot]      // full domain, display order, incl. empties
    let yLabel: String?
    let yFormat: ChartFormat        // bars are always zero-based
    let series: [BarSeries]
    let stacking: BarStacking
}

/// `values` is dense and aligned to `BarModel.domain` by index.
/// `nil` = no data (distinct from a measured 0, which is `0.0`).
struct BarSeries {
    let meta: SeriesMeta
    let values: [Double?]
}

enum BarStacking {
    case stacked, grouped
}

// MARK: Line

/// Series are unified regardless of origin — "one measure split by a dimension"
/// and "several measures as lines" both arrive as labeled series. One y-axis:
/// all series share a compatible format (no dual axis, by design).
struct LineModel {
    let title: String?
    let grain: TimeGrain
    let yLabel: String?
    let yFormat: ChartFormat
    let yIncludesZero: Bool         // resolved upstream: counts true, bodyweight false
    let series: [LineSeries]
}

/// Points are sparse with real timestamps — gaps stay visible on the time scale.
/// (Epoch-ms i64 at the FFI boundary; `Date` here.)
struct LineSeries {
    let meta: SeriesMeta
    let step: Bool                  // running-max ("PR so far") renders as a step line
    let points: [TimePoint]
}

struct TimePoint {
    let instant: Date
    let value: Double
}

// MARK: Pie

/// Parts-of-whole. Slices arrive value-descending; the slice cap (a ChartSpec
/// presentation option) was applied in render_model, bundling the remainder
/// into `other`. Zero-value/empty categories are omitted (unlike bars).
struct PieModel {
    let title: String?
    let slices: [PieSlice]
    let other: OtherSlice?
    let format: ChartFormat
    let isDonut: Bool
}

struct PieSlice {
    let label: String
    let value: Double
    let colorSlot: Int
}

/// Renders last regardless of value, in the neutral gvChartOther color.
struct OtherSlice {
    let value: Double
    let bundledCount: Int
}

// MARK: Table

/// Deliberately minimal: a thin projection of the ResultTable, legal for every
/// result shape — the universal "what am I actually charting?" fallback.
struct TableModel {
    let title: String?
    let columns: [TableColumn]
    let rows: [[TableCell]]         // query's order; the view scrolls, no cap
}

struct TableColumn {
    let label: String
    let format: ChartFormat
}

enum TableCell {
    case number(Double)
    case text(String)
    case instant(Date)
    case empty
}

// MARK: Empty detection
//
// An empty *result* (zero rows matched) still produces a total, renderable
// model — full domain, series present, no data in them. Components branch on
// these to render the designed empty state instead of a blank plot area.

extension BarModel {
    var isEmpty: Bool {
        series.isEmpty || series.allSatisfy { s in s.values.allSatisfy { $0 == nil } }
    }
}

extension LineModel {
    var isEmpty: Bool { series.allSatisfy { $0.points.isEmpty } }

    /// Earliest and latest instants across all series (nil when empty).
    var instantRange: ClosedRange<Date>? {
        let instants = series.flatMap { $0.points.map(\.instant) }
        guard let min = instants.min(), let max = instants.max() else { return nil }
        return min...max
    }
}

extension PieModel {
    var isEmpty: Bool { slices.isEmpty && other == nil }
}

import Foundation
internal import Combine

/// A UTC interval representing a single local calendar day (local midnight to local midnight).
///
/// Using an explicit interval rather than a bare `Date` avoids timezone bugs at midnight
/// boundaries — the FFI takes Unix-millisecond `from`/`to` values, not calendar dates.
struct LogDay: Hashable, Equatable, Codable {
    let start: Date   // local midnight, expressed as UTC
    let end: Date     // next local midnight

    var fromMs: Int64 { Int64(start.timeIntervalSince1970 * 1000) }
    var toMs:   Int64 { Int64(end.timeIntervalSince1970   * 1000) }

    var isToday: Bool { Calendar.current.isDateInToday(start) }

    static func forLocalDate(_ date: Date, calendar: Calendar = .current) -> LogDay {
        let s = calendar.startOfDay(for: date)
        return LogDay(start: s, end: calendar.date(byAdding: .day, value: 1, to: s)!)
    }

    static var today: LogDay { .forLocalDate(.now) }

    func next(calendar: Calendar = .current) -> LogDay {
        .forLocalDate(calendar.date(byAdding: .day, value: 1, to: start)!, calendar: calendar)
    }

    func previous(calendar: Calendar = .current) -> LogDay {
        .forLocalDate(calendar.date(byAdding: .day, value: -1, to: start)!, calendar: calendar)
    }
}

/// Source of truth for the currently-viewed log day across the app.
///
/// Injected into the SwiftUI environment at the app level so it's accessible from any
/// view — not just within the Log tab. Deep linking can update `logDay` from outside
/// the Log route without pushing a navigation destination.
@MainActor
class LogDayStore: ObservableObject {
    @Published var logDay: LogDay = .today
}

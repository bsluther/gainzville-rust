//! Parsing of the day-document's human temporal fields — local wall-clock
//! times ("3:03pm", "15:03") and durations ("40s", "25m", "1h30m") — into
//! core [`Temporal`]s. The document date plus the workspace timezone anchor
//! wall-clock times to UTC; dates never come from the model.

use chrono::{DateTime, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use gv_core::models::entry::Temporal;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TemporalError {
    #[error("unparseable time '{0}' (expected forms like '3:03pm', '7am', '15:03')")]
    BadTime(String),
    #[error("unparseable duration '{0}' (expected forms like '40s', '25m', '1h30m')")]
    BadDuration(String),
    #[error("ambiguous or nonexistent local time '{0}' in this timezone (DST transition)")]
    AmbiguousLocal(String),
    #[error("invalid temporal combination: {0}")]
    BadCombination(&'static str),
}

/// Parse a wall-clock time-of-day. Accepted: "3:03pm", "3pm", "12:30 am",
/// "15:03", "07:50". Case-insensitive, optional space before am/pm.
pub fn parse_time_of_day(s: &str) -> Result<NaiveTime, TemporalError> {
    let raw = s.trim().to_lowercase().replace(' ', "");
    let bad = || TemporalError::BadTime(s.to_string());

    let (body, meridiem) = if let Some(b) = raw.strip_suffix("am") {
        (b, Some(false))
    } else if let Some(b) = raw.strip_suffix("pm") {
        (b, Some(true))
    } else {
        (raw.as_str(), None)
    };

    let (h, m) = match body.split_once(':') {
        Some((h, m)) => (
            h.parse::<u32>().map_err(|_| bad())?,
            m.parse::<u32>().map_err(|_| bad())?,
        ),
        // A bare number with no meridiem ("7") is ambiguous — reject it, like
        // the duration parser rejects unitless numbers.
        None if meridiem.is_none() => return Err(bad()),
        None => (body.parse::<u32>().map_err(|_| bad())?, 0),
    };

    let h = match meridiem {
        None => h,
        Some(_) if !(1..=12).contains(&h) => return Err(bad()),
        Some(false) => {
            if h == 12 {
                0
            } else {
                h
            }
        }
        Some(true) => {
            if h == 12 {
                12
            } else {
                h + 12
            }
        }
    };

    NaiveTime::from_hms_opt(h, m, 0).ok_or_else(bad)
}

/// Anchor a local time-of-day on `date` in `tz`, yielding UTC.
pub fn local_to_utc(date: NaiveDate, time: NaiveTime, tz: Tz) -> Result<DateTime<Utc>, TemporalError> {
    tz.from_local_datetime(&date.and_time(time))
        .single()
        .map(|dt| dt.with_timezone(&Utc))
        .ok_or_else(|| TemporalError::AmbiguousLocal(format!("{date} {time}")))
}

/// Parse a duration string into milliseconds. Accepted: "40s", "25m",
/// "25 min", "1h", "1h30m", "2h15m30s". Bare numbers are rejected — the unit
/// must be explicit so "20" can't silently mean the wrong thing.
pub fn parse_duration_ms(s: &str) -> Result<u32, TemporalError> {
    let raw = s.trim().to_lowercase().replace(' ', "");
    let bad = || TemporalError::BadDuration(s.to_string());

    let mut total_ms: f64 = 0.0;
    let mut num = String::new();
    let mut unit = String::new();
    let mut saw_component = false;

    // Tokenize into number/unit pairs ("1h30m" → (1,h),(30,m)).
    let mut chars = raw.chars().peekable();
    while chars.peek().is_some() {
        num.clear();
        unit.clear();
        while chars.peek().is_some_and(|c| c.is_ascii_digit() || *c == '.') {
            num.push(chars.next().unwrap());
        }
        while chars.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            unit.push(chars.next().unwrap());
        }
        if num.is_empty() || unit.is_empty() {
            return Err(bad());
        }
        let n: f64 = num.parse().map_err(|_| bad())?;
        let per_unit_ms: f64 = match unit.as_str() {
            "h" | "hr" | "hrs" | "hour" | "hours" => 3_600_000.0,
            "m" | "min" | "mins" | "minute" | "minutes" => 60_000.0,
            "s" | "sec" | "secs" | "second" | "seconds" => 1_000.0,
            _ => return Err(bad()),
        };
        total_ms += n * per_unit_ms;
        saw_component = true;
    }

    if !saw_component || !total_ms.is_finite() || total_ms < 0.0 || total_ms > u32::MAX as f64 {
        return Err(bad());
    }
    Ok(total_ms as u32)
}

/// Build a core [`Temporal`] from the document's optional start/end/duration
/// strings, anchored on `date` in `tz`. An end at or before the start is
/// taken to cross midnight and rolls to the next day ("11:30pm" – "1am").
pub fn build_temporal(
    date: NaiveDate,
    tz: Tz,
    start: Option<&str>,
    end: Option<&str>,
    duration: Option<&str>,
) -> Result<Temporal, TemporalError> {
    let start_utc = start
        .map(|s| parse_time_of_day(s).and_then(|t| local_to_utc(date, t, tz)))
        .transpose()?;
    let end_tod = end.map(parse_time_of_day).transpose()?;
    let mut end_utc = end_tod.map(|t| local_to_utc(date, t, tz)).transpose()?;
    if let (Some(s), Some(e), Some(tod)) = (start_utc, end_utc, end_tod)
        && e < s
    {
        // Crossed midnight: re-anchor the wall-clock end on the next LOCAL
        // day (not +24h in UTC, which is wrong across DST transitions).
        // start == end stays instantaneous rather than becoming a 24h entry.
        let next_day = date
            .succ_opt()
            .ok_or_else(|| TemporalError::AmbiguousLocal(format!("{date} +1 day")))?;
        end_utc = Some(local_to_utc(next_day, tod, tz)?);
    }
    let duration_ms = duration.map(parse_duration_ms).transpose()?;

    // Over-determined start+end+duration: drop the duration, it's derivable.
    let duration_ms = if start_utc.is_some() && end_utc.is_some() {
        None
    } else {
        duration_ms
    };

    Temporal::parse(start_utc, end_utc, duration_ms)
        .map_err(|_| TemporalError::BadCombination("start/end/duration"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_parse() {
        assert_eq!(
            parse_time_of_day("3:03pm").unwrap(),
            NaiveTime::from_hms_opt(15, 3, 0).unwrap()
        );
        assert_eq!(
            parse_time_of_day("7am").unwrap(),
            NaiveTime::from_hms_opt(7, 0, 0).unwrap()
        );
        assert_eq!(
            parse_time_of_day("12:15 AM").unwrap(),
            NaiveTime::from_hms_opt(0, 15, 0).unwrap()
        );
        assert_eq!(
            parse_time_of_day("15:03").unwrap(),
            NaiveTime::from_hms_opt(15, 3, 0).unwrap()
        );
        assert!(parse_time_of_day("25:00").is_err());
        assert!(parse_time_of_day("13pm").is_err());
        // Bare numbers without a meridiem are ambiguous.
        assert!(parse_time_of_day("7").is_err());
    }

    #[test]
    fn durations_parse() {
        assert_eq!(parse_duration_ms("40s").unwrap(), 40_000);
        assert_eq!(parse_duration_ms("25 min").unwrap(), 1_500_000);
        assert_eq!(parse_duration_ms("1h30m").unwrap(), 5_400_000);
        assert!(parse_duration_ms("90").is_err());
        assert!(parse_duration_ms("").is_err());
        // Absurd magnitudes error instead of overflowing.
        assert!(parse_duration_ms("99999999999999999999h1s").is_err());
    }

    #[test]
    fn equal_start_and_end_is_instantaneous_not_a_day() {
        let date = NaiveDate::from_ymd_opt(2026, 2, 24).unwrap();
        let t = build_temporal(date, chrono_tz::America::Denver, Some("7:31am"), Some("7:31am"), None)
            .unwrap();
        assert_eq!(t.start(), t.end());
    }

    #[test]
    fn midnight_roll_reanchors_across_spring_forward() {
        // Denver springs forward 2026-03-08 02:00 -> 03:00. Start 11:30pm on
        // the 7th, end 3am "the next day" = 3am MDT: 150 elapsed minutes, not
        // the 210 a naive +24h would produce.
        let date = NaiveDate::from_ymd_opt(2026, 3, 7).unwrap();
        let t = build_temporal(date, chrono_tz::America::Denver, Some("11:30pm"), Some("3am"), None)
            .unwrap();
        let (s, e) = (t.start().unwrap(), t.end().unwrap());
        assert_eq!((e - s).num_minutes(), 150);
    }

    #[test]
    fn midnight_spanning_end_rolls_forward() {
        let date = NaiveDate::from_ymd_opt(2026, 2, 24).unwrap();
        let t = build_temporal(date, chrono_tz::America::Denver, Some("11:30pm"), Some("1am"), None)
            .unwrap();
        let (s, e) = (t.start().unwrap(), t.end().unwrap());
        assert!(e > s);
        assert_eq!((e - s).num_minutes(), 90);
    }
}

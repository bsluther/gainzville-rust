use crate::components::calendar::{
    Calendar, CalendarGrid, CalendarHeader, CalendarMonthTitle, CalendarNavigation,
    CalendarNextMonthButton, CalendarPreviousMonthButton, CalendarView,
};
use crate::components::PlatformPopover;
use chrono::{Datelike, Duration, NaiveDate};
use dioxus::prelude::*;
use dioxus_free_icons::icons::io_icons::{IoCaretBackOutline, IoCaretForwardOutline};
use dioxus_free_icons::Icon;
use dioxus_primitives::ContentAlign;
use time::Date;

fn naive_to_time(date: NaiveDate) -> time::Date {
    time::Date::from_calendar_date(
        date.year(),
        time::Month::try_from(date.month() as u8).unwrap(),
        date.day() as u8,
    )
    .unwrap()
}

fn time_to_naive(date: time::Date) -> NaiveDate {
    NaiveDate::from_ymd_opt(date.year(), date.month() as u32, date.day() as u32).unwrap()
}

#[component]
pub fn LogDatePicker(selected_date: NaiveDate, on_date_change: EventHandler<NaiveDate>) -> Element {
    let mut view_date = use_signal(|| naive_to_time(selected_date));
    let mut show_calendar = use_signal(|| false);

    let year = selected_date.format("%Y").to_string();
    let month_day = selected_date.format("%b %-d").to_string();

    rsx! {
        div { class: "flex flex-row items-center gap-2",
            button {
                class: "p-1",
                onclick: move |_| on_date_change.call(selected_date - Duration::days(1)),
                Icon { icon: IoCaretBackOutline, width: 16, height: 16 }
            }

            PlatformPopover {
                open: show_calendar(),
                on_open_change: move |v| {
                    if v {
                        view_date.set(naive_to_time(selected_date));
                    }
                    show_calendar.set(v);
                },
                align: ContentAlign::Center,
                trigger: rsx! {
                    div { class: "flex flex-col items-center cursor-pointer select-none",
                        span { class: "text-xs text-gray-500", "{year}" }
                        span { class: "text-sm font-medium", "{month_day}" }
                    }
                },
                content: rsx! {
                    Calendar {
                        selected_date: Some(naive_to_time(selected_date)),
                        on_date_change: move |date: Option<Date>| {
                            if let Some(d) = date {
                                on_date_change.call(time_to_naive(d));
                                show_calendar.set(false);
                            }
                        },
                        view_date: view_date(),
                        on_view_change: move |new_view: Date| {
                            view_date.set(new_view);
                        },
                        CalendarView {
                            CalendarHeader {
                                CalendarNavigation {
                                    CalendarPreviousMonthButton {}
                                    CalendarMonthTitle {}
                                    CalendarNextMonthButton {}
                                }
                            }
                            CalendarGrid {}
                        }
                    }
                },
            }

            button {
                class: "p-1",
                onclick: move |_| on_date_change.call(selected_date + Duration::days(1)),
                Icon { icon: IoCaretForwardOutline, width: 16, height: 16 }
            }
        }
    }
}

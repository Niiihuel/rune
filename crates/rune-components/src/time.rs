//! Dates and times as the timeline writes them, in the local time zone,
//! on a 24-hour clock or a 12-hour one as the account prefers.

use std::cell::Cell;

use chrono::{DateTime, Datelike, Local, NaiveDate, TimeZone, Timelike};
use rune_i18n::{es, t, tr};
use rune_model::Timestamp;

thread_local! {
    // The window is built on one thread, rows of virtual lists included, so
    // the preference is set once per frame there and every caller follows it
    // without carrying it.
    static TWELVE_HOUR: Cell<bool> = const { Cell::new(false) };
}

/// Write times on a 12-hour clock ("2:05 p. m.") from now on, on this thread.
pub fn set_twelve_hour(twelve: bool) {
    TWELVE_HOUR.with(|cell| cell.set(twelve));
}

/// Whether times are written on a 12-hour clock.
pub fn twelve_hour() -> bool {
    TWELVE_HOUR.with(Cell::get)
}

const MONTHS: [&str; 12] = [
    es("enero"),
    es("febrero"),
    es("marzo"),
    es("abril"),
    es("mayo"),
    es("junio"),
    es("julio"),
    es("agosto"),
    es("septiembre"),
    es("octubre"),
    es("noviembre"),
    es("diciembre"),
];

/// From Monday, as chrono counts them.
const WEEKDAYS: [&str; 7] = [
    es("lunes"),
    es("martes"),
    es("miércoles"),
    es("jueves"),
    es("viernes"),
    es("sábado"),
    es("domingo"),
];

fn local(timestamp: Timestamp) -> DateTime<Local> {
    Local
        .timestamp_millis_opt(timestamp.0)
        .single()
        .unwrap_or_else(Local::now)
}

/// The local calendar day of `timestamp`.
pub fn day(timestamp: Timestamp) -> NaiveDate {
    local(timestamp).date_naive()
}

/// An ISO local calendar day as an inclusive lower or exclusive next-day
/// boundary. Ambiguous midnight uses its first instant; a missing midnight
/// or invalid calendar date is rejected rather than changing the query silently.
pub fn local_date_boundary(value: &str, next_day: bool) -> Option<Timestamp> {
    date_boundary(value, next_day, &Local)
}

fn date_boundary<T: TimeZone>(value: &str, next_day: bool, zone: &T) -> Option<Timestamp> {
    if value.len() != 10 {
        return None;
    }
    let mut date = NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()?;
    if !(1..=9999).contains(&date.year()) {
        return None;
    }
    if next_day {
        date = date.succ_opt()?;
    }
    zone.from_local_datetime(&date.and_hms_opt(0, 0, 0)?)
        .earliest()
        .map(|time| Timestamp(time.timestamp_millis()))
}

/// The month a controlled date picker should open on.
pub fn picker_month(value: &str) -> (i32, u32) {
    let date =
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap_or_else(|_| Local::now().date_naive());
    (date.year(), date.month())
}

/// "14:05", or "2:05 p. m." on a 12-hour clock.
pub fn clock(timestamp: Timestamp) -> String {
    let time = local(timestamp);
    if !twelve_hour() {
        return time.format("%H:%M").to_string();
    }
    let (afternoon, hour) = time.hour12();
    format!("{hour}:{:02} {}", time.minute(), half(afternoon))
}

/// "a. m." or "p. m.": which half of the day a 12-hour clock is in.
fn half(afternoon: bool) -> &'static str {
    if afternoon {
        t("p. m.")
    } else {
        t("a. m.")
    }
}

/// A numeric date, "12/10/2026", in the order the language writes it.
fn numeric_date(time: &DateTime<Local>) -> String {
    time.format(t("%d/%m/%Y")).to_string()
}

/// "Hoy", "Ayer", or "12 de octubre de 2026".
pub fn day_label(timestamp: Timestamp, now: Timestamp) -> String {
    let (date, today) = (day(timestamp), day(now));
    if date == today {
        t("Hoy").into()
    } else if Some(date) == today.pred_opt() {
        t("Ayer").into()
    } else {
        date_label(timestamp)
    }
}

/// The local calendar day of `timestamp` in words: "12 de octubre de 2026".
pub fn date_label(timestamp: Timestamp) -> String {
    let date = day(timestamp);
    tr!(
        "{day} de {month} de {year}",
        day = date.day(),
        month = t(MONTHS[date.month0() as usize]),
        year = date.year()
    )
}

/// "miércoles, 14 de octubre de 2026, 08:20".
pub fn full_date(at: Timestamp) -> String {
    let weekday = Local
        .timestamp_millis_opt(at.0)
        .single()
        .map_or("", |local| {
            t(WEEKDAYS[local.weekday().num_days_from_monday() as usize])
        });
    tr!(
        "{weekday}, {date}, {time}",
        weekday,
        date = date_label(at),
        time = clock(at)
    )
}

/// When a message was written, beside its author: "Hoy a las 14:05",
/// "Ayer a las 09:12" or "12/10/2026 14:05".
pub fn stamp(timestamp: Timestamp, now: Timestamp) -> String {
    let (date, today) = (day(timestamp), day(now));
    if date == today {
        tr!("Hoy a las {time}", time = clock(timestamp))
    } else if Some(date) == today.pred_opt() {
        tr!("Ayer a las {time}", time = clock(timestamp))
    } else {
        format!("{} {}", numeric_date(&local(timestamp)), clock(timestamp))
    }
}

// Message actions (P4).
/// A moment written in a message, as `style` asks, in the local zone; a
/// relative one against `now`.
pub fn moment(at: Timestamp, style: rune_model::TimeStyle, now: Timestamp) -> String {
    use rune_model::TimeStyle;
    let time = local(at);
    let seconds = || {
        if twelve_hour() {
            let (afternoon, hour) = time.hour12();
            format!(
                "{hour}:{:02}:{:02} {}",
                time.minute(),
                time.second(),
                half(afternoon)
            )
        } else {
            time.format("%H:%M:%S").to_string()
        }
    };
    match style {
        TimeStyle::ShortTime => clock(at),
        TimeStyle::LongTime => seconds(),
        TimeStyle::ShortDate => numeric_date(&time),
        TimeStyle::LongDate => date_label(at),
        TimeStyle::ShortDateTime => format!("{} {}", date_label(at), clock(at)),
        TimeStyle::LongDateTime => full_date(at),
        TimeStyle::Relative => relative(at, now),
    }
}

/// «hace 3 minutos», «en 2 días»: how far `at` is from `now`, rounded as
/// the official client words it.
pub fn relative(at: Timestamp, now: Timestamp) -> String {
    let span = lasted(now.0 - at.0);
    if at.0 <= now.0 {
        tr!("hace {span}", span)
    } else {
        tr!("en {span}", span)
    }
}

/// «unos segundos», «3 minutos»: how long `millis` is, rounded as the
/// official client words a time span, such as a call's.
pub fn lasted(millis: i64) -> String {
    let seconds = millis.unsigned_abs() / 1000;
    let minutes = (seconds + 30) / 60;
    let hours = (minutes + 30) / 60;
    let days = (hours + 12) / 24;
    if seconds < 45 {
        t("unos segundos").to_owned()
    } else if seconds < 90 {
        t("un minuto").to_owned()
    } else if minutes < 45 {
        tr!("{minutes} minutos", minutes)
    } else if minutes < 90 {
        t("una hora").to_owned()
    } else if hours < 22 {
        tr!("{hours} horas", hours)
    } else if hours < 36 {
        t("un día").to_owned()
    } else if days < 26 {
        tr!("{days} días", days)
    } else if days < 45 {
        t("un mes").to_owned()
    } else if days < 320 {
        tr!("{months} meses", months = (days + 15) / 30)
    } else if days < 548 {
        t("un año").to_owned()
    } else {
        tr!("{years} años", years = (days + 182) / 365)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moments_follow_their_style_and_relative_ones_round() {
        use rune_model::TimeStyle;
        let at = Timestamp(
            Local
                .with_ymd_and_hms(2023, 11, 14, 22, 13, 20)
                .unwrap()
                .timestamp_millis(),
        );
        assert_eq!(moment(at, TimeStyle::ShortTime, at), "22:13");
        assert_eq!(moment(at, TimeStyle::LongTime, at), "22:13:20");
        assert_eq!(moment(at, TimeStyle::ShortDate, at), "14/11/2023");
        assert_eq!(
            moment(at, TimeStyle::LongDate, at),
            "14 de noviembre de 2023"
        );
        assert_eq!(
            moment(at, TimeStyle::ShortDateTime, at),
            "14 de noviembre de 2023 22:13"
        );
        assert_eq!(
            moment(at, TimeStyle::LongDateTime, at),
            "martes, 14 de noviembre de 2023, 22:13"
        );
        let minutes = |n: i64| Timestamp(at.0 + n * 60_000);
        assert_eq!(relative(at, Timestamp(at.0 + 10_000)), "hace unos segundos");
        assert_eq!(relative(at, minutes(1)), "hace un minuto");
        assert_eq!(relative(at, minutes(10)), "hace 10 minutos");
        assert_eq!(relative(minutes(60), at), "en una hora");
        assert_eq!(relative(at, minutes(5 * 60)), "hace 5 horas");
        assert_eq!(relative(minutes(3 * 24 * 60), at), "en 3 días");
        assert_eq!(relative(at, minutes(90 * 24 * 60)), "hace 3 meses");
        assert_eq!(relative(at, minutes(800 * 24 * 60)), "hace 2 años");
    }

    #[test]
    fn local_search_dates_use_creation_day_and_exclusive_following_midnight() {
        let zone = chrono::FixedOffset::west_opt(3 * 60 * 60).unwrap();
        let start = date_boundary("2024-02-29", false, &zone).unwrap();
        let end = date_boundary("2024-02-29", true, &zone).unwrap();
        assert_eq!(
            start.0,
            zone.with_ymd_and_hms(2024, 2, 29, 0, 0, 0)
                .unwrap()
                .timestamp_millis()
        );
        assert_eq!(
            end.0,
            zone.with_ymd_and_hms(2024, 3, 1, 0, 0, 0)
                .unwrap()
                .timestamp_millis()
        );
        assert_eq!(end.0 - start.0, 24 * 60 * 60 * 1000);
        for invalid in [
            "2023-02-29",
            "2026-13-01",
            "2026-2-01",
            "0000-01-01",
            "not a date",
        ] {
            assert!(date_boundary(invalid, false, &zone).is_none(), "{invalid}");
        }
        assert_eq!(picker_month("2024-02-29"), (2024, 2));
    }

    #[test]
    fn days_are_named_relative_to_today() {
        let now = Timestamp(Local::now().timestamp_millis());
        assert_eq!(day_label(now, now), "Hoy");
        assert_eq!(day_label(now.plus_minutes(-24 * 60), now), "Ayer");
        let long_ago = Timestamp(
            Local
                .with_ymd_and_hms(2024, 3, 9, 12, 0, 0)
                .unwrap()
                .timestamp_millis(),
        );
        assert_eq!(day_label(long_ago, now), "9 de marzo de 2024");
        assert_eq!(stamp(long_ago, now), "09/03/2024 12:00");
        assert!(stamp(now, now).starts_with("Hoy a las "));
    }

    #[test]
    fn english_writes_dates_its_own_way() {
        use rune_model::TimeStyle;
        let _english = rune_i18n::scoped(rune_i18n::Language::English);
        let at = |hour, minute| {
            Timestamp(
                Local
                    .with_ymd_and_hms(2023, 11, 14, hour, minute, 0)
                    .unwrap()
                    .timestamp_millis(),
            )
        };
        let now = Timestamp(Local::now().timestamp_millis());
        assert_eq!(date_label(at(22, 13)), "November 14, 2023");
        assert_eq!(stamp(at(22, 13), now), "11/14/2023 22:13");
        assert_eq!(moment(at(22, 13), TimeStyle::ShortDate, now), "11/14/2023");
        assert_eq!(full_date(at(22, 13)), "Tuesday, November 14, 2023 at 22:13");
        assert_eq!(day_label(now, now), "Today");
        assert!(stamp(now.plus_minutes(-24 * 60), now).starts_with("Yesterday at "));
        assert_eq!(relative(at(22, 0), at(22, 10)), "10 minutes ago");
        assert_eq!(relative(at(22, 0), at(22, 1)), "a minute ago");
        assert_eq!(relative(at(23, 0), at(22, 0)), "in an hour");
        set_twelve_hour(true);
        assert_eq!(clock(at(14, 5)), "2:05 PM");
        assert_eq!(clock(at(0, 30)), "12:30 AM");
        set_twelve_hour(false);
    }

    #[test]
    fn a_twelve_hour_clock_says_which_half_of_the_day() {
        let at = |hour, minute| {
            Timestamp(
                Local
                    .with_ymd_and_hms(2024, 3, 9, hour, minute, 0)
                    .unwrap()
                    .timestamp_millis(),
            )
        };
        assert_eq!(clock(at(14, 5)), "14:05");
        set_twelve_hour(true);
        assert_eq!(clock(at(14, 5)), "2:05 p. m.");
        assert_eq!(clock(at(0, 30)), "12:30 a. m.");
        assert_eq!(clock(at(12, 0)), "12:00 p. m.");
        assert_eq!(stamp(at(9, 7), at(20, 0)), "Hoy a las 9:07 a. m.");
        set_twelve_hour(false);
        assert_eq!(clock(at(0, 30)), "00:30");
    }
}

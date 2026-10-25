//! Date arithmetic for gantt charts, in plain Rust.
//!
//! Times are milliseconds since 1970-01-01T00:00 in a zone-free calendar:
//! every day is 24 hours, so a chart reads the same wherever it is rendered.
//! Civil conversions are Howard Hinnant's `days_from_civil` /
//! `civil_from_days`, valid far beyond any chart's range.

pub const SECOND: i64 = 1000;
pub const MINUTE: i64 = 60 * SECOND;
pub const HOUR: i64 = 60 * MINUTE;
pub const DAY: i64 = 24 * HOUR;

/// Days since 1970-01-01 of a proleptic Gregorian date.
pub fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = month as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The date `days` after 1970-01-01, as (year, month 1–12, day 1–31).
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

pub fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if is_leap(year) => 29,
        _ => 28,
    }
}

/// ISO weekday of a day number: 1 = Monday … 7 = Sunday.
pub fn iso_weekday(days: i64) -> u32 {
    // 1970-01-01 was a Thursday.
    ((days + 3).rem_euclid(7) + 1) as u32
}

/// A time broken into its calendar fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Civil {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub millis: u32,
}

impl Civil {
    pub fn of(ms: i64) -> Civil {
        let days = ms.div_euclid(DAY);
        let rest = ms.rem_euclid(DAY);
        let (year, month, day) = civil_from_days(days);
        Civil {
            year,
            month,
            day,
            hour: (rest / HOUR) as u32,
            minute: (rest % HOUR / MINUTE) as u32,
            second: (rest % MINUTE / SECOND) as u32,
            millis: (rest % SECOND) as u32,
        }
    }

    pub fn to_ms(self) -> i64 {
        days_from_civil(self.year, self.month, self.day) * DAY
            + self.hour as i64 * HOUR
            + self.minute as i64 * MINUTE
            + self.second as i64 * SECOND
            + self.millis as i64
    }
}

/// The midnight starting the day `ms` falls in.
pub fn start_of_day(ms: i64) -> i64 {
    ms.div_euclid(DAY) * DAY
}

/// `ms` moved by whole calendar months, clamping the day to the target
/// month's length (31 January + 1 month is 28 or 29 February), as dayjs does.
pub fn add_months(ms: i64, months: i64) -> i64 {
    let mut civil = Civil::of(ms);
    let index = civil.year * 12 + (civil.month as i64 - 1) + months;
    civil.year = index.div_euclid(12);
    civil.month = (index.rem_euclid(12) + 1) as u32;
    civil.day = civil.day.min(days_in_month(civil.year, civil.month));
    civil.to_ms()
}

/// A duration unit, as gantt's `3d`, `2w`, `5h` spell them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Millis,
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
}

/// Parse a duration such as `3d`, `1.5h` or `250ms`.
pub fn parse_duration(text: &str) -> Option<(f64, Unit)> {
    let text = text.trim();
    let split = text.find(|c: char| !(c.is_ascii_digit() || c == '.'))?;
    let (number, unit) = text.split_at(split);
    if number.is_empty() || number.starts_with('.') || number.ends_with('.') {
        return None;
    }
    let value: f64 = number.parse().ok()?;
    let unit = match unit {
        "ms" => Unit::Millis,
        "s" => Unit::Second,
        "m" => Unit::Minute,
        "h" => Unit::Hour,
        "d" => Unit::Day,
        "w" => Unit::Week,
        "M" => Unit::Month,
        "y" => Unit::Year,
        _ => return None,
    };
    Some((value, unit))
}

/// `ms` plus a duration. Days and weeks round to whole days and months and
/// years truncate to whole months, following dayjs's `add`.
pub fn add_duration(ms: i64, value: f64, unit: Unit) -> i64 {
    let fixed = |per: i64| ms + (value * per as f64).round() as i64;
    match unit {
        Unit::Millis => fixed(1),
        Unit::Second => fixed(SECOND),
        Unit::Minute => fixed(MINUTE),
        Unit::Hour => fixed(HOUR),
        Unit::Day => ms + value.round() as i64 * DAY,
        Unit::Week => ms + (value * 7.0).round() as i64 * DAY,
        Unit::Month => add_months(ms, value.trunc() as i64),
        Unit::Year => add_months(ms, value.trunc() as i64 * 12),
    }
}

/// One piece of a `dateFormat`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Year4,
    Year2,
    /// `Y`: a signed year of any length.
    YearSigned,
    /// A field of `min..=max` digits: month, day, hour, minute, second.
    Field(Field, usize, usize),
    /// `S`, `SS`, `SSS`: tenths, hundredths or thousandths of a second.
    Fraction(usize),
    /// `MMM` (`Jan`) or `MMMM` (`January`), case-insensitive.
    MonthName {
        full: bool,
    },
    /// `Do`: a day of the month with an English ordinal suffix, `1st`.
    Ordinal,
    /// `d`, `dd`, `ddd`, `dddd`: a weekday, matched and then ignored, as
    /// moment ignores it — the date fields decide the day.
    Weekday(WeekdayStyle),
    /// `A` / `a`: `AM` or `PM`, turning `h`/`hh` into a 12-hour clock.
    Meridiem,
    /// `Q`: a quarter, 1–4, meaning its first month.
    Quarter,
    /// `Z` / `ZZ`: `+05:00`, `-0800` or `Z`; the time is moved to UTC.
    Offset,
    Literal(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeekdayStyle {
    /// `d`: 0 (Sunday) to 6.
    Number,
    /// `dd`: `Mo`.
    Min,
    /// `ddd`: `Mon`.
    Short,
    /// `dddd`: `Monday`.
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Month,
    Day,
    Hour,
    Minute,
    Second,
}

/// A parsed moment.js-style `dateFormat`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DateFormat {
    Tokens(Vec<Token>),
    /// `X`: seconds since the epoch.
    UnixSeconds,
    /// `x`: milliseconds since the epoch.
    UnixMillis,
}

impl Default for DateFormat {
    fn default() -> Self {
        DateFormat::parse("YYYY-MM-DD").expect("the default format parses")
    }
}

const SUPPORTED_TOKENS: &str = "YYYY, YY, Y, Q, MMMM, MMM, MM, M, DD, D, Do, dddd, ddd, dd, \
     d, HH, H, hh, h, A, a, mm, m, ss, s, SSS, SS, S, ZZ, Z, X or x";

/// Every letter a moment.js or dayjs format token is made of; the others are
/// literal text.
const TOKEN_LETTERS: &str = "AaDdEeGgHhkMmNnQSsWwXxYyZz";

impl DateFormat {
    /// Parse a format, naming the first token it does not support.
    pub fn parse(format: &str) -> Result<DateFormat, String> {
        match format.trim() {
            "X" => return Ok(DateFormat::UnixSeconds),
            "x" => return Ok(DateFormat::UnixMillis),
            _ => {}
        }
        let chars: Vec<char> = format.trim().chars().collect();
        let mut tokens = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == '[' {
                let end = chars[i..]
                    .iter()
                    .position(|&c| c == ']')
                    .ok_or("an unclosed `[` in dateFormat")?;
                tokens.push(Token::Literal(chars[i + 1..i + end].iter().collect()));
                i += end + 1;
                continue;
            }
            // Letters no moment or dayjs token uses, such as the `T` in
            // `YYYY-MM-DDTHH:mm`, stand for themselves, as they do in dayjs.
            if !c.is_ascii_alphabetic() || !TOKEN_LETTERS.contains(c) {
                tokens.push(Token::Literal(c.to_string()));
                i += 1;
                continue;
            }
            let mut run = chars[i..].iter().take_while(|&&x| x == c).count();
            if c == 'D' && run == 1 && chars.get(i + 1) == Some(&'o') {
                run = 2;
            }
            let token: String = chars[i..i + run].iter().collect();
            tokens.push(match token.as_str() {
                "YYYY" => Token::Year4,
                "YY" => Token::Year2,
                "Y" => Token::YearSigned,
                "Q" => Token::Quarter,
                "MMMM" => Token::MonthName { full: true },
                "MMM" => Token::MonthName { full: false },
                "MM" => Token::Field(Field::Month, 2, 2),
                "M" => Token::Field(Field::Month, 1, 2),
                "DD" => Token::Field(Field::Day, 2, 2),
                "D" => Token::Field(Field::Day, 1, 2),
                "Do" => Token::Ordinal,
                "dddd" => Token::Weekday(WeekdayStyle::Full),
                "ddd" => Token::Weekday(WeekdayStyle::Short),
                "dd" => Token::Weekday(WeekdayStyle::Min),
                "d" => Token::Weekday(WeekdayStyle::Number),
                // `h` is the 12-hour clock; a meridiem token, if any, moves
                // it into the afternoon.
                "HH" | "hh" => Token::Field(Field::Hour, 2, 2),
                "H" | "h" => Token::Field(Field::Hour, 1, 2),
                "A" | "a" => Token::Meridiem,
                "mm" => Token::Field(Field::Minute, 2, 2),
                "m" => Token::Field(Field::Minute, 1, 2),
                "ss" => Token::Field(Field::Second, 2, 2),
                "s" => Token::Field(Field::Second, 1, 2),
                "SSS" => Token::Fraction(3),
                "SS" => Token::Fraction(2),
                "S" => Token::Fraction(1),
                "ZZ" | "Z" => Token::Offset,
                other => {
                    return Err(format!(
                        "dateFormat token `{other}` is not supported; use {SUPPORTED_TOKENS} \
                         (see DESIGN.md)"
                    ));
                }
            });
            i += run;
        }
        Ok(DateFormat::Tokens(tokens))
    }

    /// Strictly parse `text`: every token must match and nothing may be left.
    pub fn parse_date(&self, text: &str) -> Option<i64> {
        let text = text.trim();
        let tokens = match self {
            DateFormat::UnixSeconds => return parse_int(text).map(|s| s * SECOND),
            DateFormat::UnixMillis => return parse_int(text),
            DateFormat::Tokens(tokens) => tokens,
        };
        let mut civil = Civil {
            year: 1970,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
            millis: 0,
        };
        let mut afternoon = None;
        let mut offset_minutes = 0i64;
        let mut rest = text;
        for token in tokens {
            rest = match token {
                Token::Literal(lit) => rest.strip_prefix(lit.as_str())?,
                Token::Year4 => {
                    let (value, tail) = digits(rest, 4, 4)?;
                    civil.year = value as i64;
                    tail
                }
                Token::Year2 => {
                    let (value, tail) = digits(rest, 2, 2)?;
                    // moment's pivot: 68 and below are 20xx.
                    civil.year = if value <= 68 { 2000 } else { 1900 } + value as i64;
                    tail
                }
                Token::YearSigned => {
                    let (sign, body) = match rest.as_bytes().first() {
                        Some(b'-') => (-1, &rest[1..]),
                        Some(b'+') => (1, &rest[1..]),
                        _ => (1, rest),
                    };
                    let (value, tail) = digits(body, 1, 6)?;
                    civil.year = sign * value as i64;
                    tail
                }
                Token::Quarter => {
                    let (value, tail) = digits(rest, 1, 1)?;
                    if !(1..=4).contains(&value) {
                        return None;
                    }
                    civil.month = (value - 1) * 3 + 1;
                    tail
                }
                Token::Fraction(width) => {
                    let (value, tail) = digits(rest, *width, *width)?;
                    civil.millis = value * 10u32.pow(3 - *width as u32);
                    tail
                }
                Token::MonthName { full } => {
                    let (month, tail) = MONTHS.iter().zip(1..).find_map(|(name, month)| {
                        let name = if *full { name } else { &name[..3] };
                        strip_word(rest, name).map(|tail| (month, tail))
                    })?;
                    civil.month = month;
                    tail
                }
                Token::Ordinal => {
                    let (value, tail) = digits(rest, 1, 2)?;
                    civil.day = value;
                    ["st", "nd", "rd", "th"]
                        .iter()
                        .find_map(|suffix| strip_word(tail, suffix))?
                }
                Token::Weekday(WeekdayStyle::Number) => {
                    let (value, tail) = digits(rest, 1, 1)?;
                    (value <= 6).then_some(tail)?
                }
                Token::Weekday(style) => {
                    let len = match style {
                        WeekdayStyle::Min => 2,
                        WeekdayStyle::Short => 3,
                        _ => usize::MAX,
                    };
                    WEEKDAYS
                        .iter()
                        .find_map(|name| strip_word(rest, &name[..len.min(name.len())]))?
                }
                Token::Meridiem => match strip_word(rest, "am") {
                    Some(tail) => {
                        afternoon = Some(false);
                        tail
                    }
                    None => {
                        afternoon = Some(true);
                        strip_word(rest, "pm")?
                    }
                },
                Token::Offset => match rest.strip_prefix('Z') {
                    Some(tail) => tail,
                    None => {
                        let sign = match rest.as_bytes().first()? {
                            b'+' => 1,
                            b'-' => -1,
                            _ => return None,
                        };
                        let (hours, tail) = digits(&rest[1..], 2, 2)?;
                        let tail = tail.strip_prefix(':').unwrap_or(tail);
                        let (minutes, tail) = digits(tail, 2, 2).unwrap_or((0, tail));
                        if minutes >= 60 {
                            return None;
                        }
                        offset_minutes = sign * (hours as i64 * 60 + minutes as i64);
                        tail
                    }
                },
                Token::Field(field, min, max) => {
                    let (value, tail) = digits(rest, *min, *max)?;
                    match field {
                        Field::Month => civil.month = value,
                        Field::Day => civil.day = value,
                        Field::Hour => civil.hour = value,
                        Field::Minute => civil.minute = value,
                        Field::Second => civil.second = value,
                    }
                    tail
                }
            };
        }
        match afternoon {
            Some(true) if civil.hour < 12 => civil.hour += 12,
            Some(false) if civil.hour == 12 => civil.hour = 0,
            _ => {}
        }
        let valid = rest.is_empty()
            && (1..=12).contains(&civil.month)
            && civil.day >= 1
            && civil.day <= days_in_month(civil.year, civil.month)
            && civil.hour < 24
            && civil.minute < 60
            && civil.second < 60;
        valid.then(|| civil.to_ms() - offset_minutes * MINUTE)
    }
}

/// `text` without a leading `word`, compared ASCII case-insensitively.
fn strip_word<'a>(text: &'a str, word: &str) -> Option<&'a str> {
    let head = text.get(..word.len())?;
    head.eq_ignore_ascii_case(word).then(|| &text[word.len()..])
}

fn parse_int(text: &str) -> Option<i64> {
    let body = text.strip_prefix('-').unwrap_or(text);
    if body.is_empty() || !body.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// Between `min` and `max` leading ASCII digits.
fn digits(text: &str, min: usize, max: usize) -> Option<(u32, &str)> {
    let count = text
        .bytes()
        .take(max)
        .take_while(u8::is_ascii_digit)
        .count();
    if count < min {
        return None;
    }
    Some((text[..count].parse().ok()?, &text[count..]))
}

/// An ISO date, `YYYY-MM-DD` with an optional `THH:mm[:ss]` or ` HH:mm[:ss]`:
/// what JavaScript's `new Date()` accepts when a date does not match the
/// diagram's own format.
pub fn parse_iso(text: &str) -> Option<i64> {
    let text = text.trim();
    let (date, time) = match text.find(['T', ' ']) {
        Some(at) => (&text[..at], Some(&text[at + 1..])),
        None => (text, None),
    };
    let day = DateFormat::parse("YYYY-MM-DD").ok()?.parse_date(date)?;
    let Some(time) = time else {
        return Some(day);
    };
    let time = time.trim_end_matches('Z');
    for format in ["HH:mm:ss.SSS", "HH:mm:ss", "HH:mm"] {
        if let Some(at) = DateFormat::parse(format).ok()?.parse_date(time) {
            return Some(day + at);
        }
    }
    None
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// Lowercase English weekday name to ISO weekday.
pub fn weekday_named(name: &str) -> Option<u32> {
    WEEKDAYS
        .iter()
        .position(|day| day.eq_ignore_ascii_case(name))
        .map(|i| i as u32 + 1)
}

/// Check an `axisFormat` (d3's strftime subset) without formatting anything.
pub fn check_axis_format(format: &str) -> Result<(), String> {
    format_axis(0, format).map(|_| ())
}

/// The ISO 8601 week-numbering year and week (1–53) of a day number: weeks
/// start on Monday and week 1 holds the year's first Thursday.
pub fn iso_week(days: i64) -> (i64, i64) {
    let thursday = days - iso_weekday(days) as i64 + 4;
    let (year, _, _) = civil_from_days(thursday);
    (year, (thursday - days_from_civil(year, 1, 1)) / 7 + 1)
}

/// Format `ms` with an `axisFormat`: every d3-time-format directive (`%Y %y
/// %G %g %m %d %e %b %B %a %A %H %I %M %S %L %f %p %j %w %u %U %W %V %q %c
/// %x %X %Q %s %Z %%`) in d3's en-US locale, each optionally padded with `-`
/// (none), `_` (space) or `0`.
pub fn format_axis(ms: i64, format: &str) -> Result<String, String> {
    let civil = Civil::of(ms);
    let days = ms.div_euclid(DAY);
    let weekday = iso_weekday(days);
    let mut out = String::new();
    let mut chars = format.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let pad = match chars.peek() {
            Some('-') => {
                chars.next();
                Some("")
            }
            Some('_') => {
                chars.next();
                Some(" ")
            }
            Some('0') => {
                chars.next();
                Some("0")
            }
            _ => None,
        };
        let Some(directive) = chars.next() else {
            return Err("axisFormat ends in a bare `%`".into());
        };
        let num = |value: i64, width: usize, default: &str| {
            let pad = pad.unwrap_or(default);
            let text = value.to_string();
            if pad.is_empty() || text.len() >= width {
                text
            } else {
                pad.repeat(width - text.len()) + &text
            }
        };
        let hour12 = match civil.hour % 12 {
            0 => 12,
            h => h,
        };
        let doy = days - days_from_civil(civil.year, 1, 1) + 1;
        let (iso_year, iso_week) = iso_week(days);
        let piece = match directive {
            'Y' => civil.year.to_string(),
            'G' => iso_year.to_string(),
            'g' => num(iso_year.rem_euclid(100), 2, "0"),
            'V' => num(iso_week, 2, "0"),
            // Weeks starting on the year's first Sunday (`%U`) or Monday
            // (`%W`); days before it are week 0.
            'U' => num((doy - 1 + 7 - (weekday % 7) as i64) / 7, 2, "0"),
            'W' => num((doy - 1 + 7 - (weekday - 1) as i64) / 7, 2, "0"),
            'q' => (civil.month.div_ceil(3)).to_string(),
            'f' => num(civil.millis as i64 * 1000, 6, "0"),
            'Q' => ms.to_string(),
            's' => ms.div_euclid(SECOND).to_string(),
            // The chart is zone-free, so its offset is always zero.
            'Z' => "+0000".to_string(),
            // d3's en-US locale: `%x` is `%-m/%-d/%Y`, `%X` is
            // `%-I:%M:%S %p` and `%c` is `%x, %X`.
            'x' | 'X' | 'c' => {
                let date = format!("{}/{}/{}", civil.month, civil.day, civil.year);
                let time = format!(
                    "{hour12}:{:02}:{:02} {}",
                    civil.minute,
                    civil.second,
                    if civil.hour < 12 { "AM" } else { "PM" }
                );
                match directive {
                    'x' => date,
                    'X' => time,
                    _ => format!("{date}, {time}"),
                }
            }
            'y' => num(civil.year.rem_euclid(100), 2, "0"),
            'm' => num(civil.month as i64, 2, "0"),
            'd' => num(civil.day as i64, 2, "0"),
            'e' => num(civil.day as i64, 2, " "),
            'b' => MONTHS[civil.month as usize - 1][..3].to_string(),
            'B' => MONTHS[civil.month as usize - 1].to_string(),
            'a' => WEEKDAYS[weekday as usize - 1][..3].to_string(),
            'A' => WEEKDAYS[weekday as usize - 1].to_string(),
            'H' => num(civil.hour as i64, 2, "0"),
            'I' => num(hour12 as i64, 2, "0"),
            'M' => num(civil.minute as i64, 2, "0"),
            'S' => num(civil.second as i64, 2, "0"),
            'L' => num(civil.millis as i64, 3, "0"),
            'p' => if civil.hour < 12 { "AM" } else { "PM" }.to_string(),
            'j' => num(doy, 3, "0"),
            'w' => (weekday % 7).to_string(),
            'u' => weekday.to_string(),
            '%' => "%".to_string(),
            other => {
                return Err(format!(
                    "axisFormat directive `%{other}` is not supported (see DESIGN.md)"
                ));
            }
        };
        out.push_str(&piece);
    }
    // A label of only spaces would vanish; `%e` pads with them.
    Ok(out.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ymd(y: i64, m: u32, d: u32) -> i64 {
        days_from_civil(y, m, d) * DAY
    }

    #[test]
    fn civil_round_trips_across_eras() {
        for days in [-800_000, -1, 0, 1, 365, 10_957, 19_723, 2_932_896] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days, "{y}-{m}-{d}");
        }
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
    }

    #[test]
    fn leap_years_follow_the_gregorian_rule() {
        assert!(is_leap(2024) && is_leap(2000));
        assert!(!is_leap(1900) && !is_leap(2023));
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2100, 2), 28);
        assert_eq!(
            civil_from_days(days_from_civil(2024, 2, 28) + 1),
            (2024, 2, 29)
        );
        assert_eq!(
            civil_from_days(days_from_civil(2023, 2, 28) + 1),
            (2023, 3, 1)
        );
    }

    #[test]
    fn weekdays_are_iso() {
        assert_eq!(iso_weekday(0), 4); // 1970-01-01, a Thursday
        assert_eq!(iso_weekday(days_from_civil(2024, 1, 6)), 6); // Saturday
        assert_eq!(iso_weekday(days_from_civil(2024, 1, 7)), 7); // Sunday
        assert_eq!(iso_weekday(days_from_civil(2024, 1, 8)), 1); // Monday
    }

    #[test]
    fn month_arithmetic_clamps_to_the_month_end() {
        assert_eq!(add_months(ymd(2024, 1, 31), 1), ymd(2024, 2, 29));
        assert_eq!(add_months(ymd(2023, 1, 31), 1), ymd(2023, 2, 28));
        assert_eq!(add_months(ymd(2024, 11, 30), 3), ymd(2025, 2, 28));
        assert_eq!(add_months(ymd(2024, 3, 15), -3), ymd(2023, 12, 15));
        assert_eq!(
            add_duration(ymd(2024, 2, 29), 1.0, Unit::Year),
            ymd(2025, 2, 28)
        );
    }

    #[test]
    fn durations_parse_and_add() {
        assert_eq!(parse_duration("3d"), Some((3.0, Unit::Day)));
        assert_eq!(parse_duration("2w"), Some((2.0, Unit::Week)));
        assert_eq!(parse_duration("1.5h"), Some((1.5, Unit::Hour)));
        assert_eq!(parse_duration("250ms"), Some((250.0, Unit::Millis)));
        assert_eq!(parse_duration("3x"), None);
        assert_eq!(parse_duration("d"), None);
        assert_eq!(parse_duration("2024-01-01"), None);
        let start = ymd(2024, 12, 30);
        assert_eq!(add_duration(start, 3.0, Unit::Day), ymd(2025, 1, 2));
        assert_eq!(add_duration(start, 2.0, Unit::Week), ymd(2025, 1, 13));
        assert_eq!(add_duration(start, 5.0, Unit::Hour), start + 5 * HOUR);
    }

    #[test]
    fn date_formats_parse_strictly() {
        let iso = DateFormat::parse("YYYY-MM-DD").unwrap();
        assert_eq!(iso.parse_date("2024-02-29"), Some(ymd(2024, 2, 29)));
        assert_eq!(iso.parse_date("2023-02-29"), None);
        assert_eq!(iso.parse_date("2024-2-29"), None);
        assert_eq!(iso.parse_date("2024-02-29x"), None);

        let euro = DateFormat::parse("DD/MM/YYYY HH:mm").unwrap();
        assert_eq!(
            euro.parse_date("31/12/2024 17:30"),
            Some(ymd(2024, 12, 31) + 17 * HOUR + 30 * MINUTE)
        );
        let loose = DateFormat::parse("D.M.YY").unwrap();
        assert_eq!(loose.parse_date("5.1.24"), Some(ymd(2024, 1, 5)));
        assert_eq!(
            DateFormat::parse("X").unwrap().parse_date("86400"),
            Some(DAY)
        );
        let error = DateFormat::parse("YYYY-DDDD").unwrap_err();
        assert!(error.contains("`DDDD`"), "{error}");
    }

    #[test]
    fn iso_fallback_accepts_times() {
        assert_eq!(parse_iso("2024-01-02"), Some(ymd(2024, 1, 2)));
        assert_eq!(
            parse_iso("2024-01-02T03:04"),
            Some(ymd(2024, 1, 2) + 3 * HOUR + 4 * MINUTE)
        );
        assert_eq!(parse_iso("tomorrow"), None);
    }

    #[test]
    fn axis_formats_follow_strftime() {
        let at = ymd(2024, 3, 5) + 14 * HOUR + 7 * MINUTE;
        assert_eq!(format_axis(at, "%Y-%m-%d").unwrap(), "2024-03-05");
        assert_eq!(format_axis(at, "%b %e").unwrap(), "Mar  5");
        assert_eq!(format_axis(at, "%-d %B").unwrap(), "5 March");
        assert_eq!(format_axis(at, "%a %I:%M %p").unwrap(), "Tue 02:07 PM");
        assert_eq!(format_axis(at, "%j").unwrap(), "065");
        assert_eq!(format_axis(at, "%q").unwrap(), "1");
        assert_eq!(format_axis(at, "%x").unwrap(), "3/5/2024");
        assert_eq!(format_axis(at, "%X").unwrap(), "2:07:00 PM");
        assert_eq!(format_axis(at, "%c").unwrap(), "3/5/2024, 2:07:00 PM");
        assert_eq!(format_axis(at, "%Z").unwrap(), "+0000");
        assert_eq!(format_axis(DAY + 5, "%Q").unwrap(), "86400005");
        assert_eq!(format_axis(DAY + 5, "%s").unwrap(), "86400");
        assert_eq!(format_axis(5, "%f").unwrap(), "005000");
        assert!(format_axis(at, "%k").unwrap_err().contains("%k"));
    }

    fn weeks(y: i64, m: u32, d: u32) -> String {
        format_axis(ymd(y, m, d), "%U %W %V %G %g").unwrap()
    }

    #[test]
    fn week_numbers_match_d3() {
        // Thursday: ISO week 1 of 2026; no Sunday or Monday yet in 2026.
        assert_eq!(weeks(2026, 1, 1), "00 00 01 2026 26");
        // Sunday 4 January starts %U week 1; Monday 5 January %W week 1.
        assert_eq!(weeks(2026, 1, 4), "01 00 01 2026 26");
        assert_eq!(weeks(2026, 1, 5), "01 01 02 2026 26");
        // Thursday in a leap year whose ISO year has 53 weeks.
        assert_eq!(weeks(2020, 12, 31), "52 52 53 2020 20");
        // Friday 1 January to Sunday 3 January 2021 still belong to 2020-W53.
        assert_eq!(weeks(2021, 1, 1), "00 00 53 2020 20");
        assert_eq!(weeks(2021, 1, 3), "01 00 53 2020 20");
        assert_eq!(weeks(2021, 1, 4), "01 01 01 2021 21");
        // Monday 29 December 2025 is already ISO week 1 of 2026.
        assert_eq!(weeks(2025, 12, 29), "52 52 01 2026 26");
        // 2024 starts on a Monday: %W week 1 on 1 January.
        assert_eq!(weeks(2024, 1, 1), "00 01 01 2024 24");
        assert_eq!(weeks(2024, 12, 31), "52 53 01 2025 25");
        assert_eq!(format_axis(ymd(2026, 1, 5), "W%-V").unwrap(), "W2");
    }

    #[test]
    fn named_months_and_days_parse() {
        let named = DateFormat::parse("MMM D YYYY").unwrap();
        assert_eq!(named.parse_date("Jan 5 2026"), Some(ymd(2026, 1, 5)));
        assert_eq!(named.parse_date("jan 31 2026"), Some(ymd(2026, 1, 31)));
        assert_eq!(named.parse_date("FEB 29 2024"), Some(ymd(2024, 2, 29)));
        assert_eq!(named.parse_date("Feb 29 2023"), None);
        assert_eq!(named.parse_date("Dec 31 2025"), Some(ymd(2025, 12, 31)));
        assert_eq!(named.parse_date("Foo 1 2025"), None);
        assert_eq!(named.parse_date("January 1 2025"), None);

        let full = DateFormat::parse("dddd, MMMM Do YYYY").unwrap();
        assert_eq!(
            full.parse_date("Thursday, January 1st 2026"),
            Some(ymd(2026, 1, 1))
        );
        assert_eq!(
            full.parse_date("monday, september 22nd 2025"),
            Some(ymd(2025, 9, 22))
        );
        // The weekday is matched, then ignored: the date fields win.
        assert_eq!(
            full.parse_date("Friday, March 3rd 2025"),
            Some(ymd(2025, 3, 3))
        );
        assert_eq!(full.parse_date("Funday, March 3rd 2025"), None);
        assert_eq!(full.parse_date("Monday, March 3 2025"), None);

        let short = DateFormat::parse("ddd DD-MMM-YY").unwrap();
        assert_eq!(short.parse_date("Sat 29-Feb-24"), Some(ymd(2024, 2, 29)));
        assert_eq!(
            DateFormat::parse("[Q]Q YYYY")
                .unwrap()
                .parse_date("Q3 2026"),
            Some(ymd(2026, 7, 1))
        );
        assert_eq!(
            DateFormat::parse("Q YYYY").unwrap().parse_date("5 2026"),
            None
        );
    }

    #[test]
    fn twelve_hour_clocks_and_offsets_parse() {
        let clock = DateFormat::parse("YYYY-MM-DD h:mm A").unwrap();
        let day = ymd(2026, 3, 1);
        assert_eq!(
            clock.parse_date("2026-03-01 9:15 AM"),
            Some(day + 9 * HOUR + 15 * MINUTE)
        );
        assert_eq!(
            clock.parse_date("2026-03-01 9:15 pm"),
            Some(day + 21 * HOUR + 15 * MINUTE)
        );
        assert_eq!(clock.parse_date("2026-03-01 12:00 AM"), Some(day));
        assert_eq!(
            clock.parse_date("2026-03-01 12:00 PM"),
            Some(day + 12 * HOUR)
        );
        assert_eq!(clock.parse_date("2026-03-01 9:15"), None);

        let zoned = DateFormat::parse("YYYY-MM-DDTHH:mmZ").unwrap();
        assert_eq!(zoned, DateFormat::parse("YYYY-MM-DD[T]HH:mmZ").unwrap());
        assert_eq!(zoned.parse_date("2026-03-01T10:00Z"), Some(day + 10 * HOUR));
        assert_eq!(
            zoned.parse_date("2026-03-01T10:00+05:30"),
            Some(day + 4 * HOUR + 30 * MINUTE)
        );
        let compact = DateFormat::parse("YYYY-MM-DD HH:mm ZZ").unwrap();
        assert_eq!(
            compact.parse_date("2026-03-01 01:00 -0800"),
            Some(day + 9 * HOUR)
        );
        // Across midnight into the previous day.
        assert_eq!(
            compact.parse_date("2026-03-01 01:00 +0200"),
            Some(day - HOUR)
        );
        let fine = DateFormat::parse("HH:mm:ss.S").unwrap();
        assert_eq!(fine.parse_date("00:00:01.5"), Some(SECOND + 500));
        assert_eq!(
            DateFormat::parse("Y-MM").unwrap().parse_date("-44-03"),
            Some(ymd(-44, 3, 1))
        );
        let error = DateFormat::parse("YYYY-ww").unwrap_err();
        assert!(error.contains("`ww`"), "{error}");
    }
}

//! gantt parsing.
//!
//! Grammar subset per `DESIGN.md`, checked against mermaid's
//! `packages/mermaid/src/diagrams/gantt/parser/gantt.jison`; task timing
//! follows `ganttDb.js` (`compileTasks`, `getStartDate`, `getEndDate`,
//! `checkTaskDates`).

use super::date::{self, DAY, DateFormat, Unit};
use crate::{Error, strip_comment};

/// A task's place on the timeline, resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct Task {
    pub name: String,
    pub id: String,
    /// Index into [`Gantt::sections`]; `None` before the first `section`.
    pub section: Option<usize>,
    /// Milliseconds since the epoch.
    pub start: i64,
    /// Where dependants start: past any excluded days the task spans.
    pub end: i64,
    /// Where the bar is drawn to: excluded days at the very end are left off.
    pub render_end: i64,
    pub done: bool,
    pub active: bool,
    pub crit: bool,
    pub milestone: bool,
    /// A vertical marker line across the chart rather than a bar.
    pub vert: bool,
    pub line: usize,
}

/// Axis tick spacing set with `tickInterval`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickUnit {
    Millisecond,
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
}

/// Which days are excluded from task durations.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Calendar {
    pub weekends: bool,
    /// ISO weekday the weekend starts on: Saturday (6) or Friday (5).
    pub weekend_start: u32,
    pub weekdays: Vec<u32>,
    /// Day numbers (days since the epoch).
    pub days: Vec<i64>,
    pub included: Vec<i64>,
}

impl Calendar {
    pub fn is_active(&self) -> bool {
        self.weekends || !self.weekdays.is_empty() || !self.days.is_empty()
    }

    /// Whether the day `ms` falls in is excluded — `isInvalidDate`.
    pub fn excludes(&self, ms: i64) -> bool {
        let day = ms.div_euclid(DAY);
        if self.included.contains(&day) {
            return false;
        }
        let weekday = date::iso_weekday(day);
        (self.weekends && (weekday == self.weekend_start || weekday == self.weekend_start + 1))
            || self.weekdays.contains(&weekday)
            || self.days.contains(&day)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Gantt {
    pub title: Option<String>,
    pub sections: Vec<String>,
    pub tasks: Vec<Task>,
    pub axis_format: String,
    pub tick_interval: Option<(i64, TickUnit)>,
    pub top_axis: bool,
    /// ISO weekday week ticks fall on (`weekday`; Sunday by default).
    pub week_start: u32,
    pub calendar: Calendar,
}

#[derive(Debug, Clone)]
enum StartSpec {
    /// Straight after the previous task in the source.
    Previous,
    After(Vec<String>),
    At(i64),
}

#[derive(Debug, Clone)]
enum EndSpec {
    Until(Vec<String>),
    /// A date; `manual` when it is a literal `YYYY-MM-DD`, which mermaid does
    /// not push past excluded days.
    At(String),
    Duration(f64, Unit),
}

#[derive(Debug, Clone)]
struct RawTask {
    task: Task,
    start: StartSpec,
    start_text: String,
    end: EndSpec,
    prev: Option<usize>,
    resolved: bool,
}

const TAGS: [&str; 5] = ["active", "done", "crit", "milestone", "vert"];

/// Parse a gantt source.
pub fn parse(source: &str) -> Result<Gantt, Error> {
    let mut gantt = Gantt {
        title: None,
        sections: Vec::new(),
        tasks: Vec::new(),
        axis_format: "%Y-%m-%d".into(),
        tick_interval: None,
        top_axis: false,
        week_start: 7,
        calendar: Calendar {
            weekend_start: 6,
            ..Calendar::default()
        },
    };
    let mut format = DateFormat::default();
    let mut format_line = 0;
    let mut inclusive = false;
    let mut excludes: Vec<(String, usize)> = Vec::new();
    let mut includes: Vec<(String, usize)> = Vec::new();
    let mut raw: Vec<RawTask> = Vec::new();
    let mut seen_header = false;
    let mut in_acc_descr = false;

    for (index, text) in source.lines().enumerate() {
        let line_number = index + 1;
        let line = strip_comment(text).trim();
        if in_acc_descr {
            if line.contains('}') {
                in_acc_descr = false;
            }
            continue;
        }
        if line.is_empty() {
            continue;
        }
        if !seen_header {
            if keyword(line, "gantt").is_some() {
                seen_header = true;
                continue;
            }
            return Err(Error::new(line_number, "expected `gantt`"));
        }

        if let Some(rest) = keyword(line, "dateFormat") {
            format = DateFormat::parse(rest).map_err(|e| Error::new(line_number, e))?;
            format_line = line_number;
        } else if is_word(line, "inclusiveEndDates") {
            inclusive = true;
        } else if is_word(line, "topAxis") {
            gantt.top_axis = true;
        } else if let Some(rest) = keyword(line, "axisFormat") {
            date::check_axis_format(rest).map_err(|e| Error::new(line_number, e))?;
            gantt.axis_format = rest.to_string();
        } else if let Some(rest) = keyword(line, "tickInterval") {
            gantt.tick_interval = Some(parse_tick(rest, line_number)?);
        } else if let Some(rest) = keyword(line, "includes") {
            includes.extend(split_list(rest).map(|item| (item, line_number)));
        } else if let Some(rest) = keyword(line, "excludes") {
            excludes.extend(split_list(rest).map(|item| (item, line_number)));
        } else if keyword(line, "todayMarker").is_some() {
            // A static render has no "today"; accepted so diagrams that set
            // or switch off the marker still render.
        } else if let Some(rest) = keyword(line, "weekday") {
            gantt.week_start = date::weekday_named(rest.trim()).ok_or_else(|| {
                Error::new(line_number, format!("`weekday {rest}` is not a weekday"))
            })?;
        } else if let Some(rest) = keyword(line, "weekend") {
            gantt.calendar.weekend_start = match rest.trim().to_ascii_lowercase().as_str() {
                "friday" => 5,
                "saturday" => 6,
                other => {
                    return Err(Error::new(
                        line_number,
                        format!("`weekend {other}`: a weekend starts on friday or saturday"),
                    ));
                }
            };
        } else if let Some(rest) = keyword(line, "title") {
            gantt.title = Some(rest.to_string());
        } else if keyword(line, "accDescription").is_some() || acc_line(line) {
            // Accessibility text is not drawn.
        } else if starts_acc_block(line) {
            in_acc_descr = !line.contains('}');
        } else if let Some(rest) = keyword(line, "section") {
            gantt.sections.push(rest.to_string());
        } else if keyword(line, "click").is_some() {
            // Interaction (`href`, `call`) has no meaning in a static drawing.
        } else {
            let task = parse_task(line, line_number, &gantt, &format, raw.len())?;
            raw.push(task);
        }
    }

    if !seen_header {
        return Err(Error::new(1, "expected `gantt`"));
    }

    gantt.calendar.days = resolve_days(&excludes, &format, true, &mut gantt.calendar)?;
    let mut ignored = Calendar::default();
    gantt.calendar.included = resolve_days(&includes, &format, false, &mut ignored)?;

    // Start and end dates in the final dateFormat, as mermaid compiles them
    // after the whole source is read.
    for task in &mut raw {
        if let StartSpec::At(_) = task.start {
            task.start = StartSpec::At(parse_date(
                &task.start_text,
                &format,
                task.task.line,
                format_line,
            )?);
        }
    }

    compile(&mut raw, &format, inclusive, &gantt.calendar)?;
    gantt.tasks = raw.into_iter().map(|raw| raw.task).collect();
    Ok(gantt)
}

/// `rest` after a case-insensitive `word` and whitespace (or the end).
fn keyword<'a>(line: &'a str, word: &str) -> Option<&'a str> {
    let head = line.get(..word.len())?;
    if !head.eq_ignore_ascii_case(word) {
        return None;
    }
    let rest = &line[word.len()..];
    if rest.is_empty() {
        return Some("");
    }
    rest.starts_with(char::is_whitespace).then(|| rest.trim())
}

fn is_word(line: &str, word: &str) -> bool {
    keyword(line, word).is_some_and(str::is_empty)
}

fn acc_line(line: &str) -> bool {
    ["accTitle", "accDescr"].iter().any(|word| {
        keyword(line, word).is_some_and(|rest| rest.starts_with(':'))
            || line
                .strip_prefix(word)
                .is_some_and(|rest| rest.trim_start().starts_with(':'))
    })
}

fn starts_acc_block(line: &str) -> bool {
    line.strip_prefix("accDescr")
        .is_some_and(|rest| rest.trim_start().starts_with('{'))
}

fn split_list(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| c.is_whitespace() || c == ',')
        .filter(|item| !item.is_empty())
        .map(str::to_ascii_lowercase)
}

fn parse_tick(text: &str, line: usize) -> Result<(i64, TickUnit), Error> {
    let text = text.trim();
    let split = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    let (count, unit) = text.split_at(split);
    let count: i64 = count.parse().ok().filter(|&n| n > 0).ok_or_else(|| {
        Error::new(
            line,
            format!("`tickInterval {text}`: expected a count and a unit, like `1week`"),
        )
    })?;
    let unit = match unit {
        "millisecond" => TickUnit::Millisecond,
        "second" => TickUnit::Second,
        "minute" => TickUnit::Minute,
        "hour" => TickUnit::Hour,
        "day" => TickUnit::Day,
        "week" => TickUnit::Week,
        "month" => TickUnit::Month,
        other => {
            return Err(Error::new(
                line,
                format!(
                    "tickInterval unit `{other}`: expected millisecond, second, minute, hour, \
                     day, week or month"
                ),
            ));
        }
    };
    Ok((count, unit))
}

/// Excluded (or included) dates as day numbers; `weekends` and weekday names
/// go into `calendar` when `exclusions` is set.
fn resolve_days(
    items: &[(String, usize)],
    format: &DateFormat,
    exclusions: bool,
    calendar: &mut Calendar,
) -> Result<Vec<i64>, Error> {
    let iso = DateFormat::default();
    let mut days = Vec::new();
    for (item, line) in items {
        if exclusions && item == "weekends" {
            calendar.weekends = true;
        } else if let Some(weekday) = date::weekday_named(item).filter(|_| exclusions) {
            calendar.weekdays.push(weekday);
        } else if let Some(ms) = format.parse_date(item).or_else(|| iso.parse_date(item)) {
            days.push(ms.div_euclid(DAY));
        } else {
            let what = if exclusions { "excludes" } else { "includes" };
            let expected = if exclusions {
                "`weekends`, a weekday name or a date"
            } else {
                "a date"
            };
            return Err(Error::new(
                *line,
                format!("{what}: `{item}` is not {expected}"),
            ));
        }
    }
    Ok(days)
}

fn parse_task(
    line: &str,
    line_number: usize,
    gantt: &Gantt,
    format: &DateFormat,
    index: usize,
) -> Result<RawTask, Error> {
    let Some(colon) = line.find(':') else {
        return Err(Error::new(
            line_number,
            format!("unexpected `{line}`; expected a task as `name : data`, or a statement"),
        ));
    };
    let name = line[..colon].trim();
    if name.is_empty() {
        return Err(Error::new(line_number, "a task needs a name before `:`"));
    }
    let mut data: Vec<&str> = line[colon + 1..].split(',').map(str::trim).collect();
    let mut task = Task {
        name: name.to_string(),
        id: format!("task{}", index + 1),
        section: gantt.sections.len().checked_sub(1),
        start: 0,
        end: 0,
        render_end: 0,
        done: false,
        active: false,
        crit: false,
        milestone: false,
        vert: false,
        line: line_number,
    };
    while let Some(&tag) = data.first().filter(|first| TAGS.contains(first)) {
        match tag {
            "active" => task.active = true,
            "done" => task.done = true,
            "crit" => task.crit = true,
            "milestone" => task.milestone = true,
            _ => task.vert = true,
        }
        data.remove(0);
    }

    let (start_text, end_text) = match data.as_slice() {
        [end] => (None, *end),
        [start, end] => (Some(*start), *end),
        [id, start, end] => {
            if id.is_empty() {
                return Err(Error::new(line_number, "empty task id"));
            }
            task.id = id.to_string();
            (Some(*start), *end)
        }
        _ => {
            return Err(Error::new(
                line_number,
                format!(
                    "task `{name}` has {} fields; expected `[tags,] [id,] [start,] end`",
                    data.len()
                ),
            ));
        }
    };
    if end_text.is_empty() {
        return Err(Error::new(
            line_number,
            format!("task `{name}` needs an end: a date, a duration or `until`"),
        ));
    }

    let start = match start_text {
        None => StartSpec::Previous,
        Some(text) => match text.strip_prefix("after") {
            Some(ids) if ids.starts_with(char::is_whitespace) => {
                StartSpec::After(ids.split_whitespace().map(String::from).collect())
            }
            _ => StartSpec::At(0),
        },
    };
    let end = match end_text.strip_prefix("until") {
        Some(ids) if ids.starts_with(char::is_whitespace) => {
            EndSpec::Until(ids.split_whitespace().map(String::from).collect())
        }
        _ => match date::parse_duration(end_text) {
            Some((value, unit)) if format.parse_date(end_text).is_none() => {
                EndSpec::Duration(value, unit)
            }
            _ => EndSpec::At(end_text.to_string()),
        },
    };
    Ok(RawTask {
        task,
        start,
        start_text: start_text.unwrap_or_default().to_string(),
        end,
        prev: index.checked_sub(1),
        resolved: false,
    })
}

fn parse_date(
    text: &str,
    format: &DateFormat,
    line: usize,
    format_line: usize,
) -> Result<i64, Error> {
    format
        .parse_date(text)
        .or_else(|| date::parse_iso(text))
        .ok_or_else(|| {
            let hint = if format_line > 0 {
                format!(" (dateFormat set on line {format_line})")
            } else {
                " (the default dateFormat is YYYY-MM-DD)".into()
            };
            Error::new(
                line,
                format!("`{text}` is not a date in the chart's dateFormat{hint}"),
            )
        })
}

/// Resolve every task's dates, repeating passes so a task may depend on one
/// written after it, as mermaid's `getTasks` does.
fn compile(
    raw: &mut [RawTask],
    format: &DateFormat,
    inclusive: bool,
    calendar: &Calendar,
) -> Result<(), Error> {
    let find = |raw: &[RawTask], id: &str, line: usize| {
        raw.iter()
            .position(|task| task.task.id == id)
            .ok_or_else(|| Error::new(line, format!("no task has the id `{id}`")))
    };
    // Unknown ids are reported up front, before any cycle is.
    for task in raw.iter() {
        let line = task.task.line;
        if let StartSpec::After(ids) = &task.start {
            for id in ids {
                find(raw, id, line)?;
            }
        }
        if let EndSpec::Until(ids) = &task.end {
            for id in ids {
                find(raw, id, line)?;
            }
        }
        if matches!(task.start, StartSpec::Previous) && task.prev.is_none() {
            return Err(Error::new(
                line,
                format!(
                    "the first task, `{}`, needs a start date or `after`",
                    task.task.name
                ),
            ));
        }
    }

    loop {
        let mut progress = false;
        for index in 0..raw.len() {
            if raw[index].resolved {
                continue;
            }
            let line = raw[index].task.line;
            let start = match &raw[index].start {
                StartSpec::At(at) => Some(*at),
                StartSpec::Previous => {
                    let prev = &raw[raw[index].prev.expect("checked above")];
                    prev.resolved.then_some(prev.task.end)
                }
                StartSpec::After(ids) => {
                    let mut latest: Option<i64> = None;
                    let mut ready = true;
                    for id in ids {
                        let other = &raw[find(raw, id, line)?];
                        if !other.resolved {
                            ready = false;
                            break;
                        }
                        latest = Some(latest.map_or(other.task.end, |l| l.max(other.task.end)));
                    }
                    if ready { latest } else { None }
                }
            };
            let Some(start) = start else { continue };

            let (end, manual) = match &raw[index].end {
                EndSpec::Duration(value, unit) => (date::add_duration(start, *value, *unit), false),
                EndSpec::At(text) => {
                    let at = format.parse_date(text).or_else(|| date::parse_iso(text));
                    let Some(mut at) = at else {
                        return Err(Error::new(
                            line,
                            format!(
                                "`{text}` is neither a date in the chart's dateFormat nor a \
                                 duration like `3d`"
                            ),
                        ));
                    };
                    if inclusive {
                        at += DAY;
                    }
                    (at, DateFormat::default().parse_date(text).is_some())
                }
                EndSpec::Until(ids) => {
                    let mut earliest: Option<i64> = None;
                    let mut ready = true;
                    for id in ids {
                        let other_index = find(raw, id, line)?;
                        let other = &raw[other_index];
                        // A task's start is known once it is resolved, or
                        // straight away when it is a literal date.
                        let other_start = if other.resolved {
                            Some(other.task.start)
                        } else if let StartSpec::At(at) = other.start {
                            Some(at)
                        } else {
                            None
                        };
                        let Some(other_start) = other_start.filter(|_| other_index != index) else {
                            ready = false;
                            break;
                        };
                        earliest = Some(earliest.map_or(other_start, |e| e.min(other_start)));
                    }
                    match earliest.filter(|_| ready) {
                        Some(at) => (at, false),
                        None => continue,
                    }
                }
            };

            let task = &mut raw[index].task;
            task.start = start;
            task.end = end;
            task.render_end = end;
            if calendar.is_active() && !manual {
                let (end, render_end) = skip_excluded(start, end, calendar);
                task.end = end;
                task.render_end = render_end.unwrap_or(end);
            }
            raw[index].resolved = true;
            progress = true;
        }
        if raw.iter().all(|task| task.resolved) {
            return Ok(());
        }
        if !progress {
            let stuck = raw.iter().find(|task| !task.resolved).expect("one is left");
            return Err(Error::new(
                stuck.task.line,
                format!(
                    "task `{}` depends on itself through `after`/`until`",
                    stuck.task.name
                ),
            ));
        }
    }
}

/// Push `end` past each excluded day the task covers — `fixTaskDates`. The
/// second value is where the bar is drawn to: excluded days at the end are
/// left off it.
pub fn skip_excluded(start: i64, mut end: i64, calendar: &Calendar) -> (i64, Option<i64>) {
    let mut day = start + DAY;
    let mut invalid = false;
    let mut render_end = None;
    // A guard against a calendar that excludes every day.
    let mut steps = 0;
    while day <= end && steps < 100_000 {
        if !invalid {
            render_end = Some(end);
        }
        invalid = calendar.excludes(day);
        if invalid {
            end += DAY;
        }
        day += DAY;
        steps += 1;
    }
    (end, render_end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gantt::date::{HOUR, days_from_civil};

    fn ymd(y: i64, m: u32, d: u32) -> i64 {
        days_from_civil(y, m, d) * DAY
    }

    fn task<'a>(gantt: &'a Gantt, name: &str) -> &'a Task {
        gantt.tasks.iter().find(|t| t.name == name).unwrap()
    }

    #[test]
    fn durations_and_dependencies_resolve() {
        let source = "gantt
    dateFormat YYYY-MM-DD
    section Build
    Design   :a1, 2024-01-29, 3d
    Code     :after a1, 2w
    Review   :5h
    section Ship
    Release  :crit, milestone, r1, after a1 b2, 0d
    Docs     :b2, 2024-01-30, 2024-02-01
";
        let gantt = parse(source).unwrap();
        assert_eq!(gantt.sections, ["Build", "Ship"]);
        assert_eq!(task(&gantt, "Design").end, ymd(2024, 2, 1));
        assert_eq!(task(&gantt, "Code").start, ymd(2024, 2, 1));
        assert_eq!(task(&gantt, "Code").end, ymd(2024, 2, 15));
        assert_eq!(task(&gantt, "Review").end, ymd(2024, 2, 15) + 5 * HOUR);
        let release = task(&gantt, "Release");
        assert!(release.crit && release.milestone);
        // The later of a1 (1 Feb) and b2 (1 Feb, written after it).
        assert_eq!(release.start, ymd(2024, 2, 1));
        assert_eq!(release.section, Some(1));
    }

    #[test]
    fn until_ends_at_the_earliest_named_start() {
        let source = "gantt
    Prep  :p, 2024-03-01, until launch
    Go    :launch, 2024-03-10, 1d
";
        let gantt = parse(source).unwrap();
        assert_eq!(task(&gantt, "Prep").end, ymd(2024, 3, 10));
    }

    #[test]
    fn weekends_push_the_end_and_leave_trailing_ones_undrawn() {
        // 2024-01-04 is a Thursday.
        let source = "gantt
    excludes weekends
    Two   :a, 2024-01-04, 2d
    Three :b, 2024-01-04, 3d
    Next  :after a, 1d
";
        let gantt = parse(source).unwrap();
        let two = task(&gantt, "Two");
        assert_eq!(two.end, ymd(2024, 1, 8), "Thu+Fri, then Monday");
        assert_eq!(two.render_end, ymd(2024, 1, 6), "the bar stops at Saturday");
        let three = task(&gantt, "Three");
        assert_eq!(three.end, ymd(2024, 1, 9));
        assert_eq!(three.render_end, three.end);
        assert_eq!(task(&gantt, "Next").start, ymd(2024, 1, 8));
    }

    #[test]
    fn specific_dates_weekdays_and_includes() {
        let source = "gantt
    excludes 2024-01-02, friday
    includes 2024-01-05
    weekend friday
    A :2024-01-01, 3d
";
        let gantt = parse(source).unwrap();
        let calendar = &gantt.calendar;
        assert!(calendar.excludes(ymd(2024, 1, 2)));
        assert!(calendar.excludes(ymd(2024, 1, 12)), "a Friday");
        assert!(!calendar.excludes(ymd(2024, 1, 5)), "included Friday");
        assert_eq!(calendar.weekend_start, 5);
        // 1 Jan + 3d = 4 Jan, plus the excluded 2 Jan.
        assert_eq!(task(&gantt, "A").end, ymd(2024, 1, 5));
    }

    #[test]
    fn a_literal_iso_end_date_is_not_moved() {
        let source = "gantt
    excludes weekends
    A :2024-01-04, 2024-01-08
";
        let gantt = parse(source).unwrap();
        assert_eq!(task(&gantt, "A").end, ymd(2024, 1, 8));
    }

    #[test]
    fn inclusive_end_dates_add_a_day() {
        let gantt = parse("gantt\ninclusiveEndDates\nA :2024-01-01, 2024-01-03\n").unwrap();
        assert_eq!(gantt.tasks[0].end, ymd(2024, 1, 4));
    }

    #[test]
    fn custom_date_formats_apply_to_every_date() {
        let source = "gantt
    A :a, 05.02.2024 08:00, 4h
    dateFormat DD.MM.YYYY HH:mm
";
        let gantt = parse(source).unwrap();
        assert_eq!(gantt.tasks[0].start, ymd(2024, 2, 5) + 8 * HOUR);
        assert_eq!(gantt.tasks[0].end, ymd(2024, 2, 5) + 12 * HOUR);
    }

    #[test]
    fn statements_that_draw_nothing_are_accepted() {
        let source = "gantt
    title Plan
    accTitle: Plan
    accDescr {
      many lines
    }
    todayMarker off
    axisFormat %b %d
    tickInterval 1week
    weekday monday
    topAxis
    A :a, 2024-01-01, 1d
    click a href \"https://example.com\"
";
        let gantt = parse(source).unwrap();
        assert_eq!(gantt.title.as_deref(), Some("Plan"));
        assert_eq!(gantt.tick_interval, Some((1, TickUnit::Week)));
        assert_eq!(gantt.week_start, 1);
        assert!(gantt.top_axis);
        assert_eq!(gantt.tasks.len(), 1);
    }

    #[test]
    fn errors_carry_lines_and_name_the_problem() {
        let cases = [
            ("gantt\nA :after nope, 1d\n", 2, "nope"),
            ("gantt\nA :3d\n", 2, "first task"),
            ("gantt\nA :2024-01-01, soon\n", 2, "soon"),
            ("gantt\nA :tomorrow, 1d\n", 2, "tomorrow"),
            ("gantt\ndateFormat YYYY-ww\n", 2, "ww"),
            ("gantt\naxisFormat %k\n", 2, "%k"),
            ("gantt\nexcludes holidays\n", 2, "holidays"),
            (
                "gantt\nA :a, after b, 1d\nB :b, after a, 1d\n",
                2,
                "depends on itself",
            ),
            ("gantt\njust words\n", 2, "expected a task"),
            ("gantt\nA :a, b, c, d\n", 2, "fields"),
        ];
        for (source, line, needle) in cases {
            let error = parse(source).unwrap_err();
            assert_eq!(error.line, line, "{source:?}: {error}");
            assert!(error.message.contains(needle), "{source:?}: {error}");
        }
    }
}

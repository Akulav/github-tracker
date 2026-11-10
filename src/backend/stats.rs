//! Derived numbers: how fresh a repository is, totals across repositories, and formatting.

use std::collections::HashMap;

use chrono::{DateTime, Datelike, Duration, TimeZone, Utc};

use super::github::{Details, History, Repo};

/// How recently a repository saw a commit — the core of the maintenance view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Freshness {
    Active,
    Recent,
    Cooling,
    Stale,
    Dormant,
    Unknown,
}

impl Freshness {
    pub const ALL: [Freshness; 6] = [Freshness::Active, Freshness::Recent, Freshness::Cooling, Freshness::Stale, Freshness::Dormant, Freshness::Unknown];

    pub fn from_days(days: Option<i64>) -> Self {
        match days {
            None => Freshness::Unknown,
            Some(d) if d <= 7 => Freshness::Active,
            Some(d) if d <= 30 => Freshness::Recent,
            Some(d) if d <= 90 => Freshness::Cooling,
            Some(d) if d <= 365 => Freshness::Stale,
            Some(_) => Freshness::Dormant,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Freshness::Active => "Active",
            Freshness::Recent => "Recent",
            Freshness::Cooling => "Cooling",
            Freshness::Stale => "Stale",
            Freshness::Dormant => "Dormant",
            Freshness::Unknown => "No commits",
        }
    }

    pub fn range(self) -> &'static str {
        match self {
            Freshness::Active => "this week",
            Freshness::Recent => "this month",
            Freshness::Cooling => "1–3 months",
            Freshness::Stale => "3–12 months",
            Freshness::Dormant => "over a year",
            Freshness::Unknown => "empty",
        }
    }

    pub fn index(self) -> usize {
        Freshness::ALL.iter().position(|f| *f == self).unwrap_or(5)
    }
}

/// Date of the last commit on the default branch, else the last push.
pub fn last_activity(repo: &Repo, details: Option<&Details>) -> Option<DateTime<Utc>> {
    details.and_then(|d| d.last_commit.as_ref()).and_then(|c| c.date).or(if details.is_some_and(|d| d.empty) { None } else { repo.pushed_at })
}

pub fn days_since(t: Option<DateTime<Utc>>, now: DateTime<Utc>) -> Option<i64> {
    t.map(|t| (now - t).num_days().max(0))
}

pub fn freshness(repo: &Repo, details: Option<&Details>, now: DateTime<Utc>) -> Freshness {
    Freshness::from_days(days_since(last_activity(repo, details), now))
}

/// How much a repository needs attention, 0–100: mostly time since the last commit, plus open
/// issues and pull requests piling up.
pub fn attention(repo: &Repo, details: Option<&Details>, now: DateTime<Utc>) -> u32 {
    if repo.archived {
        return 0;
    }
    let days = days_since(last_activity(repo, details), now).unwrap_or(0) as f64;
    // 0 at today, ~50 at three months, ~80 at a year, approaching 85 for very old repositories.
    let age = 85.0 * (1.0 - (-days / 240.0).exp());
    let issues = (repo.open_issues as f64).sqrt() * 4.0;
    (age + issues).round().clamp(0.0, 100.0) as u32
}

/// Start (Sunday 00:00 UTC) of the week containing `t`, as GitHub's statistics count weeks.
pub fn week_start(t: DateTime<Utc>) -> i64 {
    let days_from_sunday = t.weekday().num_days_from_sunday() as i64;
    let d = t.date_naive() - Duration::days(days_from_sunday);
    Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).unwrap()).timestamp()
}

pub const WEEKS: usize = 52;
const WEEK_SECS: i64 = 7 * 24 * 3600;

/// Commits per day for the last [`WEEKS`] weeks (oldest first, Sunday first), aligned to `now`.
/// Returns (first week's timestamp, 52×7 day counts).
pub fn daily_grid<'a>(weeks: impl IntoIterator<Item = &'a super::github::Week>, now: DateTime<Utc>) -> (i64, Vec<u32>) {
    let first = week_start(now) - (WEEKS as i64 - 1) * WEEK_SECS;
    let mut grid = vec![0u32; WEEKS * 7];
    for w in weeks {
        // GitHub's week stamps may be off by hours around time zones; round to the nearest week.
        let idx = ((w.week - first) as f64 / WEEK_SECS as f64).round() as i64;
        if (0..WEEKS as i64).contains(&idx) {
            for (d, c) in w.days.iter().enumerate() {
                grid[idx as usize * 7 + d] += c;
            }
        }
    }
    (first, grid)
}

pub fn weekly_from_grid(grid: &[u32]) -> Vec<u32> {
    grid.chunks(7).map(|w| w.iter().sum()).collect()
}

#[derive(Debug, Clone, Default)]
pub struct Totals {
    pub repos: usize,
    pub commits: u64,
    pub lines: u64,
    /// Some line counts are estimates from language bytes.
    pub lines_estimated: bool,
    pub stars: u64,
    pub forks: u64,
    pub open_issues: u64,
    pub private: usize,
    /// Bytes per language across repositories, largest first.
    pub languages: Vec<(String, u64)>,
    pub first_week: i64,
    pub daily: Vec<u32>,
    pub buckets: [usize; 6],
    /// Commits in the last 30 days (from weekly statistics).
    pub commits_30d: u32,
    /// Days in a row (ending today or yesterday) with at least one commit.
    pub streak: u32,
}

pub fn totals<'a>(repos: impl IntoIterator<Item = &'a Repo>, details: &HashMap<u64, Details>, now: DateTime<Utc>) -> Totals {
    let mut t = Totals::default();
    let mut langs: HashMap<String, u64> = HashMap::new();
    let mut weeks = Vec::new();
    for r in repos {
        let d = details.get(&r.id);
        t.repos += 1;
        t.stars += r.stars;
        t.forks += r.forks;
        t.open_issues += r.open_issues;
        t.private += r.private as usize;
        t.buckets[freshness(r, d, now).index()] += 1;
        if let Some(d) = d {
            t.commits += d.commit_count.unwrap_or(0);
            if let Some((n, exact)) = d.lines() {
                t.lines += n;
                t.lines_estimated |= !exact;
            }
            for (l, b) in &d.languages {
                *langs.entry(l.clone()).or_default() += b;
            }
            weeks.extend(d.weekly.iter());
        }
    }
    let mut langs: Vec<(String, u64)> = langs.into_iter().collect();
    langs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    t.languages = langs;
    let (first, daily) = daily_grid(weeks, now);
    t.first_week = first;
    let today = ((now.timestamp() - first) / 86400) as usize;
    let today = today.min(daily.len().saturating_sub(1));
    t.commits_30d = daily[today.saturating_sub(29)..=today].iter().sum();
    // Streak: allow today to be empty (the day isn't over yet).
    let mut i = today as i64;
    if daily.get(i as usize) == Some(&0) {
        i -= 1;
    }
    while i >= 0 && daily[i as usize] > 0 {
        t.streak += 1;
        i -= 1;
    }
    t.daily = daily;
    t
}

/// Total lines after each commit of `history` (same order, newest first), and whether the totals
/// are absolute. A complete history counts up from zero. A truncated one counts back from
/// `current` (GitHub's net line count) when known, else from zero at the oldest commit read.
pub fn line_totals(history: &History, current: Option<u64>) -> (Vec<i64>, bool) {
    let sum: i64 = history.commits.iter().map(|c| c.net()).sum();
    let (end, absolute) = match (history.complete, current) {
        (true, _) => (sum, true),
        (false, Some(n)) => (n as i64, true),
        (false, None) => (sum, false),
    };
    let mut totals = Vec::with_capacity(history.commits.len());
    let mut t = end;
    for c in &history.commits {
        totals.push(t);
        t -= c.net();
    }
    (totals, absolute)
}

/// "just now", "5 minutes ago", "3 days ago", "2 months ago", "4 years ago".
pub fn ago(t: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let s = (now - t).num_seconds().max(0);
    let plural = |n: i64, unit: &str| format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" });
    match s {
        0..60 => "just now".into(),
        60..3600 => plural(s / 60, "minute"),
        3600..86400 => plural(s / 3600, "hour"),
        86400..2_592_000 => plural(s / 86400, "day"),
        2_592_000..31_536_000 => plural(s / 2_592_000, "month"),
        _ => plural(s / 31_536_000, "year"),
    }
}

/// Compact age: "3d", "5w", "8mo", "2y".
pub fn short_age(days: i64) -> String {
    match days {
        0 => "today".into(),
        1..14 => format!("{days}d"),
        14..60 => format!("{}w", days / 7),
        60..365 => format!("{}mo", days / 30),
        _ => {
            let y = days as f64 / 365.0;
            if y < 10.0 {
                format!("{:.1}y", y).replace(".0y", "y")
            } else {
                format!("{}y", y as i64)
            }
        }
    }
}

/// 950, 1.2k, 12k, 3.4M.
pub fn compact(n: u64) -> String {
    match n {
        0..1000 => n.to_string(),
        1000..10_000 => format!("{:.1}k", n as f64 / 1000.0).replace(".0k", "k"),
        10_000..1_000_000 => format!("{}k", n / 1000),
        1_000_000..10_000_000 => format!("{:.1}M", n as f64 / 1e6).replace(".0M", "M"),
        _ => format!("{}M", n / 1_000_000),
    }
}

/// 1234567 → "1,234,567".
pub fn grouped(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::github::Week;

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn freshness_buckets() {
        assert_eq!(Freshness::from_days(Some(0)), Freshness::Active);
        assert_eq!(Freshness::from_days(Some(8)), Freshness::Recent);
        assert_eq!(Freshness::from_days(Some(45)), Freshness::Cooling);
        assert_eq!(Freshness::from_days(Some(200)), Freshness::Stale);
        assert_eq!(Freshness::from_days(Some(400)), Freshness::Dormant);
        assert_eq!(Freshness::from_days(None), Freshness::Unknown);
    }

    #[test]
    fn weeks_start_on_sunday() {
        // 2026-11-09 is a Monday.
        assert_eq!(week_start(at("2026-11-09T15:00:00Z")), at("2026-11-08T00:00:00Z").timestamp());
        assert_eq!(week_start(at("2026-11-08T00:00:00Z")), at("2026-11-08T00:00:00Z").timestamp());
    }

    #[test]
    fn grid_aligns_weeks_and_streaks() {
        let now = at("2026-11-10T12:00:00Z"); // Tuesday
        let this_week = week_start(now);
        let weeks = [
            Week { week: this_week, total: 3, days: [1, 1, 1, 0, 0, 0, 0] },
            Week { week: this_week - 7 * 86400, total: 2, days: [0, 0, 0, 0, 0, 1, 1] },
            Week { week: this_week - 60 * 7 * 86400, total: 9, days: [9, 0, 0, 0, 0, 0, 0] }, // too old
        ];
        let (first, grid) = daily_grid(weeks.iter(), now);
        assert_eq!(first, this_week - 51 * 7 * 86400);
        assert_eq!(grid.len(), 364);
        assert_eq!(grid.iter().sum::<u32>(), 5);
        assert_eq!(weekly_from_grid(&grid)[51], 3);

        let repo: Repo = serde_json::from_str(
            r#"{"id":1,"name":"x","full_name":"a/x","owner":"a","owner_avatar":"","private":false,"fork":false,"archived":false,
            "description":null,"html_url":"","clone_url":"","language":null,"stars":2,"forks":0,"open_issues":0,"size_kb":0,
            "default_branch":"main","pushed_at":"2026-11-10T00:00:00Z","created_at":null,"topics":[]}"#,
        )
        .unwrap();
        let mut details = HashMap::new();
        details.insert(1, Details { weekly: weeks.to_vec(), commit_count: Some(10), ..Default::default() });
        let t = totals([&repo], &details, now);
        // Sun, Mon, Tue of this week + Fri, Sat of last week = 5 days in a row.
        assert_eq!(t.streak, 5);
        assert_eq!(t.commits, 10);
        assert_eq!(t.buckets[Freshness::Active.index()], 1);
    }

    #[test]
    fn attention_grows_with_age() {
        let now = at("2026-11-10T00:00:00Z");
        let mut r: Repo = serde_json::from_str(
            r#"{"id":1,"name":"x","full_name":"a/x","owner":"a","owner_avatar":"","private":false,"fork":false,"archived":false,
            "description":null,"html_url":"","clone_url":"","language":null,"stars":0,"forks":0,"open_issues":0,"size_kb":0,
            "default_branch":"main","pushed_at":"2026-11-09T00:00:00Z","created_at":null,"topics":[]}"#,
        )
        .unwrap();
        let fresh = attention(&r, None, now);
        r.pushed_at = Some(at("2024-01-01T00:00:00Z"));
        let old = attention(&r, None, now);
        assert!(fresh < 5 && old > 75, "{fresh} {old}");
        r.archived = true;
        assert_eq!(attention(&r, None, now), 0);
    }

    #[test]
    fn line_totals_per_commit() {
        use crate::backend::github::CommitStat;
        let c = |add: u64, del: u64, merge: bool| CommitStat { sha: String::new(), title: String::new(), author: String::new(), date: None, additions: add, deletions: del, merge, url: String::new() };
        // Newest first: +10/-4, merge (ignored), +100.
        let mut h = History { commits: vec![c(10, 4, false), c(999, 0, true), c(100, 0, false)], complete: true, seen_push: None };
        assert_eq!(line_totals(&h, Some(5000)), (vec![106, 100, 100], true));
        h.complete = false;
        assert_eq!(line_totals(&h, Some(5000)), (vec![5000, 4994, 4994], true));
        assert_eq!(line_totals(&h, None), (vec![106, 100, 100], false));
    }

    #[test]
    fn formatting() {
        assert_eq!(compact(950), "950");
        assert_eq!(compact(1000), "1k");
        assert_eq!(compact(1250), "1.2k");
        assert_eq!(compact(45_000), "45k");
        assert_eq!(compact(3_400_000), "3.4M");
        assert_eq!(grouped(1234567), "1,234,567");
        assert_eq!(grouped(12), "12");
        assert_eq!(short_age(0), "today");
        assert_eq!(short_age(3), "3d");
        assert_eq!(short_age(21), "3w");
        assert_eq!(short_age(90), "3mo");
        assert_eq!(short_age(730), "2y");
        assert_eq!(short_age(550), "1.5y");
        let now = at("2026-11-10T00:00:00Z");
        assert_eq!(ago(at("2026-11-09T23:59:30Z"), now), "just now");
        assert_eq!(ago(at("2026-11-07T00:00:00Z"), now), "3 days ago");
        assert_eq!(ago(at("2025-11-01T00:00:00Z"), now), "1 year ago");
        assert_eq!(bytes(1536), "1.5 KB");
    }
}

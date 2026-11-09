//! Per-repository numbers the pages draw from, computed once per frame.

use chrono::{DateTime, Utc};

use super::app::App;
use crate::backend::github::Repo;
use crate::backend::stats::{self, Freshness};

#[derive(Clone)]
pub struct RepoInfo {
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub owner: String,
    pub owned: bool,
    pub description: Option<String>,
    pub language: Option<String>,
    pub private: bool,
    pub fork: bool,
    pub archived: bool,
    pub stars: u64,
    pub open_issues: u64,
    pub last: Option<DateTime<Utc>>,
    pub days: Option<i64>,
    pub fresh: Freshness,
    pub commits: Option<u64>,
    pub lines: Option<(u64, bool)>,
    /// Commits per week, last 26 weeks (for sparklines).
    pub recent_weeks: Vec<u32>,
    pub commits_4w: u32,
    pub attention: u32,
    pub loaded: bool,
}

impl App {
    pub fn info(&self, r: &Repo, now: DateTime<Utc>) -> RepoInfo {
        let d = self.details.get(&r.id);
        let last = stats::last_activity(r, d);
        let days = stats::days_since(last, now);
        let (_, grid) = stats::daily_grid(d.map(|d| d.weekly.as_slice()).unwrap_or(&[]), now);
        let weekly = stats::weekly_from_grid(&grid);
        RepoInfo {
            id: r.id,
            name: r.name.clone(),
            full_name: r.full_name.clone(),
            owner: r.owner.clone(),
            owned: self.is_owned(r),
            description: r.description.clone(),
            language: r.language.clone(),
            private: r.private,
            fork: r.fork,
            archived: r.archived,
            stars: r.stars,
            open_issues: r.open_issues,
            last,
            days,
            fresh: Freshness::from_days(days),
            commits: d.and_then(|d| d.commit_count),
            lines: d.and_then(|d| d.lines()),
            commits_4w: weekly[weekly.len() - 4..].iter().sum(),
            recent_weeks: weekly[weekly.len() - 26..].to_vec(),
            attention: stats::attention(r, d, now),
            loaded: d.is_some(),
        }
    }

    pub fn infos<'a>(&self, repos: impl IntoIterator<Item = &'a Repo>, now: DateTime<Utc>) -> Vec<RepoInfo> {
        repos.into_iter().map(|r| self.info(r, now)).collect()
    }
}

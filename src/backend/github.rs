//! GitHub REST API: the signed-in user, their repositories and per-repository statistics.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use reqwest::header::{HeaderMap, ACCEPT, LINK};
use serde::{Deserialize, Serialize};

const API: &str = "https://api.github.com";

// ---------------------------------------------------------------------------------------------
// Models (also what the cache stores)
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct User {
    pub login: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub avatar_url: String,
    #[serde(default)]
    pub html_url: String,
    #[serde(default)]
    pub public_repos: u64,
    #[serde(default)]
    pub followers: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repo {
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub owner: String,
    pub owner_avatar: String,
    pub private: bool,
    pub fork: bool,
    pub archived: bool,
    pub description: Option<String>,
    pub html_url: String,
    pub clone_url: String,
    pub language: Option<String>,
    pub stars: u64,
    pub forks: u64,
    pub open_issues: u64,
    /// Repository size in KB, as GitHub reports it.
    pub size_kb: u64,
    pub default_branch: String,
    pub pushed_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub topics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Commit {
    pub sha: String,
    /// First line of the message.
    pub title: String,
    pub author: String,
    pub date: Option<DateTime<Utc>>,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Week {
    /// Unix timestamp of the week's Sunday.
    pub week: i64,
    pub total: u32,
    pub days: [u32; 7],
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Details {
    pub fetched_at: Option<DateTime<Utc>>,
    /// `pushed_at` of the repository when these details were fetched; a change means refetch.
    pub seen_push: Option<DateTime<Utc>>,
    pub empty: bool,
    pub commit_count: Option<u64>,
    pub last_commit: Option<Commit>,
    /// Bytes per language, largest first.
    pub languages: Vec<(String, u64)>,
    /// Last 52 weeks of commits on the default branch.
    pub weekly: Vec<Week>,
    /// (week, additions, deletions) for the whole history.
    pub code_frequency: Vec<(i64, i64, i64)>,
    /// GitHub is still computing statistics (HTTP 202); ask again shortly.
    pub stats_pending: bool,
    /// GitHub doesn't compute statistics for this repository (very large history).
    pub stats_unavailable: bool,
}

impl Details {
    /// Net lines (additions − deletions over the whole history) when known.
    pub fn net_lines(&self) -> Option<u64> {
        if self.code_frequency.is_empty() {
            return None;
        }
        let net: i64 = self.code_frequency.iter().map(|(_, a, d)| a + d).sum();
        Some(net.max(0) as u64)
    }

    /// Lines of code: net lines from the history, or an estimate from language bytes.
    pub fn lines(&self) -> Option<(u64, bool)> {
        if let Some(n) = self.net_lines().filter(|n| *n > 0) {
            return Some((n, true));
        }
        let bytes: u64 = self.languages.iter().map(|(_, b)| b).sum();
        (bytes > 0).then(|| (bytes / BYTES_PER_LINE, false))
    }
}

/// Average bytes per source line, used when GitHub has no line statistics.
pub const BYTES_PER_LINE: u64 = 38;

// ---------------------------------------------------------------------------------------------
// Wire formats
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct WireOwner {
    login: String,
    #[serde(default)]
    avatar_url: String,
}

#[derive(Deserialize)]
struct WireRepo {
    id: u64,
    name: String,
    full_name: String,
    owner: WireOwner,
    #[serde(default)]
    private: bool,
    #[serde(default)]
    fork: bool,
    #[serde(default)]
    archived: bool,
    description: Option<String>,
    html_url: String,
    #[serde(default)]
    clone_url: String,
    language: Option<String>,
    #[serde(default)]
    stargazers_count: u64,
    #[serde(default)]
    forks_count: u64,
    #[serde(default)]
    open_issues_count: u64,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    default_branch: String,
    pushed_at: Option<DateTime<Utc>>,
    created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    topics: Vec<String>,
}

impl From<WireRepo> for Repo {
    fn from(w: WireRepo) -> Self {
        Repo {
            id: w.id,
            name: w.name,
            full_name: w.full_name,
            owner: w.owner.login,
            owner_avatar: w.owner.avatar_url,
            private: w.private,
            fork: w.fork,
            archived: w.archived,
            description: w.description.filter(|d| !d.trim().is_empty()),
            html_url: w.html_url,
            clone_url: w.clone_url,
            language: w.language,
            stars: w.stargazers_count,
            forks: w.forks_count,
            open_issues: w.open_issues_count,
            size_kb: w.size,
            default_branch: if w.default_branch.is_empty() { "main".into() } else { w.default_branch },
            pushed_at: w.pushed_at,
            created_at: w.created_at,
            topics: w.topics,
        }
    }
}

#[derive(Deserialize)]
struct WireSig {
    #[serde(default)]
    name: String,
    date: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct WireCommitInner {
    message: String,
    author: Option<WireSig>,
    committer: Option<WireSig>,
}

#[derive(Deserialize)]
struct WireLogin {
    login: String,
}

#[derive(Deserialize)]
struct WireCommit {
    sha: String,
    commit: WireCommitInner,
    author: Option<WireLogin>,
    #[serde(default)]
    html_url: String,
}

impl From<WireCommit> for Commit {
    fn from(w: WireCommit) -> Self {
        let date = w.commit.committer.as_ref().and_then(|c| c.date).or_else(|| w.commit.author.as_ref().and_then(|a| a.date));
        let author = w.author.map(|a| a.login).or_else(|| w.commit.author.map(|a| a.name)).unwrap_or_default();
        Commit { sha: w.sha, title: w.commit.message.lines().next().unwrap_or("").trim().to_string(), author, date, url: w.html_url }
    }
}

// ---------------------------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------------------------

/// Rate limit as last reported by GitHub (-1 = unknown).
#[derive(Debug)]
pub struct RateLimit {
    pub remaining: AtomicI64,
    pub limit: AtomicI64,
    pub reset: AtomicI64,
}

impl Default for RateLimit {
    fn default() -> Self {
        Self { remaining: AtomicI64::new(-1), limit: AtomicI64::new(-1), reset: AtomicI64::new(-1) }
    }
}

impl RateLimit {
    fn observe(&self, h: &HeaderMap) {
        let num = |k: &str| h.get(k).and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<i64>().ok());
        if let Some(v) = num("x-ratelimit-remaining") {
            self.remaining.store(v, Ordering::Relaxed);
        }
        if let Some(v) = num("x-ratelimit-limit") {
            self.limit.store(v, Ordering::Relaxed);
        }
        if let Some(v) = num("x-ratelimit-reset") {
            self.reset.store(v, Ordering::Relaxed);
        }
    }
}

pub enum Fetched<T> {
    Ok(T, Option<String>),
    /// 202: GitHub is computing it.
    Pending,
    /// 204/409: empty repository; 422: not computed for huge repositories.
    Empty,
    NotFound,
}

pub struct Api {
    http: reqwest::Client,
    token: String,
    pub rate: Arc<RateLimit>,
}

impl Api {
    pub fn new(http: reqwest::Client, token: String) -> Self {
        Self { http, token, rate: Arc::new(RateLimit::default()) }
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<Fetched<T>> {
        let mut attempt = 0;
        let resp = loop {
            let r = self
                .http
                .get(url)
                .bearer_auth(&self.token)
                .header(ACCEPT, "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .send()
                .await;
            match r {
                Ok(r) if r.status().is_server_error() && attempt < 2 => {}
                Ok(r) => break r,
                Err(e) if attempt >= 2 => return Err(e).with_context(|| format!("GET {url}")),
                Err(_) => {}
            }
            attempt += 1;
            tokio::time::sleep(std::time::Duration::from_millis(600 * attempt)).await;
        };
        self.rate.observe(resp.headers());
        let status = resp.status().as_u16();
        match status {
            202 => return Ok(Fetched::Pending),
            204 | 409 | 422 => return Ok(Fetched::Empty),
            404 => return Ok(Fetched::NotFound),
            401 => bail!("GitHub rejected the token (401). Sign in again."),
            403 | 429 if self.rate.remaining.load(Ordering::Relaxed) == 0 => {
                let reset = self.rate.reset.load(Ordering::Relaxed);
                let at = DateTime::<Utc>::from_timestamp(reset, 0).map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string());
                bail!("GitHub rate limit reached; it resets at {}", at.unwrap_or_else(|| "the top of the hour".into()))
            }
            _ => {}
        }
        let link = resp.headers().get(LINK).and_then(|v| v.to_str().ok()).map(str::to_string);
        let ok = resp.status().is_success();
        let text = resp.text().await.with_context(|| format!("GET {url}"))?;
        if !ok {
            let msg = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(str::to_string))
                .unwrap_or(text);
            bail!("GitHub returned {status} for {url}: {msg}");
        }
        let value = serde_json::from_str(&text).with_context(|| format!("unexpected answer from {url}"))?;
        Ok(Fetched::Ok(value, link))
    }

    pub async fn user(&self) -> Result<User> {
        match self.get::<User>(&format!("{API}/user")).await? {
            Fetched::Ok(u, _) => Ok(u),
            _ => bail!("could not read your GitHub profile"),
        }
    }

    /// Every repository the user can see as owner, collaborator or organization member.
    pub async fn repos(&self) -> Result<Vec<Repo>> {
        let mut out = Vec::new();
        let mut url = Some(format!("{API}/user/repos?per_page=100&sort=pushed&affiliation=owner,collaborator,organization_member"));
        while let Some(u) = url.take() {
            match self.get::<Vec<WireRepo>>(&u).await? {
                Fetched::Ok(page, link) => {
                    out.extend(page.into_iter().map(Repo::from));
                    url = link.as_deref().and_then(|l| link_rel(l, "next"));
                }
                _ => break,
            }
            if out.len() > 5000 {
                break;
            }
        }
        Ok(out)
    }

    pub async fn recent_commits(&self, repo: &Repo, n: usize) -> Result<Vec<Commit>> {
        let url = format!("{API}/repos/{}/commits?per_page={n}&sha={}", repo.full_name, repo.default_branch);
        Ok(match self.get::<Vec<WireCommit>>(&url).await? {
            Fetched::Ok(list, _) => list.into_iter().map(Commit::from).collect(),
            _ => Vec::new(),
        })
    }

    /// Commit count, last commit, languages and weekly statistics. `prev` keeps statistics that
    /// GitHub is still computing.
    pub async fn details(&self, repo: &Repo, prev: Option<&Details>) -> Result<Details> {
        let base = format!("{API}/repos/{}", repo.full_name);
        let commits_url = format!("{base}/commits?per_page=1&sha={}", repo.default_branch);
        let (langs_url, activity_url, freq_url) = (format!("{base}/languages"), format!("{base}/stats/commit_activity"), format!("{base}/stats/code_frequency"));
        let (commits, languages, activity, freq) = tokio::join!(
            self.get::<Vec<WireCommit>>(&commits_url),
            self.get::<BTreeMap<String, u64>>(&langs_url),
            self.get::<Vec<Week>>(&activity_url),
            self.get::<Vec<(i64, i64, i64)>>(&freq_url),
        );

        let mut d = Details { fetched_at: Some(Utc::now()), seen_push: repo.pushed_at, ..Default::default() };
        match commits? {
            Fetched::Ok(list, link) => {
                let count = link.as_deref().and_then(|l| link_rel(l, "last")).and_then(|u| page_param(&u)).unwrap_or(list.len() as u64);
                d.commit_count = Some(count);
                d.last_commit = list.into_iter().next().map(Commit::from);
            }
            Fetched::Empty => {
                d.empty = true;
                d.commit_count = Some(0);
            }
            _ => {}
        }
        if let Fetched::Ok(map, _) = languages? {
            let mut v: Vec<(String, u64)> = map.into_iter().collect();
            v.sort_by(|a, b| b.1.cmp(&a.1));
            d.languages = v;
        }
        match activity? {
            Fetched::Ok(w, _) => d.weekly = w,
            Fetched::Pending => {
                d.stats_pending = true;
                d.weekly = prev.map(|p| p.weekly.clone()).unwrap_or_default();
            }
            Fetched::Empty => d.stats_unavailable = !d.empty,
            Fetched::NotFound => {}
        }
        match freq? {
            Fetched::Ok(f, _) => d.code_frequency = f,
            Fetched::Pending => {
                d.stats_pending = true;
                d.code_frequency = prev.map(|p| p.code_frequency.clone()).unwrap_or_default();
            }
            Fetched::Empty => d.stats_unavailable |= !d.empty,
            Fetched::NotFound => {}
        }
        Ok(d)
    }
}

/// URL of `rel` in a `Link` header.
pub fn link_rel(link: &str, rel: &str) -> Option<String> {
    link.split(',').find_map(|part| {
        let mut it = part.split(';');
        let url = it.next()?.trim().trim_start_matches('<').trim_end_matches('>');
        it.any(|p| p.trim() == format!("rel=\"{rel}\"")).then(|| url.to_string())
    })
}

/// The `page` query parameter of a URL (not `per_page`).
pub fn page_param(url: &str) -> Option<u64> {
    let query = url.split_once('?')?.1;
    query.split('&').find_map(|kv| kv.split_once('=').filter(|(k, _)| *k == "page").and_then(|(_, v)| v.parse().ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_header() {
        let l = r#"<https://api.github.com/repositories/1/commits?per_page=1&sha=main&page=2>; rel="next", <https://api.github.com/repositories/1/commits?per_page=1&sha=main&page=1234>; rel="last""#;
        assert_eq!(link_rel(l, "next").as_deref(), Some("https://api.github.com/repositories/1/commits?per_page=1&sha=main&page=2"));
        assert_eq!(link_rel(l, "last").and_then(|u| page_param(&u)), Some(1234));
        assert_eq!(link_rel(l, "prev"), None);
        assert_eq!(page_param("https://x/y?per_page=1"), None);
    }

    #[test]
    fn parses_repo_and_commit() {
        let r: WireRepo = serde_json::from_str(
            r#"{"id":7,"name":"x","full_name":"a/x","owner":{"login":"a","avatar_url":"u"},"private":true,"html_url":"h",
                "description":"  ","language":"Rust","stargazers_count":3,"default_branch":"dev","pushed_at":"2026-01-02T03:04:05Z"}"#,
        )
        .unwrap();
        let r = Repo::from(r);
        assert_eq!((r.owner.as_str(), r.default_branch.as_str(), r.stars), ("a", "dev", 3));
        assert!(r.description.is_none() && r.private && r.pushed_at.is_some());

        let c: WireCommit = serde_json::from_str(
            r#"{"sha":"abc","html_url":"h","author":null,"commit":{"message":"Fix it\n\nbody","author":{"name":"Ann","date":"2026-01-01T00:00:00Z"},"committer":{"name":"B","date":"2026-01-02T00:00:00Z"}}}"#,
        )
        .unwrap();
        let c = Commit::from(c);
        assert_eq!((c.title.as_str(), c.author.as_str()), ("Fix it", "Ann"));
        assert_eq!(c.date.unwrap().to_rfc3339(), "2026-01-02T00:00:00+00:00");
    }

    #[test]
    fn lines_of_code() {
        let mut d = Details { code_frequency: vec![(0, 100, -20), (1, 50, -30)], ..Default::default() };
        assert_eq!(d.lines(), Some((100, true)));
        d.code_frequency.clear();
        d.languages = vec![("Rust".into(), 3800)];
        assert_eq!(d.lines(), Some((100, false)));
        d.languages.clear();
        assert_eq!(d.lines(), None);
    }
}

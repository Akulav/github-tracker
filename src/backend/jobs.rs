//! Background work. Every job runs on the tokio runtime and reports back to the UI with [`Event`]s.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use futures::StreamExt;
use tokio::runtime::Handle;

use super::auth::{self, DeviceCode};
use super::github::{Api, Commit, Details, Repo, User};
use super::install;
use super::update::{self, UpdateInfo};

pub enum Event {
    // Sign-in
    /// A signed-in GitHub CLI was found; offered as a one-click sign-in.
    GhCliToken(String),
    DeviceCode(DeviceCode),
    SignedIn { token: String, user: User },
    SignInFailed(String),
    // Sync
    Repos { session: u64, repos: Vec<Repo> },
    Details { session: u64, id: u64, details: Details },
    SyncProgress { session: u64, done: usize, total: usize },
    SyncDone { session: u64, errors: Vec<String> },
    SyncFailed { session: u64, error: String },
    Commits { id: u64, result: Result<Vec<Commit>, String> },
    Avatar { url: String, bytes: Option<Vec<u8>> },
    // Updates
    UpdateChecked { manual: bool, result: Result<Option<UpdateInfo>, String> },
    UpdateProgress { done: u64, total: u64 },
    UpdateReady(PathBuf),
    UpdateFailed(String),
}

/// Sends events to the UI and wakes it up.
#[derive(Clone)]
pub struct Bus {
    tx: Sender<Event>,
    ctx: egui::Context,
}

impl Bus {
    pub fn new(tx: Sender<Event>, ctx: egui::Context) -> Self {
        Self { tx, ctx }
    }
    pub fn send(&self, e: Event) {
        let _ = self.tx.send(e);
        self.ctx.request_repaint();
    }
}

// ---------------------------------------------------------------------------------------------
// Sign-in
// ---------------------------------------------------------------------------------------------

/// Validate `token` by reading the profile.
pub fn sign_in_with_token(rt: &Handle, bus: Bus, http: reqwest::Client, token: String) {
    rt.spawn(async move {
        let api = Api::new(http, token.clone());
        match api.user().await {
            Ok(user) => bus.send(Event::SignedIn { token, user }),
            Err(e) => bus.send(Event::SignInFailed(format!("{e:#}"))),
        }
    });
}

pub fn sign_in_with_device(rt: &Handle, bus: Bus, http: reqwest::Client, client_id: String, cancel: Arc<AtomicBool>) {
    rt.spawn(async move {
        let dc = match auth::start_device_flow(&http, &client_id).await {
            Ok(dc) => dc,
            Err(e) => return bus.send(Event::SignInFailed(format!("{e:#}"))),
        };
        bus.send(Event::DeviceCode(dc.clone()));
        match auth::poll_device_flow(&http, &client_id, &dc, &cancel).await {
            Ok(token) => {
                let api = Api::new(http, token.clone());
                match api.user().await {
                    Ok(user) => bus.send(Event::SignedIn { token, user }),
                    Err(e) => bus.send(Event::SignInFailed(format!("{e:#}"))),
                }
            }
            Err(e) if cancel.load(Ordering::Relaxed) => drop(e),
            Err(e) => bus.send(Event::SignInFailed(format!("{e:#}"))),
        }
    });
}

// ---------------------------------------------------------------------------------------------
// Sync
// ---------------------------------------------------------------------------------------------

/// Whether a repository's details must be fetched again.
pub fn needs_refresh(repo: &Repo, prev: Option<&Details>, force: bool) -> bool {
    let Some(prev) = prev else { return true };
    if force || prev.stats_pending {
        return true;
    }
    let age = prev.fetched_at.map(|t| Utc::now() - t).unwrap_or(chrono::Duration::days(999));
    // Something was pushed since, or weekly numbers have shifted by a day.
    prev.seen_push != repo.pushed_at || age > chrono::Duration::hours(20)
}

const CONCURRENCY: usize = 6;
/// Rounds of re-asking for statistics GitHub was still computing.
const PENDING_ROUNDS: u32 = 4;

pub fn sync(rt: &Handle, bus: Bus, api: Arc<Api>, session: u64, cached: HashMap<u64, Details>, force: bool, cancel: Arc<AtomicBool>) {
    rt.spawn(async move {
        let repos = match api.repos().await {
            Ok(r) => r,
            Err(e) => return bus.send(Event::SyncFailed { session, error: format!("{e:#}") }),
        };
        bus.send(Event::Repos { session, repos: repos.clone() });

        let mut todo: Vec<Repo> = repos.into_iter().filter(|r| needs_refresh(r, cached.get(&r.id), force)).collect();
        let total = todo.len();
        let done = AtomicUsize::new(0);
        let mut errors = Vec::new();
        bus.send(Event::SyncProgress { session, done: 0, total });
        let mut prev = cached;

        for round in 0..=PENDING_ROUNDS {
            if todo.is_empty() || cancel.load(Ordering::Relaxed) {
                break;
            }
            if round > 0 {
                tokio::time::sleep(Duration::from_secs(3 + round as u64 * 2)).await;
            }
            let results: Vec<(Repo, anyhow::Result<Details>)> = futures::stream::iter(todo.drain(..))
                .map(|repo| {
                    let api = api.clone();
                    let p = prev.get(&repo.id).cloned();
                    let (bus, done, cancel) = (&bus, &done, &cancel);
                    async move {
                        if cancel.load(Ordering::Relaxed) {
                            return (repo, Err(anyhow::anyhow!("cancelled")));
                        }
                        let r = api.details(&repo, p.as_ref()).await;
                        if round == 0 {
                            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                            bus.send(Event::SyncProgress { session, done: n, total });
                        }
                        (repo, r)
                    }
                })
                .buffer_unordered(CONCURRENCY)
                .collect()
                .await;
            for (repo, r) in results {
                match r {
                    Ok(d) => {
                        if d.stats_pending && round < PENDING_ROUNDS {
                            todo.push(repo.clone());
                        }
                        prev.insert(repo.id, d.clone());
                        bus.send(Event::Details { session, id: repo.id, details: d });
                    }
                    Err(e) => {
                        let msg = format!("{e:#}");
                        if msg.contains("rate limit") || msg.contains("401") {
                            return bus.send(Event::SyncFailed { session, error: msg });
                        }
                        if msg != "cancelled" {
                            errors.push(format!("{}: {msg}", repo.full_name));
                        }
                    }
                }
            }
        }
        bus.send(Event::SyncDone { session, errors });
    });
}

pub fn recent_commits(rt: &Handle, bus: Bus, api: Arc<Api>, repo: Repo) {
    rt.spawn(async move {
        let result = api.recent_commits(&repo, 30).await.map_err(|e| format!("{e:#}"));
        bus.send(Event::Commits { id: repo.id, result });
    });
}

/// Avatars are cached in `<data>/avatars` by a hash of their URL.
pub fn avatar(rt: &Handle, bus: Bus, http: reqwest::Client, url: String, dir: PathBuf) {
    rt.spawn(async move {
        use sha2::Digest;
        let file = dir.join(format!("{}.img", &hex::encode(sha2::Sha256::digest(url.as_bytes()))[..24]));
        if let Ok(b) = tokio::fs::read(&file).await {
            return bus.send(Event::Avatar { url, bytes: Some(b) });
        }
        let sized = if url.contains('?') { format!("{url}&s=96") } else { format!("{url}?s=96") };
        let bytes = match http.get(&sized).send().await.and_then(|r| r.error_for_status()) {
            Ok(r) => r.bytes().await.ok().map(|b| b.to_vec()),
            Err(_) => None,
        };
        if let Some(b) = &bytes {
            let _ = tokio::fs::create_dir_all(&dir).await;
            let _ = tokio::fs::write(&file, b).await;
        }
        bus.send(Event::Avatar { url, bytes });
    });
}

// ---------------------------------------------------------------------------------------------
// Updates
// ---------------------------------------------------------------------------------------------

pub fn check_update(rt: &Handle, bus: Bus, http: reqwest::Client, manual: bool) {
    rt.spawn(async move {
        let result = update::check(&http).await.map_err(|e| format!("{e:#}"));
        bus.send(Event::UpdateChecked { manual, result });
    });
}

pub fn download_update(rt: &Handle, bus: Bus, http: reqwest::Client, info: UpdateInfo) {
    rt.spawn(async move {
        let cancel = AtomicBool::new(false);
        let last = std::sync::Mutex::new(std::time::Instant::now());
        let progress = |done: u64, total: u64| {
            let mut l = last.lock().unwrap();
            if l.elapsed() > Duration::from_millis(100) {
                *l = std::time::Instant::now();
                bus.send(Event::UpdateProgress { done, total });
            }
        };
        match install::download_update(&http, &info, &progress, &cancel).await {
            Ok(path) => bus.send(Event::UpdateReady(path)),
            Err(e) => bus.send(Event::UpdateFailed(format!("{e:#}"))),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_rules() {
        let now = Utc::now();
        let repo: Repo = serde_json::from_value(serde_json::json!({
            "id":1,"name":"x","full_name":"a/x","owner":"a","owner_avatar":"","private":false,"fork":false,"archived":false,
            "description":null,"html_url":"","clone_url":"","language":null,"stars":0,"forks":0,"open_issues":0,"size_kb":0,
            "default_branch":"main","pushed_at": now.to_rfc3339(),"created_at":null,"topics":[]
        }))
        .unwrap();
        assert!(needs_refresh(&repo, None, false));
        let fresh = Details { fetched_at: Some(now), seen_push: repo.pushed_at, ..Default::default() };
        assert!(!needs_refresh(&repo, Some(&fresh), false));
        assert!(needs_refresh(&repo, Some(&fresh), true));
        let pushed = Details { seen_push: None, ..fresh.clone() };
        assert!(needs_refresh(&repo, Some(&pushed), false));
        let pending = Details { stats_pending: true, ..fresh };
        assert!(needs_refresh(&repo, Some(&pending), false));
    }
}

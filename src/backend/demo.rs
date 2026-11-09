//! `--demo`: made-up repositories for trying the app (and taking screenshots) without signing in.

use chrono::{DateTime, Duration, Utc};

use super::cache::Cache;
use super::github::{Commit, Details, Repo, User, Week};
use super::stats;

pub fn enabled() -> bool {
    std::env::var_os("AKULAV_DEMO").is_some()
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.next() % (hi - lo).max(1)
    }
    fn f(&mut self) -> f64 {
        (self.next() % 10_000) as f64 / 10_000.0
    }
}

const REPOS: &[(&str, &str, &str, u64, bool, bool, bool)] = &[
    // name, language, description, days since last commit, private, fork, archived
    ("github-tracker", "Rust", "All your GitHub repositories at a glance", 0, false, false, false),
    ("akulav-mc-launcher", "Rust", "A lightweight Minecraft modpack launcher", 2, false, false, false),
    ("dotfiles", "Shell", "My shell, editor and terminal setup", 5, false, false, false),
    ("portfolio-site", "TypeScript", "Personal website built with Astro", 11, false, false, false),
    ("modpack-server", "Java", "Server-side helpers for the modpacks", 19, true, false, false),
    ("photo-sorter", "Python", "Sorts photos into folders by EXIF date", 27, false, false, false),
    ("budget-app", "C#", "Household budgeting for Windows", 41, true, false, false),
    ("shader-playground", "GLSL", "Little shader experiments", 64, false, false, false),
    ("discord-bot", "TypeScript", "Moderation and music bot", 83, true, false, false),
    ("rust-raytracer", "Rust", "Ray tracing in one weekend, in Rust", 120, false, false, false),
    ("inventory-api", "Go", "REST API for the warehouse inventory", 160, true, false, false),
    ("advent-of-code", "Rust", "Solutions, one folder per year", 340, false, false, false),
    ("home-automation", "Python", "Scripts for lights, heating and sensors", 210, true, false, false),
    ("egui", "Rust", "Fork with a few patches", 290, false, true, false),
    ("chess-engine", "C++", "UCI chess engine with alpha-beta search", 430, false, false, false),
    ("old-blog", "HTML", "The blog before the portfolio site", 900, false, false, true),
    ("android-notes", "Kotlin", "Simple notes app", 610, false, false, false),
    ("cli-weather", "Rust", "Weather in your terminal", 75, false, false, false),
    ("game-jam-2024", "C#", "Unity entry for the spring jam", 520, false, false, false),
    ("installer-scripts", "Inno Setup", "Shared Inno Setup snippets", 33, false, false, false),
];

fn weeks_for(rng: &mut Rng, last: DateTime<Utc>, now: DateTime<Utc>, intensity: f64) -> Vec<Week> {
    let first = stats::week_start(now) - (stats::WEEKS as i64 - 1) * 604800;
    (0..stats::WEEKS)
        .map(|w| {
            let start = first + w as i64 * 604800;
            let mut days = [0u32; 7];
            for (d, slot) in days.iter_mut().enumerate() {
                let t = start + d as i64 * 86400;
                if t > last.timestamp() || t > now.timestamp() {
                    continue;
                }
                // More activity closer to the last commit.
                let age_w = (last.timestamp() - t) as f64 / 604800.0;
                let p = intensity * (-age_w / 14.0).exp() + 0.04 * intensity;
                if rng.f() < p.min(0.9) {
                    *slot = rng.range(1, (2.0 + intensity * 7.0) as u64) as u32;
                }
            }
            // The last commit day itself.
            if (start..start + 7 * 86400).contains(&last.timestamp()) {
                let d = ((last.timestamp() - start) / 86400) as usize;
                days[d.min(6)] = days[d.min(6)].max(1);
            }
            Week { week: start, total: days.iter().sum(), days }
        })
        .collect()
}

pub fn cache(now: DateTime<Utc>) -> Cache {
    let mut rng = Rng(0x5eed_a1u64);
    let user = User { login: "akulav".into(), name: Some("Akulav".into()), avatar_url: String::new(), html_url: "https://github.com/Akulav".into(), public_repos: 16, followers: 42 };
    let mut c = Cache { user, synced_at: Some(now - Duration::minutes(3)), ..Default::default() };
    for (i, (name, lang, desc, days, private, fork, archived)) in REPOS.iter().enumerate() {
        let id = 1000 + i as u64;
        let last = now - Duration::days(*days as i64) - Duration::hours(rng.range(0, 20) as i64);
        let created = last - Duration::days(rng.range(120, 1600) as i64);
        let intensity = 0.25 + rng.f() * 0.6;
        let repo = Repo {
            id,
            name: name.to_string(),
            full_name: format!("akulav/{name}"),
            owner: if *name == "inventory-api" { "acme-tools".into() } else { "akulav".into() },
            owner_avatar: String::new(),
            private: *private,
            fork: *fork,
            archived: *archived,
            description: Some(desc.to_string()),
            html_url: format!("https://github.com/Akulav/{name}"),
            clone_url: format!("https://github.com/Akulav/{name}.git"),
            language: Some(lang.to_string()),
            stars: if *private { 0 } else { rng.range(0, 140) / (1 + i as u64 / 4) },
            forks: rng.range(0, 12),
            open_issues: rng.range(0, 14),
            size_kb: rng.range(80, 40_000),
            default_branch: "main".into(),
            pushed_at: Some(last),
            created_at: Some(created),
            topics: if i % 3 == 0 { vec![lang.to_lowercase().replace(' ', "-"), "desktop".into()] } else { vec![] },
        };
        let weekly = weeks_for(&mut rng, last, now, intensity);
        let total_weeks = ((last - created).num_days() / 7).max(4);
        let code_frequency: Vec<(i64, i64, i64)> = (0..total_weeks)
            .map(|w| {
                let t = stats::week_start(created) + w * 604800;
                let add = if rng.f() < 0.55 { rng.range(10, 900) as i64 * if w < 3 { 8 } else { 1 } } else { 0 };
                let del = -((add as f64 * rng.f() * 0.7) as i64);
                (t, add, del)
            })
            .collect();
        let bytes = rng.range(40_000, 2_500_000);
        let mut languages = vec![(lang.to_string(), bytes)];
        for extra in ["Shell", "PowerShell", "HTML", "CSS", "Dockerfile"] {
            if extra != *lang && rng.f() < 0.3 {
                languages.push((extra.to_string(), bytes / rng.range(8, 60)));
            }
        }
        let commit_count = rng.range(30, 1400) + weekly.iter().map(|w| w.total as u64).sum::<u64>();
        let details = Details {
            fetched_at: Some(now),
            seen_push: Some(last),
            empty: false,
            commit_count: Some(commit_count),
            last_commit: Some(Commit { sha: format!("{:040x}", rng.next() as u128 * 7919), title: "Polish things".into(), author: "akulav".into(), date: Some(last), url: String::new() }),
            languages,
            weekly,
            code_frequency,
            stats_pending: false,
            stats_unavailable: false,
        };
        c.repos.push(repo);
        c.details.insert(id, details);
    }
    c
}

/// A believable recent history for a demo repository.
pub fn commits(repo: &Repo) -> Vec<Commit> {
    let titles = [
        "Polish the overview cards",
        "Fix off-by-one in weekly totals",
        "Add keyboard navigation to the list",
        "Bump dependencies",
        "Cache avatars on disk",
        "Handle 202 from the stats endpoints",
        "Refactor sync into a background job",
        "Tweak colors for the dark theme",
        "Write tests for version ordering",
        "Initial commit",
    ];
    let last = repo.pushed_at.unwrap_or_else(Utc::now);
    titles
        .iter()
        .enumerate()
        .map(|(i, t)| Commit {
            sha: format!("{:07x}{}", 0xa1b2c3u32.wrapping_mul(i as u32 + 3), "0".repeat(33)),
            title: t.to_string(),
            author: "akulav".into(),
            date: Some(last - Duration::hours(i as i64 * 31)),
            url: String::new(),
        })
        .collect()
}

//! One repository: header, key numbers, code evolution, activity, code frequency, languages and
//! the commit history.

use chrono::{DateTime, Utc};
use egui::{pos2, vec2, Align, Align2, Color32, CornerRadius, Layout, RichText, Sense, Ui};
use egui_phosphor::regular as ph;

use super::app::{App, Loadable, ToastKind};
use super::charts;
use super::overview::{columns, page, top_languages};
use super::theme::*;
use super::widgets::*;
use crate::backend::github::{CommitStat, Details, History, Repo, BYTES_PER_LINE};
use crate::backend::stats;

/// Commits listed at first, and how many more each "Show more" adds.
const LIST_STEP: usize = 30;

impl App {
    pub fn repo_page(&mut self, ui: &mut Ui, id: u64) {
        let now = Utc::now();
        let Some(repo) = self.repo(id).cloned() else {
            page(ui, "repo_missing", |ui| {
                ui.label(RichText::new("This repository is no longer in your list.").color(MUTED));
            });
            return;
        };
        let info = self.info(&repo, now);
        let details = self.details.get(&id).cloned().unwrap_or_default();
        let owner_tex = self.avatar(&repo.owner_avatar);
        let mut back = false;
        let mut reload = false;
        let mut copied = None;
        let history = self.commit_history.get(&id);

        page(ui, "repo", |ui| {
            if ghost_button(ui, &format!("{}  Back", ph::ARROW_LEFT)).on_hover_text("Esc, Backspace, Alt+← or mouse back").clicked() {
                back = true;
            }
            ui.add_space(4.0);

            // Header: avatar, name and badges on the left, actions on the right.
            ui.horizontal(|ui| {
                avatar(ui, owner_tex.as_ref(), 56.0, &repo.owner);
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    kicker(ui, &repo.owner);
                    ui.add_space(-4.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&repo.name).font(black(28.0)).color(TEXT));
                        ui.add_space(4.0);
                        chip(ui, ph::PULSE, info.fresh.label(), freshness_color(info.fresh));
                        if repo.private {
                            chip(ui, ph::LOCK_SIMPLE, "Private", MUTED);
                        } else {
                            chip(ui, ph::GLOBE_SIMPLE, "Public", MUTED);
                        }
                        if repo.fork {
                            chip(ui, ph::GIT_FORK, "Fork", PURPLE);
                        }
                        if repo.archived {
                            chip(ui, ph::ARCHIVE, "Archived", GOLD);
                        }
                    });
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let size = vec2(150.0, 36.0);
                    if primary_button(ui, &format!("{}  Open on GitHub", ph::GITHUB_LOGO), size, true).clicked() {
                        open_url(&repo.html_url);
                    }
                    if secondary_button(ui, &format!("{}  Copy clone URL", ph::COPY), size, !repo.clone_url.is_empty()).clicked() {
                        copied = Some(repo.clone_url.clone());
                    }
                    if secondary_button(ui, &format!("{}  Issues", ph::BUG), vec2(110.0, 36.0), true).clicked() {
                        open_url(&format!("{}/issues", repo.html_url));
                    }
                });
            });
            if let Some(d) = &repo.description {
                ui.add_space(2.0);
                ui.label(RichText::new(d).font(regular(14.5)).color(MUTED));
            }
            if !repo.topics.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    for t in &repo.topics {
                        chip(ui, "", t, BLUE);
                    }
                });
            }
            ui.add_space(GAP);

            stat_tiles(ui, &repo, &details, &info, now);
            vgap(ui);

            // Code evolution (the full width), from the per-commit history.
            full_card(ui, |ui| {
                evolution_card(ui, history, &details, &mut reload);
            });
            vgap(ui);

            // Activity.
            let (first, grid) = stats::daily_grid(&details.weekly, now);
            full_card(ui, |ui| {
                card_header(ui, ph::CALENDAR_DOTS, "Commit activity", "commits per day and per week, last 12 months", |ui| {
                    if details.stats_pending {
                        ui.label(RichText::new("GitHub is still computing statistics; they appear on the next sync").font(regular(12.0)).color(GOLD));
                    } else if details.stats_unavailable {
                        ui.label(RichText::new("GitHub doesn't compute statistics for repositories this large").font(regular(12.0)).color(FAINT));
                    }
                });
                charts::heatmap(ui, first, &grid, now);
                ui.add_space(6.0);
                charts::week_bars(ui, &stats::weekly_from_grid(&grid), first, 150.0, ACCENT);
            });
            vgap(ui);

            // Code frequency and languages, side by side when there is room.
            let freq = |ui: &mut Ui| {
                card_header(ui, ph::CHART_BAR, "Code frequency", "lines added and removed per week, whole history", |_| {});
                if details.code_frequency.is_empty() {
                    ui.label(RichText::new("No data yet.").color(MUTED));
                    return;
                }
                charts::code_frequency(ui, &details.code_frequency, 170.0);
                let added: i64 = details.code_frequency.iter().map(|w| w.1).sum();
                let removed: i64 = details.code_frequency.iter().map(|w| -w.2).sum();
                ui.horizontal(|ui| {
                    dot_label(ui, ACCENT, &format!("+{} added", stats::grouped(added.max(0) as u64)), MUTED);
                    dot_label(ui, RED, &format!("−{} removed", stats::grouped(removed.max(0) as u64)), MUTED);
                });
            };
            let langs = |ui: &mut Ui| {
                card_header(ui, ph::CODE, "Languages", "share of the code, by file size", |_| {});
                if details.languages.is_empty() {
                    ui.label(RichText::new("No code detected.").color(MUTED));
                    return;
                }
                let segs = top_languages(&details.languages, 8);
                let total: u64 = details.languages.iter().map(|l| l.1).sum();
                charts::stacked_bar(ui, &segs, 12.0, &|b| stats::bytes(b));
                ui.add_space(8.0);
                for (l, b, c) in &segs {
                    ui.horizontal(|ui| {
                        dot_label(ui, *c, l, TEXT);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(format!("{:.1}%", *b as f64 * 100.0 / total.max(1) as f64)).font(semibold(12.5)).color(TEXT));
                            ui.label(RichText::new(format!("≈{} lines", stats::compact(b / BYTES_PER_LINE))).font(regular(11.5)).color(FAINT));
                        });
                    });
                    ui.add_space(-6.0);
                }
            };
            if ui.available_width() > 900.0 {
                card_row(ui, "freq_langs", 2, &[1, 1], |ui, i| if i == 0 { freq(ui) } else { langs(ui) });
            } else {
                full_card(ui, freq);
                vgap(ui);
                full_card(ui, langs);
            }
            vgap(ui);

            // Commit list.
            full_card(ui, |ui| commit_list(ui, history, &details, now, &mut reload));
        });

        if let Some(url) = copied {
            ui.ctx().copy_text(url.clone());
            self.toast(ToastKind::Success, "Clone URL copied", url);
        }
        if reload {
            self.load_history(id);
        }
        if back {
            self.back();
        }
    }
}

fn stat_tiles(ui: &mut Ui, repo: &Repo, details: &Details, info: &super::derive::RepoInfo, now: DateTime<Utc>) {
    let last = info
        .last
        .map(|t| (stats::ago(t, now), t.with_timezone(&chrono::Local).format("%b %-d, %Y").to_string()))
        .unwrap_or_else(|| ("never".into(), "no commits yet".into()));
    let lines = match details.lines() {
        Some((n, true)) => (stats::compact(n), "net lines across history".to_string()),
        Some((n, false)) => (format!("≈{}", stats::compact(n)), format!("estimated from file sizes (~{BYTES_PER_LINE} B/line)")),
        None => ("—".into(), if details.stats_pending { "GitHub is still counting".into() } else { "no data".into() }),
    };
    let created = repo.created_at.map(|t| t.with_timezone(&chrono::Local).format("%b %Y").to_string()).unwrap_or_default();
    let tiles: [(&str, Color32, String, &str, String); 6] = [
        (ph::CLOCK, freshness_color(info.fresh), last.0, "Last commit", last.1),
        (ph::GIT_COMMIT, ACCENT_HI, info.commits.map(stats::grouped).unwrap_or_else(|| "—".into()), "Commits", format!("on {}", repo.default_branch)),
        (ph::CODE, PURPLE, lines.0, "Lines of code", lines.1),
        (ph::STAR, GOLD, stats::grouped(repo.stars), "Stars", format!("{} forks", stats::grouped(repo.forks))),
        (ph::BUG, ORANGE, stats::grouped(repo.open_issues), "Open issues", "issues + pull requests".into()),
        (ph::HARD_DRIVES, BLUE, stats::bytes(repo.size_kb * 1024), "Size", if created.is_empty() { String::new() } else { format!("created {created}") }),
    ];
    tile_grid(ui, &tiles, |_, _| {});
}

/// Six KPI tiles: one row when wide, two rows of three otherwise. `on` sees each tile's response.
pub fn tile_grid(ui: &mut Ui, tiles: &[(&str, Color32, String, &str, String)], mut on: impl FnMut(usize, egui::Response)) {
    let per_row = if ui.available_width() > 1100.0 { 6 } else { 3 };
    for (row, chunk) in tiles.chunks(per_row).enumerate() {
        if row > 0 {
            vgap(ui);
        }
        columns(ui, per_row, GAP, |ui, i, w| {
            if let Some((icon, color, value, label, note)) = chunk.get(i) {
                let r = stat_tile(ui, w, icon, *color, value, label, note);
                on(row * per_row + i, r);
            }
        });
    }
}

fn evolution_card(ui: &mut Ui, history: Option<&Loadable<History>>, details: &Details, reload: &mut bool) {
    let sub = match history {
        Some(Loadable::Ready(h)) if !h.commits.is_empty() => {
            let span = if h.complete { format!("all {} commits", stats::grouped(h.commits.len() as u64)) } else { format!("last {} commits", stats::grouped(h.commits.len() as u64)) };
            format!("total lines after each commit, and the lines each one added and removed · {span}")
        }
        _ => "total lines after each commit, and the lines each one added and removed".into(),
    };
    card_header(ui, ph::TREND_UP, "Code evolution", &sub, |ui| {
        if icon_button(ui, ph::ARROWS_CLOCKWISE, "Reload the commit history").clicked() {
            *reload = true;
        }
    });
    let h = match history {
        Some(Loadable::Ready(h)) => h,
        Some(Loadable::Failed(e)) => {
            ui.label(RichText::new(format!("{}  Could not read the commit history: {e}", ph::WARNING)).font(regular(12.5)).color(RED));
            return;
        }
        _ => {
            loading(ui, "Reading the commit history…");
            return;
        }
    };
    if h.commits.is_empty() {
        ui.label(RichText::new("No commits on the default branch.").color(MUTED));
        return;
    }
    let (totals, absolute) = stats::line_totals(h, details.net_lines());
    if let Some(i) = charts::code_evolution(ui, &h.commits, &totals, 260.0) {
        open_url(&h.commits[i].url);
    }

    // Summary under the chart, in equal columns.
    let real: Vec<&CommitStat> = h.commits.iter().filter(|c| !c.merge).collect();
    let added: u64 = real.iter().map(|c| c.additions).sum();
    let removed: u64 = real.iter().map(|c| c.deletions).sum();
    let biggest = real.iter().max_by_key(|c| c.additions + c.deletions);
    let first_total = totals.last().copied().unwrap_or(0) - h.commits.last().map(|c| c.net()).unwrap_or(0);
    let now_total = totals.first().copied().unwrap_or(0);
    let growth = now_total - first_total;
    let facts: [(Color32, String, String); 4] = [
        (PURPLE, signed(now_total), if absolute { "lines now".into() } else { "lines (relative to the oldest commit read)".into() }),
        (if growth >= 0 { ACCENT_HI } else { RED }, format!("{}{}", if growth >= 0 { "+" } else { "" }, signed(growth)), if h.complete { "since the first commit".into() } else { "over these commits".into() }),
        (ACCENT_HI, format!("+{}  −{}", stats::compact(added), stats::compact(removed)), "added and removed".into()),
        (
            GOLD,
            biggest.map(|c| stats::compact(c.additions + c.deletions)).unwrap_or_else(|| "—".into()),
            biggest.map(|c| format!("largest change: {}", charts::truncate(&c.title, 28))).unwrap_or_else(|| "largest change".into()),
        ),
    ];
    ui.add_space(4.0);
    columns(ui, 4, GAP, |ui, i, _| {
        let (c, v, l) = &facts[i];
        ui.label(RichText::new(v).font(bold(17.0)).color(*c));
        ui.add_space(-6.0);
        ui.label(RichText::new(l).font(regular(11.5)).color(FAINT));
    });
    if !h.complete {
        ui.add_space(2.0);
        ui.label(
            RichText::new(format!(
                "Shows the newest {} commits.{}",
                stats::grouped(h.commits.len() as u64),
                if absolute { " Totals count back from GitHub's line count for the whole history." } else { "" }
            ))
            .font(regular(11.5))
            .color(FAINT),
        );
    }
}

fn commit_list(ui: &mut Ui, history: Option<&Loadable<History>>, details: &Details, now: DateTime<Utc>, reload: &mut bool) {
    let count = match history {
        Some(Loadable::Ready(h)) => format!("{} on the default branch{}", stats::grouped(h.commits.len() as u64), if h.complete { "" } else { " (newest)" }),
        _ => "on the default branch".into(),
    };
    card_header(ui, ph::GIT_COMMIT, "Commits", &format!("{count} · lines added and removed, and the total after each"), |ui| {
        if icon_button(ui, ph::ARROWS_CLOCKWISE, "Reload the commit history").clicked() {
            *reload = true;
        }
    });
    let h = match history {
        Some(Loadable::Ready(h)) => h,
        Some(Loadable::Failed(e)) => {
            ui.label(RichText::new(e).color(RED));
            return;
        }
        _ => {
            loading(ui, "Loading commits…");
            return;
        }
    };
    if h.commits.is_empty() {
        ui.label(RichText::new("No commits on the default branch.").color(MUTED));
        return;
    }
    let (totals, _) = stats::line_totals(h, details.net_lines());
    let shown_id = ui.id().with("commits_shown");
    let shown: usize = ui.data(|d| d.get_temp(shown_id)).unwrap_or(LIST_STEP);
    for (c, total) in h.commits.iter().zip(&totals).take(shown) {
        commit_row(ui, c, *total, now);
    }
    if h.commits.len() > shown {
        ui.add_space(6.0);
        ui.vertical_centered(|ui| {
            let more = (h.commits.len() - shown).min(LIST_STEP);
            if ghost_button(ui, &format!("Show {more} more  {}", ph::CARET_DOWN)).clicked() {
                ui.data_mut(|d| d.insert_temp(shown_id, shown + LIST_STEP));
            }
        });
    }
}

fn commit_row(ui: &mut Ui, c: &CommitStat, total: i64, now: DateTime<Utc>) {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::click());
    let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.08);
    let p = ui.painter();
    if hover > 0.0 {
        p.rect_filled(rect, CornerRadius::same(R_MD), with_alpha(Color32::WHITE, (hover * 8.0) as u8));
    }
    // Timeline rail.
    let x = rect.left() + 14.0;
    let cy = rect.center().y;
    p.vline(x, rect.y_range(), egui::Stroke::new(2.0_f32, SURFACE_3));
    p.circle_filled(pos2(x, cy), 5.0, if c.merge { FAINT } else { ACCENT });
    p.circle_filled(pos2(x, cy), 2.2, SURFACE);

    // Right side, right to left in fixed columns: sha, total, removed, added.
    let sha: String = c.sha.chars().take(7).collect();
    let mut right = rect.right() - 10.0;
    p.text(pos2(right, cy), Align2::RIGHT_CENTER, sha, mono(12.0), FAINT);
    right -= 76.0;
    p.text(pos2(right, cy), Align2::RIGHT_CENTER, format!("{} lines", if total < 0 { format!("−{}", stats::compact(total.unsigned_abs())) } else { stats::compact(total as u64) }), regular(12.0), PURPLE);
    right -= 90.0;
    if c.merge {
        p.text(pos2(right, cy), Align2::RIGHT_CENTER, "merge", semibold(12.0), FAINT);
    } else {
        p.text(pos2(right, cy), Align2::RIGHT_CENTER, format!("−{}", stats::compact(c.deletions)), semibold(12.5), if c.deletions > 0 { RED } else { FAINT });
        p.text(pos2(right - 58.0, cy), Align2::RIGHT_CENTER, format!("+{}", stats::compact(c.additions)), semibold(12.5), if c.additions > 0 { ACCENT_HI } else { FAINT });
    }
    let text_right = right - 120.0;

    let chars = ((text_right - rect.left() - 30.0) / 7.0).max(10.0) as usize;
    let title = if c.title.is_empty() { "(no message)" } else { &c.title };
    p.text(rect.left_center() + vec2(30.0, -8.0), Align2::LEFT_CENTER, charts::truncate(title, chars), semibold(13.5), TEXT);
    let when = c.date.map(|d| stats::ago(d, now)).unwrap_or_default();
    p.text(rect.left_center() + vec2(30.0, 10.0), Align2::LEFT_CENTER, charts::truncate(&format!("{}  ·  {when}", c.author), chars + 10), regular(11.5), MUTED);
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let resp = if c.url.is_empty() { resp } else { resp.on_hover_text("Open this commit on GitHub") };
    if resp.clicked() {
        open_url(&c.url);
    }
}

fn loading(ui: &mut Ui, text: &str) {
    ui.horizontal(|ui| {
        spinner(ui, 18.0, ACCENT_HI);
        ui.label(RichText::new(text).color(MUTED));
    });
}

fn signed(v: i64) -> String {
    if v < 0 {
        format!("−{}", stats::grouped(v.unsigned_abs()))
    } else {
        stats::grouped(v as u64)
    }
}

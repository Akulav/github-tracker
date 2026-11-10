//! Overview: totals, activity over the year, languages and what needs attention.

use chrono::Utc;
use egui::{vec2, Align, Align2, Color32, CornerRadius, Layout, RichText, Sense, Ui};use egui_phosphor::regular as ph;

use super::app::{App, Page};
use super::charts;
use super::derive::RepoInfo;
use super::theme::*;
use super::widgets::*;
use crate::backend::stats::{self, Freshness};

pub const PAGE_MARGIN: f32 = 28.0;

/// Scrollable page body with the standard margins.
pub fn page(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui)) {
    egui::ScrollArea::vertical().id_salt(id).auto_shrink([false, false]).show(ui, |ui| {
        egui::Frame::new().inner_margin(egui::Margin { left: PAGE_MARGIN as i8, right: PAGE_MARGIN as i8, top: 22, bottom: 28 }).show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        });
    });
}

/// Lay out `n` equal columns with `gap` between them.
pub fn columns(ui: &mut Ui, n: usize, gap: f32, mut add: impl FnMut(&mut Ui, usize, f32)) {
    let w = (ui.available_width() - gap * (n as f32 - 1.0)) / n as f32;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for i in 0..n {
            ui.allocate_ui_with_layout(vec2(w, 0.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(w);
                add(ui, i, w)
            });
        }
    });
}

/// A clickable compact repository row: language dot, name, and a value on the right.
pub fn mini_row(ui: &mut Ui, r: &RepoInfo, right: &str, right_color: Color32, sub: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
    let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.1);
    let p = ui.painter();
    if hover > 0.0 {
        p.rect_filled(rect, CornerRadius::same(R_MD), with_alpha(egui::Color32::WHITE, (hover * 8.0) as u8));
    }
    let lc = r.language.as_deref().map(language_color).unwrap_or(FAINT);
    p.circle_filled(rect.left_center() + vec2(12.0, 0.0), 5.0, lc);
    p.text(rect.left_center() + vec2(26.0, -8.0), Align2::LEFT_CENTER, charts::truncate(&r.name, 30), semibold(13.5), TEXT);
    p.text(rect.left_center() + vec2(26.0, 9.0), Align2::LEFT_CENTER, sub, regular(11.5), MUTED);
    p.text(rect.right_center() - vec2(10.0, 0.0), Align2::RIGHT_CENTER, right, semibold(13.0), right_color);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

impl App {
    pub fn overview_page(&mut self, ui: &mut Ui) {
        let now = Utc::now();
        let counted = self.counted();
        let totals = stats::totals(counted.iter().copied(), &self.details, now);
        let infos = self.infos(counted, now);
        let name = self.session().map(|s| s.user.name.clone().unwrap_or_else(|| s.user.login.clone())).unwrap_or_default();
        let mut open: Option<u64> = None;
        let mut goto: Option<Page> = None;

        page(ui, "overview", |ui| {
            let first = name.split_whitespace().next().unwrap_or(&name).to_string();
            let synced = self.synced_at.map(|t| format!("synced {}", stats::ago(t, now))).unwrap_or_else(|| "not synced yet".into());
            page_header(ui, "Overview", &format!("Hey, {first}"), Some(&format!("{} repositories  ·  {synced}", totals.repos)));
            ui.add_space(GAP);

            if self.repos.is_empty() {
                empty_state(ui, self.sync.running);
                return;
            }

            // KPI tiles.
            let attention = infos.iter().filter(|r| !r.archived && matches!(r.fresh, Freshness::Stale | Freshness::Dormant)).count();
            let tiles: [(&str, Color32, String, &str, String); 6] = [
                (ph::BOOKS, BLUE, stats::grouped(totals.repos as u64), "Repositories", format!("{} private  ·  {} public", totals.private, totals.repos - totals.private)),
                (ph::GIT_COMMIT, ACCENT_HI, stats::compact(totals.commits), "Commits", format!("{} in the last 30 days", stats::grouped(totals.commits_30d as u64))),
                (
                    ph::CODE,
                    PURPLE,
                    format!("{}{}", if totals.lines_estimated { "≈" } else { "" }, stats::compact(totals.lines)),
                    "Lines of code",
                    if totals.lines_estimated { "some estimated from file sizes".into() } else { "net lines across history".into() },
                ),
                (ph::STAR, GOLD, stats::compact(totals.stars), "Stars", format!("{} forks  ·  {} open issues", stats::compact(totals.forks), stats::compact(totals.open_issues))),
                (ph::FIRE, ORANGE, totals.streak.to_string(), "Day streak", if totals.streak > 0 { "days in a row with commits".into() } else { "commit today to start one".into() }),
                (ph::HEARTBEAT, freshness_color(Freshness::Dormant), attention.to_string(), "Need attention", "no commits in 3+ months".into()),
            ];
            const ATTENTION_TILE: usize = 5;
            super::detail::tile_grid(ui, &tiles, |i, r| {
                if i == ATTENTION_TILE && r.interact(Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Open the maintenance radar").clicked() {
                    goto = Some(Page::Maintenance);
                }
            });
            vgap(ui);

            // Everything below sits on one 3-column grid; cards in a row share their height.
            let wide = ui.available_width() > 900.0;
            let activity = |ui: &mut Ui| {
                card_header(ui, ph::CHART_LINE_UP, "Commit activity", "commits per week, last 12 months", |_| {});
                charts::area_chart(ui, &stats::weekly_from_grid(&totals.daily), totals.first_week, 210.0, ACCENT_HI);
            };
            let languages = |ui: &mut Ui| {
                card_header(ui, ph::CODE, "Languages", "share of the code, by file size", |_| {});
                let top: Vec<(String, u64, Color32)> = top_languages(&totals.languages, 6);
                let total_bytes: u64 = totals.languages.iter().map(|l| l.1).sum();
                ui.horizontal(|ui| {
                    charts::donut(ui, &top, 150.0, &totals.languages.len().to_string(), "languages", &|b| stats::bytes(b));
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.add_space(4.0);
                        for (l, b, c) in &top {
                            let pct = *b as f64 * 100.0 / total_bytes.max(1) as f64;
                            dot_label(ui, *c, &format!("{l}  {pct:.0}%"), MUTED);
                            ui.add_space(-6.0);
                        }
                    });
                });
            };
            if wide {
                card_row(ui, "activity_languages", 3, &[2, 1], |ui, i| if i == 0 { activity(ui) } else { languages(ui) });
            } else {
                full_card(ui, activity);
                vgap(ui);
                full_card(ui, languages);
            }
            vgap(ui);

            full_card(ui, |ui| {
                card_header(ui, ph::CALENDAR_DOTS, "Contribution calendar", "commits per day across all counted repositories", |_| {});
                charts::heatmap(ui, totals.first_week, &totals.daily, now);
            });
            vgap(ui);

            // Freshness, most active and needs attention.
            let mut active: Vec<&RepoInfo> = infos.iter().filter(|r| r.commits_4w > 0).collect();
            active.sort_by(|a, b| b.commits_4w.cmp(&a.commits_4w));
            let mut needs: Vec<&RepoInfo> = infos.iter().filter(|r| !r.archived && r.days.is_some()).collect();
            needs.sort_by(|a, b| b.attention.cmp(&a.attention));
            let mut block = |ui: &mut Ui, i: usize| match i {
                0 => {
                    card_header(ui, ph::CLOCK_COUNTDOWN, "Freshness", "time since each repository's last commit", |_| {});
                    let segs: Vec<(String, u64, Color32)> = Freshness::ALL.iter().map(|f| (format!("{} · {}", f.label(), f.range()), totals.buckets[f.index()] as u64, freshness_color(*f))).collect();
                    charts::stacked_bar(ui, &segs, 14.0, &|n| format!("{n} repositories"));
                    ui.add_space(8.0);
                    for f in Freshness::ALL {
                        let c = totals.buckets[f.index()];
                        if c == 0 && f == Freshness::Unknown {
                            continue;
                        }
                        ui.horizontal(|ui| {
                            dot_label(ui, freshness_color(f), f.label(), TEXT);
                            ui.label(RichText::new(f.range()).font(regular(11.5)).color(FAINT));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(RichText::new(c.to_string()).font(semibold(13.0)).color(TEXT));
                            });
                        });
                        ui.add_space(-6.0);
                    }
                    ui.add_space(10.0);
                    if ghost_button(ui, &format!("Open maintenance radar  {}", ph::ARROW_RIGHT)).clicked() {
                        goto = Some(Page::Maintenance);
                    }
                }
                1 => {
                    card_header(ui, ph::LIGHTNING, "Most active", "commits in the last 4 weeks", |_| {});
                    if active.is_empty() {
                        ui.label(RichText::new("No commits in the last 4 weeks.").color(MUTED));
                    }
                    for r in active.iter().take(6) {
                        let sub = r.last.map(|t| format!("last commit {}", stats::ago(t, now))).unwrap_or_default();
                        if mini_row(ui, r, &format!("{} {}", r.commits_4w, ph::GIT_COMMIT), ACCENT_HI, &sub) {
                            open = Some(r.id);
                        }
                    }
                }
                _ => {
                    card_header(ui, ph::WRENCH, "Needs attention", "longest without a commit", |_| {});
                    if needs.is_empty() {
                        ui.label(RichText::new("Nothing needs you right now.").color(MUTED));
                    }
                    for r in needs.iter().take(6) {
                        let sub = format!("{}  ·  {} open issues", r.fresh.label(), r.open_issues);
                        if mini_row(ui, r, &stats::short_age(r.days.unwrap_or(0)), freshness_color(r.fresh), &sub) {
                            open = Some(r.id);
                        }
                    }
                }
            };
            if wide {
                card_row(ui, "overview_lists", 3, &[1, 1, 1], &mut block);
            } else {
                for i in 0..3 {
                    if i > 0 {
                        vgap(ui);
                    }
                    full_card(ui, |ui| block(ui, i));
                }
            }
        });

        if let Some(id) = open {
            self.open_repo(id);
        } else if let Some(p) = goto {
            self.go(p);
        }
    }
}

pub fn top_languages(langs: &[(String, u64)], n: usize) -> Vec<(String, u64, Color32)> {
    let mut v: Vec<(String, u64, Color32)> = langs.iter().take(n).map(|(l, b)| (l.clone(), *b, language_color(l))).collect();
    let rest: u64 = langs.iter().skip(n).map(|l| l.1).sum();
    if rest > 0 {
        v.push(("Other".into(), rest, FAINT));
    }
    v
}

pub fn empty_state(ui: &mut Ui, syncing: bool) {
    ui.add_space(60.0);
    ui.vertical_centered(|ui| {
        if syncing {
            spinner(ui, 40.0, ACCENT_HI);
            ui.add_space(10.0);
            ui.label(RichText::new("Fetching your repositories…").font(bold(18.0)));
            ui.label(RichText::new("The first sync reads every repository's history; it takes a moment.").color(MUTED));
        } else {
            ui.label(RichText::new(ph::GIT_BRANCH).font(regular(48.0)).color(FAINT));
            ui.label(RichText::new("No repositories yet").font(bold(18.0)));
            ui.label(RichText::new("Press F5 to sync, or check the filters in Settings.").color(MUTED));
        }
    });
}

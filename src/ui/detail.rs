//! One repository: header, key numbers, activity, code frequency, languages and recent commits.

use chrono::Utc;
use egui::{vec2, Align, Align2, Color32, CornerRadius, Layout, RichText, Sense, Ui};
use egui_phosphor::regular as ph;

use super::app::{App, Loadable, ToastKind};
use super::charts;
use super::overview::{columns, page, top_languages};
use super::theme::*;
use super::widgets::*;
use crate::backend::github::BYTES_PER_LINE;
use crate::backend::stats;

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
        let commits = match self.commits.get(&id) {
            Some(Loadable::Ready(c)) => Loadable::Ready(c.clone()),
            Some(Loadable::Failed(e)) => Loadable::Failed(e.clone()),
            _ => Loadable::Loading,
        };

        page(ui, "repo", |ui| {
            // Breadcrumb / back.
            ui.horizontal(|ui| {
                if ghost_button(ui, &format!("{}  Back", ph::ARROW_LEFT)).on_hover_text("Esc, Backspace, Alt+← or mouse back").clicked() {
                    back = true;
                }
            });
            ui.add_space(6.0);

            // Header.
            ui.horizontal(|ui| {
                avatar(ui, owner_tex.as_ref(), 56.0, &repo.owner);
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    kicker(ui, &repo.owner);
                    ui.add_space(-4.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&repo.name).font(black(28.0)).color(TEXT));
                        ui.add_space(4.0);
                        let fc = freshness_color(info.fresh);
                        chip(ui, ph::PULSE, info.fresh.label(), fc);
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
            });
            if let Some(d) = &repo.description {
                ui.add_space(4.0);
                ui.label(RichText::new(d).font(regular(14.5)).color(MUTED));
            }
            if !repo.topics.is_empty() {
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    for t in &repo.topics {
                        chip(ui, "", t, BLUE);
                    }
                });
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if primary_button(ui, &format!("{}  Open on GitHub", ph::GITHUB_LOGO), vec2(170.0, 36.0), true).clicked() {
                    let _ = open::that_detached(&repo.html_url);
                }
                if secondary_button(ui, &format!("{}  Copy clone URL", ph::COPY), vec2(160.0, 36.0), true).clicked() {
                    copied = Some(repo.clone_url.clone());
                }
                if secondary_button(ui, &format!("{}  Issues", ph::BUG), vec2(100.0, 36.0), true).clicked() {
                    let _ = open::that_detached(format!("{}/issues", repo.html_url));
                }
                if icon_button(ui, ph::ARROWS_CLOCKWISE, "Reload recent commits").clicked() {
                    reload = true;
                }
            });
            ui.add_space(14.0);

            // Numbers.
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
                (ph::CLOCK, freshness_color(info.fresh), last.0.clone(), "Last commit", last.1.clone()),
                (ph::GIT_COMMIT, ACCENT_HI, info.commits.map(stats::grouped).unwrap_or_else(|| "—".into()), "Commits", format!("on {}", repo.default_branch)),
                (ph::CODE, PURPLE, lines.0, "Lines of code", lines.1),
                (ph::STAR, GOLD, stats::grouped(repo.stars), "Stars", format!("{} forks", stats::grouped(repo.forks))),
                (ph::BUG, Color32::from_rgb(251, 146, 60), stats::grouped(repo.open_issues), "Open issues", "issues + pull requests".into()),
                (ph::HARD_DRIVES, BLUE, stats::bytes(repo.size_kb * 1024), "Size", if created.is_empty() { String::new() } else { format!("created {created}") }),
            ];
            let per_row = if ui.available_width() > 1100.0 { 6 } else { 3 };
            for chunk in tiles.chunks(per_row) {
                columns(ui, per_row, 12.0, |ui, i, w| {
                    if let Some((icon, color, value, label, note)) = chunk.get(i) {
                        stat_tile(ui, w, icon, *color, value, label, note);
                    }
                });
                ui.add_space(4.0);
            }
            ui.add_space(10.0);

            // Activity.
            let (first, grid) = stats::daily_grid(&details.weekly, now);
            card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    section_title(ui, ph::CALENDAR_DOTS, "Commit activity");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if details.stats_pending {
                            ui.label(RichText::new("GitHub is still computing statistics — they appear on the next sync").font(regular(12.0)).color(GOLD));
                        } else if details.stats_unavailable {
                            ui.label(RichText::new("GitHub doesn't compute statistics for repositories this large").font(regular(12.0)).color(FAINT));
                        }
                    });
                });
                ui.add_space(8.0);
                charts::heatmap(ui, first, &grid, now);
                ui.add_space(6.0);
                charts::week_bars(ui, &stats::weekly_from_grid(&grid), first, 150.0, ACCENT);
            });
            ui.add_space(12.0);

            let wide = ui.available_width() > 900.0;
            let freq = |ui: &mut Ui| {
                card().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    section_title(ui, ph::CHART_BAR, "Code frequency");
                    ui.label(RichText::new("lines added and removed per week, whole history").font(regular(12.0)).color(FAINT));
                    ui.add_space(6.0);
                    if details.code_frequency.is_empty() {
                        ui.label(RichText::new("No data yet.").color(MUTED));
                    } else {
                        charts::code_frequency(ui, &details.code_frequency, 170.0);
                        let added: i64 = details.code_frequency.iter().map(|w| w.1).sum();
                        let removed: i64 = details.code_frequency.iter().map(|w| -w.2).sum();
                        ui.horizontal(|ui| {
                            dot_label(ui, ACCENT, &format!("+{} added", stats::grouped(added.max(0) as u64)), MUTED);
                            dot_label(ui, RED, &format!("−{} removed", stats::grouped(removed.max(0) as u64)), MUTED);
                        });
                    }
                });
            };
            let langs = |ui: &mut Ui| {
                card().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    section_title(ui, ph::CODE, "Languages");
                    ui.add_space(8.0);
                    if details.languages.is_empty() {
                        ui.label(RichText::new("No code detected.").color(MUTED));
                        return;
                    }
                    let segs = top_languages(&details.languages, 8);
                    let total: u64 = details.languages.iter().map(|l| l.1).sum();
                    charts::stacked_bar(ui, &segs, 12.0, &|b| stats::bytes(b));
                    ui.add_space(10.0);
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
                });
            };
            if wide {
                columns(ui, 2, 12.0, |ui, i, _| if i == 0 { freq(ui) } else { langs(ui) });
            } else {
                freq(ui);
                ui.add_space(4.0);
                langs(ui);
            }
            ui.add_space(12.0);

            // Recent commits.
            card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                section_title(ui, ph::GIT_COMMIT, "Recent commits");
                ui.add_space(6.0);
                match &commits {
                    Loadable::Loading => {
                        ui.horizontal(|ui| {
                            spinner(ui, 18.0, ACCENT_HI);
                            ui.label(RichText::new("Loading commits…").color(MUTED));
                        });
                    }
                    Loadable::Failed(e) => {
                        ui.label(RichText::new(e).color(RED));
                    }
                    Loadable::Ready(list) if list.is_empty() => {
                        ui.label(RichText::new("No commits on the default branch.").color(MUTED));
                    }
                    Loadable::Ready(list) => {
                        for c in list {
                            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::click());
                            let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.08);
                            let p = ui.painter();
                            if hover > 0.0 {
                                p.rect_filled(rect, CornerRadius::same(R_MD), with_alpha(Color32::WHITE, (hover * 8.0) as u8));
                            }
                            // Timeline rail.
                            let x = rect.left() + 14.0;
                            p.vline(x, rect.y_range(), egui::Stroke::new(2.0_f32, SURFACE_3));
                            p.circle_filled(egui::pos2(x, rect.center().y), 5.0, ACCENT);
                            p.circle_filled(egui::pos2(x, rect.center().y), 2.2, SURFACE);
                            let chars = ((rect.width() - 260.0) / 7.0).max(10.0) as usize;
                            p.text(rect.left_center() + vec2(30.0, -8.0), Align2::LEFT_CENTER, charts::truncate(&c.title, chars), semibold(13.5), TEXT);
                            let when = c.date.map(|d| stats::ago(d, now)).unwrap_or_default();
                            p.text(rect.left_center() + vec2(30.0, 10.0), Align2::LEFT_CENTER, format!("{}  ·  {when}", c.author), regular(11.5), MUTED);
                            p.text(rect.right_center() - vec2(10.0, 0.0), Align2::RIGHT_CENTER, c.sha.chars().take(7).collect::<String>(), mono(12.0), FAINT);
                            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Open this commit on GitHub").clicked() && !c.url.is_empty() {
                                let _ = open::that_detached(&c.url);
                            }
                        }
                    }
                }
            });
        });

        if let Some(url) = copied {
            ui.ctx().copy_text(url.clone());
            self.toast(ToastKind::Success, "Clone URL copied", url);
        }
        if reload {
            self.load_commits(id);
        }
        if back {
            self.back();
        }
    }
}

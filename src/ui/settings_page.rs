//! Settings: account, updates, sync, install and about, on a centered two-column grid.

use std::sync::atomic::Ordering;

use chrono::Utc;
use egui::{vec2, Align, Layout, RichText, Sense, Ui};
use egui_phosphor::regular as ph;

use super::app::{App, Confirm, UpdateState};
use super::overview::page;
use super::theme::*;
use super::widgets::*;
use crate::backend::install;
use crate::backend::stats;
use crate::backend::update;

/// Widest the settings grid gets; wider windows center it.
const MAX_W: f32 = 1240.0;
/// Below this the cards stack in one column (capped at [`NARROW_W`]).
const TWO_COLUMNS_FROM: f32 = 1000.0;
const NARROW_W: f32 = 760.0;
const BUTTON: egui::Vec2 = vec2(132.0, 34.0);

#[derive(Clone, Copy)]
enum Section {
    Account,
    Updates,
    Sync,
    About,
    Install,
}

impl App {
    pub fn settings_page(&mut self, ui: &mut Ui) {
        let now = Utc::now();
        let (login, name, avatar_url, html_url, rate) = match self.session() {
            Some(s) => (
                s.user.login.clone(),
                s.user.name.clone().unwrap_or_else(|| s.user.login.clone()),
                s.user.avatar_url.clone(),
                s.user.html_url.clone(),
                (s.api.rate.remaining.load(Ordering::Relaxed), s.api.rate.limit.load(Ordering::Relaxed), s.api.rate.reset.load(Ordering::Relaxed)),
            ),
            None => return,
        };
        let tex = self.avatar(&avatar_url);
        let mut changed = false;
        let mut action: Option<&str> = None;

        page(ui, "settings", |ui| {
            let avail = ui.available_width();
            let two = avail >= TWO_COLUMNS_FROM;
            let w = avail.min(if two { MAX_W } else { NARROW_W });
            ui.horizontal_top(|ui| {
                ui.add_space(((avail - w) / 2.0).max(0.0));
                ui.allocate_ui_with_layout(vec2(w, 0.0), Layout::top_down(Align::Min), |ui| {
                    ui.set_width(w);
                    page_header(ui, "Settings", "Settings", Some("Your account, what gets counted, updates and where the app lives."));
                    ui.add_space(GAP);

                    let mut section = |ui: &mut Ui, s: Section| match s {
                        Section::Account => {
                            card_header(ui, ph::USER_CIRCLE, "Account", "the GitHub account this app reads", |_| {});
                            ui.horizontal(|ui| {
                                avatar(ui, tex.as_ref(), 52.0, &login);
                                ui.add_space(6.0);
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(&name).font(bold(17.0)));
                                    ui.add_space(-6.0);
                                    if ui.link(RichText::new(format!("@{login}")).font(regular(13.0))).clicked() && !html_url.is_empty() {
                                        open_url(&html_url);
                                    }
                                });
                            });
                            ui.add_space(6.0);
                            action_bar(
                                ui,
                                |ui| {
                                    let text = if rate.0 >= 0 {
                                        let reset = chrono::DateTime::<Utc>::from_timestamp(rate.2, 0).map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string()).unwrap_or_default();
                                        format!("GitHub API: {} of {} requests left (resets {reset})", rate.0, rate.1)
                                    } else {
                                        "GitHub API usage shows after the first request".into()
                                    };
                                    ui.label(RichText::new(text).font(regular(12.0)).color(FAINT));
                                },
                                |ui| {
                                    if danger_button(ui, &format!("{}  Sign out", ph::SIGN_OUT), BUTTON, true).clicked() {
                                        action = Some("signout");
                                    }
                                },
                            );
                        }
                        Section::Updates => {
                            card_header(ui, ph::ARROW_CIRCLE_UP, "Updates", "new releases from GitHub, verified by SHA-256", |_| {});
                            changed |= setting_row(ui, "Check automatically", "At start and every few hours.", &mut self.settings.auto_check_updates);
                            ui.add_space(6.0);
                            let (text, color) = match &self.update {
                                UpdateState::None => (format!("You have version {}", update::CURRENT_VERSION), MUTED),
                                UpdateState::Checking => ("Checking for updates…".into(), MUTED),
                                UpdateState::UpToDate => (format!("{}  Version {} is the latest", ph::CHECK_CIRCLE, update::CURRENT_VERSION), ACCENT_HI),
                                UpdateState::Available(i) => (format!("Version {} is available", i.version), GOLD),
                                UpdateState::Downloading { done, total, .. } => (format!("Downloading… {}", if *total > 0 { format!("{:.0}%", *done as f32 * 100.0 / *total as f32) } else { stats::bytes(*done) }), BLUE),
                                UpdateState::Ready { info, .. } => (format!("Version {} is ready to install", info.version), ACCENT_HI),
                                UpdateState::Failed { error, .. } => (format!("Update check failed: {error}"), RED),
                            };
                            let checking = matches!(self.update, UpdateState::Checking);
                            let offer = matches!(self.update, UpdateState::Available(_) | UpdateState::Ready { .. } | UpdateState::Downloading { .. });
                            action_bar(
                                ui,
                                |ui| {
                                    if checking {
                                        spinner(ui, 16.0, ACCENT_HI);
                                    }
                                    ui.add(egui::Label::new(RichText::new(text).font(semibold(13.0)).color(color)).truncate());
                                },
                                |ui| {
                                    if offer {
                                        if primary_button(ui, &format!("{}  Update", ph::ROCKET_LAUNCH), BUTTON, true).clicked() {
                                            action = Some("show_update");
                                        }
                                    } else if secondary_button(ui, &format!("{}  Check now", ph::MAGNIFYING_GLASS), BUTTON, !checking).clicked() {
                                        action = Some("check");
                                    }
                                },
                            );
                        }
                        Section::Sync => {
                            card_header(ui, ph::DATABASE, "Repositories & sync", "what is counted, and when it is read", |_| {});
                            changed |= setting_row(ui, "Sync when the app starts", "Fetch changes from GitHub every time you open the app.", &mut self.settings.sync_on_start);
                            ui.separator();
                            changed |= setting_row(ui, "Count forks", "Include forked repositories in the overview and maintenance radar.", &mut self.settings.include_forks);
                            ui.separator();
                            changed |= setting_row(ui, "Count archived repositories", "Archived repositories are read-only; they rarely need maintenance.", &mut self.settings.include_archived);
                            ui.separator();
                            changed |= setting_row(ui, "Only my own repositories", "Leave out organization and collaborator repositories from totals.", &mut self.settings.only_owned);
                            ui.add_space(6.0);
                            let synced = self.synced_at.map(|t| format!("Last synced {}", stats::ago(t, now))).unwrap_or_else(|| "Never synced".into());
                            let idle = !self.sync.running;
                            action_bar(
                                ui,
                                |ui| {
                                    ui.label(RichText::new(synced).font(regular(12.5)).color(MUTED));
                                },
                                |ui| {
                                    if secondary_button(ui, &format!("{}  Refetch all", ph::ARROWS_COUNTER_CLOCKWISE), BUTTON, idle).on_hover_text("Ignore the cache and read every repository again").clicked() {
                                        action = Some("force_sync");
                                    }
                                    if secondary_button(ui, &format!("{}  Sync now", ph::ARROWS_CLOCKWISE), BUTTON, idle).clicked() {
                                        action = Some("sync");
                                    }
                                },
                            );
                            if !self.sync.errors.is_empty() {
                                egui::CollapsingHeader::new(RichText::new(format!("{} repositories could not be read in the last sync", self.sync.errors.len())).color(GOLD)).show(ui, |ui| {
                                    for e in &self.sync.errors {
                                        ui.label(RichText::new(e).font(regular(12.0)).color(MUTED));
                                    }
                                });
                            }
                        }
                        Section::About => {
                            card_header(ui, ph::INFO, "About", "version, source and keyboard shortcuts", |_| {});
                            ui.horizontal(|ui| {
                                if let Some(icon) = &self.icon {
                                    ui.add(egui::Image::new(icon).fit_to_exact_size(vec2(48.0, 48.0)));
                                }
                                ui.vertical(|ui| {
                                    ui.label(RichText::new("Akulav Tracker").font(black(18.0)));
                                    ui.add_space(-6.0);
                                    ui.label(RichText::new(format!("Version {}  ·  made by {} {}", update::CURRENT_VERSION, update::AUTHOR, ph::HEART)).font(regular(12.5)).color(MUTED));
                                    ui.add_space(-6.0);
                                    ui.label(RichText::new("No telemetry. Talks only to GitHub.").font(regular(12.0)).color(FAINT));
                                });
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if secondary_button(ui, &format!("{}  Source", ph::GITHUB_LOGO), BUTTON, true).clicked() {
                                        open_url(update::REPO_URL);
                                    }
                                });
                            });
                            ui.add_space(8.0);
                            let key_w = 220.0_f32.min(ui.available_width() * 0.45);
                            for (k, d) in [
                                ("Ctrl+1 … 4", "Switch pages"),
                                ("Ctrl+F  or  /", "Search repositories"),
                                ("↑ ↓  PgUp PgDn  Enter", "Move through the list, open"),
                                ("Esc  Backspace  Alt+←", "Go back (also mouse back)"),
                                ("F5  Ctrl+R", "Sync (Shift+F5 refetches all)"),
                            ] {
                                ui.horizontal(|ui| {
                                    ui.allocate_ui_with_layout(vec2(key_w, 18.0), Layout::left_to_right(Align::Center), |ui| {
                                        ui.set_width(key_w);
                                        ui.add(egui::Label::new(RichText::new(k).font(mono(12.0)).color(ACCENT_HI)).truncate());
                                    });
                                    ui.add(egui::Label::new(RichText::new(d).font(regular(12.5)).color(MUTED)).truncate());
                                });
                                ui.add_space(-6.0);
                            }
                        }
                        Section::Install => {
                            card_header(ui, ph::FOLDER_OPEN, "Install", "where the app and its data live", |_| {});
                            let installed = install::running_installed();
                            let where_ = if installed { "Installed in" } else if install::portable() { "Portable copy, data in" } else { "Not installed; data in" };
                            let path = self.root.display().to_string();
                            let (mut open_path, mut open_button) = (false, false);
                            action_bar(
                                ui,
                                |ui| {
                                    ui.label(RichText::new(where_).color(MUTED));
                                    let r = ui.add(egui::Label::new(RichText::new(&path).font(mono(12.5)).color(ACCENT_HI)).truncate().sense(Sense::click()));
                                    if r.on_hover_text(format!("{path}\nClick to open the folder")).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                        open_path = true;
                                    }
                                },
                                |ui| {
                                    if installed && danger_button(ui, &format!("{}  Uninstall", ph::TRASH), BUTTON, true).on_hover_text("Removes the app; asks before deleting your cache and settings").clicked() {
                                        action = Some("uninstall");
                                    }
                                    if secondary_button(ui, &format!("{}  Open folder", ph::FOLDER_OPEN), BUTTON, true).clicked() {
                                        open_button = true;
                                    }
                                },
                            );
                            if open_path || open_button {
                                let _ = open::that_detached(&self.root);
                            }
                        }
                    };

                    if two {
                        card_row(ui, "settings_1", 2, &[1, 1], |ui, i| section(ui, [Section::Account, Section::Updates][i]));
                        vgap(ui);
                        card_row(ui, "settings_2", 2, &[1, 1], |ui, i| section(ui, [Section::Sync, Section::About][i]));
                        vgap(ui);
                        full_card(ui, |ui| section(ui, Section::Install));
                    } else {
                        for (i, s) in [Section::Account, Section::Sync, Section::Updates, Section::Install, Section::About].into_iter().enumerate() {
                            if i > 0 {
                                vgap(ui);
                            }
                            full_card(ui, |ui| section(ui, s));
                        }
                    }
                });
            });
        });

        if changed {
            self.mark_dirty();
        }
        match action {
            Some("signout") => self.confirm = Some(Confirm::SignOut),
            Some("uninstall") => self.confirm = Some(Confirm::Uninstall { delete_data: false }),
            Some("sync") => self.start_sync(false),
            Some("force_sync") => self.start_sync(true),
            Some("check") => self.check_updates(true),
            Some("show_update") => self.show_update_dialog = true,
            _ => {}
        }
    }
}

/// A card's bottom line: status on the left (shrinks and truncates), buttons on the right. In
/// a row of cards it sits on the card's bottom edge, level with its neighbours.
fn action_bar(ui: &mut Ui, left: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui)) {
    bottom_spacer(ui, BUTTON.y);
    ui.horizontal(|ui| {
        ui.set_min_height(BUTTON.y);
        // Lay the buttons out first (right to left) so the left side gets what remains.
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            right(ui);
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.set_max_width(ui.available_width());
                left(ui)
            });
        });
    });
}

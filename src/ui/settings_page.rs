//! Settings: account, updates, sync, install and about.

use std::sync::atomic::Ordering;

use chrono::Utc;
use egui::{vec2, Align, Layout, RichText, Ui};
use egui_phosphor::regular as ph;

use super::app::{App, Confirm, UpdateState};
use super::overview::page;
use super::theme::*;
use super::widgets::*;
use crate::backend::install;
use crate::backend::stats;
use crate::backend::update;

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
            page_header(ui, "Settings", "Settings", None);
            ui.add_space(GAP);
            let w = ui.available_width().min(760.0);
            ui.allocate_ui_with_layout(vec2(w, 0.0), Layout::top_down(Align::Min), |ui| {
                // Account.
                full_card(ui, |ui| {
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
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if danger_button(ui, &format!("{}  Sign out", ph::SIGN_OUT), vec2(120.0, 36.0), true).clicked() {
                                action = Some("signout");
                            }
                        });
                    });
                    if rate.0 >= 0 {
                        ui.add_space(8.0);
                        let reset = chrono::DateTime::<Utc>::from_timestamp(rate.2, 0).map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string()).unwrap_or_default();
                        ui.label(RichText::new(format!("GitHub API: {} of {} requests left this hour (resets at {reset})", rate.0, rate.1)).font(regular(12.0)).color(FAINT));
                    }
                });
                vgap(ui);

                // Data.
                full_card(ui, |ui| {
                    card_header(ui, ph::DATABASE, "Repositories & sync", "what is counted, and when it is read", |_| {});
                    changed |= setting_row(ui, "Sync when the app starts", "Fetch changes from GitHub every time you open the app.", &mut self.settings.sync_on_start);
                    ui.separator();
                    changed |= setting_row(ui, "Count forks", "Include forked repositories in the overview and maintenance radar.", &mut self.settings.include_forks);
                    ui.separator();
                    changed |= setting_row(ui, "Count archived repositories", "Archived repositories are read-only; they rarely need maintenance.", &mut self.settings.include_archived);
                    ui.separator();
                    changed |= setting_row(ui, "Only my own repositories", "Leave out organization and collaborator repositories from totals.", &mut self.settings.only_owned);
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        let synced = self.synced_at.map(|t| format!("Last synced {}", stats::ago(t, now))).unwrap_or_else(|| "Never synced".into());
                        ui.label(RichText::new(synced).font(regular(12.5)).color(MUTED));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if secondary_button(ui, &format!("{}  Refetch everything", ph::ARROWS_CLOCKWISE), vec2(190.0, 34.0), !self.sync.running).on_hover_text("Ignore the cache and read every repository again").clicked() {
                                action = Some("force_sync");
                            }
                            if secondary_button(ui, &format!("{}  Sync now", ph::ARROWS_CLOCKWISE), vec2(120.0, 34.0), !self.sync.running).clicked() {
                                action = Some("sync");
                            }
                        });
                    });
                    if !self.sync.errors.is_empty() {
                        ui.add_space(4.0);
                        egui::CollapsingHeader::new(RichText::new(format!("{} repositories could not be read in the last sync", self.sync.errors.len())).color(GOLD)).show(ui, |ui| {
                            for e in &self.sync.errors {
                                ui.label(RichText::new(e).font(regular(12.0)).color(MUTED));
                            }
                        });
                    }
                });
                vgap(ui);

                // Updates.
                full_card(ui, |ui| {
                    card_header(ui, ph::ARROW_CIRCLE_UP, "Updates", "new versions from this app's GitHub releases, checked by SHA-256", |_| {});
                    changed |= setting_row(ui, "Check for updates automatically", "Looks for a new release on GitHub at start and every few hours.", &mut self.settings.auto_check_updates);
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        let (text, color) = match &self.update {
                            UpdateState::None => (format!("You have version {}", update::CURRENT_VERSION), MUTED),
                            UpdateState::Checking => ("Checking for updates…".into(), MUTED),
                            UpdateState::UpToDate => (format!("{}  Version {} is the latest", ph::CHECK_CIRCLE, update::CURRENT_VERSION), ACCENT_HI),
                            UpdateState::Available(i) => (format!("Version {} is available", i.version), GOLD),
                            UpdateState::Downloading { done, total, .. } => (format!("Downloading… {}", if *total > 0 { format!("{:.0}%", *done as f32 * 100.0 / *total as f32) } else { stats::bytes(*done) }), BLUE),
                            UpdateState::Ready { info, .. } => (format!("Version {} is ready to install", info.version), ACCENT_HI),
                            UpdateState::Failed { error, .. } => (format!("Update check failed: {error}"), RED),
                        };
                        if matches!(self.update, UpdateState::Checking) {
                            spinner(ui, 16.0, ACCENT_HI);
                        }
                        ui.label(RichText::new(text).font(semibold(13.0)).color(color));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            match &self.update {
                                UpdateState::Available(_) | UpdateState::Ready { .. } | UpdateState::Downloading { .. } => {
                                    if primary_button(ui, &format!("{}  Update", ph::ROCKET_LAUNCH), vec2(120.0, 34.0), true).clicked() {
                                        action = Some("show_update");
                                    }
                                }
                                _ => {
                                    if secondary_button(ui, "Check now", vec2(110.0, 34.0), !matches!(self.update, UpdateState::Checking)).clicked() {
                                        action = Some("check");
                                    }
                                }
                            }
                        });
                    });
                });
                vgap(ui);

                // Install.
                full_card(ui, |ui| {
                    card_header(ui, ph::FOLDER_OPEN, "Install", "where the app and its data live", |_| {});
                    let installed = install::running_installed();
                    let where_ = if installed { "Installed in" } else if install::portable() { "Portable copy, data in" } else { "Not installed; data in" };
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(where_).color(MUTED));
                        if ui.link(RichText::new(self.root.display().to_string()).font(mono(12.5))).on_hover_text("Open the folder").clicked() {
                            let _ = open::that_detached(&self.root);
                        }
                    });
                    if installed {
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Removes the app; asks before deleting your cache and settings.").font(regular(12.5)).color(FAINT));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if danger_button(ui, &format!("{}  Uninstall", ph::TRASH), vec2(120.0, 34.0), true).clicked() {
                                    action = Some("uninstall");
                                }
                            });
                        });
                    }
                });
                vgap(ui);

                // About.
                full_card(ui, |ui| {
                    ui.horizontal(|ui| {
                        if let Some(icon) = &self.icon {
                            ui.add(egui::Image::new(icon).fit_to_exact_size(vec2(54.0, 54.0)));
                        }
                        ui.vertical(|ui| {
                            ui.label(RichText::new("Akulav Tracker").font(black(20.0)));
                            ui.add_space(-6.0);
                            ui.label(RichText::new(format!("Version {}  ·  made by {} {}", update::CURRENT_VERSION, update::AUTHOR, ph::HEART)).color(MUTED));
                            ui.add_space(-6.0);
                            ui.label(RichText::new("No telemetry. Talks only to GitHub.").font(regular(12.0)).color(FAINT));
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if secondary_button(ui, &format!("{}  GitHub", ph::GITHUB_LOGO), vec2(110.0, 34.0), true).clicked() {
                                open_url(update::REPO_URL);
                            }
                        });
                    });
                    ui.add_space(8.0);
                    ui.label(RichText::new("Shortcuts").font(semibold(13.0)));
                    for (k, d) in [
                        ("Ctrl+1 … 4", "Overview, Repositories, Maintenance, Settings"),
                        ("Ctrl+F  or  /", "Search repositories"),
                        ("↑ ↓  PgUp PgDn  Enter", "Move through the list and open a repository"),
                        ("Esc  Backspace  Alt+←  Mouse back", "Go back"),
                        ("F5  Ctrl+R", "Sync (Shift+F5 refetches everything)"),
                    ] {
                        ui.horizontal(|ui| {
                            ui.add_sized(vec2(230.0, 18.0), egui::Label::new(RichText::new(k).font(mono(12.0)).color(ACCENT_HI)));
                            ui.label(RichText::new(d).font(regular(12.5)).color(MUTED));
                        });
                        ui.add_space(-6.0);
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

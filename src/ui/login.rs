//! Sign-in screen: GitHub device flow, GitHub CLI token or a personal access token.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use egui::{vec2, Align, Align2, CornerRadius, Layout, Margin, RichText, Sense, Stroke, StrokeKind};
use egui_phosphor::regular as ph;

use super::app::App;
use super::theme::*;
use super::widgets::*;
use crate::backend::auth;
use crate::backend::jobs;
use crate::backend::update;

const NEW_TOKEN_URL: &str = "https://github.com/settings/tokens/new?scopes=repo,read:user,read:org&description=Akulav%20Tracker";

impl App {
    pub fn login_page(&mut self, ui: &mut egui::Ui) {
        let client_id = auth::client_id();
        let icon = self.icon.clone();
        let Some(login) = self.login_mut() else { return };
        let mut action: Option<&'static str> = None;

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space((ui.available_height() * 0.5 - 300.0).max(28.0));
            ui.vertical_centered(|ui| {
                // Hero.
                let (r, _) = ui.allocate_exact_size(vec2(84.0, 84.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 70.0, with_alpha(ACCENT, 10));
                ui.painter().circle_filled(r.center(), 52.0, with_alpha(ACCENT, 14));
                if let Some(icon) = &icon {
                    egui::Image::new(icon).paint_at(ui, r);
                }
                ui.add_space(10.0);
                ui.label(RichText::new("Akulav Tracker").font(black(34.0)).color(TEXT));
                ui.add_space(-6.0);
                ui.label(RichText::new("Every GitHub repository you own, at a glance — and which ones need you.").font(regular(15.0)).color(MUTED));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let items = [(ph::CHART_LINE_UP, "Commit activity"), (ph::CODE, "Lines of code"), (ph::HEARTBEAT, "Maintenance radar"), (ph::LOCK_SIMPLE, "Private repos too")];
                    let w: f32 = 4.0 * 150.0;
                    ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
                    for (i, t) in items {
                        chip(ui, i, t, MUTED);
                    }
                });
                ui.add_space(22.0);

                card().inner_margin(Margin::same(26)).show(ui, |ui| {
                    ui.set_width(440.0);
                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                        if let Some(dc) = login.device.clone() {
                            device_panel(ui, login, &dc, &mut action);
                            return;
                        }
                        kicker(ui, "Sign in");
                        ui.label(RichText::new("Connect your GitHub account").font(bold(19.0)));
                        ui.add_space(-4.0);
                        ui.label(RichText::new("Read-only: the app only reads repositories and statistics. Your token stays on this computer.").font(regular(12.5)).color(MUTED));
                        ui.add_space(14.0);

                        if login.busy {
                            ui.horizontal(|ui| {
                                spinner(ui, 20.0, ACCENT_HI);
                                ui.label(RichText::new("Signing you in…").font(semibold(14.0)));
                            });
                            ui.add_space(6.0);
                            if ghost_button(ui, "Cancel").clicked() {
                                action = Some("cancel");
                            }
                            return;
                        }

                        let full = vec2(ui.available_width(), 42.0);
                        if client_id.is_some() {
                            if primary_button(ui, &format!("{}  Sign in with GitHub", ph::GITHUB_LOGO), full, true).clicked() {
                                action = Some("device");
                            }
                            ui.add_space(4.0);
                        }
                        if login.gh_token.is_some() {
                            let btn = if client_id.is_some() { secondary_button(ui, &format!("{}  Use my GitHub CLI sign-in", ph::TERMINAL_WINDOW), full, true) } else { primary_button(ui, &format!("{}  Use my GitHub CLI sign-in", ph::TERMINAL_WINDOW), full, true) };
                            if btn.on_hover_text("gh is installed and signed in on this computer; reuse its token").clicked() {
                                action = Some("gh");
                            }
                            ui.add_space(4.0);
                        }

                        if client_id.is_some() || login.gh_token.is_some() {
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            let w = ui.available_width();
                            let y = ui.cursor().center().y + 9.0;
                            let x0 = ui.cursor().left();
                            ui.painter().hline(x0..=x0 + w * 0.5 - 70.0, y, Stroke::new(1.0_f32, BORDER));
                            ui.painter().hline(x0 + w * 0.5 + 70.0..=x0 + w, y, Stroke::new(1.0_f32, BORDER));
                            ui.allocate_exact_size(vec2(w, 18.0), Sense::hover());
                            ui.painter().text(egui::pos2(x0 + w / 2.0, y), Align2::CENTER_CENTER, "or use a token", regular(12.0), FAINT);
                        });
                        ui.add_space(8.0);
                        }

                        ui.label(RichText::new("Personal access token").font(semibold(13.0)));
                        let edit = egui::TextEdit::singleline(&mut login.pat)
                            .password(true)
                            .hint_text("ghp_… or github_pat_…")
                            .desired_width(f32::INFINITY)
                            .font(mono(13.0))
                            .margin(Margin::symmetric(10, 9));
                        let resp = ui.add(edit);
                        if resp.changed() {
                            login.error = None;
                        }
                        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if ui.link(RichText::new(format!("{} Create a token", ph::ARROW_SQUARE_OUT)).font(regular(12.5))).on_hover_text("Opens GitHub with the needed scopes (repo, read:user, read:org) preselected").clicked() {
                                open_url(NEW_TOKEN_URL);
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let valid = auth::looks_like_token(&login.pat);
                                if secondary_button(ui, &format!("Continue  {}", ph::ARROW_RIGHT), vec2(130.0, 34.0), valid).clicked() || (enter && valid) {
                                    action = Some("pat");
                                }
                            });
                        });
                        if client_id.is_none() && login.gh_token.is_none() {
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new("Tip: classic tokens need the repo scope for private repositories. Fine-grained tokens need read access to Contents and Metadata.")
                                    .font(regular(11.5))
                                    .color(FAINT),
                            );
                        }
                        if let Some(e) = &login.error {
                            ui.add_space(8.0);
                            ui.label(RichText::new(format!("{}  {e}", ph::WARNING)).font(regular(12.5)).color(RED));
                        }
                    });
                });
                ui.add_space(18.0);
                ui.label(RichText::new(format!("Version {}  ·  made by {} {}", update::CURRENT_VERSION, update::AUTHOR, ph::HEART)).font(regular(11.5)).color(FAINT));
            });
        });

        let Some(login) = self.login_mut() else { return };
        match action {
            Some("device") => {
                if let Some(id) = client_id {
                    login.error = None;
                    login.busy = true;
                    login.cancel = Arc::new(AtomicBool::new(false));
                    let cancel = login.cancel.clone();
                    jobs::sign_in_with_device(self.rt.handle(), self.bus.clone(), self.http.clone(), id, cancel);
                }
            }
            Some("gh") => {
                if let Some(t) = login.gh_token.clone() {
                    login.busy = true;
                    login.error = None;
                    jobs::sign_in_with_token(self.rt.handle(), self.bus.clone(), self.http.clone(), t);
                }
            }
            Some("pat") => {
                let t = login.pat.trim().to_string();
                login.busy = true;
                login.error = None;
                jobs::sign_in_with_token(self.rt.handle(), self.bus.clone(), self.http.clone(), t);
            }
            Some("cancel") => {
                login.cancel.store(true, Ordering::Relaxed);
                login.busy = false;
                login.device = None;
            }
            Some("copy") => {
                if let Some(dc) = &login.device {
                    ui.ctx().copy_text(dc.user_code.clone());
                    login.copied = Some(Instant::now());
                }
            }
            _ => {}
        }
    }
}

fn device_panel(ui: &mut egui::Ui, login: &mut super::app::Login, dc: &crate::backend::auth::DeviceCode, action: &mut Option<&'static str>) {
    kicker(ui, "Sign in with GitHub");
    ui.label(RichText::new("Enter this code on GitHub").font(bold(19.0)));
    ui.add_space(-4.0);
    ui.label(RichText::new(format!("Open {} and type the code below, then approve Akulav Tracker.", dc.verification_uri)).font(regular(12.5)).color(MUTED));
    ui.add_space(14.0);

    // The code, big and copyable.
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 74.0), Sense::click());
    let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.1);
    ui.painter().rect_filled(r, CornerRadius::same(R_MD), lerp_color(SURFACE_2, SURFACE_3, hover));
    ui.painter().rect_stroke(r, CornerRadius::same(R_MD), egui::Stroke::new(1.0_f32, with_alpha(ACCENT, 90)), StrokeKind::Inside);
    ui.painter().text(r.center() - vec2(0.0, 6.0), Align2::CENTER_CENTER, &dc.user_code, egui::FontId::monospace(32.0), ACCENT_HI);
    let copied = login.copied.is_some_and(|t| t.elapsed().as_secs_f32() < 2.0);
    ui.painter().text(r.center_bottom() - vec2(0.0, 12.0), Align2::CENTER_CENTER, if copied { "Copied!" } else { "Click to copy" }, regular(11.0), FAINT);
    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        *action = Some("copy");
    }
    ui.add_space(12.0);
    let full = vec2(ui.available_width(), 40.0);
    if primary_button(ui, &format!("{}  Copy code & open GitHub", ph::ARROW_SQUARE_OUT), full, true).clicked() {
        *action = Some("copy");
        open_url(&dc.verification_uri);
    }
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        spinner(ui, 18.0, ACCENT_HI);
        let left = login.device_started.map(|t| dc.expires_in.saturating_sub(t.elapsed().as_secs())).unwrap_or(dc.expires_in);
        ui.label(RichText::new(format!("Waiting for approval…  code expires in {}:{:02}", left / 60, left % 60)).font(regular(12.5)).color(MUTED));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ghost_button(ui, "Cancel").clicked() {
                *action = Some("cancel");
            }
        });
    });
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
}

//! Maintenance radar: how recent each repository's last commit is, so the ones that need you most
//! stand out.

use chrono::Utc;
use egui::{pos2, vec2, Align2, CornerRadius, Rect, RichText, Sense, Stroke, Ui};
use egui_phosphor::regular as ph;

use super::app::App;
use super::charts::{self, TimelineItem};
use super::derive::RepoInfo;
use super::overview::{columns, page};
use super::theme::*;
use super::widgets::*;
use crate::backend::stats::{self, Freshness};

impl App {
    pub fn maintenance_page(&mut self, ui: &mut Ui) {
        let now = Utc::now();
        let mut infos = self.infos(self.counted(), now);
        infos.sort_by(|a, b| b.attention.cmp(&a.attention).then(b.days.cmp(&a.days)));
        let mut open = None;
        let mut toggled = false;

        page(ui, "maintenance", |ui| {
            page_header(ui, "Maintenance", "Maintenance radar", Some("Every repository by how long ago it saw a commit. The further right, the more it needs you."));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                toggled |= toggle(ui, &mut self.settings.include_forks).changed();
                ui.label(RichText::new("Include forks").font(regular(12.5)).color(MUTED));
                ui.add_space(12.0);
                toggled |= toggle(ui, &mut self.settings.include_archived).changed();
                ui.label(RichText::new("Include archived").font(regular(12.5)).color(MUTED));
            });
            ui.add_space(GAP);

            if infos.is_empty() {
                super::overview::empty_state(ui, self.sync.running);
                return;
            }

            // Bucket tiles.
            let mut counts = [0usize; 6];
            for r in &infos {
                counts[r.fresh.index()] += 1;
            }
            let shown: Vec<Freshness> = Freshness::ALL.into_iter().filter(|f| *f != Freshness::Unknown || counts[f.index()] > 0).collect();
            columns(ui, shown.len(), GAP, |ui, i, w| {
                let f = shown[i];
                let (rect, _) = ui.allocate_exact_size(vec2(w, 78.0), Sense::hover());
                let p = ui.painter();
                let c = freshness_color(f);
                p.rect_filled(rect, CornerRadius::same(R_LG), SURFACE);
                p.rect_stroke(rect, CornerRadius::same(R_LG), Stroke::new(1.0_f32, BORDER), egui::StrokeKind::Inside);
                p.rect_filled(Rect::from_min_size(rect.min + vec2(14.0, 14.0), vec2(4.0, 50.0)), CornerRadius::same(2), c);
                p.text(rect.min + vec2(28.0, 22.0), Align2::LEFT_CENTER, f.label(), semibold(13.0), c);
                p.text(rect.min + vec2(28.0, if w >= 160.0 { 50.0 } else { 56.0 }), Align2::LEFT_CENTER, counts[f.index()].to_string(), black(24.0), TEXT);
                if w >= 160.0 {
                    p.text(pos2(rect.right() - 12.0, rect.top() + 54.0), Align2::RIGHT_CENTER, f.range(), regular(11.0), FAINT);
                } else {
                    p.text(pos2(rect.left() + 28.0, rect.top() + 36.0), Align2::LEFT_CENTER, f.range(), regular(10.5), FAINT);
                }
            });
            vgap(ui);

            // Timeline.
            full_card(ui, |ui| {
                card_header(ui, ph::CLOCK_COUNTDOWN, "Time since last commit", "log scale, today on the left · hover a dot for details, click to open", |_| {});
                let items: Vec<TimelineItem> = infos
                    .iter()
                    .filter_map(|r| {
                        r.days.map(|d| TimelineItem {
                            id: r.id,
                            name: r.full_name.clone(),
                            days: d,
                            color: if r.archived { FAINT } else { freshness_color(r.fresh) },
                            note: format!("{}{} open issues", if r.archived { "archived · " } else { "" }, r.open_issues),
                        })
                    })
                    .collect();
                if let Some(id) = charts::freshness_timeline(ui, &items) {
                    open = Some(id);
                }
            });
            vgap(ui);

            // Ranked list.
            full_card(ui, |ui| {
                card_header(ui, ph::LIST_NUMBERS, "Ranked by need", "attention score: mostly time since the last commit, plus open issues and pull requests", |_| {});
                for (rank, r) in infos.iter().enumerate() {
                    if attention_row(ui, rank + 1, r, now) {
                        open = Some(r.id);
                    }
                }
            });
        });

        if toggled {
            self.mark_dirty();
        }
        if let Some(id) = open {
            self.open_repo(id);
        }
    }
}

fn attention_row(ui: &mut Ui, rank: usize, r: &RepoInfo, now: chrono::DateTime<Utc>) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 50.0), Sense::click());
    let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.08);
    let p = ui.painter();
    if hover > 0.0 {
        p.rect_filled(rect, CornerRadius::same(R_MD), with_alpha(egui::Color32::WHITE, (hover * 8.0) as u8));
    }
    let c = if r.archived { FAINT } else { freshness_color(r.fresh) };
    let cy = rect.center().y;
    p.text(pos2(rect.left() + 22.0, cy), Align2::CENTER_CENTER, rank.to_string(), semibold(12.0), FAINT);
    p.text(pos2(rect.left() + 44.0, cy - 8.0), Align2::LEFT_CENTER, charts::truncate(&r.full_name, 46), semibold(14.0), TEXT);
    let reason = match (r.days, r.archived) {
        (_, true) => "Archived".to_string(),
        (None, _) => "No commits yet".to_string(),
        (Some(0), _) => "Committed today".to_string(),
        (Some(d), _) => format!("No commits for {} day{}", stats::grouped(d as u64), if d == 1 { "" } else { "s" }),
    };
    let mut sub = reason;
    if r.open_issues > 0 {
        sub.push_str(&format!("  ·  {} open issues/PRs", r.open_issues));
    }
    if let Some(t) = r.last {
        sub.push_str(&format!("  ·  last {}", t.with_timezone(&chrono::Local).format("%b %-d, %Y")));
    }
    let _ = now;

    // Score bar on the right; it shrinks (and finally goes) when the row is narrow.
    let bar_w = ((rect.width() - 560.0) * 0.6).clamp(0.0, 160.0);
    let bar = Rect::from_min_size(pos2(rect.right() - bar_w - 58.0, cy - 4.0), vec2(bar_w, 8.0));
    if bar_w >= 40.0 {
        p.rect_filled(bar, CornerRadius::same(255), SURFACE_3);
        let fw = bar_w * r.attention as f32 / 100.0;
        if fw > 0.5 {
            p.rect_filled(Rect::from_min_size(bar.min, vec2(fw.max(8.0), 8.0)), CornerRadius::same(255), c);
        }
    }
    let label_right = if bar_w >= 40.0 { bar.left() - 10.0 } else { rect.right() - 58.0 };
    let label = p.layout_no_wrap(r.fresh.label().to_string(), semibold(12.0), c);
    let label_left = label_right - label.size().x;
    p.galley(pos2(label_left, cy - label.size().y / 2.0), label, c);
    p.text(pos2(rect.right() - 14.0, cy), Align2::RIGHT_CENTER, r.attention.to_string(), bold(15.0), c);
    // Text on the left stops before the label.
    let chars = ((label_left - rect.left() - 60.0) / 6.4).max(10.0) as usize;
    p.text(pos2(rect.left() + 44.0, cy + 10.0), Align2::LEFT_CENTER, charts::truncate(&sub, chars), regular(11.5), MUTED);
    p.hline(rect.left() + 10.0..=rect.right() - 10.0, rect.bottom(), Stroke::new(1.0_f32, with_alpha(BORDER, 120)));
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

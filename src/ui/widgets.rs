//! Small reusable pieces: buttons, cards, chips, stat tiles, toggles.

use egui::{pos2, vec2, Align2, Color32, CornerRadius, Frame, Margin, Rect, Response, RichText, Sense, Stroke, StrokeKind, Ui};

use super::theme::*;

/// The one gutter used between cards and tiles, horizontally and vertically.
pub const GAP: f32 = 12.0;
const CARD_PAD: i8 = 18;

pub fn card() -> Frame {
    glass_frame().inner_margin(Margin::same(CARD_PAD))
}

/// Vertical space so consecutive blocks sit exactly [`GAP`] apart.
pub fn vgap(ui: &mut Ui) {
    ui.add_space(GAP - ui.spacing().item_spacing.y);
}

/// Open a web page in the browser. Only http(s) URLs: links come from the API and the cache on
/// disk, and must never launch a local file or program.
pub fn open_url(url: &str) {
    let u = url.trim();
    if (u.starts_with("https://") || u.starts_with("http://")) && !u.chars().any(|c| c.is_whitespace() || c.is_control()) {
        let _ = open::that_detached(u);
    }
}

/// Width of a cell spanning `span` of `cols` equal columns in `total` width.
pub fn span_width(total: f32, cols: usize, span: usize) -> f32 {
    let unit = (total - GAP * (cols as f32 - 1.0)) / cols as f32;
    (unit * span as f32 + GAP * (span as f32 - 1.0)).max(0.0)
}

/// A row of cards on a `cols`-column grid; card `i` spans `spans[i]` columns. All cards in the
/// row get the height of the tallest one (measured on the previous frame).
pub fn card_row(ui: &mut Ui, id: &str, cols: usize, spans: &[usize], mut add: impl FnMut(&mut Ui, usize)) {
    let id = ui.id().with(("card_row", id));
    let target: f32 = ui.data(|d| d.get_temp(id)).unwrap_or(0.0);
    let total = ui.available_width();
    let mut tallest = 0.0f32;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = GAP;
        for (i, span) in spans.iter().enumerate() {
            let w = span_width(total, cols, *span);
            ui.allocate_ui_with_layout(vec2(w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                card().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    // `set_min_height` counts from the cursor, so it must come before the content.
                    // The cursor then still tells where the content really ends.
                    let top = ui.cursor().top();
                    ui.set_min_height(target);
                    ui.data_mut(|d| {
                        d.insert_temp(card_bottom_id(), Some(top + target));
                        d.insert_temp(card_spacer_id(), 0.0f32);
                    });
                    add(ui, i);
                    // Space added by `bottom_spacer` isn't content; leave it out so rows can shrink.
                    let spacer: f32 = ui.data(|d| d.get_temp(card_spacer_id())).unwrap_or(0.0);
                    ui.data_mut(|d| d.insert_temp(card_bottom_id(), None::<f32>));
                    let natural = ui.cursor().top() - ui.spacing().item_spacing.y - top - spacer;
                    tallest = tallest.max(natural);
                });
            });
        }
    });
    if (tallest - target).abs() > 0.5 {
        ui.data_mut(|d| d.insert_temp(id, tallest));
        ui.ctx().request_repaint();
    }
}

fn card_bottom_id() -> egui::Id {
    egui::Id::new("card_row_bottom")
}
fn card_spacer_id() -> egui::Id {
    egui::Id::new("card_row_spacer")
}

/// Inside a [`card_row`] card: push what follows down so a block `reserve` tall ends at the
/// card's bottom, lining up action bars across the row. Does nothing elsewhere.
pub fn bottom_spacer(ui: &mut Ui, reserve: f32) {
    let Some(bottom) = ui.data(|d| d.get_temp::<Option<f32>>(card_bottom_id())).flatten() else {
        return;
    };
    let space = bottom - reserve - ui.cursor().top();
    if space > 0.5 {
        ui.add_space(space);
        ui.data_mut(|d| *d.get_temp_mut_or(card_spacer_id(), 0.0f32) += space);
    }
}

/// Small uppercase label above a heading.
pub fn kicker(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text.to_uppercase()).font(bold(11.0)).color(FAINT).extra_letter_spacing(1.2));
}

pub fn section_title(ui: &mut Ui, icon: &str, title: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(icon).font(regular(17.0)).color(ACCENT_HI));
        ui.label(RichText::new(title).font(bold(16.0)).color(TEXT));
    });
}

/// Card heading: icon and title, a faint line below it, and optional controls on the right.
/// Every card uses it, so headings line up across a row.
pub fn card_header(ui: &mut Ui, icon: &str, title: &str, sub: &str, right: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            section_title(ui, icon, title);
            ui.add_space(-6.0);
            ui.label(RichText::new(if sub.is_empty() { " " } else { sub }).font(regular(12.0)).color(FAINT));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), right);
    });
    ui.add_space(4.0);
}

pub fn page_header(ui: &mut Ui, kick: &str, title: &str, sub: Option<&str>) {
    kicker(ui, kick);
    ui.add_space(-4.0);
    ui.label(RichText::new(title).font(black(28.0)).color(TEXT));
    if let Some(s) = sub {
        ui.add_space(-4.0);
        ui.label(RichText::new(s).color(MUTED));
    }
}

fn button_base(ui: &mut Ui, text: &str, size: egui::Vec2, enabled: bool, fill: Color32, hover_fill: Color32, fg: Color32, stroke: Option<Color32>) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let hover = ui.ctx().animate_bool_with_time(resp.id, enabled && resp.hovered(), 0.12);
    let press = enabled && resp.is_pointer_button_down_on();
    let mut bg = lerp_color(fill, hover_fill, hover);
    if !enabled {
        bg = with_alpha(bg, 90);
    }
    let r = if press { rect.shrink(1.0) } else { rect };
    ui.painter().rect_filled(r, CornerRadius::same(R_MD), bg);
    if let Some(s) = stroke {
        ui.painter().rect_stroke(r, CornerRadius::same(R_MD), Stroke::new(1.0_f32, s), StrokeKind::Inside);
    }
    let fg = if enabled { fg } else { with_alpha(fg, 110) };
    ui.painter().text(r.center(), Align2::CENTER_CENTER, text, semibold(14.0), fg);
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

pub fn primary_button(ui: &mut Ui, text: &str, size: egui::Vec2, enabled: bool) -> Response {
    button_base(ui, text, size, enabled, ACCENT, ACCENT_HI, Color32::from_rgb(4, 30, 22), None)
}

pub fn secondary_button(ui: &mut Ui, text: &str, size: egui::Vec2, enabled: bool) -> Response {
    // Hyve's .btn: translucent white, a bit brighter on hover.
    button_base(ui, text, size, enabled, with_alpha(Color32::WHITE, 15), with_alpha(Color32::WHITE, 29), TEXT, Some(with_alpha(Color32::WHITE, 40)))
}

pub fn danger_button(ui: &mut Ui, text: &str, size: egui::Vec2, enabled: bool) -> Response {
    button_base(ui, text, size, enabled, with_alpha(RED, 30), with_alpha(RED, 60), RED, Some(with_alpha(RED, 90)))
}

/// A full-width card (the single-column counterpart of [`card_row`]).
pub fn full_card(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        add(ui)
    });
}

/// Text-only button sized to its label.
pub fn ghost_button(ui: &mut Ui, text: &str) -> Response {
    let galley = ui.painter().layout_no_wrap(text.to_string(), semibold(13.5), MUTED);
    let size = vec2(galley.size().x + 24.0, 34.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.1);
    if hover > 0.0 {
        ui.painter().rect_filled(rect, CornerRadius::same(R_MD), with_alpha(Color32::WHITE, (hover * 10.0) as u8));
    }
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, semibold(13.5), lerp_color(MUTED, TEXT, hover));
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Round icon button.
pub fn icon_button(ui: &mut Ui, icon: &str, tip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
    let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.1);
    ui.painter().rect_filled(rect, CornerRadius::same(R_MD), with_alpha(Color32::WHITE, (15.0 + 14.0 * hover) as u8));
    ui.painter().rect_stroke(rect, CornerRadius::same(R_MD), Stroke::new(1.0_f32, with_alpha(Color32::WHITE, (38.0 + 22.0 * hover) as u8)), StrokeKind::Inside);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon, regular(16.0), lerp_color(MUTED, TEXT, hover));
    resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Rounded pill with an optional icon.
pub fn chip(ui: &mut Ui, icon: &str, text: &str, color: Color32) -> Response {
    let font = semibold(12.0);
    let label = if icon.is_empty() { text.to_string() } else { format!("{icon} {text}") };
    let galley = ui.painter().layout_no_wrap(label, font, color);
    let size = vec2(galley.size().x + 18.0, 24.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(255), with_alpha(color, 28));
    ui.painter().rect_stroke(rect, CornerRadius::same(255), Stroke::new(1.0_f32, with_alpha(color, 60)), StrokeKind::Inside);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
    resp
}

/// Selectable filter pill.
pub fn filter_pill(ui: &mut Ui, text: &str, count: Option<usize>, active: bool) -> Response {
    let label = match count {
        Some(c) => format!("{text}  {c}"),
        None => text.to_string(),
    };
    let galley = ui.painter().layout_no_wrap(label, semibold(12.5), TEXT);
    let size = vec2(galley.size().x + 22.0, 28.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.1);
    let (bg, stroke, fg) = if active {
        (with_alpha(ACCENT, 40), with_alpha(ACCENT, 140), ACCENT_HI)
    } else {
        (with_alpha(Color32::WHITE, (12.0 + 12.0 * hover) as u8), with_alpha(Color32::WHITE, 36), lerp_color(MUTED, TEXT, hover))
    };
    ui.painter().rect_filled(rect, CornerRadius::same(255), bg);
    ui.painter().rect_stroke(rect, CornerRadius::same(255), Stroke::new(1.0_f32, stroke), StrokeKind::Inside);
    let g = ui.painter().layout_no_wrap(galley.text().to_string(), semibold(12.5), fg);
    ui.painter().galley(rect.center() - g.size() / 2.0, g, fg);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A KPI tile: icon, big value, label and a small note.
pub fn stat_tile(ui: &mut Ui, width: f32, icon: &str, color: Color32, value: &str, label: &str, note: &str) -> Response {
    let h = 104.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, h), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(R_LG), GLASS);
    p.rect_stroke(rect, CornerRadius::same(R_LG), Stroke::new(1.0_f32, GLASS_BORDER), StrokeKind::Inside);
    // Soft glow in the corner.
    let glow = Rect::from_min_size(rect.right_top() + vec2(-70.0, 0.0), vec2(70.0, 70.0));
    p.with_clip_rect(rect.shrink(1.0)).circle_filled(glow.center() + vec2(20.0, -20.0), 46.0, with_alpha(color, 14));
    let ib = Rect::from_min_size(rect.min + vec2(16.0, 16.0), vec2(30.0, 30.0));
    p.rect_filled(ib, CornerRadius::same(R_SM + 2), with_alpha(color, 34));
    p.text(ib.center(), Align2::CENTER_CENTER, icon, regular(16.0), color);
    p.text(pos2(ib.right() + 10.0, ib.center().y), Align2::LEFT_CENTER, label, semibold(12.5), MUTED);
    p.text(pos2(rect.left() + 16.0, rect.bottom() - 34.0), Align2::LEFT_CENTER, value, black(26.0), TEXT);
    if !note.is_empty() {
        p.text(pos2(rect.left() + 16.0, rect.bottom() - 13.0), Align2::LEFT_CENTER, note, regular(11.5), FAINT);
    }
    resp
}

/// iOS-style switch.
pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(40.0, 22.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(resp.id, *on, 0.15);
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(255), lerp_color(with_alpha(Color32::WHITE, 34), ACCENT, t));
    p.rect_stroke(rect, CornerRadius::same(255), Stroke::new(1.0_f32, with_alpha(Color32::WHITE, (46.0 * (1.0 - t)) as u8)), StrokeKind::Inside);
    let x = egui::lerp(rect.left() + 11.0..=rect.right() - 11.0, t);
    p.circle_filled(pos2(x, rect.center().y), 8.0, Color32::WHITE);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Label + description on the left, a toggle on the right. Returns true when changed.
pub fn setting_row(ui: &mut Ui, title: &str, desc: &str, on: &mut bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width(ui.available_width() - 60.0);
            ui.label(RichText::new(title).font(semibold(14.0)).color(TEXT));
            ui.add_space(-6.0);
            ui.label(RichText::new(desc).font(regular(12.5)).color(MUTED));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            changed = toggle(ui, on).changed();
        });
    });
    changed
}

pub fn progress_bar(ui: &mut Ui, frac: Option<f32>, height: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(255), SURFACE_3);
    match frac {
        Some(f) => {
            let w = rect.width() * f.clamp(0.0, 1.0);
            if w > 0.5 {
                p.rect_filled(Rect::from_min_size(rect.min, vec2(w.max(height), height)), CornerRadius::same(255), ACCENT);
            }
        }
        None => {
            // Indeterminate: a sliding segment.
            let t = ui.input(|i| i.time) as f32;
            let seg = rect.width() * 0.3;
            let x = rect.left() + ((t * 0.8).fract() * (rect.width() + seg)) - seg;
            let r = Rect::from_min_max(pos2(x.max(rect.left()), rect.top()), pos2((x + seg).min(rect.right()), rect.bottom()));
            if r.width() > 0.0 {
                p.rect_filled(r, CornerRadius::same(255), ACCENT);
            }
            ui.ctx().request_repaint();
        }
    }
}

/// Spinning arc.
pub fn spinner(ui: &mut Ui, size: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let t = ui.input(|i| i.time) as f32;
    let c = rect.center();
    let r = size * 0.4;
    let start = t * 5.0;
    let pts: Vec<egui::Pos2> = (0..=24)
        .map(|i| {
            let a = start + i as f32 / 24.0 * 4.4;
            c + vec2(a.cos(), a.sin()) * r
        })
        .collect();
    ui.painter().add(egui::Shape::line(pts, Stroke::new(2.2_f32, color)));
    ui.ctx().request_repaint();
}

/// A colored dot followed by text.
pub fn dot_label(ui: &mut Ui, color: Color32, text: &str, text_color: Color32) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let (r, _) = ui.allocate_exact_size(vec2(10.0, 16.0), Sense::hover());
        ui.painter().circle_filled(r.center(), 4.5, color);
        ui.label(RichText::new(text).font(regular(12.5)).color(text_color));
    });
}

/// Paint a texture as a circle (e.g. an avatar), or a placeholder with an initial.
pub fn avatar(ui: &mut Ui, tex: Option<&egui::TextureHandle>, size: f32, fallback: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    paint_avatar(ui.painter(), rect, tex, fallback);
    resp
}

pub fn paint_avatar(p: &egui::Painter, rect: Rect, tex: Option<&egui::TextureHandle>, fallback: &str) {
    match tex {
        Some(t) => {
            // A textured triangle fan: a round image without any masking.
            let mut mesh = egui::Mesh::with_texture(t.id());
            let c = rect.center();
            let r = rect.width() / 2.0;
            mesh.vertices.push(egui::epaint::Vertex { pos: c, uv: pos2(0.5, 0.5), color: Color32::WHITE });
            let n = 48;
            for i in 0..=n {
                let a = i as f32 / n as f32 * std::f32::consts::TAU;
                let (s, co) = a.sin_cos();
                mesh.vertices.push(egui::epaint::Vertex { pos: c + vec2(co, s) * r, uv: pos2(0.5 + co * 0.5, 0.5 + s * 0.5), color: Color32::WHITE });
            }
            for i in 1..=n as u32 {
                mesh.add_triangle(0, i, i + 1);
            }
            p.add(egui::Shape::mesh(mesh));
        }
        None => {
            p.circle_filled(rect.center(), rect.width() / 2.0, SURFACE_3);
            let initial = fallback.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
            p.text(rect.center(), Align2::CENTER_CENTER, initial, bold(rect.width() * 0.45), MUTED);
        }
    }
    p.circle_stroke(rect.center(), rect.width() / 2.0, Stroke::new(1.0_f32, with_alpha(Color32::WHITE, 18)));
}

/// Horizontal gradient fill.
pub fn hgradient(painter: &egui::Painter, rect: Rect, left: Color32, right: Color32) {
    use egui::epaint::{Mesh, Vertex, WHITE_UV};
    let mut mesh = Mesh::default();
    for (pos, c) in [(rect.left_top(), left), (rect.right_top(), right), (rect.right_bottom(), right), (rect.left_bottom(), left)] {
        mesh.vertices.push(Vertex { pos, uv: WHITE_UV, color: c });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
}

/// Rounded rect with a horizontal gradient. Each cap is clipped to its own strip so translucent colors don't
/// overlap (no visible bands).
pub fn hgradient_rounded(p: &egui::Painter, rect: Rect, left: Color32, right: Color32, radius: u8) {
    let r = radius as f32;
    if rect.width() <= 2.0 * r {
        p.rect_filled(rect, CornerRadius::same(radius), lerp_color(left, right, 0.5));
        return;
    }
    let left_strip = Rect::from_min_max(rect.min, pos2(rect.min.x + r, rect.max.y));
    let right_strip = Rect::from_min_max(pos2(rect.max.x - r, rect.min.y), rect.max);
    p.with_clip_rect(left_strip.intersect(p.clip_rect())).rect_filled(
        Rect::from_min_max(rect.min, pos2(rect.min.x + 2.0 * r, rect.max.y)),
        CornerRadius { nw: radius, sw: radius, ne: 0, se: 0 },
        left,
    );
    p.with_clip_rect(right_strip.intersect(p.clip_rect())).rect_filled(
        Rect::from_min_max(pos2(rect.max.x - 2.0 * r, rect.min.y), rect.max),
        CornerRadius { nw: 0, sw: 0, ne: radius, se: radius },
        right,
    );
    hgradient(p, Rect::from_min_max(pos2(rect.min.x + r, rect.min.y), pos2(rect.max.x - r, rect.max.y)), left, right);
}

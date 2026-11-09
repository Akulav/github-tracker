//! Hand-painted charts: sparkline, area chart, contribution heatmap, stacked bar, donut, code
//! frequency and the freshness timeline. Each draws instantly-following hover tooltips of its own.

use chrono::{DateTime, Datelike, Utc};
use egui::{pos2, vec2, Align2, Color32, CornerRadius, Id, LayerId, Mesh, Order, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui};

use super::theme::*;
use crate::backend::stats;

/// Tooltip box next to the pointer. Lines are (text, color, bold).
pub fn tip(ctx: &egui::Context, at: Pos2, lines: &[(String, Color32, bool)]) {
    let p = ctx.layer_painter(LayerId::new(Order::Tooltip, Id::new("chart_tip")));
    let galleys: Vec<_> = lines
        .iter()
        .map(|(t, c, b)| p.layout_no_wrap(t.clone(), if *b { semibold(13.0) } else { regular(12.0) }, *c))
        .collect();
    let w = galleys.iter().map(|g| g.size().x).fold(0.0, f32::max) + 20.0;
    let h = galleys.iter().map(|g| g.size().y + 2.0).sum::<f32>() + 14.0;
    let screen = ctx.screen_rect();
    let mut min = at + vec2(14.0, 14.0);
    if min.x + w > screen.right() - 6.0 {
        min.x = at.x - w - 14.0;
    }
    if min.y + h > screen.bottom() - 6.0 {
        min.y = at.y - h - 14.0;
    }
    let rect = Rect::from_min_size(min, vec2(w, h));
    p.add(egui::epaint::RectShape::filled(rect.translate(vec2(0.0, 4.0)), CornerRadius::same(R_MD), Color32::from_black_alpha(90)));
    p.rect_filled(rect, CornerRadius::same(R_MD), SURFACE_3);
    p.rect_stroke(rect, CornerRadius::same(R_MD), Stroke::new(1.0_f32, Color32::from_rgb(60, 66, 80)), StrokeKind::Inside);
    let mut y = rect.top() + 7.0;
    for g in galleys {
        let gh = g.size().y;
        p.galley(pos2(rect.left() + 10.0, y), g, TEXT);
        y += gh + 2.0;
    }
}

fn week_label(ts: i64) -> String {
    DateTime::<Utc>::from_timestamp(ts, 0).map(|d| format!("Week of {}", d.format("%b %-d, %Y"))).unwrap_or_default()
}

fn plural(n: u64, one: &str) -> String {
    format!("{} {one}{}", stats::grouped(n), if n == 1 { "" } else { "s" })
}

/// Filled polygon under a polyline down to `base_y` (works for non-convex outlines). The gradient
/// runs from `top` at the highest point to `bottom` at the base, the same for every column.
fn fill_under(points: &[Pos2], base_y: f32, top: Color32, bottom: Color32) -> Shape {
    let mut mesh = Mesh::default();
    let top_y = points.iter().map(|p| p.y).fold(base_y, f32::min);
    let span = (base_y - top_y).max(1.0);
    let at = |y: f32| lerp_color(bottom, top, (base_y - y) / span);
    for w in points.windows(2) {
        let i = mesh.vertices.len() as u32;
        mesh.colored_vertex(w[0], at(w[0].y));
        mesh.colored_vertex(w[1], at(w[1].y));
        mesh.colored_vertex(pos2(w[1].x, base_y), bottom);
        mesh.colored_vertex(pos2(w[0].x, base_y), bottom);
        mesh.add_triangle(i, i + 1, i + 2);
        mesh.add_triangle(i, i + 2, i + 3);
    }
    Shape::mesh(mesh)
}

pub fn sparkline(ui: &mut Ui, values: &[u32], size: egui::Vec2, color: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    let max = values.iter().copied().max().unwrap_or(0).max(1) as f32;
    if values.len() < 2 {
        return resp;
    }
    let pts: Vec<Pos2> = values
        .iter()
        .enumerate()
        .map(|(i, v)| pos2(rect.left() + rect.width() * i as f32 / (values.len() - 1) as f32, rect.bottom() - 1.0 - (rect.height() - 2.0) * (*v as f32 / max)))
        .collect();
    if values.iter().all(|v| *v == 0) {
        p.hline(rect.x_range(), rect.bottom() - 1.0, Stroke::new(1.0_f32, with_alpha(FAINT, 120)));
        return resp;
    }
    p.add(fill_under(&pts, rect.bottom(), with_alpha(color, 70), with_alpha(color, 0)));
    p.add(Shape::line(pts, Stroke::new(1.5_f32, color)));
    resp
}

/// Weekly totals as a smooth area chart with week tooltips.
pub fn area_chart(ui: &mut Ui, values: &[u32], first_week: i64, height: f32, color: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let p = ui.painter();
    let plot = Rect::from_min_max(rect.min + vec2(34.0, 6.0), rect.max - vec2(4.0, 20.0));
    let max = values.iter().copied().max().unwrap_or(0).max(1);
    let nice = nice_max(max);
    // Grid lines and y labels.
    for i in 0..=3 {
        let v = nice * i / 3;
        let y = plot.bottom() - plot.height() * (v as f32 / nice as f32);
        p.hline(plot.x_range(), y, Stroke::new(1.0_f32, with_alpha(BORDER, if i == 0 { 255 } else { 120 })));
        p.text(pos2(plot.left() - 8.0, y), Align2::RIGHT_CENTER, stats::compact(v as u64), regular(10.5), FAINT);
    }
    if values.len() < 2 {
        return resp;
    }
    let n = values.len();
    let x_at = |i: usize| plot.left() + plot.width() * i as f32 / (n - 1) as f32;
    let pts: Vec<Pos2> = values.iter().enumerate().map(|(i, v)| pos2(x_at(i), plot.bottom() - plot.height() * (*v as f32 / nice as f32))).collect();
    // Month labels along the bottom.
    let mut last_month = None;
    for i in 0..n {
        if let Some(d) = DateTime::<Utc>::from_timestamp(first_week + i as i64 * 604800, 0) {
            if last_month != Some(d.month()) && i > 0 && d.day() <= 7 {
                p.text(pos2(x_at(i), plot.bottom() + 5.0), Align2::CENTER_TOP, d.format("%b").to_string(), regular(10.5), FAINT);
            }
            last_month = Some(d.month());
        }
    }
    p.add(fill_under(&pts, plot.bottom(), with_alpha(color, 90), with_alpha(color, 4)));
    p.add(Shape::line(pts.clone(), Stroke::new(2.0_f32, color)));

    if let Some(hp) = resp.hover_pos().filter(|h| plot.expand2(vec2(6.0, 20.0)).contains(*h)) {
        let i = (((hp.x - plot.left()) / plot.width()) * (n - 1) as f32).round().clamp(0.0, (n - 1) as f32) as usize;
        let pt = pts[i];
        p.vline(pt.x, plot.y_range(), Stroke::new(1.0_f32, with_alpha(TEXT, 50)));
        p.circle_filled(pt, 5.0, BG);
        p.circle_stroke(pt, 5.0, Stroke::new(2.0_f32, color));
        tip(ui.ctx(), hp, &[(plural(values[i] as u64, "commit"), TEXT, true), (week_label(first_week + i as i64 * 604800), MUTED, false)]);
    }
    resp
}

fn nice_max(max: u32) -> u32 {
    let steps = [3, 6, 9, 12, 15, 30, 45, 60, 90, 120, 150, 300, 450, 600, 900, 1200, 1500, 3000, 6000, 9000, 15000, 30000];
    steps.into_iter().find(|s| *s >= max).unwrap_or(max.div_ceil(3) * 3)
}

fn heat_level(v: u32, max: u32) -> u8 {
    if v == 0 {
        return 0;
    }
    let f = v as f32 / max.max(1) as f32;
    match f {
        f if f <= 0.25 => 1,
        f if f <= 0.5 => 2,
        f if f <= 0.75 => 3,
        _ => 4,
    }
}

/// GitHub-style contribution calendar: 52 week columns × 7 day rows.
pub fn heatmap(ui: &mut Ui, first_week: i64, daily: &[u32], now: DateTime<Utc>) -> Response {
    let avail = ui.available_width();
    let left = 30.0;
    let gap = 3.0;
    let cell = ((avail - left - 4.0) / stats::WEEKS as f32 - gap).clamp(6.0, 16.0);
    let step = cell + gap;
    let grid_h = step * 7.0;
    let size = vec2(left + step * stats::WEEKS as f32, 18.0 + grid_h + 26.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(avail, size.y), Sense::hover());
    let p = ui.painter();
    let origin = rect.min + vec2(left, 18.0);
    // Clip high outliers so one huge day doesn't wash out the rest.
    let mut sorted: Vec<u32> = daily.iter().copied().filter(|v| *v > 0).collect();
    sorted.sort_unstable();
    let max = sorted.get((sorted.len() as f32 * 0.95) as usize).or(sorted.last()).copied().unwrap_or(1);
    let today = ((now.timestamp() - first_week) / 86400) as usize;

    for (i, d) in ["", "Mon", "", "Wed", "", "Fri", ""].iter().enumerate() {
        if !d.is_empty() {
            p.text(pos2(rect.left(), origin.y + step * i as f32 + cell / 2.0), Align2::LEFT_CENTER, *d, regular(10.0), FAINT);
        }
    }
    let mut last_month = None;
    let mut hovered = None;
    for w in 0..stats::WEEKS {
        let date = DateTime::<Utc>::from_timestamp(first_week + w as i64 * 604800, 0);
        if let Some(d) = date {
            if last_month != Some(d.month()) {
                if w > 0 && w < stats::WEEKS - 2 {
                    p.text(pos2(origin.x + step * w as f32, rect.top()), Align2::LEFT_TOP, d.format("%b").to_string(), regular(10.5), FAINT);
                }
                last_month = Some(d.month());
            }
        }
        for day in 0..7 {
            let idx = w * 7 + day;
            if idx > today {
                continue;
            }
            let v = daily.get(idx).copied().unwrap_or(0);
            let r = Rect::from_min_size(origin + vec2(step * w as f32, step * day as f32), vec2(cell, cell));
            p.rect_filled(r, CornerRadius::same(2), heat_color(heat_level(v, max)));
            if resp.hover_pos().is_some_and(|h| r.expand(gap / 2.0).contains(h)) {
                hovered = Some((r, idx, v));
            }
        }
    }
    // Legend.
    let ly = origin.y + grid_h + 8.0;
    let mut x = rect.left() + left + step * stats::WEEKS as f32 - (5.0 * step + 64.0);
    p.text(pos2(x, ly + cell / 2.0), Align2::LEFT_CENTER, "Less", regular(10.5), FAINT);
    x += 30.0;
    for l in 0..5u8 {
        p.rect_filled(Rect::from_min_size(pos2(x, ly), vec2(cell, cell)), CornerRadius::same(2), heat_color(l));
        x += step;
    }
    p.text(pos2(x + 4.0, ly + cell / 2.0), Align2::LEFT_CENTER, "More", regular(10.5), FAINT);
    let total: u32 = daily.iter().sum();
    p.text(pos2(rect.left() + left, ly + cell / 2.0), Align2::LEFT_CENTER, format!("{} in the last year", plural(total as u64, "commit")), regular(11.5), MUTED);

    if let Some((r, idx, v)) = hovered {
        p.rect_stroke(r.expand(1.0), CornerRadius::same(3), Stroke::new(1.5_f32, TEXT), StrokeKind::Outside);
        let date = DateTime::<Utc>::from_timestamp(first_week + idx as i64 * 86400, 0).map(|d| d.format("%A, %b %-d, %Y").to_string()).unwrap_or_default();
        tip(ui.ctx(), resp.hover_pos().unwrap(), &[(plural(v as u64, "commit"), TEXT, true), (date, MUTED, false)]);
    }
    resp
}

/// Horizontal stacked bar (languages, freshness buckets). Segments: (label, value, color).
pub fn stacked_bar(ui: &mut Ui, segments: &[(String, u64, Color32)], height: f32, unit: &dyn Fn(u64) -> String) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let p = ui.painter();
    let total: u64 = segments.iter().map(|s| s.1).sum();
    p.rect_filled(rect, CornerRadius::same(255), SURFACE_3);
    if total == 0 {
        return resp;
    }
    let mut x = rect.left();
    let mut hovered = None;
    let n = segments.iter().filter(|s| s.1 > 0).count();
    let mut k = 0;
    for (label, v, color) in segments {
        if *v == 0 {
            continue;
        }
        let w = rect.width() * (*v as f32 / total as f32);
        let r = Rect::from_min_max(pos2(x, rect.top()), pos2((x + w).min(rect.right()), rect.bottom()));
        let cr = CornerRadius {
            nw: if k == 0 { 255 } else { 0 },
            sw: if k == 0 { 255 } else { 0 },
            ne: if k == n - 1 { 255 } else { 0 },
            se: if k == n - 1 { 255 } else { 0 },
        };
        let hov = resp.hover_pos().is_some_and(|h| r.contains(h));
        p.rect_filled(r, cr, if hov { lerp_color(*color, Color32::WHITE, 0.2) } else { *color });
        if k > 0 && w > 2.0 {
            p.vline(r.left(), r.y_range(), Stroke::new(1.5_f32, SURFACE));
        }
        if hov {
            hovered = Some((label.clone(), *v));
        }
        x += w;
        k += 1;
    }
    if let Some((label, v)) = hovered {
        let pct = v as f64 * 100.0 / total as f64;
        tip(ui.ctx(), resp.hover_pos().unwrap(), &[(label, TEXT, true), (format!("{}  ·  {pct:.1}%", unit(v)), MUTED, false)]);
    }
    resp
}

fn arc_mesh(mesh: &mut Mesh, c: Pos2, r0: f32, r1: f32, a0: f32, a1: f32, color: Color32) {
    let steps = ((a1 - a0).abs() * 40.0).ceil().max(2.0) as usize;
    let base = mesh.vertices.len() as u32;
    for i in 0..=steps {
        let a = a0 + (a1 - a0) * i as f32 / steps as f32;
        let d = vec2(a.cos(), a.sin());
        mesh.colored_vertex(c + d * r0, color);
        mesh.colored_vertex(c + d * r1, color);
    }
    for i in 0..steps as u32 {
        let j = base + i * 2;
        mesh.add_triangle(j, j + 1, j + 3);
        mesh.add_triangle(j, j + 3, j + 2);
    }
}

/// Donut chart with a caption in the middle. Returns the hovered segment index.
pub fn donut(ui: &mut Ui, segments: &[(String, u64, Color32)], size: f32, center: &str, sub: &str, unit: &dyn Fn(u64) -> String) -> Option<usize> {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let p = ui.painter();
    let c = rect.center();
    let r1 = size / 2.0 - 4.0;
    let r0 = r1 * 0.68;
    let total: u64 = segments.iter().map(|s| s.1).sum();
    let mut hovered = None;
    if let Some(h) = resp.hover_pos() {
        let d = h - c;
        let len = d.length();
        if total > 0 && len >= r0 - 2.0 && len <= r1 + 6.0 {
            let mut a = d.y.atan2(d.x) + std::f32::consts::FRAC_PI_2;
            if a < 0.0 {
                a += std::f32::consts::TAU;
            }
            let mut acc = 0.0;
            for (i, s) in segments.iter().enumerate() {
                let span = s.1 as f32 / total as f32 * std::f32::consts::TAU;
                if a >= acc && a < acc + span {
                    hovered = Some(i);
                    break;
                }
                acc += span;
            }
        }
    }
    let mut mesh = Mesh::default();
    if total == 0 {
        arc_mesh(&mut mesh, c, r0, r1, 0.0, std::f32::consts::TAU, SURFACE_3);
    } else {
        let mut a = -std::f32::consts::FRAC_PI_2;
        let gap: f32 = if segments.iter().filter(|s| s.1 > 0).count() > 1 { 0.025 } else { 0.0 };
        for (i, (_, v, color)) in segments.iter().enumerate() {
            let span = *v as f32 / total as f32 * std::f32::consts::TAU;
            if span <= 0.0 {
                continue;
            }
            let grow = if hovered == Some(i) { 5.0 } else { 0.0 };
            let g = gap.min(span * 0.3);
            arc_mesh(&mut mesh, c, r0, r1 + grow, a + g / 2.0, a + span - g / 2.0, *color);
            a += span;
        }
    }
    p.add(Shape::mesh(mesh));
    match hovered {
        Some(i) => {
            let (l, v, col) = &segments[i];
            let pct = *v as f64 * 100.0 / total.max(1) as f64;
            p.text(c - vec2(0.0, 8.0), Align2::CENTER_CENTER, format!("{pct:.0}%"), black(22.0), *col);
            p.text(c + vec2(0.0, 14.0), Align2::CENTER_CENTER, truncate(l, 16), semibold(11.5), MUTED);
            tip(ui.ctx(), resp.hover_pos().unwrap(), &[(l.clone(), TEXT, true), (format!("{}  ·  {pct:.1}%", unit(*v)), MUTED, false)]);
        }
        None => {
            p.text(c - vec2(0.0, 8.0), Align2::CENTER_CENTER, center, black(22.0), TEXT);
            p.text(c + vec2(0.0, 14.0), Align2::CENTER_CENTER, sub, semibold(11.5), MUTED);
        }
    }
    hovered
}

pub fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n.saturating_sub(1)).collect::<String>())
    }
}

/// Weekly bars: commits per week (single color).
pub fn week_bars(ui: &mut Ui, values: &[u32], first_week: i64, height: f32, color: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let p = ui.painter();
    let plot = Rect::from_min_max(rect.min + vec2(34.0, 6.0), rect.max - vec2(4.0, 20.0));
    let max = values.iter().copied().max().unwrap_or(0).max(1);
    let nice = nice_max(max);
    for i in 0..=3 {
        let v = nice * i / 3;
        let y = plot.bottom() - plot.height() * (v as f32 / nice as f32);
        p.hline(plot.x_range(), y, Stroke::new(1.0_f32, with_alpha(BORDER, if i == 0 { 255 } else { 120 })));
        p.text(pos2(plot.left() - 8.0, y), Align2::RIGHT_CENTER, stats::compact(v as u64), regular(10.5), FAINT);
    }
    let n = values.len().max(1);
    let slot = plot.width() / n as f32;
    let bw = (slot * 0.68).max(1.0);
    let hover_i = resp.hover_pos().filter(|h| plot.expand2(vec2(0.0, 20.0)).contains(*h)).map(|h| (((h.x - plot.left()) / slot) as usize).min(n - 1));
    let mut last_month = None;
    for (i, v) in values.iter().enumerate() {
        let x = plot.left() + slot * i as f32 + (slot - bw) / 2.0;
        let h = plot.height() * (*v as f32 / nice as f32);
        if *v > 0 {
            let col = if hover_i == Some(i) { lerp_color(color, Color32::WHITE, 0.25) } else { color };
            p.rect_filled(Rect::from_min_max(pos2(x, plot.bottom() - h.max(2.0)), pos2(x + bw, plot.bottom())), CornerRadius::same(2), col);
        }
        if let Some(d) = DateTime::<Utc>::from_timestamp(first_week + i as i64 * 604800, 0) {
            if last_month != Some(d.month()) && i > 0 && d.day() <= 7 {
                p.text(pos2(x + bw / 2.0, plot.bottom() + 5.0), Align2::CENTER_TOP, d.format("%b").to_string(), regular(10.5), FAINT);
            }
            last_month = Some(d.month());
        }
    }
    if let Some(i) = hover_i {
        tip(ui.ctx(), resp.hover_pos().unwrap(), &[(plural(values[i] as u64, "commit"), TEXT, true), (week_label(first_week + i as i64 * 604800), MUTED, false)]);
    }
    resp
}

/// Additions above, deletions below the axis, per week. Square-root scale so one giant import
/// doesn't flatten everything else.
pub fn code_frequency(ui: &mut Ui, weeks: &[(i64, i64, i64)], height: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let p = ui.painter();
    let plot = Rect::from_min_max(rect.min + vec2(4.0, 6.0), rect.max - vec2(4.0, 20.0));
    let mid = plot.center().y;
    p.hline(plot.x_range(), mid, Stroke::new(1.0_f32, BORDER));
    if weeks.is_empty() {
        return resp;
    }
    let max = weeks.iter().map(|w| w.1.max(-w.2)).max().unwrap_or(1).max(1) as f32;
    let scale = |v: i64| (v.unsigned_abs() as f32 / max).sqrt() * (plot.height() / 2.0 - 2.0);
    let n = weeks.len();
    let slot = plot.width() / n as f32;
    let bw = (slot * 0.7).max(1.0);
    let hover_i = resp.hover_pos().filter(|h| plot.contains(*h)).map(|h| (((h.x - plot.left()) / slot) as usize).min(n - 1));
    for (i, (_, a, d)) in weeks.iter().enumerate() {
        let x = plot.left() + slot * i as f32 + (slot - bw) / 2.0;
        let hl = hover_i == Some(i);
        if *a > 0 {
            let c = if hl { Color32::from_rgb(120, 240, 190) } else { ACCENT };
            p.rect_filled(Rect::from_min_max(pos2(x, mid - scale(*a).max(1.5)), pos2(x + bw, mid - 0.5)), CornerRadius::same(1), c);
        }
        if *d < 0 {
            let c = if hl { Color32::from_rgb(255, 160, 160) } else { RED };
            p.rect_filled(Rect::from_min_max(pos2(x, mid + 0.5), pos2(x + bw, mid + scale(*d).max(1.5))), CornerRadius::same(1), c);
        }
    }
    let first = DateTime::<Utc>::from_timestamp(weeks[0].0, 0).map(|d| d.format("%b %Y").to_string()).unwrap_or_default();
    let last = DateTime::<Utc>::from_timestamp(weeks[n - 1].0, 0).map(|d| d.format("%b %Y").to_string()).unwrap_or_default();
    p.text(pos2(plot.left(), plot.bottom() + 5.0), Align2::LEFT_TOP, first, regular(10.5), FAINT);
    p.text(pos2(plot.right(), plot.bottom() + 5.0), Align2::RIGHT_TOP, last, regular(10.5), FAINT);
    if let Some(i) = hover_i {
        let (w, a, d) = weeks[i];
        tip(
            ui.ctx(),
            resp.hover_pos().unwrap(),
            &[
                (week_label(w), MUTED, false),
                (format!("+{} lines", stats::grouped(a.max(0) as u64)), ACCENT_HI, true),
                (format!("−{} lines", stats::grouped((-d).max(0) as u64)), RED, true),
            ],
        );
    }
    resp
}

pub struct TimelineItem {
    pub id: u64,
    pub name: String,
    pub days: i64,
    pub color: Color32,
    pub note: String,
}

/// Days → x fraction on a log-like scale (today on the left, the oldest repository on the right).
fn age_x(days: i64, max_days: i64) -> f32 {
    ((1.0 + days.max(0) as f32).ln() / (1.0 + max_days.max(30) as f32).ln()).clamp(0.0, 1.0)
}

/// Every repository as a dot on a "time since last commit" axis, stacked where they collide.
/// Returns the clicked repository.
pub fn freshness_timeline(ui: &mut Ui, items: &[TimelineItem]) -> Option<u64> {
    let max_days = items.iter().map(|i| i.days).max().unwrap_or(365).max(400);
    let width = ui.available_width();
    let r = 6.5;
    let plot_l = 8.0;
    let plot_w = width - 16.0;
    // Beeswarm: place each dot in the lowest free row at its x position.
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by_key(|&i| items[i].days);
    let mut placed: Vec<(f32, usize)> = Vec::new(); // (x, row)
    let mut pos = vec![(0.0f32, 0usize); items.len()];
    for &i in &order {
        let x = plot_l + plot_w * age_x(items[i].days, max_days);
        let mut row = 0;
        while placed.iter().any(|(px, pr)| *pr == row && (px - x).abs() < r * 2.0 + 1.5) {
            row += 1;
        }
        placed.push((x, row));
        pos[i] = (x, row);
    }
    let rows = placed.iter().map(|p| p.1 + 1).max().unwrap_or(1).max(3);
    let step = r * 2.0 + 2.0;
    let height = rows as f32 * step + 52.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    let p = ui.painter();
    let axis_y = rect.bottom() - 30.0;

    // Zones behind the dots, colored like the freshness buckets.
    let zones = [(0, 7), (7, 30), (30, 90), (90, 365), (365, i64::MAX)];
    for (k, (a, b)) in zones.iter().enumerate() {
        let x0 = rect.left() + plot_l + plot_w * age_x(*a, max_days);
        let x1 = rect.left() + plot_l + plot_w * age_x((*b).min(max_days * 2), max_days);
        if x1 <= x0 {
            continue;
        }
        let f = stats::Freshness::ALL[k];
        let zone = Rect::from_min_max(pos2(x0, rect.top() + 4.0), pos2(x1, axis_y));
        p.rect_filled(zone, CornerRadius::ZERO, with_alpha(freshness_color(f), 10));
        p.text(pos2(zone.left() + 6.0, zone.top() + 4.0), Align2::LEFT_TOP, f.label(), semibold(10.5), with_alpha(freshness_color(f), 170));
    }
    p.hline(rect.left() + plot_l..=rect.left() + plot_l + plot_w, axis_y, Stroke::new(1.0_f32, BORDER));
    for (label, d) in [("today", 0), ("1 week", 7), ("1 month", 30), ("3 months", 90), ("6 months", 182), ("1 year", 365), ("2 years", 730), ("5 years", 1825), ("10 years", 3650)] {
        if d > max_days * 2 {
            continue;
        }
        let x = rect.left() + plot_l + plot_w * age_x(d, max_days);
        p.vline(x, axis_y..=axis_y + 4.0, Stroke::new(1.0_f32, FAINT));
        p.text(pos2(x, axis_y + 7.0), Align2::CENTER_TOP, label, regular(10.5), FAINT);
    }

    let mut hovered = None;
    for (i, it) in items.iter().enumerate() {
        let (x, row) = pos[i];
        let c = pos2(rect.left() + x, axis_y - 6.0 - r - row as f32 * step);
        let hov = resp.hover_pos().is_some_and(|h| h.distance(c) <= r + 1.5);
        if hov {
            hovered = Some(i);
        }
        p.circle_filled(c, if hov { r + 2.0 } else { r }, it.color);
        p.circle_stroke(c, if hov { r + 2.0 } else { r }, Stroke::new(1.5_f32, if hov { TEXT } else { with_alpha(BG, 200) }));
    }
    if let Some(i) = hovered {
        let it = &items[i];
        let when = if it.days == 0 { "Last commit today".to_string() } else { format!("Last commit {} days ago", stats::grouped(it.days as u64)) };
        let mut lines = vec![(it.name.clone(), TEXT, true), (when, it.color, false)];
        if !it.note.is_empty() {
            lines.push((it.note.clone(), MUTED, false));
        }
        lines.push(("Click to open".into(), FAINT, false));
        tip(ui.ctx(), resp.hover_pos().unwrap(), &lines);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        if resp.clicked() {
            return Some(it.id);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales() {
        assert_eq!(nice_max(1), 3);
        assert_eq!(nice_max(14), 15);
        assert_eq!(nice_max(100_000), 100_002);
        assert_eq!(heat_level(0, 10), 0);
        assert_eq!(heat_level(1, 10), 1);
        assert_eq!(heat_level(10, 10), 4);
        assert!(age_x(0, 1000) == 0.0 && age_x(1000, 1000) == 1.0);
        assert!(age_x(7, 1000) < age_x(30, 1000));
        assert_eq!(truncate("abcdef", 4), "abc…");
    }
}

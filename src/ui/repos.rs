//! Repositories: searchable, filterable, sortable list. ↑/↓ to move, Enter to open.

use chrono::Utc;
use egui::{pos2, vec2, Align, Align2, Color32, CornerRadius, Key, Layout, Margin, Rect, RichText, Sense, Stroke, Ui};
use egui_phosphor::regular as ph;

use super::app::{App, Filter};
use super::charts;
use super::derive::RepoInfo;
use super::overview::{empty_state, PAGE_MARGIN};
use super::theme::*;
use super::widgets::*;
use crate::backend::settings::SortKey;
use crate::backend::stats::{self, Freshness};

/// Row height without item spacing.
const ROW_H: f32 = 62.0;

fn matches_filter(r: &RepoInfo, f: Filter) -> bool {
    match f {
        Filter::All => true,
        Filter::Owned => r.owned,
        Filter::Private => r.private,
        Filter::Public => !r.private,
        Filter::Forks => r.fork,
        Filter::Archived => r.archived,
        Filter::Attention => !r.archived && matches!(r.fresh, Freshness::Stale | Freshness::Dormant),
    }
}

fn matches_search(r: &RepoInfo, q: &str) -> bool {
    if q.is_empty() {
        return true;
    }
    let q = q.to_lowercase();
    q.split_whitespace().all(|w| {
        r.full_name.to_lowercase().contains(w)
            || r.description.as_deref().is_some_and(|d| d.to_lowercase().contains(w))
            || r.language.as_deref().is_some_and(|l| l.to_lowercase() == w)
    })
}

pub fn sort(list: &mut [RepoInfo], key: SortKey, desc: bool) {
    list.sort_by(|a, b| {
        let o = match key {
            SortKey::LastCommit => a.last.cmp(&b.last),
            SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortKey::Commits => a.commits.cmp(&b.commits),
            SortKey::Lines => a.lines.map(|l| l.0).cmp(&b.lines.map(|l| l.0)),
            SortKey::Stars => a.stars.cmp(&b.stars),
        };
        let o = o.then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        if desc {
            o.reverse()
        } else {
            o
        }
    });
}

impl App {
    pub fn repos_page(&mut self, ui: &mut Ui) {
        let now = Utc::now();
        let all = self.infos(self.repos.iter(), now);
        let counts: Vec<usize> = Filter::ALL.iter().map(|f| all.iter().filter(|r| matches_filter(r, *f)).count()).collect();
        let mut list: Vec<RepoInfo> = all.into_iter().filter(|r| matches_filter(r, self.list.filter) && matches_search(r, &self.list.search)).collect();
        sort(&mut list, self.settings.sort, self.settings.sort_desc);
        if self.list.selected >= list.len() {
            self.list.selected = list.len().saturating_sub(1);
        }
        let mut open = None;
        let mut sort_changed = false;

        egui::Frame::new().inner_margin(Margin { left: PAGE_MARGIN as i8, right: PAGE_MARGIN as i8, top: 22, bottom: 0 }).show(ui, |ui| {
            page_header(ui, "Repositories", "Your repositories", Some(&format!("{} shown of {}", list.len(), self.repos.len())));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                // Search.
                let search = egui::TextEdit::singleline(&mut self.list.search)
                    .hint_text(format!("{}  Search repositories   ( / )", ph::MAGNIFYING_GLASS))
                    .desired_width((ui.available_width() - 230.0).clamp(200.0, 460.0))
                    .margin(Margin::symmetric(10, 7));
                let resp = ui.add(search);
                if self.list.focus_search {
                    resp.request_focus();
                    self.list.focus_search = false;
                }
                if resp.changed() {
                    self.list.selected = 0;
                }
                if resp.has_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
                    if self.list.search.is_empty() {
                        resp.surrender_focus();
                    }
                    self.list.search.clear();
                }
                if !ui.ctx().wants_keyboard_input() && ui.input(|i| i.key_pressed(Key::Slash)) {
                    self.list.focus_search = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // Sort direction + key.
                    let dir_icon = if self.settings.sort_desc { ph::SORT_DESCENDING } else { ph::SORT_ASCENDING };
                    if icon_button(ui, dir_icon, if self.settings.sort_desc { "Descending" } else { "Ascending" }).clicked() {
                        self.settings.sort_desc = !self.settings.sort_desc;
                        sort_changed = true;
                    }
                    egui::ComboBox::from_id_salt("sort")
                        .selected_text(RichText::new(format!("{}  {}", ph::FUNNEL_SIMPLE, self.settings.sort.label())).font(semibold(13.0)))
                        .width(160.0)
                        .show_ui(ui, |ui| {
                            for k in SortKey::ALL {
                                if ui.selectable_value(&mut self.settings.sort, k, k.label()).changed() {
                                    self.settings.sort_desc = k != SortKey::Name;
                                    sort_changed = true;
                                }
                            }
                        });
                });
            });
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                for (i, f) in Filter::ALL.iter().enumerate() {
                    if filter_pill(ui, f.label(), Some(counts[i]), self.list.filter == *f).clicked() {
                        self.list.filter = *f;
                        self.list.selected = 0;
                    }
                }
            });
            ui.add_space(10.0);
            if let Some(k) = header_row(ui, self.settings.sort, self.settings.sort_desc) {
                if self.settings.sort == k {
                    self.settings.sort_desc = !self.settings.sort_desc;
                } else {
                    self.settings.sort = k;
                    self.settings.sort_desc = k != SortKey::Name;
                }
                sort_changed = true;
            }
        });

        // Keyboard: arrows move, Enter opens (also while typing in the search box).
        let (down, up, enter, pgdn, pgup, home, end) = ui.input(|i| {
            (i.key_pressed(Key::ArrowDown), i.key_pressed(Key::ArrowUp), i.key_pressed(Key::Enter), i.key_pressed(Key::PageDown), i.key_pressed(Key::PageUp), i.key_pressed(Key::Home), i.key_pressed(Key::End))
        });
        let typing = ui.ctx().wants_keyboard_input();
        let n = list.len();
        if n > 0 && !self.confirm.is_some() {
            let before = self.list.selected;
            if down {
                self.list.selected = (self.list.selected + 1).min(n - 1);
            }
            if up {
                self.list.selected = self.list.selected.saturating_sub(1);
            }
            if pgdn {
                self.list.selected = (self.list.selected + 8).min(n - 1);
            }
            if pgup {
                self.list.selected = self.list.selected.saturating_sub(8);
            }
            if !typing && home {
                self.list.selected = 0;
            }
            if !typing && end {
                self.list.selected = n - 1;
            }
            if before != self.list.selected {
                self.list.scroll_to_selected = true;
            }
            if enter {
                open = Some(list[self.list.selected].id);
            }
        }

        if self.repos.is_empty() {
            empty_state(ui, self.sync.running);
        } else if list.is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(ph::MAGNIFYING_GLASS).font(regular(40.0)).color(FAINT));
                ui.label(RichText::new("Nothing matches").font(bold(17.0)));
                ui.label(RichText::new("Try another search or filter.").color(MUTED));
            });
        } else {
            let selected = self.list.selected;
            let scroll_to = std::mem::take(&mut self.list.scroll_to_selected);
            let mut clicked = None;
            let mut hovered_sel = None;
            // show_rows only lays out visible rows, so keep the selection in view by hand.
            let stride = ROW_H + ui.spacing().item_spacing.y;
            let view_h = ui.available_height();
            let mut area = egui::ScrollArea::vertical().id_salt("repo_list").auto_shrink([false, false]);
            if scroll_to {
                let top = selected as f32 * stride;
                let mut off = self.list.offset;
                if top < off {
                    off = top;
                } else if top + stride > off + view_h {
                    off = top + stride - view_h;
                }
                area = area.vertical_scroll_offset(off.max(0.0));
            }
            let out = area.show_rows(ui, ROW_H, n, |ui, range| {
                for i in range {
                    let r = &list[i];
                    ui.horizontal(|ui| {
                        ui.add_space(PAGE_MARGIN - 8.0);
                        let resp = repo_row(ui, r, i == selected, now);
                        if resp.clicked() {
                            clicked = Some(r.id);
                        }
                        if resp.hovered() && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
                            hovered_sel = Some(i);
                        }
                    });
                }
            });
            // Selection follows the mouse (when it moves), so Enter opens what you point at.
            if let Some(i) = hovered_sel {
                self.list.selected = i;
            }
            self.list.offset = out.state.offset.y;
            if clicked.is_some() {
                open = clicked;
            }
        }

        if sort_changed {
            self.mark_dirty();
        }
        if let Some(id) = open {
            self.open_repo(id);
        }
    }
}

const COLS: [(f32, &str, Option<SortKey>); 5] =
    [(130.0, "Last 6 months", None), (86.0, "Commits", Some(SortKey::Commits)), (96.0, "Lines", Some(SortKey::Lines)), (70.0, "Stars", Some(SortKey::Stars)), (118.0, "Last commit", Some(SortKey::LastCommit))];

/// Which columns fit next to a readable name: the sparkline goes first, then lines, then stars.
fn shown_cols(width: f32) -> [bool; 5] {
    const NAME_MIN: f32 = 300.0;
    let mut shown = [true; 5];
    let used = |s: &[bool; 5]| COLS.iter().zip(s).filter(|(_, on)| **on).map(|(c, _)| c.0 + 8.0).sum::<f32>() + 34.0;
    for drop in [0, 2, 3] {
        if width - used(&shown) >= NAME_MIN {
            break;
        }
        shown[drop] = false;
    }
    shown
}

fn header_row(ui: &mut Ui, key: SortKey, desc: bool) -> Option<SortKey> {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
    let shown = shown_cols(rect.width());
    let mut clicked = None;
    let mut right = rect.right() - 12.0;
    let arrow = if desc { ph::CARET_DOWN } else { ph::CARET_UP };
    for (i, (w, label, k)) in COLS.iter().enumerate().rev() {
        if !shown[i] {
            continue;
        }
        let cell = Rect::from_min_max(pos2(right - w, rect.top()), pos2(right, rect.bottom()));
        let active = *k == Some(key);
        let text = if active { format!("{label} {arrow}") } else { label.to_string() };
        if let Some(k) = k {
            let resp = ui.interact(cell, ui.id().with(label), Sense::click());
            let c = if active { ACCENT_HI } else if resp.hovered() { TEXT } else { FAINT };
            ui.painter().text(cell.right_center(), Align2::RIGHT_CENTER, text, semibold(11.5), c);
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                clicked = Some(*k);
            }
        } else {
            ui.painter().text(cell.right_center(), Align2::RIGHT_CENTER, text, semibold(11.5), FAINT);
        }
        right -= w + 8.0;
    }
    let name = Rect::from_min_max(rect.min + vec2(22.0, 0.0), pos2(right, rect.bottom()));
    let active = key == SortKey::Name;
    let resp = ui.interact(name, ui.id().with("name_col"), Sense::click());
    let c = if active { ACCENT_HI } else if resp.hovered() { TEXT } else { FAINT };
    ui.painter().text(name.left_center(), Align2::LEFT_CENTER, if active { format!("Repository {arrow}") } else { "Repository".into() }, semibold(11.5), c);
    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        clicked = Some(SortKey::Name);
    }
    ui.painter().hline(rect.x_range(), rect.bottom(), Stroke::new(1.0_f32, BORDER));
    clicked
}

fn repo_row(ui: &mut Ui, r: &RepoInfo, selected: bool, now: chrono::DateTime<Utc>) -> egui::Response {
    let width = ui.available_width() - PAGE_MARGIN + 8.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::click());
    let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.08);
    let sel = ui.ctx().animate_bool_with_time(resp.id.with("sel"), selected, 0.1);
    let p = ui.painter().clone();
    let bg = lerp_color(lerp_color(BG, SURFACE, hover), SURFACE_2, sel);
    p.rect_filled(rect, CornerRadius::same(R_MD), bg);
    if sel > 0.0 {
        p.rect_stroke(rect, CornerRadius::same(R_MD), Stroke::new(1.0_f32, with_alpha(ACCENT, (sel * 110.0) as u8)), egui::StrokeKind::Inside);
    }
    let fc = freshness_color(r.fresh);
    // Freshness stripe.
    p.rect_filled(Rect::from_min_size(rect.min + vec2(0.0, 10.0), vec2(3.0, rect.height() - 20.0)), CornerRadius::same(2), fc);

    // Right-hand columns.
    let mut right = rect.right() - 12.0;
    let cy = rect.center().y;
    let cols: Vec<Box<dyn Fn(&egui::Painter, Rect)>> = vec![
        Box::new(|_p, _c| {}), // sparkline drawn separately (needs Ui)
        Box::new(|p, c| {
            let t = r.commits.map(stats::grouped).unwrap_or_else(|| "—".into());
            p.text(c.right_center(), Align2::RIGHT_CENTER, t, semibold(13.5), TEXT);
        }),
        Box::new(|p, c| {
            let t = match r.lines {
                Some((n, exact)) => format!("{}{}", if exact { "" } else { "≈" }, stats::compact(n)),
                None => "—".into(),
            };
            p.text(c.right_center(), Align2::RIGHT_CENTER, t, semibold(13.5), TEXT);
        }),
        Box::new(|p, c| {
            p.text(c.right_center(), Align2::RIGHT_CENTER, format!("{} {}", stats::compact(r.stars), ph::STAR), semibold(13.0), if r.stars > 0 { GOLD } else { FAINT });
        }),
        Box::new(|p, c| {
            let text = match r.days {
                Some(d) => stats::short_age(d),
                None => "never".into(),
            };
            let g = p.layout_no_wrap(text, semibold(12.0), fc);
            let pill = Rect::from_min_size(pos2(c.right() - g.size().x - 18.0, c.center().y - 12.0), vec2(g.size().x + 18.0, 24.0));
            p.rect_filled(pill, CornerRadius::same(255), with_alpha(fc, 30));
            p.galley(pill.center() - g.size() / 2.0, g, fc);
        }),
    ];
    let mut spark_rect = None;
    let shown = shown_cols(rect.width());
    for (i, (w, _, _)) in COLS.iter().enumerate().rev() {
        if !shown[i] {
            continue;
        }
        let cell = Rect::from_min_max(pos2(right - w, rect.top()), pos2(right, rect.bottom()));
        if i == 0 {
            spark_rect = Some(cell);
        } else {
            cols[i](&p, cell);
        }
        right -= w + 8.0;
    }
    if let Some(c) = spark_rect {
        let sr = Rect::from_center_size(c.center(), vec2(c.width() - 10.0, 28.0));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(sr));
        charts::sparkline(&mut child, &r.recent_weeks, sr.size(), if r.commits_4w > 0 { ACCENT_HI } else { with_alpha(ACCENT, 140) });
    }

    // Name, badges, description.
    let left = rect.left() + 22.0;
    let max_w = right - left;
    let name_g = p.layout_no_wrap(charts::truncate(&r.name, 48), semibold(15.0), TEXT);
    let name_w = name_g.size().x;
    p.galley(pos2(left, cy - 18.0), name_g, TEXT);
    let mut bx = left + name_w + 8.0;
    let mut badge = |icon: &str, text: &str, color: Color32| {
        let g = p.layout_no_wrap(format!("{icon} {text}"), semibold(10.5), color);
        let br = Rect::from_min_size(pos2(bx, cy - 17.0), vec2(g.size().x + 12.0, 18.0));
        if br.right() < right {
            p.rect_filled(br, CornerRadius::same(255), with_alpha(color, 26));
            p.galley(br.center() - g.size() / 2.0, g, color);
        }
        bx = br.right() + 5.0;
    };
    if !r.owned {
        badge(ph::BUILDINGS, &r.owner, BLUE);
    }
    if r.private {
        badge(ph::LOCK_SIMPLE, "private", MUTED);
    }
    if r.fork {
        badge(ph::GIT_FORK, "fork", PURPLE);
    }
    if r.archived {
        badge(ph::ARCHIVE, "archived", GOLD);
    }
    let lang = r.language.clone().unwrap_or_default();
    let mut sub_x = left;
    if !lang.is_empty() {
        p.circle_filled(pos2(sub_x + 4.0, cy + 11.0), 4.0, language_color(&lang));
        let g = p.layout_no_wrap(lang, regular(12.0), MUTED);
        let w = g.size().x;
        p.galley(pos2(sub_x + 13.0, cy + 11.0 - g.size().y / 2.0), g, MUTED);
        sub_x += 13.0 + w + 12.0;
    }
    let desc = r.description.clone().unwrap_or_else(|| if r.loaded { String::new() } else { "loading statistics…".into() });
    if !desc.is_empty() && sub_x < right - 40.0 {
        let chars = ((right - sub_x) / 6.6) as usize;
        p.text(pos2(sub_x, cy + 11.0), Align2::LEFT_CENTER, charts::truncate(&desc, chars.max(4)), regular(12.0), FAINT);
    }
    let _ = max_w;
    let tip = format!(
        "{}\nLast commit: {}\nEnter or click to open",
        r.full_name,
        r.last.map(|t| format!("{} ({})", t.with_timezone(&chrono::Local).format("%b %-d, %Y"), stats::ago(t, now))).unwrap_or_else(|| "none".into())
    );
    resp.on_hover_text_at_pointer(tip).on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(name: &str, stars: u64, days: Option<i64>) -> RepoInfo {
        let last = days.map(|d| Utc::now() - chrono::Duration::days(d));
        RepoInfo {
            id: stars,
            name: name.into(),
            full_name: format!("me/{name}"),
            owner: "me".into(),
            owned: true,
            description: Some("A thing in Rust".into()),
            language: Some("Rust".into()),
            private: false,
            fork: false,
            archived: false,
            stars,
            open_issues: 0,
            last,
            days,
            fresh: Freshness::from_days(days),
            commits: Some(stars * 10),
            lines: None,
            recent_weeks: vec![],
            commits_4w: 0,
            attention: 0,
            loaded: true,
        }
    }

    #[test]
    fn search_and_filters() {
        let r = info("tracker", 1, Some(400));
        assert!(matches_search(&r, "track"));
        assert!(matches_search(&r, "me/ thing"));
        assert!(matches_search(&r, "rust"));
        assert!(!matches_search(&r, "python"));
        assert!(matches_filter(&r, Filter::Attention));
        assert!(!matches_filter(&r, Filter::Forks));
    }

    #[test]
    fn sorting() {
        let mut v = vec![info("b", 5, Some(10)), info("a", 9, Some(1)), info("c", 1, None)];
        sort(&mut v, SortKey::Stars, true);
        assert_eq!(v.iter().map(|r| r.name.as_str()).collect::<String>(), "abc");
        sort(&mut v, SortKey::Name, false);
        assert_eq!(v.iter().map(|r| r.name.as_str()).collect::<String>(), "abc");
        sort(&mut v, SortKey::LastCommit, true);
        assert_eq!(v.iter().map(|r| r.name.as_str()).collect::<String>(), "abc", "most recent first, never-committed last");
    }
}

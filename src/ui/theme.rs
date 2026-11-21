//! Colors, fonts and the global egui style. The look follows Akulav's newer apps (Hyve, Lumen): a deep blue-black
//! backdrop with soft glows and glass panels with hairline borders, with a commit-green accent.

use std::sync::Arc;

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Frame, Margin, Rect, Shadow, Stroke, Visuals};

use crate::backend::stats::Freshness;

pub const BG: Color32 = Color32::from_rgb(32, 32, 32);
pub const SURFACE: Color32 = Color32::from_rgb(43, 43, 43);
pub const SURFACE_2: Color32 = Color32::from_rgb(45, 45, 45);
pub const SURFACE_3: Color32 = Color32::from_rgb(56, 56, 56);
pub const BORDER: Color32 = Color32::from_rgb(58, 58, 58);
pub const BORDER_HI: Color32 = Color32::from_rgb(76, 76, 76);
/// Cards and panels: a flat layer one step above the window (Windows 11's card fill, no blur).
pub const GLASS: Color32 = SURFACE;
pub const GLASS_BORDER: Color32 = BORDER;
pub const TEXT: Color32 = Color32::from_rgb(255, 255, 255);
pub const MUTED: Color32 = Color32::from_rgb(207, 207, 207);
pub const FAINT: Color32 = Color32::from_rgb(157, 157, 157);
pub const ACCENT: Color32 = Color32::from_rgb(96, 205, 255);
pub const ACCENT_HI: Color32 = Color32::from_rgb(153, 235, 255);
pub const ON_ACCENT: Color32 = Color32::from_rgb(0, 0, 0);
pub const GREEN: Color32 = Color32::from_rgb(108, 203, 95);
pub const ACCENT_LO: Color32 = Color32::from_rgb(0, 120, 212);
pub const GOLD: Color32 = Color32::from_rgb(252, 225, 0);
pub const RED: Color32 = Color32::from_rgb(255, 153, 164);
pub const BLUE: Color32 = Color32::from_rgb(96, 205, 255);
pub const PURPLE: Color32 = Color32::from_rgb(167, 139, 250);
pub const ORANGE: Color32 = Color32::from_rgb(251, 146, 60);

pub const R_SM: u8 = 4;
pub const R_MD: u8 = 4;
pub const R_LG: u8 = 8;
/// Gap between the window edge, the sidebar and the content.
pub const WINDOW_GAP: f32 = 14.0;

/// A card: the layer fill, a hairline border, 8 px corners.
pub fn glass_frame() -> Frame {
    Frame::new().fill(GLASS).stroke(Stroke::new(1.0_f32, GLASS_BORDER)).corner_radius(CornerRadius::same(R_LG))
}

/// The window background: Windows 11's flat dark base.
pub fn paint_backdrop(ctx: &egui::Context) {
    let r: Rect = ctx.screen_rect();
    ctx.layer_painter(egui::LayerId::background()).rect_filled(r, CornerRadius::ZERO, BG);
}

pub fn freshness_color(f: Freshness) -> Color32 {
    match f {
        Freshness::Active => Color32::from_rgb(52, 211, 153),
        Freshness::Recent => Color32::from_rgb(45, 190, 210),
        Freshness::Cooling => Color32::from_rgb(250, 204, 21),
        Freshness::Stale => ORANGE,
        Freshness::Dormant => Color32::from_rgb(248, 113, 113),
        Freshness::Unknown => FAINT,
    }
}

/// GitHub-style contribution levels.
pub fn heat_color(level: u8) -> Color32 {
    match level {
        0 => Color32::from_rgb(52, 52, 52),
        1 => Color32::from_rgb(14, 68, 52),
        2 => Color32::from_rgb(6, 112, 80),
        3 => Color32::from_rgb(16, 165, 115),
        _ => Color32::from_rgb(70, 225, 165),
    }
}

/// Colors GitHub uses for common languages; others get a stable hue from their name.
pub fn language_color(name: &str) -> Color32 {
    let hex = match name {
        "Rust" => 0xdea584,
        "TypeScript" => 0x3178c6,
        "JavaScript" => 0xf1e05a,
        "Python" => 0x3572a5,
        "C#" => 0x178600,
        "C++" => 0xf34b7d,
        "C" => 0x8a8f99,
        "Go" => 0x00add8,
        "Java" => 0xb07219,
        "Kotlin" => 0xa97bff,
        "Swift" => 0xf05138,
        "HTML" => 0xe34c26,
        "CSS" => 0x663399,
        "SCSS" => 0xc6538c,
        "Shell" => 0x89e051,
        "PowerShell" => 0x3a7bd5,
        "Batchfile" => 0xc1f12e,
        "Lua" => 0x6a6af0,
        "Ruby" => 0xcc342d,
        "PHP" => 0x4f5d95,
        "Dart" => 0x00b4ab,
        "Vue" => 0x41b883,
        "Svelte" => 0xff3e00,
        "Jupyter Notebook" => 0xda5b0b,
        "Dockerfile" => 0x5b8fa8,
        "Makefile" => 0x427819,
        "CMake" => 0xda3434,
        "Inno Setup" => 0x4a7bd0,
        "GLSL" => 0x5686a5,
        "HLSL" => 0xaace60,
        "Objective-C" => 0x438eff,
        "Zig" => 0xec915c,
        "Haskell" => 0x7e6bc0,
        "Elixir" => 0x9b6fc7,
        "Scala" => 0xc22d40,
        "Nix" => 0x7e7eff,
        "TeX" => 0x5c9e2e,
        "Assembly" => 0xa8763e,
        "Astro" => 0xff5a03,
        "MDX" => 0xfcb32c,
        "Markdown" => 0x6b8bd6,
        "Visual Basic .NET" => 0x945db7,
        "F#" => 0xb845fc,
        "R" => 0x198ce7,
        "Perl" => 0x0298c3,
        "Clojure" => 0xdb5855,
        "Erlang" => 0xb83998,
        "OCaml" => 0xef7a08,
        "Groovy" => 0x4298b8,
        "Solidity" => 0xaa6746,
        "WGSL" => 0x1a5e9a,
        "QML" => 0x44a51c,
        "Meson" => 0x007800,
        _ => {
            let h = name.bytes().fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
            let c = egui::ecolor::Hsva::new((h % 360) as f32 / 360.0, 0.55, 0.85, 1.0);
            return c.into();
        }
    };
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub fn with_alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}
pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}
pub fn black(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("black".into()))
}
pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}
pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

/// A font from the Windows fonts folder (Segoe UI has no Private Use Area glyphs, so the icons stay visible).
fn system_font(file: &str) -> Option<Vec<u8>> {
    if !cfg!(windows) {
        return None;
    }
    let windir = std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into());
    std::fs::read(std::path::Path::new(&windir).join("Fonts").join(file)).ok()
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let add = |fonts: &mut FontDefinitions, name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    };
    // Windows 11's UI font, read from Windows (it can't be shipped); the bundled Inter everywhere else.
    let pick = |system: &str, bundled: &'static [u8]| match system_font(system) {
        Some(bytes) => FontData::from_owned(bytes),
        None => FontData::from_static(bundled),
    };
    fonts.font_data.insert("inter".to_owned(), Arc::new(pick("segoeui.ttf", include_bytes!("../../assets/fonts/Inter-Regular.otf"))));
    fonts.font_data.insert("inter-semibold".to_owned(), Arc::new(pick("seguisb.ttf", include_bytes!("../../assets/fonts/Inter-SemiBold.otf"))));
    fonts.font_data.insert("inter-bold".to_owned(), Arc::new(pick("segoeuib.ttf", include_bytes!("../../assets/fonts/Inter-Bold.otf"))));
    fonts.font_data.insert("inter-black".to_owned(), Arc::new(pick("seguisb.ttf", include_bytes!("../../assets/fonts/Inter-ExtraBold.otf"))));
    add(&mut fonts, "mono", include_bytes!("../../assets/fonts/DejaVuSansMono.ttf"));

    // Phosphor icons register into Proportional; copy that fallback chain to every family. The
    // bundled Inter files have their Private Use Area glyphs stripped so they don't shadow icons.
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    let fallbacks: Vec<String> = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let others: Vec<String> = fallbacks.into_iter().filter(|f| !f.starts_with("inter")).collect();
    let family = |primary: &str| {
        let mut v = vec![primary.to_string()];
        v.extend(others.iter().cloned());
        v
    };
    fonts.families.insert(FontFamily::Proportional, family("inter"));
    fonts.families.insert(FontFamily::Name("semibold".into()), family("inter-semibold"));
    fonts.families.insert(FontFamily::Name("bold".into()), family("inter-bold"));
    fonts.families.insert(FontFamily::Name("black".into()), family("inter-black"));
    fonts.families.insert(FontFamily::Monospace, family("mono"));
    ctx.set_fonts(fonts);
}

pub fn apply_style(ctx: &egui::Context) {
    ctx.style_mut(|style| {
        let mut v = Visuals::dark();
        v.override_text_color = None;
        v.panel_fill = BG;
        v.window_fill = SURFACE;
        v.extreme_bg_color = Color32::from_rgb(28, 28, 28); // text fields, like Windows 11's text boxes
        v.faint_bg_color = SURFACE_2;
        v.code_bg_color = SURFACE_2;
        v.window_stroke = Stroke::new(1.0_f32, BORDER);
        v.window_corner_radius = CornerRadius::same(R_LG);
        v.menu_corner_radius = CornerRadius::same(R_MD);
        v.window_shadow = Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(90) };
        v.popup_shadow = Shadow { offset: [0, 4], blur: 12, spread: 0, color: Color32::from_black_alpha(80) };
        v.selection.bg_fill = ACCENT_LO;
        v.selection.stroke = Stroke::new(1.0_f32, ACCENT); // also the focused text field border
        v.hyperlink_color = ACCENT_HI;
        v.text_cursor.stroke = Stroke::new(2.0_f32, ACCENT_HI);

        let r = CornerRadius::same(R_MD);
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = SURFACE_2;
        w.noninteractive.weak_bg_fill = SURFACE_2;
        w.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
        w.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
        w.noninteractive.corner_radius = r;

        w.inactive.bg_fill = SURFACE_2;
        w.inactive.weak_bg_fill = SURFACE_2;
        w.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER_HI);
        w.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
        w.inactive.corner_radius = r;

        w.hovered.bg_fill = SURFACE_3;
        w.hovered.weak_bg_fill = SURFACE_3;
        w.hovered.bg_stroke = Stroke::new(1.0_f32, BORDER_HI);
        w.hovered.fg_stroke = Stroke::new(1.5_f32, TEXT);
        w.hovered.corner_radius = r;
        w.hovered.expansion = 0.0;

        w.active.bg_fill = ACCENT_LO;
        w.active.weak_bg_fill = SURFACE_3;
        w.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
        w.active.fg_stroke = Stroke::new(1.5_f32, TEXT);
        w.active.corner_radius = r;
        w.active.expansion = 0.0;

        w.open = w.hovered;
        style.visuals = v;

        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        style.spacing.interact_size.y = 30.0;
        style.spacing.window_margin = Margin::same(16);
        style.spacing.scroll.bar_width = 6.0;
        style.spacing.scroll.floating = true;

        use egui::TextStyle::*;
        style.text_styles = [(Small, regular(11.5)), (Body, regular(14.0)), (Button, semibold(13.5)), (Heading, semibold(24.0)), (Monospace, mono(12.5))].into();
    });
}

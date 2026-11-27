//! Colours, text tones, the HUD palette and the window / font / style setup of both looks.

use super::*;

// Cyberpunk HUD palette: deep black, electric cyan, neon magenta, cyber yellow. (The old "logo"
// names are kept so every widget picks the new colours up.)
pub(super) const LAVENDER: Color32 = CYAN;                               // accent text / selection
pub(super) const ICE: Color32 = Color32::from_rgb(120, 240, 255);
pub(super) const TEXT_HUD: Color32 = Color32::from_rgb(214, 238, 246);
/// Text colours: cool cyan-white in the HUD look, warm neutral white in the Cinematic one.
pub(super) fn tx() -> Color32 { if hud::soft() { hud::C_TEXT } else { TEXT_HUD } }
pub(super) fn subtle() -> Color32 { if hud::soft() { Color32::from_rgb(200, 201, 208) } else { Color32::from_rgb(150, 190, 204) } }
pub(super) fn muted() -> Color32 { if hud::soft() { hud::C_TEXT2 } else { Color32::from_rgb(88, 128, 142) } }
pub(super) const GREEN: Color32 = Color32::from_rgb(0, 255, 160);
pub(super) const AMBER: Color32 = YELLOW;

#[derive(Clone, Copy)]
pub(super) struct Palette { pub(super) sidebar: Color32, pub(super) sheet: Color32, pub(super) card: Color32, pub(super) hair: Color32 }

impl Palette {
    /// Solid near-black panes with thin neon-cyan edges (no glass in the HUD look).
    pub(super) fn new(_glass: bool, _m: Material) -> Self {
        Palette { sidebar: BG0, sheet: BG1, card: Color32::from_rgb(10, 14, 20), hair: hud::with_alpha(CYAN, 70) }
    }
}

/// Dark title bar in HUD colours (Windows 11: black caption, cyan frame). Returns false: the
/// dashboard is opaque now, no system backdrop.
pub(super) fn apply_backdrop(_m: Material) -> bool {
    unsafe {
        let Ok(hwnd) = FindWindowW(None, &HSTRING::from(APP_TITLE)) else { return false };
        let dark = BOOL(1);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const _ as *const _, 4);
        let colorref = |c: Color32| c.r() as u32 | (c.g() as u32) << 8 | (c.b() as u32) << 16;
        // Title bar + window border: neon cyan in the HUD look; in the Cinematic one a black bar,
        // a quiet graphite hairline and near-white title (like a macOS window).
        let set = if hud::soft() { [(35u32, hud::C_BG), (34, Color32::from_rgb(46, 48, 56)), (36, hud::C_TEXT)] }
            else { [(35u32, BG0), (34, hud::with_alpha(CYAN, 255)), (36, CYAN)] };
        for (attr, c) in set { // caption, border, caption text
            let v = colorref(c);
            let _ = DwmSetWindowAttribute(hwnd, windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE(attr as i32), &v as *const _ as *const _, 4);
        }
        false
    }
}

/// Monospace terminal look: Consolas (with Segoe UI behind it for any glyph it lacks).
pub(super) fn setup_fonts(ctx: &egui::Context, soft: bool) {
    let mut fonts = FontDefinitions::empty();
    let load = |f: &str| std::fs::read(format!("C:\\Windows\\Fonts\\{f}")).ok();
    if soft {
        // Cinematic: Segoe UI Variable-style text (regular / semibold / bold), Consolas for numbers.
        let mut fam = |key: &str, files: &[&str]| -> Vec<String> {
            let mut v = Vec::new();
            for (i, f) in files.iter().enumerate() {
                if let Some(b) = load(f) { let k = format!("{key}{i}"); fonts.font_data.insert(k.clone(), FontData::from_owned(b)); v.push(k); }
            }
            v
        };
        let sym = fam("sym", &["seguisym.ttf", "seguiemj.ttf"]);
        let mut reg = fam("reg", &["SegUIVar.ttf", "segoeui.ttf"]); reg.extend(sym.iter().cloned());
        let mut sem = fam("sem", &["seguisb.ttf"]); sem.extend(reg.iter().cloned());
        let mut bol = fam("bol", &["segoeuib.ttf"]); bol.extend(reg.iter().cloned());
        // No terminal font in this look: "monospace" text (times, tags) is Segoe UI too.
        let mono = reg.clone();
        fonts.families.insert(FontFamily::Name("semi".into()), sem);
        fonts.families.insert(FontFamily::Name("bold".into()), bol);
        fonts.families.insert(FontFamily::Proportional, reg);
        fonts.families.insert(FontFamily::Monospace, mono);
        ctx.set_fonts(fonts);
        return;
    }
    let mut fallback: Vec<String> = Vec::new();
    for (name, file) in [("segoe", "segoeui.ttf"), ("symbol", "seguisym.ttf")] {
        if let Some(b) = load(file) { fonts.font_data.insert(name.into(), FontData::from_owned(b)); fallback.push(name.into()); }
    }
    let mut regular: Vec<String> = Vec::new();
    if let Some(b) = load("consola.ttf") { fonts.font_data.insert("mono".into(), FontData::from_owned(b)); regular.push("mono".into()); }
    regular.extend(fallback.iter().cloned());
    let mut strong: Vec<String> = Vec::new();
    if let Some(b) = load("consolab.ttf") { fonts.font_data.insert("monob".into(), FontData::from_owned(b)); strong.push("monob".into()); }
    strong.extend(regular.iter().cloned());
    fonts.families.insert(FontFamily::Name("semi".into()), regular.clone());
    fonts.families.insert(FontFamily::Name("bold".into()), strong);
    fonts.families.insert(FontFamily::Proportional, regular.clone());
    fonts.families.insert(FontFamily::Monospace, regular);
    ctx.set_fonts(fonts);
}

pub(super) fn setup_style(ctx: &egui::Context, pal: Palette) {
    if hud::soft() { setup_style_soft(ctx); return; }
    let mut v = egui::Visuals::dark();
    v.panel_fill = Color32::TRANSPARENT;
    v.window_fill = Color32::from_rgb(8, 11, 16);
    v.window_rounding = Rounding::ZERO;
    v.menu_rounding = Rounding::ZERO;
    v.window_stroke = Stroke::new(1.0_f32, CYAN);
    v.window_shadow = egui::epaint::Shadow { offset: vec2(0.0, 0.0), blur: 18.0, spread: 0.0, color: hud::with_alpha(CYAN, 50) };
    v.popup_shadow = v.window_shadow;
    v.extreme_bg_color = Color32::from_rgb(4, 6, 9);
    v.override_text_color = Some(tx());
    v.selection.bg_fill = hud::with_alpha(MAGENTA, 120);
    v.selection.stroke = Stroke::new(1.0_f32, CYAN);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, pal.hair);
    v.widgets.inactive.bg_fill = Color32::from_rgb(10, 16, 22);
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(10, 16, 22);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, hud::with_alpha(CYAN, 110));
    v.widgets.hovered.bg_fill = CYAN;
    v.widgets.hovered.weak_bg_fill = CYAN;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, CYAN);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, BG0);
    v.widgets.active.bg_fill = YELLOW;
    v.widgets.active.weak_bg_fill = YELLOW;
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, BG0);
    for wd in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        wd.rounding = Rounding::ZERO;
    }
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Rect { aspect_ratio: 0.6 };
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = vec2(10.0, 10.0);
        s.spacing.button_padding = vec2(14.0, 7.0);
        s.spacing.slider_width = 180.0;
        s.spacing.interact_size.y = 26.0;
        s.text_styles.insert(egui::TextStyle::Body, FontId::proportional(14.0));
        s.text_styles.insert(egui::TextStyle::Button, FontId::proportional(14.0));
        s.text_styles.insert(egui::TextStyle::Small, FontId::proportional(12.0));
    });
}

/// Cinematic Dark: soft rounded widgets, violet selection, frosted popups.
pub(super) fn setup_style_soft(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    let glass = Color32::from_rgb(22, 21, 32);
    v.panel_fill = Color32::TRANSPARENT;
    v.window_fill = glass;
    v.window_rounding = Rounding::same(14.0);
    v.menu_rounding = Rounding::same(12.0);
    v.window_stroke = Stroke::new(1.0_f32, Color32::from_white_alpha(28));
    v.window_shadow = egui::epaint::Shadow { offset: vec2(0.0, 10.0), blur: 32.0, spread: 0.0, color: Color32::from_black_alpha(140) };
    v.popup_shadow = v.window_shadow;
    v.extreme_bg_color = Color32::from_rgba_unmultiplied(255, 255, 255, 12);
    v.override_text_color = Some(Color32::from_rgb(236, 234, 245));
    v.selection.bg_fill = hud::with_alpha(hud::accent(), 110);
    v.selection.stroke = Stroke::new(1.0_f32, hud::accent());
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, Color32::from_white_alpha(20));
    for (w, a) in [(&mut v.widgets.inactive, 16u8), (&mut v.widgets.hovered, 34), (&mut v.widgets.active, 52), (&mut v.widgets.open, 34)] {
        w.bg_fill = Color32::from_white_alpha(a);
        w.weak_bg_fill = Color32::from_white_alpha(a);
        w.bg_stroke = Stroke::new(1.0_f32, Color32::from_white_alpha(a + 10));
        w.fg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(236, 234, 245));
    }
    for wd in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        wd.rounding = Rounding::same(10.0);
    }
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = vec2(10.0, 10.0);
        s.spacing.button_padding = vec2(14.0, 7.0);
        s.spacing.slider_width = 180.0;
        s.spacing.interact_size.y = 28.0;
        s.text_styles.insert(egui::TextStyle::Body, FontId::proportional(14.5));
        s.text_styles.insert(egui::TextStyle::Button, FontId::proportional(14.5));
        s.text_styles.insert(egui::TextStyle::Small, FontId::proportional(12.5));
    });
}

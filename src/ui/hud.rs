//! Cyberpunk / sci-fi HUD drawing kit for the dashboard: chamfered (45°) panels, neon edges with a
//! glow, glitch text, scanlines + grid, and the small decorations (brackets, crosshairs, barcodes,
//! hex strings). Everything is painted with egui shapes — no images, no extra memory.

use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke};

pub const BG0: Color32 = Color32::from_rgb(5, 5, 5);
pub const BG1: Color32 = Color32::from_rgb(13, 17, 23);
pub const CYAN: Color32 = Color32::from_rgb(0, 229, 255);
pub const MAGENTA: Color32 = Color32::from_rgb(255, 0, 255);
pub const YELLOW: Color32 = Color32::from_rgb(252, 238, 9);

pub fn with_alpha(c: Color32, a: u8) -> Color32 { Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a) }

// ---- "Cinematic Dark" look ------------------------------------------------------------------
// The same drawing kit doubles as the soft theme: when SOFT is on, panels become rounded frosted
// glass with hairline edges, the neon colours are toned down to violet / soft cyan, and the HUD
// decorations (brackets, barcodes, crosshairs, glitches) simply disappear. Every page of the
// dashboard is drawn through these helpers, so all of them follow the theme with no rewrite.

static SOFT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn set_soft(on: bool) { SOFT.store(on, std::sync::atomic::Ordering::Relaxed); }
pub fn soft() -> bool { SOFT.load(std::sync::atomic::Ordering::Relaxed) }

/// Cinematic palette (Apple-like): deep black, charcoal, raised surfaces, near-white text.
pub const C_BG: Color32 = Color32::from_rgb(8, 9, 12);
pub const C_CHARCOAL: Color32 = Color32::from_rgb(17, 19, 24);
pub const C_TEXT: Color32 = Color32::from_rgb(245, 245, 247);
pub const C_TEXT2: Color32 = Color32::from_rgb(152, 155, 165);
pub const C_VIOLET: Color32 = Color32::from_rgb(167, 139, 250);
/// Translucent surface: the blurred wallpaper behind shows through (= frosted glass).
pub const C_GLASS: Color32 = Color32::from_rgba_premultiplied(15, 17, 21, 150);

// The accent follows the wallpaper (picked from its colours, eased in when it changes).
static ACCENT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0xA78BFA);
pub fn set_accent(c: Color32) { ACCENT.store(((c.r() as u32) << 16) | ((c.g() as u32) << 8) | c.b() as u32, std::sync::atomic::Ordering::Relaxed); }
pub fn accent() -> Color32 { let v = ACCENT.load(std::sync::atomic::Ordering::Relaxed); Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8) }
/// Readable glyph colour on top of the accent.
pub fn on_accent() -> Color32 {
    let a = accent();
    let l = 0.2126 * a.r() as f32 + 0.7152 * a.g() as f32 + 0.0722 * a.b() as f32;
    if l > 150.0 { C_CHARCOAL } else { Color32::WHITE }
}

/// Maps a HUD colour to its Cinematic counterpart (alpha kept). Identity in the HUD theme.
pub fn tone(c: Color32) -> Color32 {
    if !soft() { return c; }
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    let to = |k: Color32| Color32::from_rgba_unmultiplied(k.r(), k.g(), k.b(), a);
    match (r, g, b) {
        (0, 229, 255) => to(accent()),
        (255, 0, 255) | (255, 80, 255) => to(accent().lerp_to_gamma(Color32::WHITE, 0.35)),
        (252, 238, 9) => to(Color32::WHITE),
        (5, 5, 5) | (13, 17, 23) => to(C_BG),
        // Card / well fills of the HUD -> frosted glass.
        (10, 14, 20) | (8, 11, 16) | (4, 6, 9) | (10, 16, 22) => C_GLASS,
        (0, 255, 160) => to(Color32::from_rgb(134, 239, 172)),
        _ => c,
    }
}

/// The light catching the top rim of a glass surface: a hairline that fades out at both ends.
pub fn top_highlight(p: &Painter, r: Rect, rad: f32) {
    let y = r.top() + 0.5;
    let (x0, x1) = (r.left() + rad, r.right() - rad);
    if x1 <= x0 { return; }
    let mut m = egui::Mesh::default();
    let mid = Color32::from_white_alpha(42);
    let n = 8;
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let a = (1.0 - (2.0 * t - 1.0).abs()).powf(0.6);
        let col = Color32::from_white_alpha((mid.a() as f32 * a) as u8);
        let x = x0 + (x1 - x0) * t;
        m.colored_vertex(pos2(x, y - 0.5), col);
        m.colored_vertex(pos2(x, y + 0.5), col);
    }
    for i in 0..n as u32 { let k = i * 2; m.add_triangle(k, k + 1, k + 2); m.add_triangle(k + 1, k + 3, k + 2); }
    p.add(Shape::mesh(m));
}

/// Glass surface of the Cinematic look: soft shadow, translucent charcoal, hairline rim, top light.
pub fn glass(p: &Painter, r: Rect, rad: f32, alpha: u8) {
    p.rect_filled(r.translate(vec2(0.0, 12.0)).expand(4.0), rad + 6.0, Color32::from_black_alpha(38));
    p.rect_filled(r.translate(vec2(0.0, 2.0)), rad, Color32::from_black_alpha(34));
    p.rect_filled(r, rad, Color32::from_rgba_unmultiplied(C_CHARCOAL.r(), C_CHARCOAL.g(), C_CHARCOAL.b(), alpha));
    p.rect_stroke(r, rad, Stroke::new(1.0_f32, Color32::from_white_alpha(14)));
    top_highlight(p, r, rad);
}

/// Accent from a (small) RGB image: the most colourful bright-ish area, pushed to a pleasant
/// saturation / brightness so it reads well on dark glass. None for grey images.
pub fn accent_from(px: &[u8]) -> Option<Color32> {
    let (mut best, mut best_w) = (None, 0.0f32);
    for q in px.chunks_exact(4) {
        let (r, g, b) = (q[0] as f32 / 255.0, q[1] as f32 / 255.0, q[2] as f32 / 255.0);
        let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
        if mx < 0.12 { continue; }
        let s = (mx - mn) / mx;
        let w = s * s * mx.sqrt();
        if w > best_w { best_w = w; best = Some((r, g, b)); }
    }
    let (r, g, b) = best?;
    if best_w < 0.05 { return None; }
    // RGB -> HSV, then fixed S/V.
    let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
    let d = mx - mn;
    let mut h = if d == 0.0 { 0.0 } else if mx == r { ((g - b) / d) % 6.0 } else if mx == g { (b - r) / d + 2.0 } else { (r - g) / d + 4.0 };
    if h < 0.0 { h += 6.0; }
    let (s, v) = (0.48, 0.96);
    let c = v * s;
    let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
    let (r1, g1, b1) = match h as u32 { 0 => (c, x, 0.0), 1 => (x, c, 0.0), 2 => (0.0, c, x), 3 => (0.0, x, c), 4 => (x, 0.0, c), _ => (c, 0.0, x) };
    let m = v - c;
    Some(Color32::from_rgb(((r1 + m) * 255.0) as u8, ((g1 + m) * 255.0) as u8, ((b1 + m) * 255.0) as u8))
}

/// Rectangle with the top-left and bottom-right corners cut at 45° (like `clip-path: polygon`).
pub fn chamfer(r: Rect, cut: f32) -> Vec<Pos2> {
    let c = cut.min(r.width() / 2.0).min(r.height() / 2.0).max(0.0);
    vec![
        pos2(r.left() + c, r.top()), pos2(r.right(), r.top()), pos2(r.right(), r.bottom() - c),
        pos2(r.right() - c, r.bottom()), pos2(r.left(), r.bottom()), pos2(r.left(), r.top() + c),
    ]
}

/// A neon panel: chamfered fill + 1 px edge + a soft glow (three widening, fainter outlines).
pub fn neon_panel(p: &Painter, r: Rect, cut: f32, fill: Color32, edge: Color32, glow: f32) {
    if soft() {
        // No neon outline here: a soft drop shadow, the fill, and a barely-there light edge on
        // top (like light catching the rim of a glass card).
        let rad = (cut * 1.3).clamp(8.0, 22.0).min(r.height() / 2.0);
        let fill = tone(fill);
        let big = r.width() > 120.0 && r.height() > 60.0;
        if big {
            p.rect_filled(r.translate(vec2(0.0, 10.0)).expand(2.0), rad + 4.0, Color32::from_black_alpha(46));
            p.rect_filled(r.translate(vec2(0.0, 3.0)), rad, Color32::from_black_alpha(40));
        }
        p.rect_filled(r, rad, fill);
        if fill.a() < 250 || big {
            p.rect_stroke(r, rad, Stroke::new(1.0_f32, Color32::from_white_alpha(if big { 14 } else { 20 })));
            if big { top_highlight(p, r, rad); }
        }
        let _ = (edge, glow);
        return;
    }
    let pts = chamfer(r, cut);
    p.add(Shape::convex_polygon(pts.clone(), fill, Stroke::NONE));
    if glow > 0.0 {
        for (w, a) in [(7.0, 10.0), (4.5, 20.0), (2.5, 38.0)] {
            p.add(Shape::closed_line(pts.clone(), Stroke::new(w * glow.max(0.3), with_alpha(edge, a as u8))));
        }
    }
    p.add(Shape::closed_line(pts, Stroke::new(1.0_f32, edge)));
}

/// Cuts the top-left and bottom-right corners of an already painted rectangle: paints the
/// background colour over the corner triangles and draws the diagonal edge with a glow.
pub fn cut_corners(p: &Painter, r: Rect, cut: f32, bg: Color32, edge: Color32) {
    if soft() { return; }
    let c = cut.min(r.width() / 2.0).min(r.height() / 2.0);
    let (tl_a, tl_b) = (pos2(r.left(), r.top() + c), pos2(r.left() + c, r.top()));
    let (br_a, br_b) = (pos2(r.right(), r.bottom() - c), pos2(r.right() - c, r.bottom()));
    // Slightly oversized so the old border pixels at the corner disappear.
    let e = 1.5;
    p.add(Shape::convex_polygon(vec![pos2(r.left() - e, r.top() - e), pos2(r.left() + c + e, r.top() - e), pos2(r.left() - e, r.top() + c + e)], bg, Stroke::NONE));
    p.add(Shape::convex_polygon(vec![pos2(r.right() + e, r.bottom() + e), pos2(r.right() - c - e, r.bottom() + e), pos2(r.right() + e, r.bottom() - c - e)], bg, Stroke::NONE));
    for (a, b) in [(tl_a, tl_b), (br_a, br_b)] {
        p.line_segment([a, b], Stroke::new(4.0_f32, with_alpha(edge, 26)));
        p.line_segment([a, b], Stroke::new(1.0_f32, edge));
    }
}

/// Text with a neon glow (text-shadow): faint copies around it, then the text itself.
pub fn glow_text(p: &Painter, pos: Pos2, align: Align2, text: &str, font: FontId, col: Color32) {
    if soft() { p.text(pos, align, text, font, tone(col)); return; }
    for (dx, dy, a) in [(-1.5, 0.0, 34), (1.5, 0.0, 34), (0.0, -1.5, 34), (0.0, 1.5, 34), (-3.0, 0.0, 14), (3.0, 0.0, 14)] {
        p.text(pos + vec2(dx, dy), align, text, font.clone(), with_alpha(col, a));
    }
    p.text(pos, align, text, font, col);
}

/// Hover "glitch": the text shifted in magenta and cyan behind the real one.
pub fn glitch_text(p: &Painter, pos: Pos2, align: Align2, text: &str, font: FontId, col: Color32, amount: f32) {
    if soft() { p.text(pos, align, text, font, tone(col)); return; }
    if amount > 0.02 {
        p.text(pos + vec2(-2.0 * amount, 0.5), align, text, font.clone(), with_alpha(MAGENTA, (200.0 * amount) as u8));
        p.text(pos + vec2(2.0 * amount, -0.5), align, text, font.clone(), with_alpha(CYAN, (200.0 * amount) as u8));
    }
    p.text(pos, align, text, font, col);
}

/// Corner brackets `[ ]` around a rect.
pub fn brackets(p: &Painter, r: Rect, len: f32, col: Color32) {
    if soft() { return; }
    let s = Stroke::new(1.5_f32, col);
    for (corner, dx, dy) in [(r.left_top(), 1.0, 1.0), (r.right_top(), -1.0, 1.0), (r.left_bottom(), 1.0, -1.0), (r.right_bottom(), -1.0, -1.0)] {
        p.line_segment([corner, corner + vec2(len * dx, 0.0)], s);
        p.line_segment([corner, corner + vec2(0.0, len * dy)], s);
    }
}

pub fn crosshair(p: &Painter, c: Pos2, r: f32, col: Color32) {
    if soft() { return; }
    let s = Stroke::new(1.0_f32, col);
    p.line_segment([c + vec2(-r, 0.0), c + vec2(-r * 0.35, 0.0)], s);
    p.line_segment([c + vec2(r * 0.35, 0.0), c + vec2(r, 0.0)], s);
    p.line_segment([c + vec2(0.0, -r), c + vec2(0.0, -r * 0.35)], s);
    p.line_segment([c + vec2(0.0, r * 0.35), c + vec2(0.0, r)], s);
    p.rect_stroke(Rect::from_center_size(c, vec2(r * 0.3, r * 0.3)), 0.0, s);
}

fn hash(seed: u32) -> u32 {
    let mut h = seed.wrapping_mul(0x9E37_79B1) ^ 0x85EB_CA6B;
    h ^= h >> 15; h = h.wrapping_mul(0x2C1B_3C6D); h ^= h >> 12; h = h.wrapping_mul(0x297A_2D39); h ^ (h >> 15)
}

/// Tiny barcode strip (deterministic from `seed`).
pub fn barcode(p: &Painter, r: Rect, seed: u32, col: Color32) {
    if soft() { return; }
    let mut x = r.left();
    let mut i = 0;
    while x < r.right() {
        let h = hash(seed.wrapping_add(i));
        let w = 1.0 + (h % 3) as f32;
        if h & 8 != 0 { p.rect_filled(Rect::from_min_size(pos2(x, r.top()), vec2(w, r.height())), 0.0, col); }
        x += w + 1.0 + ((h >> 4) % 2) as f32;
        i += 1;
    }
}

pub fn hex(seed: u32, digits: usize) -> String {
    let h = hash(seed) as u64 | ((hash(seed ^ 0xA5A5) as u64) << 32);
    format!("0x{:0width$X}", h & ((1u64 << (digits * 4)) - 1), width = digits)
}

/// Dark grid + scanlines over the whole window (painted once, under everything else).
pub fn backdrop(p: &Painter, r: Rect) {
    p.rect_filled(r, 0.0, BG0);
    let grid = with_alpha(CYAN, 9);
    let step = 36.0;
    let mut x = (r.left() / step).ceil() * step;
    while x < r.right() { p.line_segment([pos2(x, r.top()), pos2(x, r.bottom())], Stroke::new(1.0_f32, grid)); x += step; }
    let mut y = (r.top() / step).ceil() * step;
    while y < r.bottom() { p.line_segment([pos2(r.left(), y), pos2(r.right(), y)], Stroke::new(1.0_f32, grid)); y += step; }
}

/// Scanlines drawn on top of everything (very faint).
pub fn scanlines(ctx: &egui::Context, r: Rect) {
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("scanlines")));
    let col = Color32::from_rgba_unmultiplied(0, 0, 0, 34);
    let mut y = r.top();
    while y < r.bottom() { p.line_segment([pos2(r.left(), y), pos2(r.right(), y)], Stroke::new(1.0_f32, col)); y += 3.0; }
}

//! Little vector icon set (~16 px) — no icon font needed.

use super::*;

#[derive(Clone, Copy)]
pub(super) enum Icon { Gallery, Displays, Taskbar, Gear, Play, Pause, Speaker, Mute, Plus, Trash, Folder, Music, Palette, Fold(bool), Widgets }

impl From<Tab> for Icon {
    fn from(t: Tab) -> Self {
        match t { Tab::Gallery => Icon::Gallery, Tab::Music => Icon::Music, Tab::Displays => Icon::Displays, Tab::Taskbar => Icon::Taskbar, Tab::Widgets => Icon::Widgets, Tab::Theme => Icon::Palette, Tab::Preferences => Icon::Gear }
    }
}

/// Small vector icon set (~16 px), so no icon font is needed.
pub(super) fn paint_icon(p: &egui::Painter, c: Pos2, icon: Icon, col: Color32) {
    let s = Stroke::new(1.6_f32, col);
    let r = |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(c + vec2(x, y), vec2(w, h));
    match icon {
        Icon::Gallery => for (x, y) in [(-7.0, -7.0), (1.0, -7.0), (-7.0, 1.0), (1.0, 1.0)] {
            p.rect_stroke(r(x, y, 6.0, 6.0), Rounding::ZERO, s);
        },
        Icon::Displays => {
            p.rect_stroke(r(-8.0, -7.0, 16.0, 11.0), Rounding::ZERO, s);
            p.line_segment([c + vec2(-4.0, 7.5), c + vec2(4.0, 7.5)], s);
        }
        Icon::Taskbar => {
            p.rect_stroke(r(-8.0, -7.0, 16.0, 14.0), Rounding::ZERO, s);
            p.rect_filled(r(-4.5, 2.0, 9.0, 2.6), Rounding::ZERO, col);
        }
        Icon::Gear => {
            // A real cog: one outline with 6 flat-topped teeth, and the hole in the middle.
            let (teeth, r_in, r_out) = (6, 5.6_f32, 8.2_f32);
            let mut pts = Vec::new();
            for i in 0..teeth {
                let a0 = i as f32 * std::f32::consts::TAU / teeth as f32;
                let step = std::f32::consts::TAU / teeth as f32;
                for (da, r) in [(0.0, r_in), (0.14, r_out), (0.36, r_out), (0.5, r_in)] {
                    let a = a0 + da * step;
                    pts.push(c + vec2(a.cos(), a.sin()) * r);
                }
                let a = a0 + 0.75 * step;
                pts.push(c + vec2(a.cos(), a.sin()) * r_in);
            }
            p.add(Shape::closed_line(pts, Stroke::new(1.5_f32, col)));
            p.circle_stroke(c, 2.6, Stroke::new(1.5_f32, col));
        }
        Icon::Play => { p.add(Shape::convex_polygon(vec![c + vec2(-4.0, -6.0), c + vec2(6.5, 0.0), c + vec2(-4.0, 6.0)], col, Stroke::NONE)); }
        Icon::Pause => { p.rect_filled(r(-5.0, -6.0, 3.6, 12.0), Rounding::ZERO, col); p.rect_filled(r(1.4, -6.0, 3.6, 12.0), Rounding::ZERO, col); }
        Icon::Speaker | Icon::Mute => {
            p.add(Shape::convex_polygon(vec![c + vec2(-7.0, -2.5), c + vec2(-4.0, -2.5), c + vec2(0.0, -6.5), c + vec2(0.0, 6.5), c + vec2(-4.0, 2.5), c + vec2(-7.0, 2.5)], col, Stroke::NONE));
            if matches!(icon, Icon::Mute) {
                p.line_segment([c + vec2(3.0, -3.0), c + vec2(8.0, 3.0)], s);
                p.line_segment([c + vec2(3.0, 3.0), c + vec2(8.0, -3.0)], s);
            } else {
                for (rad, n) in [(4.5_f32, 7), (8.0, 9)] {
                    let pts: Vec<Pos2> = (0..=n).map(|i| {
                        let a = -0.8 + 1.6 * i as f32 / n as f32;
                        c + vec2(a.cos() * rad, a.sin() * rad)
                    }).collect();
                    p.add(Shape::line(pts, s));
                }
            }
        }
        Icon::Plus => { p.line_segment([c + vec2(-5.0, 0.0), c + vec2(5.0, 0.0)], Stroke::new(2.0_f32, col)); p.line_segment([c + vec2(0.0, -5.0), c + vec2(0.0, 5.0)], Stroke::new(2.0_f32, col)); }
        Icon::Music => {
            p.circle_filled(c + vec2(-3.2, 5.0), 3.3, col);
            p.line_segment([c + vec2(-0.1, 5.0), c + vec2(-0.1, -6.0)], Stroke::new(1.9_f32, col));
            p.line_segment([c + vec2(-0.1, -6.0), c + vec2(5.6, -3.4)], Stroke::new(2.4_f32, col));
        }
        Icon::Folder => {
            p.rect_stroke(r(-7.5, -4.5, 15.0, 11.0), Rounding::ZERO, s);
            p.line_segment([c + vec2(-7.5, -4.5), c + vec2(-7.5, -6.5)], s);
            p.line_segment([c + vec2(-7.5, -6.5), c + vec2(-2.0, -6.5)], s);
            p.line_segment([c + vec2(-2.0, -6.5), c + vec2(0.0, -4.5)], s);
        }
        Icon::Widgets => {
            // one big and two small rounded tiles, like the widget gallery
            p.rect_stroke(r(-8.0, -7.0, 9.0, 14.0), Rounding::same(2.0), s);
            p.rect_stroke(r(3.0, -7.0, 5.0, 6.0), Rounding::same(1.5), s);
            p.rect_stroke(r(3.0, 1.0, 5.0, 6.0), Rounding::same(1.5), s);
        }
        Icon::Palette => {
            p.circle_stroke(c, 7.5, s);
            for (dx, dy) in [(-3.2, -2.4), (0.6, -4.0), (3.6, -1.0)] { p.circle_filled(c + vec2(dx, dy), 1.5, col); }
            p.circle_filled(c + vec2(-1.5, 3.5), 2.0, col);
        }
        Icon::Fold(open) => {
            // « or »
            let d = if open { -1.0 } else { 1.0 };
            for k in [-3.0, 2.0] {
                p.line_segment([c + vec2(k - 2.5 * d, -5.0), c + vec2(k + 2.5 * d, 0.0)], s);
                p.line_segment([c + vec2(k + 2.5 * d, 0.0), c + vec2(k - 2.5 * d, 5.0)], s);
            }
        }
        Icon::Trash => {
            p.line_segment([c + vec2(-6.0, -4.0), c + vec2(6.0, -4.0)], s);
            p.rect_stroke(r(-4.5, -4.0, 9.0, 10.0), Rounding::ZERO, s);
            p.line_segment([c + vec2(-2.0, -6.5), c + vec2(2.0, -6.5)], s);
        }
    }
}

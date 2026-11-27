//! Small settings controls shared with the desktop widgets' own settings panels (so a widget kind
//! can draw its options without reaching into the dashboard's internals).

use super::*;

pub(crate) use super::widgets::{segmented, toggle};

/// "Title ........ control": returns what the control returned (true = changed).
pub(crate) fn row(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui) -> bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(vec2(190.0, 28.0), Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_size(vec2(190.0, 28.0));
            ui.label(RichText::new(title).font(semi(13.5)).color(tx()));
        });
        changed = add(ui);
    });
    changed
}

/// Drop-down list; Some(index) when the user picked a (different) entry.
pub(crate) fn combo(ui: &mut Ui, id: &str, selected: usize, labels: &[&str]) -> Option<usize> {
    let mut picked = None;
    egui::ComboBox::from_id_salt(id).width(230.0).selected_text(labels.get(selected).copied().unwrap_or("")).show_ui(ui, |ui| {
        for (i, l) in labels.iter().enumerate() {
            if ui.selectable_label(i == selected, *l).clicked() && i != selected { picked = Some(i); }
        }
    });
    picked
}

/// Colour swatch that opens egui's colour picker.
pub(crate) fn color(ui: &mut Ui, c: &mut [u8; 3]) -> bool { ui.color_edit_button_srgb(c).changed() }

/// A titled slider row with a formatted value.
pub(crate) fn slider_row(ui: &mut Ui, title: &str, v: &mut f32, range: std::ops::RangeInclusive<f32>, fmt: impl Fn(f32) -> String + 'static) -> bool {
    row(ui, title, |ui| {
        ui.spacing_mut().slider_width = 200.0;
        let ch = ui.add(egui::Slider::new(v, range).show_value(false)).changed();
        ui.label(RichText::new(fmt(*v)).font(semi(12.5)).color(subtle()));
        ch
    })
}

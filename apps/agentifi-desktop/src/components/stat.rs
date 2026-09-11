//! Minimal stat presentation.
//!
//! The Overview relies on whitespace instead of cards, so a stat is just a
//! monospace value followed by a muted label.

use crate::{components::text, theme};
use eframe::egui::{self, Align2, Color32, Pos2, Sense, Ui, Vec2};

/// Borderless `value label` stat for the minimal Overview strip.
pub fn stat(ui: &mut Ui, value: &str, label: &str, accent: Color32) -> egui::Response {
    let font_value = egui::FontId::new(15.0, egui::FontFamily::Monospace);
    let font_label = theme::font_meta();
    let value_width = text::measure(ui.painter(), value, font_value.clone()).x;
    let label_width = text::measure(ui.painter(), label, font_label.clone()).x;
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(value_width + label_width + 10.0, 22.0),
        Sense::click(),
    );
    let painter = ui.painter();
    text::line(
        painter,
        rect.left_center(),
        Align2::LEFT_CENTER,
        value,
        font_value,
        accent,
        value_width + 2.0,
    );
    text::line(
        painter,
        Pos2::new(rect.left() + value_width + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        font_label,
        theme::TEXT_MUTED,
        label_width + 2.0,
    );
    response
}

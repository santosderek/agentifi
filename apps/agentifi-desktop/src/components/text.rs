//! Painting primitives shared by every dense component.
//!
//! Rows, cards, and metrics are painted rather than stacked from widgets so that
//! long titles truncate cleanly instead of wrapping mid-word and breaking the
//! layout.

use crate::theme;
use eframe::egui::{
    text::{LayoutJob, TextFormat, TextWrapping},
    Align, Align2, Color32, FontId, Painter, Pos2, Rect, Sense, Ui, Vec2,
};

/// Draws one truncated line of text and returns the rect it occupied.
pub fn line(
    painter: &Painter,
    pos: Pos2,
    align: Align2,
    text: &str,
    font: FontId,
    color: Color32,
    max_width: f32,
) -> Rect {
    let mut job = LayoutJob::single_section(
        text.to_owned(),
        TextFormat {
            font_id: font,
            color,
            ..Default::default()
        },
    );
    job.wrap = TextWrapping::truncate_at_width(max_width.max(16.0));
    let galley = painter.layout_job(job);
    let size = galley.size();
    let offset = Vec2::new(
        match align.x() {
            Align::Min => 0.0,
            Align::Center => -size.x / 2.0,
            Align::Max => -size.x,
        },
        match align.y() {
            Align::Min => 0.0,
            Align::Center => -size.y / 2.0,
            Align::Max => -size.y,
        },
    );
    let min = pos + offset;
    painter.galley(min, galley, color);
    Rect::from_min_size(min, size)
}

/// Measures a single line without drawing it.
#[must_use]
pub fn measure(painter: &Painter, text: &str, font: FontId) -> Vec2 {
    painter
        .layout_no_wrap(text.to_owned(), font, theme::TEXT_PRIMARY)
        .size()
}

/// Small status dot.
pub fn dot(painter: &Painter, center: Pos2, color: Color32) {
    painter.circle_filled(center, 3.5, color);
}

/// Joins metadata fragments with the standard middle-dot separator.
#[must_use]
pub fn meta_line(parts: &[Option<String>]) -> String {
    parts
        .iter()
        .filter_map(|part| part.as_deref())
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join("  ·  ")
}

/// Muted uppercase section label followed by a hairline rule.
pub fn section_label(ui: &mut Ui, text: &str) {
    ui.add_space(theme::SPACE_SM);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 16.0), Sense::hover());
    line(
        ui.painter(),
        rect.left_center(),
        Align2::LEFT_CENTER,
        &text.to_uppercase(),
        theme::font_label(),
        theme::TEXT_MUTED,
        rect.width(),
    );
    ui.add_space(theme::SPACE_XS);
    theme::hairline(ui);
    ui.add_space(theme::SPACE_SM);
}

/// Right-aligned keyboard hint, in the style of a terminal dashboard.
pub fn key_hint(painter: &Painter, right_center: Pos2, key: &str) {
    line(
        painter,
        right_center,
        Align2::RIGHT_CENTER,
        key,
        theme::font_meta(),
        theme::TEXT_MUTED,
        60.0,
    );
}

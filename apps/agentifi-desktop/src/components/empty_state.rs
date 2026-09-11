//! Intentional empty states: a short explanation, optionally with one action.
//! Large blank regions are never left unexplained.

use crate::{components::text, theme};
use eframe::egui::{self, Align2, Pos2, Sense, Ui, Vec2};

/// Explanatory empty state without an action.
pub fn empty_state(ui: &mut Ui, title: &str, body: &str) {
    draw(ui, title, body, None);
}

/// Empty state with a single call to action. Returns true when it is clicked.
pub fn empty_state_action(ui: &mut Ui, title: &str, body: &str, action: &str) -> bool {
    draw(ui, title, body, Some(action))
}

fn draw(ui: &mut Ui, title: &str, body: &str, action: Option<&str>) -> bool {
    ui.add_space(theme::SPACE_LG);
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 46.0), Sense::hover());
    let painter = ui.painter();
    text::line(
        painter,
        Pos2::new(rect.left(), rect.top()),
        Align2::LEFT_TOP,
        title,
        theme::font_body(),
        theme::TEXT_SECONDARY,
        width,
    );
    text::line(
        painter,
        Pos2::new(rect.left(), rect.top() + 20.0),
        Align2::LEFT_TOP,
        body,
        theme::font_meta(),
        theme::TEXT_MUTED,
        width,
    );
    let mut clicked = false;
    if let Some(action) = action {
        ui.add_space(theme::SPACE_XS);
        clicked = ui.add(egui::Button::new(action)).clicked();
    }
    ui.add_space(theme::SPACE_SM);
    clicked
}

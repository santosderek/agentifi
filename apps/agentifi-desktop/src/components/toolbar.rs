//! Toolbar primitives: title block, search field, filter chips, breadcrumbs.

use crate::{components::text, theme};
use eframe::egui::{self, Align2, Id, Pos2, RichText, Sense, Ui, Vec2};

/// View title with a one-line explanation underneath.
pub fn title_block(ui: &mut Ui, title: &str, subtitle: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.0), Sense::hover());
    let painter = ui.painter();
    text::line(
        painter,
        Pos2::new(rect.left(), rect.top()),
        Align2::LEFT_TOP,
        title,
        theme::font_title(),
        theme::TEXT_PRIMARY,
        rect.width(),
    );
    text::line(
        painter,
        Pos2::new(rect.left(), rect.top() + 22.0),
        Align2::LEFT_TOP,
        subtitle,
        theme::font_meta(),
        theme::TEXT_MUTED,
        rect.width(),
    );
}

/// Search field. Focus is requested from the `/` shortcut.
pub fn search_field(
    ui: &mut Ui,
    value: &mut String,
    hint: &str,
    width: f32,
    focus: bool,
) -> egui::Response {
    let id = Id::new("agentifi-search");
    let response = ui.add_sized(
        [width, 28.0],
        egui::TextEdit::singleline(value)
            .id(id)
            .hint_text(RichText::new(hint).color(theme::TEXT_MUTED)),
    );
    if focus {
        response.request_focus();
    }
    response
}

/// Filter chip that shows its current value, e.g. `Status: active`.
///
/// The chip is the menu trigger, so an active filter is always readable without
/// opening anything.
pub fn filter_menu<R>(
    ui: &mut Ui,
    label: &str,
    value: Option<&str>,
    contents: impl FnOnce(&mut Ui) -> R,
) -> egui::Response {
    let display = match value {
        Some(value) => format!("{label}: {value}  ⌄"),
        None => format!("{label}: all  ⌄"),
    };
    let color = if value.is_some() {
        theme::TEXT_PRIMARY
    } else {
        theme::TEXT_SECONDARY
    };
    ui.menu_button(
        RichText::new(display).font(theme::font_meta()).color(color),
        contents,
    )
    .response
}

/// `Explorer / project / session` trail.
pub fn breadcrumb(ui: &mut Ui, parts: &[String]) {
    let joined = parts.join("  /  ");
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), Sense::hover());
    text::line(
        ui.painter(),
        rect.left_center(),
        Align2::LEFT_CENTER,
        &joined,
        theme::font_meta(),
        theme::TEXT_MUTED,
        rect.width(),
    );
}

/// Right-aligned result count.
pub fn count_label(ui: &mut Ui, count: usize, noun: &str) {
    ui.label(
        RichText::new(format!("{count} {noun}"))
            .font(theme::font_meta())
            .color(theme::TEXT_SECONDARY),
    );
}

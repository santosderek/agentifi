//! Left navigation rail items.
//!
//! Selection is a slightly lighter surface plus a two-pixel accent, never a
//! filled blue button.

use crate::{
    components::{icon::Icon, text},
    theme,
};
use eframe::egui::{Align2, CornerRadius, Pos2, Sense, Ui, Vec2};

const ITEM_HEIGHT: f32 = 30.0;

/// Primary view entry with an icon and a keyboard hint.
pub fn nav_item(ui: &mut Ui, selected: bool, icon: Icon, label: &str, key: &str) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ITEM_HEIGHT), Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(
            rect,
            CornerRadius::same(theme::RADIUS_CONTROL),
            theme::SURFACE_SELECTED,
        );
    } else if response.hovered() {
        painter.rect_filled(
            rect,
            CornerRadius::same(theme::RADIUS_CONTROL),
            theme::SURFACE_ELEVATED,
        );
    }
    if selected {
        theme::accent_strip(ui, rect.shrink2(Vec2::new(0.0, 5.0)), theme::BLUE);
    }

    let color = if selected {
        theme::TEXT_PRIMARY
    } else {
        theme::TEXT_SECONDARY
    };
    crate::components::icon::paint_at(
        ui,
        icon,
        Pos2::new(rect.left() + 18.0, rect.center().y),
        if selected {
            theme::BLUE
        } else {
            theme::TEXT_MUTED
        },
        14.0,
    );
    let painter = ui.painter();
    text::line(
        painter,
        Pos2::new(rect.left() + 34.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        theme::font_body(),
        color,
        rect.width() - 74.0,
    );
    text::key_hint(painter, Pos2::new(rect.right() - 8.0, rect.center().y), key);
    response.clicked()
}

/// Project shortcut with a session count.
pub fn rail_project(ui: &mut Ui, name: &str, count: usize, selected: bool) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::click());
    let painter = ui.painter();
    if selected || response.hovered() {
        painter.rect_filled(
            rect,
            CornerRadius::same(theme::RADIUS_CONTROL),
            if selected {
                theme::SURFACE_SELECTED
            } else {
                theme::SURFACE_ELEVATED
            },
        );
    }
    text::line(
        painter,
        Pos2::new(rect.left() + 18.0, rect.center().y),
        Align2::LEFT_CENTER,
        name,
        theme::font_body(),
        if selected {
            theme::TEXT_PRIMARY
        } else {
            theme::TEXT_SECONDARY
        },
        rect.width() - 56.0,
    );
    text::line(
        painter,
        Pos2::new(rect.right() - 8.0, rect.center().y),
        Align2::RIGHT_CENTER,
        &count.to_string(),
        theme::font_meta(),
        theme::TEXT_MUTED,
        40.0,
    );
    response.clicked()
}

/// Connection state footer.
pub fn rail_connection(ui: &mut Ui, live: bool, endpoint: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::hover());
    let painter = ui.painter();
    text::dot(
        painter,
        Pos2::new(rect.left() + 4.0, rect.top() + 8.0),
        if live { theme::GREEN } else { theme::RED },
    );
    text::line(
        painter,
        Pos2::new(rect.left() + 14.0, rect.top() + 2.0),
        Align2::LEFT_TOP,
        if live { "Connected" } else { "Offline" },
        theme::font_body(),
        theme::TEXT_SECONDARY,
        rect.width() - 20.0,
    );
    text::line(
        painter,
        Pos2::new(rect.left() + 14.0, rect.top() + 19.0),
        Align2::LEFT_TOP,
        endpoint,
        theme::font_label(),
        theme::TEXT_MUTED,
        rect.width() - 20.0,
    );
}

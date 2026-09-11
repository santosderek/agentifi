//! Compact board card. Cards represent live, controllable Pi sessions, so the
//! status mark reflects the Pi process while the lane reflects the user's own
//! organisation.

use crate::{components::text, theme};
use agentifi_domain::AgentSession;
use eframe::egui::{self, Align2, CornerRadius, Pos2, Sense, Stroke, Ui, Vec2};

const CARD_HEIGHT: f32 = 92.0;

/// Draws one card and returns its click response.
pub fn board_card(ui: &mut Ui, session: &AgentSession, selected: bool, now: i64) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, CARD_HEIGHT), Sense::click());
    let painter = ui.painter();
    let fill = if response.hovered() {
        theme::SURFACE_HOVER
    } else if selected {
        theme::SURFACE_SELECTED
    } else {
        theme::SURFACE
    };
    painter.rect(
        rect,
        CornerRadius::same(theme::RADIUS_CARD),
        fill,
        Stroke::new(1.0_f32, theme::BORDER_SUBTLE),
        egui::StrokeKind::Inside,
    );
    if selected {
        theme::accent_strip(ui, rect.shrink2(Vec2::new(0.0, 8.0)), theme::BLUE);
    }

    let painter = ui.painter();
    let left = rect.left() + 14.0;
    let right = rect.right() - 14.0;
    let inner = right - left;

    text::line(
        painter,
        Pos2::new(left, rect.top() + 12.0),
        Align2::LEFT_TOP,
        &session.display_title(),
        theme::font_row_title(),
        theme::TEXT_PRIMARY,
        inner,
    );
    if let Some(summary) = session.summary.as_deref() {
        text::line(
            painter,
            Pos2::new(left, rect.top() + 33.0),
            Align2::LEFT_TOP,
            summary,
            theme::font_body(),
            theme::TEXT_SECONDARY,
            inner,
        );
    }
    text::line(
        painter,
        Pos2::new(left, rect.bottom() - 32.0),
        Align2::LEFT_TOP,
        &text::meta_line(&[Some(session.project.clone()), session.model_label()]),
        theme::font_meta(),
        theme::TEXT_MUTED,
        inner,
    );

    let footer_y = rect.bottom() - 14.0;
    text::dot(
        painter,
        Pos2::new(left + 3.0, footer_y),
        theme::status_color(session.status),
    );
    text::line(
        painter,
        Pos2::new(left + 14.0, footer_y),
        Align2::LEFT_CENTER,
        session.status.label(),
        theme::font_meta(),
        theme::status_color(session.status),
        inner - 90.0,
    );
    text::line(
        painter,
        Pos2::new(right, footer_y),
        Align2::RIGHT_CENTER,
        &text::meta_line(&[
            session.message_count.map(|count| format!("{count} msgs")),
            Some(session.age_label(now)),
        ]),
        theme::font_meta(),
        theme::TEXT_SECONDARY,
        150.0,
    );
    ui.add_space(theme::SPACE_SM);
    response
}

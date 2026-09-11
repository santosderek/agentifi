//! Dense session row: title first, summary second, metadata last.
//!
//! Storage identifiers never appear here; they live in the workspace inspector's
//! diagnostics tab.

use crate::{components::text, theme};
use agentifi_domain::AgentSession;
use eframe::egui::{self, Align2, CornerRadius, Pos2, Rect, Sense, Ui, Vec2};

/// Options for the three row densities used across the product.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RowStyle {
    /// Explorer: title, summary, metadata, age, status.
    #[default]
    Full,
    /// Overview: title plus one metadata line and an age column.
    Compact,
    /// Workspace context rail: title only.
    Minimal,
}

impl RowStyle {
    fn height(self) -> f32 {
        match self {
            Self::Full => theme::ROW_HEIGHT,
            Self::Compact => 42.0,
            Self::Minimal => 26.0,
        }
    }
}

/// Draws a session row and returns its click response.
pub fn session_row(
    ui: &mut Ui,
    session: &AgentSession,
    selected: bool,
    now: i64,
    style: RowStyle,
) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, style.height()), Sense::click());
    let painter = ui.painter();

    if selected {
        painter.rect_filled(
            rect,
            CornerRadius::same(theme::RADIUS_CONTROL),
            theme::SURFACE_SELECTED,
        );
        theme::accent_strip(ui, rect.shrink2(Vec2::new(0.0, 4.0)), theme::BLUE);
    } else if response.hovered() {
        painter.rect_filled(
            rect,
            CornerRadius::same(theme::RADIUS_CONTROL),
            theme::SURFACE_ELEVATED,
        );
    }

    let painter = ui.painter();
    let left = rect.left() + 14.0;
    let right = rect.right() - 12.0;
    let age = session.age_label(now);
    let age_width = text::measure(painter, &age, theme::font_meta()).x;
    let status_label = session.status.label();
    let status_width = text::measure(painter, status_label, theme::font_meta()).x;
    let trailing = age_width + status_width + 42.0;
    let text_width = (right - left - trailing).max(80.0);

    match style {
        RowStyle::Minimal => {
            text::dot(
                painter,
                Pos2::new(left, rect.center().y),
                theme::status_color(session.status),
            );
            text::line(
                painter,
                Pos2::new(left + 12.0, rect.center().y),
                Align2::LEFT_CENTER,
                &session.display_title(),
                theme::font_body(),
                if selected {
                    theme::TEXT_PRIMARY
                } else {
                    theme::TEXT_SECONDARY
                },
                rect.width() - 40.0,
            );
        }
        RowStyle::Compact => {
            text::dot(
                painter,
                Pos2::new(left, rect.center().y),
                theme::status_color(session.status),
            );
            let title_top = rect.top() + 6.0;
            text::line(
                painter,
                Pos2::new(left + 14.0, title_top),
                Align2::LEFT_TOP,
                &session.display_title(),
                theme::font_row_title(),
                theme::TEXT_PRIMARY,
                text_width,
            );
            text::line(
                painter,
                Pos2::new(left + 14.0, title_top + 19.0),
                Align2::LEFT_TOP,
                &compact_meta(session),
                theme::font_meta(),
                theme::TEXT_MUTED,
                text_width,
            );
            text::line(
                painter,
                Pos2::new(right, rect.center().y),
                Align2::RIGHT_CENTER,
                &age,
                theme::font_meta(),
                theme::TEXT_SECONDARY,
                80.0,
            );
        }
        RowStyle::Full => {
            text::dot(
                painter,
                Pos2::new(left, rect.top() + 20.0),
                theme::status_color(session.status),
            );
            let column = left + 14.0;
            let top = rect.top() + 11.0;
            text::line(
                painter,
                Pos2::new(column, top),
                Align2::LEFT_TOP,
                &session.display_title(),
                theme::font_row_title(),
                theme::TEXT_PRIMARY,
                text_width,
            );
            if let Some(summary) = session.summary.as_deref() {
                text::line(
                    painter,
                    Pos2::new(column, top + 19.0),
                    Align2::LEFT_TOP,
                    summary,
                    theme::font_body(),
                    theme::TEXT_SECONDARY,
                    text_width,
                );
            }
            text::line(
                painter,
                Pos2::new(column, rect.bottom() - 20.0),
                Align2::LEFT_TOP,
                &full_meta(session),
                theme::font_meta(),
                theme::TEXT_MUTED,
                text_width,
            );
            text::line(
                painter,
                Pos2::new(right, rect.center().y),
                Align2::RIGHT_CENTER,
                &age,
                theme::font_meta(),
                theme::TEXT_SECONDARY,
                80.0,
            );
            text::line(
                painter,
                Pos2::new(right - age_width - 24.0, rect.center().y),
                Align2::RIGHT_CENTER,
                status_label,
                theme::font_meta(),
                theme::status_color(session.status),
                120.0,
            );
        }
    }

    bottom_hairline(ui, rect, style);
    response
}

fn bottom_hairline(ui: &Ui, rect: Rect, style: RowStyle) {
    if style == RowStyle::Minimal {
        return;
    }
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(1.0_f32, theme::BORDER_SUBTLE),
    );
}

/// `project · 39 msgs` for compact rows.
fn compact_meta(session: &AgentSession) -> String {
    text::meta_line(&[
        Some(session.project.clone()),
        session.message_count.map(|count| format!("{count} msgs")),
        Some(session.status.label().to_owned()),
    ])
}

/// `project/branch · model · counts` for the Explorer.
fn full_meta(session: &AgentSession) -> String {
    text::meta_line(&[
        Some(match session.branch.as_deref() {
            Some(branch) => format!("{}/{branch}", session.project),
            None => session.project.clone(),
        }),
        session.model_label(),
        session
            .message_count
            .map(|count| format!("{count} messages")),
        session
            .tool_call_count
            .filter(|count| *count > 0)
            .map(|count| format!("{count} tool calls")),
        (!session.tags.is_empty()).then(|| session.tags.join(" ")),
    ])
}

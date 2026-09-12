//! Hand-drawn line icons, so the desktop needs no icon-font dependency.

use crate::theme;
use eframe::egui::{self, Color32, Pos2, Sense, Stroke, Ui, Vec2};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Overview,
    Explorer,
    Board,
    Workspace,
    Search,
    Refresh,
    Back,
    Settings,
}

/// Paints an icon centred on `center`.
pub fn paint_at(ui: &Ui, icon: Icon, center: Pos2, color: Color32, size: f32) {
    let painter = ui.painter();
    let s = size * 0.5;
    let stroke = Stroke::new((size / 9.0).max(1.2), color);
    match icon {
        Icon::Overview => {
            // Three stacked bars of decreasing length: a dashboard summary.
            for (index, width) in [1.0_f32, 0.7, 0.45].into_iter().enumerate() {
                let y = center.y - s + index as f32 * s;
                painter.line_segment(
                    [
                        Pos2::new(center.x - s, y),
                        Pos2::new(center.x - s + 2.0 * s * width, y),
                    ],
                    stroke,
                );
            }
        }
        Icon::Explorer => {
            for offset in [-0.6_f32, 0.0, 0.6] {
                let y = center.y + offset * s;
                painter.circle_filled(Pos2::new(center.x - s, y), stroke.width * 0.9, color);
                painter.line_segment(
                    [Pos2::new(center.x - s + 4.0, y), Pos2::new(center.x + s, y)],
                    stroke,
                );
            }
        }
        Icon::Board => {
            for (index, height) in [1.0_f32, 0.62, 0.84].into_iter().enumerate() {
                let x = center.x - s + index as f32 * s;
                painter.rect_stroke(
                    egui::Rect::from_min_max(
                        Pos2::new(x, center.y - s),
                        Pos2::new(x + s * 0.6, center.y - s + 2.0 * s * height),
                    ),
                    2.0,
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
        }
        Icon::Workspace => {
            painter.rect_stroke(
                egui::Rect::from_center_size(center, Vec2::splat(size * 0.9)),
                2.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x - s * 0.2, center.y - s),
                    Pos2::new(center.x - s * 0.2, center.y + s),
                ],
                stroke,
            );
        }
        Icon::Search => {
            painter.circle_stroke(
                Pos2::new(center.x - s * 0.2, center.y - s * 0.2),
                s * 0.7,
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x + s * 0.3, center.y + s * 0.3),
                    Pos2::new(center.x + s, center.y + s),
                ],
                stroke,
            );
        }
        Icon::Refresh => {
            let rect = egui::Rect::from_center_size(center, Vec2::splat(size * 0.8));
            painter.circle_stroke(center, rect.width() * 0.5, stroke);
            painter.line_segment(
                [
                    Pos2::new(center.x + s * 0.4, center.y - s),
                    Pos2::new(center.x + s * 0.9, center.y - s * 0.4),
                ],
                stroke,
            );
        }
        Icon::Settings => {
            painter.circle_stroke(center, s * 0.55, stroke);
            painter.circle_filled(center, s * 0.18, color);
            for angle in [
                0.0_f32,
                std::f32::consts::FRAC_PI_2,
                std::f32::consts::PI,
                std::f32::consts::TAU * 0.75,
            ] {
                let direction = Vec2::angled(angle) * s;
                painter.line_segment([center + direction * 0.65, center + direction], stroke);
            }
        }
        Icon::Back => {
            painter.line_segment(
                [
                    Pos2::new(center.x + s * 0.6, center.y),
                    Pos2::new(center.x - s * 0.6, center.y),
                ],
                stroke,
            );
            for direction in [-1.0_f32, 1.0] {
                painter.line_segment(
                    [
                        Pos2::new(center.x - s * 0.6, center.y),
                        Pos2::new(center.x - s * 0.1, center.y + direction * s * 0.5),
                    ],
                    stroke,
                );
            }
        }
    }
}

/// Allocates space and paints an icon inline in the current layout.
pub fn paint(ui: &mut Ui, icon: Icon, color: Color32, size: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint_at(ui, icon, rect.center(), color, size);
    response
}

/// Icon-only button used in the toolbar.
pub fn icon_button(ui: &mut Ui, icon: Icon, tooltip: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(theme::RADIUS_CONTROL),
            theme::SURFACE_ELEVATED,
        );
    }
    paint_at(
        ui,
        icon,
        rect.center(),
        if response.hovered() {
            theme::TEXT_PRIMARY
        } else {
            theme::TEXT_SECONDARY
        },
        14.0,
    );
    response.on_hover_text(tooltip)
}

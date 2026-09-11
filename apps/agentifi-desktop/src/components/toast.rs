//! Toast layer for command results, connection changes, and errors.

use crate::{
    components::text,
    state::{Toast, Tone},
    theme,
};
use eframe::egui::{self, Align2, CornerRadius, Pos2, Sense, Stroke, Ui, Vec2};

/// Draws stacked toasts in the bottom-right corner of the content area.
pub fn toast_layer(ctx: &egui::Context, toasts: &[Toast]) {
    if toasts.is_empty() {
        return;
    }
    egui::Area::new(egui::Id::new("agentifi-toasts"))
        .anchor(Align2::RIGHT_BOTTOM, Vec2::new(-20.0, -46.0))
        .interactable(false)
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                for toast in toasts {
                    single(ui, toast);
                }
            });
        });
}

fn single(ui: &mut Ui, toast: &Toast) {
    let font = theme::font_body();
    let width = text::measure(ui.painter(), &toast.text, font.clone()).x + 46.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width.min(420.0), 34.0), Sense::hover());
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(theme::RADIUS_CONTROL),
        theme::SURFACE_ELEVATED,
        Stroke::new(1.0_f32, theme::BORDER),
        egui::StrokeKind::Inside,
    );
    text::dot(
        painter,
        Pos2::new(rect.left() + 14.0, rect.center().y),
        tone_color(toast.tone),
    );
    text::line(
        painter,
        Pos2::new(rect.left() + 26.0, rect.center().y),
        Align2::LEFT_CENTER,
        &toast.text,
        font,
        theme::TEXT_PRIMARY,
        rect.width() - 38.0,
    );
    ui.add_space(theme::SPACE_XS);
}

fn tone_color(tone: Tone) -> egui::Color32 {
    match tone {
        Tone::Info => theme::BLUE,
        Tone::Success => theme::GREEN,
        Tone::Warning => theme::ORANGE,
        Tone::Error => theme::RED,
    }
}

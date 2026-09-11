use crate::theme;
use eframe::egui::{Color32, Ui};
pub fn metric(ui: &mut Ui, value: String, label: &str, color: Color32) {
    theme::surface().show(ui, |ui| {
        ui.colored_label(color, "●");
        ui.heading(value);
        ui.label(
            eframe::egui::RichText::new(label)
                .small()
                .color(theme::TEXT_MUTED),
        );
    });
}

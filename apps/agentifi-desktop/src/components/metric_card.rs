use crate::theme;
use eframe::egui::{Color32, RichText, Ui};
pub fn metric(ui: &mut Ui, value: String, label: &str, color: Color32) {
    ui.vertical(|ui| {
        ui.colored_label(color, "●");
        ui.add_space(3.0);
        ui.heading(RichText::new(value).size(27.0));
        ui.label(RichText::new(label).small().color(theme::TEXT_MUTED));
    });
}

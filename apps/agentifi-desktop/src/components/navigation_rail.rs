use crate::components::Icon;
use crate::theme;
use eframe::egui::{RichText, Ui};
pub fn nav_button(ui: &mut Ui, selected: bool, icon: Icon, label: &str) -> bool {
    ui.horizontal(|ui| {
        super::paint_icon(
            ui,
            icon,
            if selected {
                theme::BLUE
            } else {
                theme::TEXT_SECONDARY
            },
            16.0,
        );
        ui.label(RichText::new(label).color(if selected {
            theme::TEXT_PRIMARY
        } else {
            theme::TEXT_SECONDARY
        }));
    })
    .response
    .clicked()
}

use crate::theme;
use eframe::egui::{RichText, Ui};
pub fn nav_button(ui: &mut Ui, selected: bool, icon: &str, label: &str) -> bool {
    ui.selectable_label(
        selected,
        RichText::new(format!("{icon}  {label}")).color(if selected {
            theme::TEXT_PRIMARY
        } else {
            theme::TEXT_SECONDARY
        }),
    )
    .clicked()
}

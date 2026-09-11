use crate::{
    components::{paint_icon, Icon},
    theme,
};
use eframe::egui::{Button, RichText, Ui};
pub fn nav_button(ui: &mut Ui, selected: bool, icon: Icon, label: &str) -> bool {
    ui.horizontal(|ui| {
        paint_icon(
            ui,
            icon,
            if selected {
                theme::BLUE
            } else {
                theme::TEXT_SECONDARY
            },
            16.0,
        );
        ui.add_sized(
            [160.0, 30.0],
            Button::new(RichText::new(label).color(if selected {
                theme::TEXT_PRIMARY
            } else {
                theme::TEXT_SECONDARY
            }))
            .selected(selected),
        )
        .clicked()
    })
    .inner
}

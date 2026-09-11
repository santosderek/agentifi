use crate::{
    components::{status, status_color},
    theme,
};
use agentifi_domain::AgentSession;
use eframe::egui::{self, Align, Layout, RichText, Stroke, Ui};
pub fn display_title(s: &AgentSession) -> String {
    if s.title.starts_with("20") || s.title.len() > 48 {
        format!("Session · {}", &s.id.to_string()[..8])
    } else {
        s.title.clone()
    }
}
pub fn session_row(ui: &mut Ui, s: &AgentSession, selected: bool) -> egui::Response {
    let response = ui
        .allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 50.0),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.colored_label(status_color(s), "●");
                ui.vertical(|ui| {
                    ui.label(RichText::new(display_title(s)).color(theme::TEXT_PRIMARY));
                    ui.small(
                        RichText::new(format!("{}  ·  {}", s.project, status(s)))
                            .color(theme::TEXT_SECONDARY),
                    );
                });
            },
        )
        .response;
    let response = ui.interact(response.rect, ui.id().with(s.id), egui::Sense::click());
    if selected {
        ui.painter().rect_stroke(
            response.rect,
            4.0,
            Stroke::new(1.0_f32, theme::BLUE),
            egui::StrokeKind::Inside,
        );
    }
    response
}

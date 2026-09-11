use crate::theme;
use agentifi_domain::{AgentSession, SessionStatus};
use eframe::egui::Color32;
pub fn status(s: &AgentSession) -> &'static str {
    match s.status {
        SessionStatus::Active => "active",
        SessionStatus::Paused => "paused",
        SessionStatus::Completed => "completed",
    }
}
pub fn status_color(s: &AgentSession) -> Color32 {
    match s.status {
        SessionStatus::Active => theme::GREEN,
        SessionStatus::Paused => theme::ORANGE,
        SessionStatus::Completed => theme::BLUE,
    }
}

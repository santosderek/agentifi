use crate::{
    components::text,
    state::AppView,
    theme,
    views::{Action, ViewContext},
};
use eframe::egui::{Align, Layout, RichText, Ui};
pub const LABEL: &str = "Settings";
pub fn show(ui: &mut Ui, ctx: &mut ViewContext<'_>) {
    ui.add_space(theme::SPACE_XL);
    ui.heading(RichText::new("Settings").font(theme::font_display()));
    ui.label(
        RichText::new("Active connections and local Agentifi server configuration")
            .color(theme::TEXT_SECONDARY),
    );
    ui.add_space(theme::SPACE_XL);
    theme::card().show(ui, |ui| {
        text::section_label(ui, "Active connections");
        ui.add_space(theme::SPACE_SM);
        connection_row(
            ui,
            "Agentifi server",
            ctx.endpoint,
            ctx.live,
            "HTTP + JSON-RPC",
        );
        ui.separator();
        connection_row(
            ui,
            "Pi event stream",
            "/api/v1/events",
            ctx.live,
            "Server-sent events",
        );
        ui.separator();
        let attached = ctx
            .sessions
            .iter()
            .filter(|s| s.attachment.label() == "attached")
            .count();
        connection_row(
            ui,
            "Pi sessions",
            &format!("{} discovered · {} attached", ctx.sessions.len(), attached),
            true,
            "Session catalog",
        );
    });
    ui.add_space(theme::SPACE_LG);
    theme::card().show(ui, |ui| {
        text::section_label(ui, "Session discovery"); ui.add_space(theme::SPACE_SM);
        ui.label(RichText::new("Pi sessions are loaded from the configured CLI session directory.").color(theme::TEXT_SECONDARY));
        ui.monospace(std::env::var("PI_CODING_AGENT_SESSION_DIR").unwrap_or_else(|_| "~/.pi/agent/sessions".into()));
        ui.add_space(theme::SPACE_SM);
        ui.label(RichText::new("The server recursively scans JSONL transcripts and refreshes when the SSE catalog changes.").color(theme::TEXT_MUTED));
        if ui.button("Refresh session catalog").clicked() { ctx.act(Action::Refresh); }
    });
    ui.add_space(theme::SPACE_LG);
    if ui.button("Back to overview").clicked() {
        ctx.state.go(AppView::Overview);
    }
}
fn connection_row(ui: &mut Ui, name: &str, endpoint: &str, connected: bool, kind: &str) {
    ui.horizontal(|ui| {
        ui.colored_label(if connected { theme::GREEN } else { theme::RED }, "●");
        ui.vertical(|ui| {
            ui.label(RichText::new(name).strong());
            ui.small(
                RichText::new(format!("{} · {}", endpoint, kind)).color(theme::TEXT_SECONDARY),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(if connected { "Connected" } else { "Offline" });
        });
    });
}

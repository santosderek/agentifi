//! Session inspector.
//!
//! Metadata the user cares about sits in `Details`; storage paths, identifiers,
//! and protocol state stay behind `Diagnostics`.

use crate::{
    components::{activity_timeline, empty_state, text},
    events::ActivityEvent,
    state::InspectorTab,
    theme,
};
use agentifi_domain::AgentSession;
use eframe::egui::{self, Align, Align2, Layout, Pos2, RichText, Sense, Ui, Vec2};

/// Draws the tab strip and returns the selected tab.
pub fn tab_strip(ui: &mut Ui, current: InspectorTab) -> InspectorTab {
    let mut selected = current;
    ui.horizontal(|ui| {
        for tab in InspectorTab::ALL {
            if ui
                .selectable_label(tab == current, RichText::new(tab.label()))
                .clicked()
            {
                selected = tab;
            }
        }
    });
    selected
}

/// Renders the inspector body for the active tab.
pub fn inspector_body(
    ui: &mut Ui,
    session: &AgentSession,
    tab: InspectorTab,
    events: &[ActivityEvent],
    now: i64,
) {
    match tab {
        InspectorTab::Details => details(ui, session, now),
        InspectorTab::Activity => activity_timeline::activity_feed(ui, events, 14),
        InspectorTab::Files => files(ui, session),
        InspectorTab::Diagnostics => diagnostics(ui, session),
    }
}

fn details(ui: &mut Ui, session: &AgentSession, now: i64) {
    row(ui, "Status", session.status.label());
    row(ui, "Attachment", session.attachment.label());
    row(ui, "Project", &session.project);
    row(
        ui,
        "Working directory",
        session
            .working_directory
            .as_deref()
            .unwrap_or("not reported"),
    );
    row(
        ui,
        "Branch",
        session.branch.as_deref().unwrap_or("not reported"),
    );
    row(
        ui,
        "Model",
        &session
            .model_label()
            .unwrap_or_else(|| "not reported".into()),
    );
    row(
        ui,
        "Messages",
        &session
            .message_count
            .map_or_else(|| "not reported".to_owned(), |count| count.to_string()),
    );
    row(
        ui,
        "Tool calls",
        &session
            .tool_call_count
            .map_or_else(|| "not reported".to_owned(), |count| count.to_string()),
    );
    row(ui, "Last activity", &session.age_label(now));
    if !session.tags.is_empty() {
        row(ui, "Tags", &session.tags.join(" "));
    }
}

fn files(ui: &mut Ui, session: &AgentSession) {
    match session.working_directory.as_deref() {
        Some(directory) => {
            row(ui, "Root", directory);
            empty_state::empty_state(
                ui,
                "No file changes reported",
                "Pi reports touched files once the supervised process streams tool results.",
            );
        }
        None => empty_state::empty_state(
            ui,
            "No working directory",
            "This session's transcript does not record a working directory.",
        ),
    }
}

fn diagnostics(ui: &mut Ui, session: &AgentSession) {
    row(ui, "Session ID", &session.id.to_string());
    row(ui, "Stored title", &session.title);
    row(
        ui,
        "Source path",
        session
            .source_path
            .as_deref()
            .unwrap_or("in-memory session"),
    );
    row(
        ui,
        "Created",
        &session
            .created_at
            .map_or_else(|| "unknown".to_owned(), |at| at.to_string()),
    );
    row(
        ui,
        "Updated",
        &session
            .updated_at
            .map_or_else(|| "unknown".to_owned(), |at| at.to_string()),
    );
    ui.add_space(theme::SPACE_SM);
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if ui.button("Copy session ID").clicked() {
            ui.ctx().copy_text(session.id.to_string());
        }
    });
}

/// Label on the left, value right-aligned and truncated.
pub fn row(ui: &mut Ui, label: &str, value: &str) {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::hover());
    let painter = ui.painter();
    let label_width = (rect.width() * 0.42).min(150.0);
    text::line(
        painter,
        rect.left_center(),
        Align2::LEFT_CENTER,
        label,
        theme::font_meta(),
        theme::TEXT_MUTED,
        label_width,
    );
    text::line(
        painter,
        Pos2::new(rect.right(), rect.center().y),
        Align2::RIGHT_CENTER,
        value,
        theme::font_meta(),
        theme::TEXT_SECONDARY,
        rect.width() - label_width - 12.0,
    );
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(1.0_f32, theme::BORDER_SUBTLE),
    );
    if value.len() > 24 {
        response.on_hover_text(value);
    }
}

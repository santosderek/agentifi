//! Conversation and execution timeline.
//!
//! Every entry uses the same gutter: clock, source label, then the text. Prompts,
//! agent output, tool calls, and errors are one stream, filterable by kind.

use crate::{
    components::text,
    events::{ActivityEvent, ActivityKind},
    state::TimelineFilter,
    theme,
};
use eframe::egui::{self, Align2, Pos2, ScrollArea, Sense, Ui, Vec2};

/// Draws a scrolling timeline of events, newest last.
pub fn activity_timeline(
    ui: &mut Ui,
    events: &[ActivityEvent],
    filter: TimelineFilter,
    max_height: f32,
) {
    let visible: Vec<&ActivityEvent> = events
        .iter()
        .filter(|event| keep(event.kind, filter))
        .collect();

    if visible.is_empty() {
        crate::components::empty_state::empty_state(
            ui,
            "No activity yet",
            "Prompts, agent output, and tool calls stream here while the session is attached.",
        );
        return;
    }

    ScrollArea::vertical()
        .max_height(max_height)
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for event in visible {
                entry(ui, event);
            }
        });
}

/// Compact single-line feed used on the Overview.
pub fn activity_feed(ui: &mut Ui, events: &[ActivityEvent], limit: usize) {
    if events.is_empty() {
        crate::components::empty_state::empty_state(
            ui,
            "Stream idle",
            "Server and Pi events appear here as they arrive.",
        );
        return;
    }
    for event in events.iter().rev().take(limit) {
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::hover());
        let painter = ui.painter();
        text::line(
            painter,
            rect.left_center(),
            Align2::LEFT_CENTER,
            &event.clock(),
            theme::font_meta(),
            theme::TEXT_MUTED,
            48.0,
        );
        text::line(
            painter,
            Pos2::new(rect.left() + 46.0, rect.center().y),
            Align2::LEFT_CENTER,
            event.kind.label(),
            theme::font_meta(),
            kind_color(event.kind),
            60.0,
        );
        text::line(
            painter,
            Pos2::new(rect.left() + 96.0, rect.center().y),
            Align2::LEFT_CENTER,
            &event.text,
            theme::font_body(),
            theme::TEXT_SECONDARY,
            rect.width() - 100.0,
        );
    }
}

fn entry(ui: &mut Ui, event: &ActivityEvent) {
    let width = ui.available_width();
    let painter_font = theme::font_body();
    // Measure the wrapped body so the row reserves the right height.
    let galley = ui.painter().layout(
        event.text.clone(),
        painter_font.clone(),
        theme::TEXT_PRIMARY,
        (width - 96.0).max(120.0),
    );
    let height = galley.size().y + if event.detail.is_some() { 34.0 } else { 20.0 };
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let painter = ui.painter();

    text::line(
        painter,
        Pos2::new(rect.left(), rect.top() + 2.0),
        Align2::LEFT_TOP,
        &event.clock(),
        theme::font_meta(),
        theme::TEXT_MUTED,
        44.0,
    );
    text::line(
        painter,
        Pos2::new(rect.left() + 46.0, rect.top() + 2.0),
        Align2::LEFT_TOP,
        event.kind.label(),
        theme::font_meta(),
        kind_color(event.kind),
        48.0,
    );
    painter.galley(
        Pos2::new(rect.left() + 96.0, rect.top()),
        galley,
        body_color(event.kind),
    );
    if let Some(detail) = event.detail.as_deref() {
        text::line(
            painter,
            Pos2::new(rect.left() + 96.0, rect.bottom() - 20.0),
            Align2::LEFT_TOP,
            detail,
            theme::font_meta(),
            theme::TEXT_MUTED,
            rect.width() - 100.0,
        );
    }
    ui.add_space(theme::SPACE_XS);
}

fn keep(kind: ActivityKind, filter: TimelineFilter) -> bool {
    match filter {
        TimelineFilter::All => true,
        TimelineFilter::Messages => {
            matches!(kind, ActivityKind::UserMessage | ActivityKind::AgentMessage)
        }
        TimelineFilter::Tools => matches!(kind, ActivityKind::Tool),
    }
}

fn kind_color(kind: ActivityKind) -> egui::Color32 {
    match kind {
        ActivityKind::Connection => theme::BLUE,
        ActivityKind::UserMessage => theme::TEXT_SECONDARY,
        ActivityKind::AgentMessage => theme::GREEN,
        ActivityKind::Tool => theme::PURPLE,
        ActivityKind::Error => theme::RED,
        ActivityKind::Session => theme::BLUE,
        ActivityKind::Info => theme::TEXT_MUTED,
    }
}

fn body_color(kind: ActivityKind) -> egui::Color32 {
    match kind {
        ActivityKind::Error => theme::RED,
        ActivityKind::Tool
        | ActivityKind::Info
        | ActivityKind::Connection
        | ActivityKind::Session => theme::TEXT_SECONDARY,
        _ => theme::TEXT_PRIMARY,
    }
}

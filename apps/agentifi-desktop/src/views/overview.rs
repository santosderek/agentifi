//! Overview: a deliberately quiet landing screen.
//!
//! The reference is a terminal dashboard: one narrow column, generous vertical
//! rhythm, monospace metadata, hairline rules instead of cards, and a keyboard
//! hint next to every action. It answers "what is happening?" in one glance and
//! then gets out of the way.

use crate::{
    components::{activity_feed, session_row, stat, text, RowStyle},
    state::AppView,
    theme,
    views::{Action, ViewContext},
};
use agentifi_domain::SessionStatus;
use eframe::egui::{self, Align, Align2, Layout, Pos2, ScrollArea, Sense, Ui, Vec2};

pub const LABEL: &str = "Overview";

/// Quick actions, in the order they appear, with their shortcut keys.
const QUICK_ACTIONS: [(&str, &str); 4] = [
    ("Resume most recent session", "r"),
    ("Browse all sessions", "e"),
    ("Open board", "b"),
    ("Refresh catalog", "F5"),
];

pub fn show(ui: &mut Ui, ctx: &mut ViewContext<'_>) {
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let indent = ((ui.available_width() - theme::CONTENT_MAX_WIDTH) / 2.0).max(0.0);
            ui.horizontal(|ui| {
                ui.add_space(indent);
                ui.allocate_ui_with_layout(
                    Vec2::new(
                        theme::CONTENT_MAX_WIDTH.min(ui.available_width()),
                        ui.available_height(),
                    ),
                    Layout::top_down(Align::Min),
                    |ui| column(ui, ctx),
                );
            });
        });
}

fn column(ui: &mut Ui, ctx: &mut ViewContext<'_>) {
    let total = ctx.sessions.len();
    let active = count(ctx, SessionStatus::Active);
    let review = count(ctx, SessionStatus::NeedsReview) + count(ctx, SessionStatus::Failed);
    let messages: u32 = ctx
        .sessions
        .iter()
        .filter_map(|session| session.message_count)
        .sum();

    ui.add_space(theme::SPACE_2XL);
    heading(ui, greeting());
    ui.add_space(theme::SPACE_SM);
    subtitle(
        ui,
        &format!(
            "{} sessions  ·  {} projects  ·  {}",
            total,
            ctx.projects().len(),
            if ctx.live {
                "connected to the local Pi server"
            } else {
                "server unreachable"
            }
        ),
    );

    ui.add_space(theme::SPACE_XL);
    ui.horizontal(|ui| {
        stat(ui, &total.to_string(), "sessions", theme::TEXT_PRIMARY);
        ui.add_space(theme::SPACE_LG);
        stat(ui, &active.to_string(), "active", theme::GREEN);
        ui.add_space(theme::SPACE_LG);
        stat(ui, &review.to_string(), "need review", theme::ORANGE);
        ui.add_space(theme::SPACE_LG);
        stat(ui, &messages.to_string(), "messages", theme::TEXT_SECONDARY);
    });

    ui.add_space(theme::SPACE_XL);
    theme::hairline(ui);
    ui.add_space(theme::SPACE_XL);

    quick_actions(ui, ctx);

    ui.add_space(theme::SPACE_2XL);
    text::section_label(ui, "Recent sessions");
    recent_sessions(ui, ctx);

    ui.add_space(theme::SPACE_2XL);
    text::section_label(ui, "Live activity");
    activity_feed(ui, ctx.events, 6);

    ui.add_space(theme::SPACE_2XL);
    footer(ui, ctx);
    ui.add_space(theme::SPACE_XL);
}

fn quick_actions(ui: &mut Ui, ctx: &mut ViewContext<'_>) {
    for (index, (label, key)) in QUICK_ACTIONS.iter().enumerate() {
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::click());
        if response.hovered() {
            ui.painter().rect_filled(
                rect,
                egui::CornerRadius::same(theme::RADIUS_CONTROL),
                theme::SURFACE_ELEVATED,
            );
        }
        let painter = ui.painter();
        text::line(
            painter,
            Pos2::new(rect.left() + 6.0, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            theme::font_body(),
            if response.hovered() {
                theme::TEXT_PRIMARY
            } else {
                theme::TEXT_SECONDARY
            },
            rect.width() - 60.0,
        );
        text::key_hint(painter, Pos2::new(rect.right() - 6.0, rect.center().y), key);
        if response.clicked() {
            run_quick_action(index, ctx);
        }
    }
}

/// Shared by the click handler and the keyboard shortcuts in the shell.
pub fn run_quick_action(index: usize, ctx: &mut ViewContext<'_>) {
    match index {
        0 => {
            if let Some(session) = ctx.sessions.first().cloned() {
                ctx.state.open_session(session.id);
                ctx.act(Action::Attach(session.id));
            }
        }
        1 => ctx.state.go(AppView::Explorer),
        2 => ctx.state.go(AppView::Board),
        _ => ctx.act(Action::Refresh),
    }
}

fn recent_sessions(ui: &mut Ui, ctx: &mut ViewContext<'_>) {
    if ctx.sessions.is_empty() {
        crate::components::empty_state(
            ui,
            "No Pi sessions discovered",
            "Set PI_CODING_AGENT_SESSION_DIR or start a Pi session; discovery picks it up automatically.",
        );
        return;
    }
    let recent: Vec<_> = ctx.sessions.iter().take(6).cloned().collect();
    for (index, session) in recent.iter().enumerate() {
        let selected = ctx.state.selected == Some(session.id);
        let response = session_row(ui, session, selected, ctx.now, RowStyle::Compact);
        // Digit hints mirror the terminal dashboard: 1–6 open a session directly.
        text::key_hint(
            ui.painter(),
            Pos2::new(response.rect.right() - 48.0, response.rect.center().y),
            &(index + 1).to_string(),
        );
        if response.clicked() {
            ctx.state.open_session(session.id);
            ctx.act(Action::Attach(session.id));
        }
    }
}

fn footer(ui: &mut Ui, ctx: &mut ViewContext<'_>) {
    theme::hairline(ui);
    ui.add_space(theme::SPACE_SM);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), Sense::hover());
    let painter = ui.painter();
    text::line(
        painter,
        rect.left_center(),
        Align2::LEFT_CENTER,
        &format!("agentifi  ·  {}", ctx.endpoint),
        theme::font_label(),
        theme::TEXT_MUTED,
        rect.width() * 0.6,
    );
    text::line(
        painter,
        Pos2::new(rect.right(), rect.center().y),
        Align2::RIGHT_CENTER,
        "g o overview  ·  g e explorer  ·  g b board  ·  / search",
        theme::font_label(),
        theme::TEXT_MUTED,
        rect.width() * 0.6,
    );
}

fn heading(ui: &mut Ui, value: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::hover());
    text::line(
        ui.painter(),
        rect.left_top(),
        Align2::LEFT_TOP,
        value,
        theme::font_display(),
        theme::TEXT_PRIMARY,
        rect.width(),
    );
}

fn subtitle(ui: &mut Ui, value: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 20.0), Sense::hover());
    text::line(
        ui.painter(),
        rect.left_top(),
        Align2::LEFT_TOP,
        value,
        theme::font_meta(),
        theme::TEXT_SECONDARY,
        rect.width(),
    );
}

fn count(ctx: &ViewContext<'_>, status: SessionStatus) -> usize {
    ctx.sessions
        .iter()
        .filter(|session| session.status == status)
        .count()
}

/// Time-of-day greeting, computed from the local clock without extra dependencies.
fn greeting() -> &'static str {
    let hour = (chrono_hour() + 24) % 24;
    match hour {
        5..=11 => "Good morning.",
        12..=17 => "Good afternoon.",
        _ => "Good evening.",
    }
}

fn chrono_hour() -> i64 {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default();
    (seconds / 3_600) % 24
}
